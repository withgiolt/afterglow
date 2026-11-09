// P2 on local workerd: the DO Worker under wrangler dev with persisted storage.
// Restarting the Worker drops the DO instance (eviction); restarting with the v2 build
// is a redeploy. Same scenario as tests/p2-scenario.json.
import { unstable_startWorker } from "wrangler";
import fs from "node:fs";

const persist = "out/do-persist";
fs.rmSync(persist, { recursive: true, force: true });
const call = Uint8Array.from([131, 104, 4, 119, 4, ...Buffer.from("call"), 119, 7, ...Buffer.from("counter"), 119, 7, ...Buffer.from("counter")]);
const msg = (m) => { const b = Buffer.from(m); return Buffer.concat([call, Buffer.from([119, b.length]), b]); };
const show = (b) => { // {vN, Int}
  const v = b.subarray(5, 7).toString(); const tag = b[7];
  return `{${v},${tag === 97 ? b[8] : b.readInt32BE(8)}}`;
};
const out = [];
async function session(config, msgs) {
  const w = await unstable_startWorker({ config, dev: { persist, server: { port: 0 }, inspector: false } });
  await w.ready;
  for (const m of msgs) {
    const r = await w.fetch("http://x/?node=counter", { method: "POST", body: msg(m) });
    const b = Buffer.from(await r.arrayBuffer());
    out.push(r.status === 200 ? show(b) : `HTTP ${r.status}: ${b}`);
    console.error(m, r.status, r.headers.get("x-glowc-boot"), r.headers.get("x-glowc-rows"), r.headers.get("x-glowc-row-bytes"));
  }
  await w.dispose();
}
await session("out/do-v1/wrangler.jsonc", ["incr", "incr"]);
await session("out/do-v1/wrangler.jsonc", ["incr"]);
await session("out/do-v2/wrangler.jsonc", ["incr", "get"]);
await session("out/do-v2/wrangler.jsonc", ["incr"]);
console.log(out.join(" "));
process.exit(0);
