// P4 on the deployed DO Worker. "arm MS": arm the timer and register the cron ticker.
// "check": read the timer result and the cron ticks (after an idle wait with no requests).
// Usage: node tools/do/p4-deployed.mjs BASE_URL arm|check [MS]
import { int, apply } from "./etf.mjs";

const [base, step, msArg = "60000"] = process.argv.slice(2);
async function call(f, args = []) {
  const r = await fetch(`${base}/?node=t`, { method: "POST", body: apply("tm", f, args) });
  const b = Buffer.from(await r.arrayBuffer());
  const o = { f, status: r.status, hex: b.toString("hex"), text: b.toString("latin1").replace(/[^\x20-\x7e]/g, "."), boot: r.headers.get("x-afterglow-boot"), alarms: r.headers.get("x-afterglow-alarm-runs") };
  console.log(JSON.stringify(o));
  return o;
}
if (step === "arm") { await call("arm", [int(Number(msArg))]); await call("ticker"); }
else { const a = await call("result"); await call("ticks"); console.log(/fired/.test(a.text) ? "PASS p4 deployed: timer fired" : "FAIL p4 deployed: timer"); }
