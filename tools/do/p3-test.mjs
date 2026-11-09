// P3 on local workerd: ping-pong between two DOs (a -> {ponger, b}), then a monitor on
// a remote process that fires noconnection when the target DO throws.
// Usage: node tools/do/p3-test.mjs [CONFIG] [ROUND_TRIPS] [BASE_URL]
// With BASE_URL the deployed Worker is used instead of a local one.
import { unstable_startWorker } from "wrangler";

const [config = "out/do-p3/wrangler.jsonc", nArg = "200", base] = process.argv.slice(2);
const N = Number(nArg);
const enc = new TextEncoder();
const atom = (s) => { const b = enc.encode(s); return [119, b.length, ...b]; };
const int = (n) => { const b = Buffer.alloc(5); b[0] = 98; b.writeInt32BE(n, 1); return [...b]; };
const list = (xs) => xs.length ? [108, 0, 0, 0, xs.length, ...xs.flat(), 106] : [106];
const apply = (f, args) => Uint8Array.from([131, 104, 4, ...atom("apply"), ...atom("pp"), ...atom(f), ...list(args)]);
function show(b) {
  let o = 1; const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const t = () => { const g = b[o++];
    if (g === 97) return b[o++];
    if (g === 98) { const v = dv.getInt32(o); o += 4; return v; }
    if (g === 110) { const n = b[o++]; const s = b[o++]; let v = 0; for (let i = n - 1; i >= 0; i--) v = v * 256 + b[o + i]; o += n; return s ? -v : v; }
    if (g === 119) { const n = b[o++]; const s = Buffer.from(b.subarray(o, o + n)).toString(); o += n; return s; }
    if (g === 104) { const n = b[o++]; const xs = []; for (let i = 0; i < n; i++) xs.push(t()); return xs; }
    return "?" + g; };
  return t();
}
let w = null, fetcher;
if (base) fetcher = (u, init) => fetch(base + u, init);
else {
  w = await unstable_startWorker({ config, dev: { persist: false, server: { port: 0 }, inspector: false } });
  await w.ready;
  fetcher = (u, init) => w.fetch("http://x" + u, init);
}
let requests = 0;
async function call(node, f, args = []) {
  requests++;
  const r = await fetcher(`/?node=${node}`, { method: "POST", body: apply(f, args) });
  const b = new Uint8Array(await r.arrayBuffer());
  if (r.status !== 200) throw new Error(`${node} ${f}: HTTP ${r.status} ${Buffer.from(b)}`);
  return show(b);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const out = {};
out.ponger = await call("b", "start_ponger");
const t0 = Date.now();
out.pinger = await call("a", "start_pinger", [atom("b"), int(N)]);
let res;
for (let i = 0; i < 600; i++) {
  res = await call("a", "result");
  if (Array.isArray(res) && res[0] === "done") break;
  await sleep(Math.min(100 * 2 ** i, 2000));
}
out.pingpong = { roundTrips: N, result: res, wallMs: Date.now() - t0, usPerRoundTrip: Array.isArray(res) ? Math.round(res[2] / N) : null };
out.watch = await call("a", "watch", [atom("b")]);
await sleep(300);
out.beforePoke = await call("a", "watch_result");
out.poke = await call("a", "poke", [atom("b")]);
let down;
for (let i = 0; i < 50; i++) {
  down = await call("a", "watch_result");
  if (Array.isArray(down)) break;
  await sleep(200);
}
out.down = down;
out.requests = requests;
console.log(JSON.stringify(out));
if (w) await w.dispose();
const pass = Array.isArray(res) && res[0] === "done" && res[1] === N && Array.isArray(down) && down[0] === "down" && down[1] === "noconnection";
console.log(pass ? "PASS p3" : "FAIL p3");
process.exit(pass ? 0 : 1);
