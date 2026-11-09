// Tier 2: a Durable Object hosting one glowc node (one instance, many processes).
// Each fetch is a turn: the request body is the turn input (ETF), the reply is the turn
// output. After the turn, dirty processes are written to SQLite, one row per process,
// tagged with BUILD (sha256 of app.wasm, filled in by build-do.sh). On wake-up the rows
// are restored; rows from another build go through the upgrade path.
import { DurableObject } from "cloudflare:workers";
import wasm from "./app.wasm";

const BUILD = "__BUILD__";
const dec = new TextDecoder();

export class GlowNode extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.inst = null;
    ctx.storage.sql.exec("CREATE TABLE IF NOT EXISTS procs (pid INTEGER PRIMARY KEY, build TEXT, bytes BLOB)");
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
    const t0 = Date.now();
    const fresh = !this.inst;
    if (fresh) this.boot();
    const x = this.inst;
    const input = new Uint8Array(await request.arrayBuffer());
    const b = x.rt_begin(input.length);
    new Uint8Array(this.mem.buffer, b, input.length).set(input);
    const r = x.main();
    let status = 200, body;
    if (r === 0n) { status = 500; body = dec.decode(this.unpack(x.rt_exception_text())); }
    else body = this.unpack(x.rt_output());
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
    return new Response(body, { status, headers: {
      "x-glowc-boot": fresh ? JSON.stringify(this.bootInfo) : "warm",
      "x-glowc-rows": String(n), "x-glowc-row-bytes": String(bytesWritten),
      "x-glowc-turn-ms": String(Date.now() - t0),
    } });
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const name = url.searchParams.get("node") || "default";
    const stub = env.NODE.get(env.NODE.idFromName(name));
    return stub.fetch(request);
  },
};
