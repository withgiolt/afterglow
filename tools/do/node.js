// Tier 2: a Durable Object hosting one glowc node (one instance, many processes).
// Each fetch is a turn: the request body is the turn input (ETF), the reply is the turn
// output. After the turn, dirty processes are written to SQLite, one row per process,
// tagged with BUILD (sha256 of app.wasm, filled in by build-do.sh). On wake-up the rows
// are restored; rows from another build go through the upgrade path.
//
// Messages to other nodes leave in the turn's outbox: one {deliver, [{Dest, Msg}]}
// turn per target DO, sent after the turn, in order per target (a promise chain), not
// awaited by the turn. If a target fails, this node runs {noconnection, Node}.
//
// Cloudflare caps how deep a chain of requests between Workers and DOs can go: a send
// made while handling another DO's send is one level deeper, and deployed, the 11th
// level never arrived (no error). So each send carries its depth (x-glowc-hop). Past
// HOP_MAX the outbox goes to SQLite (outq) instead, and the DO alarm, which starts a
// new chain, sends it.
import { DurableObject } from "cloudflare:workers";
import wasm from "./app.wasm";

const BUILD = "__BUILD__";
const dec = new TextDecoder();
const HOP_MAX = 8;

export class GlowNode extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.inst = null;
    this.chains = new Map();
    ctx.storage.sql.exec("CREATE TABLE IF NOT EXISTS procs (pid INTEGER PRIMARY KEY, build TEXT, bytes BLOB)");
    ctx.storage.sql.exec("CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v TEXT)");
    ctx.storage.sql.exec("CREATE TABLE IF NOT EXISTS outq (id INTEGER PRIMARY KEY AUTOINCREMENT, node TEXT, body BLOB)");
    const row = [...ctx.storage.sql.exec("SELECT v FROM meta WHERE k = 'name'")][0];
    this.name = row ? row.v : null;
  }

  boot() {
    let mem;
    const instance = new WebAssembly.Instance(wasm, { host: {
      log: (p, n) => console.log(dec.decode(new Uint8Array(mem.buffer, p, n))),
      random_bytes: (p, n) => crypto.getRandomValues(new Uint8Array(mem.buffer, p, n)),
      now_us: () => Date.now() * 1000,
    } });
    const x = instance.exports;
    mem = x.memory;
    x.init();
    x.rt_node_mode();
    const nb = new TextEncoder().encode(this.name);
    const np = x.rt_restore_buf(nb.length);
    new Uint8Array(mem.buffer, np, nb.length).set(nb);
    x.rt_set_node(np, nb.length);
    let restored = 0, upgraded = 0;
    for (const row of this.ctx.storage.sql.exec("SELECT pid, build, bytes FROM procs ORDER BY pid")) {
      const bytes = new Uint8Array(row.bytes);
      const p = x.rt_restore_buf(bytes.length);
      new Uint8Array(mem.buffer, p, bytes.length).set(bytes);
      const up = row.build !== BUILD ? 1 : 0;
      if (x.rt_restore(up) !== 0) throw new Error("restore failed for pid " + row.pid);
      restored++; upgraded += up;
    }
    x.rt_restore_done();
    this.inst = x;
    this.mem = mem;
    this.bootInfo = { restored, upgraded };
  }

  unpack(v) {
    const u = BigInt.asUintN(64, v);
    return new Uint8Array(this.mem.buffer, Number(u >> 32n), Number(u & 0xffffffffn)).slice();
  }

  async fetch(request) {
    const name = request.headers.get("x-glowc-node") || "default";
    const hop = Number(request.headers.get("x-glowc-hop") || 0);
    if (this.name !== name) {
      this.name = name;
      this.ctx.storage.sql.exec("INSERT OR REPLACE INTO meta (k, v) VALUES ('name', ?)", name);
    }
    if (request.headers.get("upgrade") === "websocket") return this.accept(request);
    const input = new Uint8Array(await request.arrayBuffer());
    const r = this.turn(input, hop);
    return new Response(r.body, { status: r.status, headers: r.headers });
  }

  // hop: how deep in a chain of sends between DOs this turn runs (0: a client, alarm,
  // WebSocket or Cron turn).
  turn(input, hop = 0) {
    this.hop = hop;
    const t0 = Date.now();
    const fresh = !this.inst;
    if (fresh) this.boot();
    const x = this.inst;
    const b = x.rt_begin(input.length);
    new Uint8Array(this.mem.buffer, b, input.length).set(input);
    const r = x.main();
    let status = 200, body;
    if (r === 0n) { status = 500; body = dec.decode(this.unpack(x.rt_exception_text())); }
    else body = this.unpack(x.rt_output());
    if (x.rt_take_crash()) {
      // a failing node: drop the instance without checkpointing this turn
      this.inst = null;
      throw new Error("glowc node crashed on purpose");
    }
    const n = x.rt_checkpoint();
    const sql = this.ctx.storage.sql;
    let bytesWritten = 0;
    for (let i = 0; i < n; i++) {
      const pid = x.rt_ckpt_pid(i) | 0;
      const bytes = this.unpack(x.rt_ckpt_blob(i));
      bytesWritten += bytes.length;
      sql.exec("INSERT OR REPLACE INTO procs (pid, build, bytes) VALUES (?, ?, ?)", pid, BUILD, bytes);
    }
    for (let i = 0; i < x.rt_dead_count(); i++) sql.exec("DELETE FROM procs WHERE pid = ?", x.rt_dead_pid(i));
    x.rt_checkpoint_done();
    this.flush(x);
    this.sendWs(x);
    // one alarm for the earliest erlang timer, or now if sends are queued
    let next = x.rt_next_timer();
    if (this.queued) { next = Date.now(); this.queued = false; }
    if (next >= 0) this.ctx.storage.setAlarm(next);
    else if (!this.pendingQueue()) this.ctx.storage.deleteAlarm();
    return { status, body, headers: {
      "x-glowc-boot": fresh ? JSON.stringify(this.bootInfo) : "warm",
      "x-glowc-rows": String(n), "x-glowc-row-bytes": String(bytesWritten),
      "x-glowc-turn-ms": String(Date.now() - t0),
      "x-glowc-alarm-runs": String(this.alarmRuns || 0),
    } };
  }

  // WebSockets: accepted with the Hibernation API, so the DO can be evicted while they
  // stay open. Each socket gets an id (persisted counter) and an owner process (?ws=
  // Name, started as Mod:start() from ?mod=, default Name), kept in its attachment.
  accept(request) {
    const url = new URL(request.url);
    const owner = url.searchParams.get("ws") || "ws";
    const mod = url.searchParams.get("mod") || owner;
    const row = [...this.ctx.storage.sql.exec("SELECT v FROM meta WHERE k = 'ws_next'")][0];
    const id = row ? Number(row.v) : 1;
    this.ctx.storage.sql.exec("INSERT OR REPLACE INTO meta (k, v) VALUES ('ws_next', ?)", String(id + 1));
    const [client, server] = Object.values(new WebSocketPair());
    this.ctx.acceptWebSocket(server, ["ws" + id]);
    server.serializeAttachment({ id, owner, mod, node: this.name });
    this.wsEvent(server, [...atom("open")]);
    return new Response(null, { status: 101, webSocket: client });
  }

  wsEvent(ws, eventEtf) {
    const a = ws.deserializeAttachment();
    if (!this.name) this.name = a.node;
    const fresh = !this.inst;
    const ref = [104, 2, ...atom("glowc_ws"), 98, ...u32be(a.id)];
    this.turn(Uint8Array.from([131, 104, 5, ...atom("ws"), ...atom(a.owner), ...atom(a.mod), ...ref, ...eventEtf]));
    if (fresh && this.bootInfo.restored > 0) {
      try { ws.send("__boot " + JSON.stringify(this.bootInfo)); } catch {}
    }
  }

  async webSocketMessage(ws, message) {
    if (message === "__evict") { this.inst = null; return; } // tests: drop the instance, keep the sockets
    if (typeof message === "string") {
      const b = enc.encode(message);
      this.wsEvent(ws, [104, 2, ...atom("text"), 109, ...u32be(b.length), ...b]);
    } else {
      const b = new Uint8Array(message);
      this.wsEvent(ws, [104, 2, ...atom("binary"), 109, ...u32be(b.length), ...b]);
    }
  }

  async webSocketClose(ws) {
    this.wsEvent(ws, [...atom("closed")]);
  }

  sendWs(x) {
    for (let i = 0; i < x.rt_ws_count(); i++) {
      const id = x.rt_ws_id(i), kind = x.rt_ws_kind(i);
      const data = this.unpack(x.rt_ws_data(i));
      for (const ws of this.ctx.getWebSockets("ws" + id)) {
        try { kind === 1 ? ws.close(1000, "closed") : ws.send(dec.decode(data)); } catch {}
      }
    }
    x.rt_ws_clear();
  }

  pendingQueue() {
    return [...this.ctx.storage.sql.exec("SELECT COUNT(*) AS n FROM outq")][0].n > 0;
  }

  async alarm() {
    this.alarmRuns = (this.alarmRuns || 0) + 1;
    const rows = [...this.ctx.storage.sql.exec("SELECT id, node, body FROM outq ORDER BY id")];
    this.ctx.storage.sql.exec("DELETE FROM outq");
    this.hop = 0;
    const sends = rows.map((r) => this.enqueue(r.node, new Uint8Array(r.body)));
    this.turn(Uint8Array.from([131, ...atom("timers")]));
    await Promise.all(sends);
  }

  flush(x) {
    const byNode = new Map();
    for (let i = 0; i < x.rt_outbox_count(); i++) {
      const node = dec.decode(this.unpack(x.rt_outbox_node(i)));
      const item = this.unpack(x.rt_outbox_item(i)).subarray(1); // drop the version byte
      if (!byNode.has(node)) byNode.set(node, []);
      byNode.get(node).push(item);
    }
    x.rt_outbox_clear();
    for (const [node, items] of byNode) {
      const body = etfDeliver(items);
      // queued sends to a node stay ahead of later ones (order per sender and receiver)
      const behind = [...this.ctx.storage.sql.exec("SELECT 1 FROM outq WHERE node = ? LIMIT 1", node)].length > 0;
      if (this.hop >= HOP_MAX || behind) {
        this.ctx.storage.sql.exec("INSERT INTO outq (node, body) VALUES (?, ?)", node, body);
        this.queued = true;
      } else this.ctx.waitUntil(this.enqueue(node, body));
    }
  }

  enqueue(node, body) {
    const hop = this.hop + 1;
    const prev = this.chains.get(node) || Promise.resolve();
    const next = prev.then(() => this.send(node, body, hop));
    this.chains.set(node, next);
    return next;
  }

  async send(node, body, hop) {
    let ok = false;
    try {
      const stub = this.env.NODE.get(this.env.NODE.idFromName(node));
      const r = await stub.fetch("http://do/", { method: "POST", body, headers: { "x-glowc-node": node, "x-glowc-hop": String(hop) } });
      await r.arrayBuffer();
      ok = r.ok;
    } catch {}
    if (!ok) this.turn(etfNoconnection(node));
  }
}

