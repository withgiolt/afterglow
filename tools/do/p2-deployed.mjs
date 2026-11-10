// P2 on the deployed DO Worker: send counter messages, print {vN, Int} and the boot header.
// Usage: node tools/do/p2-deployed.mjs BASE_URL MSG...
const [base, ...msgs] = process.argv.slice(2);
const call = Uint8Array.from([131, 104, 4, 119, 4, ...Buffer.from("call"), 119, 7, ...Buffer.from("counter"), 119, 7, ...Buffer.from("counter")]);
const msg = (m) => { const b = Buffer.from(m); return Buffer.concat([call, Buffer.from([119, b.length]), b]); };
const show = (b) => `{${b.subarray(5, 7).toString()},${b[7] === 97 ? b[8] : b.readInt32BE(8)}}`;
for (const m of msgs) {
  const r = await fetch(`${base}/?node=counter`, { method: "POST", body: msg(m) });
  const b = Buffer.from(await r.arrayBuffer());
  console.log(m, r.status === 200 ? show(b) : `HTTP ${r.status}: ${b.toString().slice(0, 200)}`,
    "boot", r.headers.get("x-afterglow-boot"), "rows", r.headers.get("x-afterglow-rows"), "bytes", r.headers.get("x-afterglow-row-bytes"),
    "turn-ms", r.headers.get("x-afterglow-turn-ms"));
}
