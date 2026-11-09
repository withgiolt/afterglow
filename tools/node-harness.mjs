// Tier 2 harness under Node: a simulated Durable Object around a glowc build in node
// mode. Rows (one per process, plus meta) live in a Map standing in for SQLite, tagged
// with the build's hash. "evict" drops the instance; the next turn restores lazily.
// Usage: node tools/node-harness.mjs SCRIPT.json
//   SCRIPT: [{"wasm": "a.wasm"} | {"call": [Name, Mod, Msg]} | {"evict": true}, ...]
//   Terms in "call" are encoded with a tiny ETF writer (atoms as strings, ints).
import fs from "node:fs";
import crypto from "node:crypto";

const dec = new TextDecoder();
let wasmPath = null, inst = null, mem = null, build = null;
const store = new Map(); // pid -> {build, bytes}
const steps = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const log = [];

function unpack(v) {
  const u = BigInt.asUintN(64, v);
  return new Uint8Array(mem.buffer, Number(u >> 32n), Number(u & 0xffffffffn)).slice();
}

async function boot() {
  const bytes = fs.readFileSync(wasmPath);
  build = crypto.createHash("sha256").update(bytes).digest("hex").slice(0, 16);
  const { instance } = await WebAssembly.instantiate(bytes, { host: {
    log: (p, n) => console.error("[log] " + dec.decode(new Uint8Array(mem.buffer, p, n))),
    random_bytes: (p, n) => crypto.randomFillSync(new Uint8Array(mem.buffer, p, n)),
    now_us: () => Date.now() * 1000,
  } });
  inst = instance.exports;
  mem = inst.memory;
  inst.init();
  if (process.env.GLOWC_DEBUG) inst.rt_set_debug(1);
  inst.rt_node_mode();
  let restored = 0, upgraded = 0;
  for (const [, row] of store) {
    const p = inst.rt_restore_buf(row.bytes.length);
    new Uint8Array(mem.buffer, p, row.bytes.length).set(row.bytes);
    const up = row.build !== build ? 1 : 0;
    if (inst.rt_restore(up) !== 0) throw new Error("restore failed");
    restored++; upgraded += up;
  }
  inst.rt_restore_done();
  return { restored, upgraded };
}

// ETF writer for small terms: strings -> atoms, numbers -> small ints, arrays -> tuples.
function etf(t) {
  const out = [131];
  const w = (x) => {
    if (typeof x === "string") { const b = Buffer.from(x); out.push(119, b.length, ...b); }
    else if (typeof x === "number") { out.push(98); const b = Buffer.alloc(4); b.writeInt32BE(x); out.push(...b); }
    else if (Array.isArray(x)) { out.push(104, x.length); x.forEach(w); }
    else throw new Error("etf: " + x);
  };
  w(t);
  return Uint8Array.from(out);
}

// ETF reader for replies: tuples, atoms, integers, lists, binaries, maps.
function show(b) {
  let o = 1;
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const t = () => {
    const tag = b[o++];
    switch (tag) {
      case 97: return String(b[o++]);
      case 98: { const v = dv.getInt32(o); o += 4; return String(v); }
      case 104: { const n = b[o++]; const xs = []; for (let i = 0; i < n; i++) xs.push(t()); return "{" + xs.join(",") + "}"; }
      case 119: { const n = b[o++]; const s = dec.decode(b.subarray(o, o + n)); o += n; return s; }
      case 118: { const n = dv.getUint16(o); o += 2; const s = dec.decode(b.subarray(o, o + n)); o += n; return s; }
      case 106: return "[]";
      case 108: { const n = dv.getUint32(o); o += 4; const xs = []; for (let i = 0; i < n; i++) xs.push(t()); t(); return "[" + xs.join(",") + "]"; }
      case 109: { const n = dv.getUint32(o); o += 4; const s = dec.decode(b.subarray(o, o + n)); o += n; return "<<\"" + s + "\">>"; }
      case 116: { const n = dv.getUint32(o); o += 4; const xs = []; for (let i = 0; i < n; i++) { const k = t(); xs.push(k + "=>" + t()); } return "#{" + xs.join(",") + "}"; }
      default: return "?" + tag;
    }
  };
  return t();
}

for (const st of steps) {
  if (st.wasm) { wasmPath = st.wasm; inst = null; log.push({ deploy: st.wasm }); continue; }
  if (st.evict) { inst = null; log.push({ evict: true }); continue; }
  let boot_info = null;
  if (!inst) boot_info = await boot();
  const req = etf(["call", ...st.call]);
  const b = inst.rt_begin(req.length);
  new Uint8Array(mem.buffer, b, req.length).set(req);
  const r = inst.main();
  let reply;
  if (r === 0n) reply = "exception: " + dec.decode(unpack(inst.rt_exception_text()));
  else reply = show(unpack(inst.rt_output()));
  const n = inst.rt_checkpoint();
  const rows = [];
  for (let i = 0; i < n; i++) {
    const pid = inst.rt_ckpt_pid(i);
    const bytes = unpack(inst.rt_ckpt_blob(i));
    store.set(pid, { build, bytes });
    if (process.env.GLOWC_DUMP) fs.writeFileSync(`/tmp/row-${log.length}-${pid}.bin`, bytes);
    rows.push([pid, bytes.length]);
  }
  for (let i = 0; i < inst.rt_dead_count(); i++) store.delete(inst.rt_dead_pid(i));
  inst.rt_checkpoint_done();
  log.push({ call: st.call, boot: boot_info, reply, rows, memKiB: mem.buffer.byteLength / 1024 });
}
console.log(JSON.stringify(log, null, 1));