const enc = new TextEncoder();
function u32be(n) { return [(n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255]; }
function atom(s) { const b = enc.encode(s); return [119, b.length, ...b]; }
function etfDeliver(items) {
  const len = items.reduce((n, i) => n + i.length, 0);
  const head = [131, 104, 2, ...atom("deliver"), 108, 0, 0, 0, 0];
  const out = new Uint8Array(head.length + len + 1);
  out.set(head);
  new DataView(out.buffer).setUint32(head.length - 4, items.length);
  let o = head.length;
  for (const i of items) { out.set(i, o); o += i.length; }
  out[o] = 106;
  return out;
}
function etfNoconnection(node) {
  return Uint8Array.from([131, 104, 2, ...atom("noconnection"), ...atom(node)]);
}

// Cron Triggers: {tick, Cron} to the process registered as GLOWC_CRON_PROC in the DO
// named GLOWC_CRON_NODE.
function etfTick(proc, cron) {
  const c = enc.encode(cron);
  const out = [131, 104, 2, ...atom("deliver"), 108, 0, 0, 0, 1,
    104, 2, ...atom(proc), 104, 2, ...atom("tick"), 109, 0, 0, 0, 0, ...c, 106];
  const u = Uint8Array.from(out);
  new DataView(u.buffer).setUint32(u.length - 1 - c.length - 4, c.length);
  return u;
}

export default {
  async scheduled(event, env, ctx) {
    const node = env.GLOWC_CRON_NODE || "default", proc = env.GLOWC_CRON_PROC || "ticker";
    const stub = env.NODE.get(env.NODE.idFromName(node));
    const r = await stub.fetch("http://do/", { method: "POST", body: etfTick(proc, event.cron), headers: { "x-glowc-node": node } });
    await r.arrayBuffer();
  },
  async fetch(request, env) {
    const url = new URL(request.url);
    const name = url.searchParams.get("node") || "default";
    const stub = env.NODE.get(env.NODE.idFromName(name));
    const headers = new Headers(request.headers);
    headers.set("x-glowc-node", name);
    return stub.fetch(new Request(request, { headers }));
  },
};
