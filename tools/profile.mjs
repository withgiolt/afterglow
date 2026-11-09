// profile.mjs WASM REQ.etf [runs]: self-time profile of main() under Node, by function.
import { Session } from 'node:inspector/promises';
import fs from 'node:fs';
const [file, reqf, runs = '300'] = process.argv.slice(2);
let mem;
const { instance } = await WebAssembly.instantiate(fs.readFileSync(file), { host: {
  log: (p, n) => console.error(new TextDecoder().decode(new Uint8Array(mem.buffer, p, n))),
  random_bytes: () => {}, now_us: () => Date.now() * 1000 } });
const x = instance.exports; mem = x.memory; x.init();
const input = fs.readFileSync(reqf);
const run = () => { const b = x.rt_begin(input.length); new Uint8Array(mem.buffer, b, input.length).set(input); if (x.main() === 0n) throw new Error('exception'); };
for (let i = 0; i < 50; i++) run();
const s = new Session(); s.connect();
await s.post('Profiler.enable'); await s.post('Profiler.setSamplingInterval', { interval: 50 });
await s.post('Profiler.start');
for (let i = 0; i < Number(runs); i++) run();
const { profile: p } = await s.post('Profiler.stop');
const byId = new Map(p.nodes.map((n) => [n.id, n])); const self = {};
p.samples.forEach((id, i) => { const n = byId.get(id).callFrame.functionName || '(anon)'; self[n] = (self[n] || 0) + (p.timeDeltas[i] || 0); });
const tot = Object.values(self).reduce((a, b) => a + b, 0);
for (const [k, t] of Object.entries(self).sort((a, b) => b[1] - a[1]).slice(0, Number(process.env.TOP || 30))) console.log((100 * t / tot).toFixed(1).padStart(5) + '%  ' + k);
