// P4 on the deployed DO Worker. "arm MS": arm the timer and register the cron ticker.
// "check": read the timer result and the cron ticks (after an idle wait with no requests).
// Usage: node tools/do/p4-deployed.mjs BASE_URL arm|check [MS]
const [base, step, msArg = "60000"] = process.argv.slice(2);
const enc = new TextEncoder();
const atom = (s) => { const b = enc.encode(s); return [119, b.length, ...b]; };
const int = (n) => { const b = Buffer.alloc(5); b[0] = 98; b.writeInt32BE(n, 1); return [...b]; };
const list = (xs) => xs.length ? [108, 0, 0, 0, xs.length, ...xs.flat(), 106] : [106];
const apply = (f, args) => Uint8Array.from([131, 104, 4, ...atom("apply"), ...atom("tm"), ...atom(f), ...list(args)]);
async function call(f, args = []) {
  const r = await fetch(`${base}/?node=t`, { method: "POST", body: apply(f, args) });
  const b = Buffer.from(await r.arrayBuffer());
  const o = { f, status: r.status, hex: b.toString("hex"), text: b.toString("latin1").replace(/[^\x20-\x7e]/g, "."), boot: r.headers.get("x-afterglow-boot"), alarms: r.headers.get("x-afterglow-alarm-runs") };
  console.log(JSON.stringify(o));
  return o;
}
if (step === "arm") { await call("arm", [int(Number(msArg))]); await call("ticker"); }
else { const a = await call("result"); await call("ticks"); console.log(/fired/.test(a.text) ? "PASS p4 deployed: timer fired" : "FAIL p4 deployed: timer"); }
