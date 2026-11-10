// P4 on local workerd: a 60-second erlang timer armed in a DO fires after the DO
// instance is gone (Worker restarted with persisted storage, no request in between),
// through the DO alarm; and Cron ticks reach a registered process.
// Usage: node tools/do/p4-test.mjs [CONFIG] [TIMER_MS]
import { unstable_startWorker } from "wrangler";
import fs from "node:fs";
import { int, apply } from "./etf.mjs";

const [config = "out/do-p4/wrangler.jsonc", msArg = "60000"] = process.argv.slice(2);
const MS = Number(msArg);
const persist = "out/do-p4-persist";
fs.rmSync(persist, { recursive: true, force: true });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const start = () => unstable_startWorker({ config, dev: { persist, server: { port: 0 }, inspector: false, testScheduled: true } });
async function call(w, f, args = []) {
  const r = await w.fetch("http://x/?node=t", { method: "POST", body: apply("tm", f, args) });
  const b = Buffer.from(await r.arrayBuffer());
  return { status: r.status, hex: b.toString("hex"), text: b.toString("latin1").replace(/[^\x20-\x7e]/g, "."), boot: r.headers.get("x-afterglow-boot"), alarms: r.headers.get("x-afterglow-alarm-runs") };
}
const out = {};
let w = await start(); await w.ready;
out.arm = await call(w, "arm", [int(MS)]);
out.ticker = await call(w, "ticker");
for (let i = 0; i < 2; i++) {
  const r = await w.fetch("http://x/__scheduled?cron=*+*+*+*+*"); // wrangler dev's test hook
  out["cron" + i] = r.status;
  await r.arrayBuffer();
}
out.before = await call(w, "result");
await w.dispose(); // the DO instance is gone; its storage (rows, alarm) stays
const t0 = Date.now();
w = await start(); await w.ready;
await sleep(MS + 5000); // no requests: only the alarm can wake the DO
out.after = await call(w, "result");
out.ticks = await call(w, "ticks");
out.waitedMs = Date.now() - t0;
await w.dispose();
console.log(JSON.stringify(out, null, 1));
const fired = /fired/.test(out.after.text), warm = out.after.boot === "warm" && Number(out.after.alarms) >= 1;
// {ticks, 2, [...]}: 68 03 77 05 "ticks" 61 02
const ticks = out.ticks.hex.startsWith("8368037705" + Buffer.from("ticks").toString("hex") + "6102");
console.log(fired && warm ? "PASS p4 timer after eviction (alarm woke the DO)" : "FAIL p4 timer");
console.log(ticks ? "PASS p4 cron ticks (2) survived the restart" : "FAIL p4 cron");
process.exit(fired && warm && ticks ? 0 : 1);
