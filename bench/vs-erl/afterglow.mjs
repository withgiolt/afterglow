// The afterglow side of bench/vs-erl.sh: an afterglow-built app.wasm in Node,
// one instance serving N requests the way the Worker entry does.
// Usage: node bench/vs-erl/afterglow.mjs APP.wasm REQ OUT N
// Prints one JSON line and writes the framed response to OUT.
import fs from 'node:fs';
import { randomFillSync } from 'node:crypto';

const [file, reqFile, outFile, n] = process.argv.slice(2);
let mem;
const host = {
  log: () => {},
  random_bytes: (p, len) => randomFillSync(new Uint8Array(mem.buffer, p, len)),
  now_us: () => Date.now() * 1000,
};
const bytes = fs.readFileSync(file);
const req = fs.readFileSync(reqFile);

const t0 = performance.now();
const { instance } = await WebAssembly.instantiate(bytes, { host });
const x = instance.exports;
mem = x.memory;
x.init();
const initMs = performance.now() - t0;

const view = (packed) => {
  const v = BigInt.asUintN(64, packed);
  return new Uint8Array(mem.buffer, Number(v >> 32n), Number(v & 0xffffffffn));
};
const serve = () => {
  const buf = x.rt_begin(req.length);
  new Uint8Array(mem.buffer, buf, req.length).set(req);
  if (x.main() === 0n) throw new Error(Buffer.from(view(x.rt_exception_text())).toString());
};

const t1 = performance.now();
serve();
const firstMs = performance.now() - t1;
fs.writeFileSync(outFile, view(x.rt_output()));
for (let i = 0; i < n / 10; i++) serve();
const t2 = performance.now();
for (let i = 0; i < n; i++) serve();
const perUs = ((performance.now() - t2) / n) * 1000;

console.log(JSON.stringify({
  init_ms: +initMs.toFixed(3),
  first_request_ms: +firstMs.toFixed(3),
  per_request_us: +perUs.toFixed(3),
  memory_kib: mem.buffer.byteLength / 1024,
}));
