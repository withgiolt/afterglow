// Runs a glowc-built module under Node (V8).
//   node tools/run.mjs app.wasm test            -> ETF of the result on stdout
//   node tools/run.mjs app.wasm request < req   -> framed output on stdout
// Exit 3 on an uncaught exception (text on stderr). GLOWC_TIME=n repeats main n times
// and reports timings on stderr.
import fs from 'node:fs';
import { randomFillSync } from 'node:crypto';

const [file, mode = 'test'] = process.argv.slice(2);
const bytes = fs.readFileSync(file);
let mem;
const dec = new TextDecoder();
const host = {
  log: (p, n) => process.stderr.write('[log] ' + dec.decode(new Uint8Array(mem.buffer, p, n)) + '\n'),
  random_bytes: (p, n) => randomFillSync(new Uint8Array(mem.buffer, p, n)),
  now_us: () => Date.now() * 1000,
};
const t0 = performance.now();
const { instance } = await WebAssembly.instantiate(bytes, { host });
const x = instance.exports;
mem = x.memory;
x.init();
if (process.env.GLOWC_DEBUG && x.rt_set_debug) x.rt_set_debug(1);
const t1 = performance.now();
const input = mode === 'request' ? fs.readFileSync(0) : new Uint8Array(0);
const unpack = (v) => {
  const p = Number(BigInt.asUintN(64, v) >> 32n), n = Number(BigInt.asUintN(64, v) & 0xffffffffn);
  return Buffer.from(new Uint8Array(mem.buffer, p, n));
};
const reps = Number(process.env.GLOWC_TIME || 1);
let res, times = [];
for (let i = 0; i < reps; i++) {
  const buf = x.rt_begin(input.length);
  new Uint8Array(mem.buffer, buf, input.length).set(input);
  const s = performance.now();
  res = x.main();
  times.push(performance.now() - s);
  if (res === 0n) break;
}
if (res === 0n) {
  process.stderr.write('exception: ' + unpack(x.rt_exception_text()).toString() + '\n');
  process.exit(3);
}
if (process.env.GLOWC_STATS) {
  process.stderr.write(`memory ${mem.buffer.byteLength / 1024} KiB, gcs ${x.rt_stats_gcs()}, heap words ${x.rt_stats_heap_words()}, arena ${(Number(x.rt_stats_arena_bytes()) / 1048576).toFixed(1)} MiB\n`);
}
if (process.env.GLOWC_TIME) {
  times.sort((a, b) => a - b);
  process.stderr.write(`init ${(t1 - t0).toFixed(1)} ms; main min ${times[0].toFixed(2)} median ${times[times.length >> 1].toFixed(2)} ms (${reps} runs)\n`);
}
if (mode === 'request') process.stdout.write(unpack(x.rt_output()));
else {
  const e = x.rt_result_etf(res);
  if (e === 0n) { process.stderr.write('result cannot be encoded\n'); process.exit(4); }
  process.stdout.write(unpack(e));
}
