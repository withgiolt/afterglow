// P3 on local workerd: ping-pong between two DOs (a -> {ponger, b}), then a monitor on
// a remote process that fires noconnection when the target DO throws.
// Usage: node tools/do/p3-test.mjs [CONFIG] [ROUND_TRIPS] [BASE_URL]
// With BASE_URL the deployed Worker is used instead of a local one.
import { unstable_startWorker } from "wrangler";
import { atom, int, apply, decode } from "./etf.mjs";

const [config = "out/do-p3/wrangler.jsonc", nArg = "200", base] = process.argv.slice(2);
const N = Number(nArg);
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
  const r = await fetcher(`/?node=${node}`, { method: "POST", body: apply("pp", f, args) });
  const b = new Uint8Array(await r.arrayBuffer());
  if (r.status !== 200) throw new Error(`${node} ${f}: HTTP ${r.status} ${Buffer.from(b)}`);
  return decode(b);
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
