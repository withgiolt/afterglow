// The Node side of bench/compute.sh: times one workload from compute.gleam on afterglow
// or on Gleam's JavaScript target, both on Node's V8, the engine Workers run. workerd
// doesn't advance its clocks during pure compute, so the timing can't happen there.
// Usage:
//   node bench/compute/harness.mjs afterglow APP.wasm WORKLOAD WARMUP ITERS
//   node bench/compute/harness.mjs js COMPUTE.mjs WORKLOAD WARMUP ITERS
// APP.wasm is built with compute:WORKLOAD/0 as its entry. Prints one JSON line in the
// shape bench_compute_erl.erl prints. Instantiation is not timed, only the workload.
import fs from "node:fs";
import { randomFillSync } from "node:crypto";
import { pathToFileURL } from "node:url";

const [target, file, workload, warmup, iters] = process.argv.slice(2);
const run = target === "afterglow" ? await afterglowRunner(file) : await jsRunner(file, workload);

for (let i = 0; i < Number(warmup); i++) run();
const ms = [];
let result;
for (let i = 0; i < Number(iters); i++) {
  const timed = run();
  ms.push(timed.ms);
  result = timed.result;
}
ms.sort((a, b) => a - b);
// The same definitions as bench_compute_erl.erl.
const median = ms[ms.length >> 1];
const p95 = ms[Math.ceil(ms.length * 0.95) - 1];
console.log(JSON.stringify({ result: String(result()), median_ms: +median.toFixed(4), p95_ms: +p95.toFixed(4) }));

async function jsRunner(path, name) {
  const fn = (await import(pathToFileURL(path).href))[name];
  if (typeof fn !== "function") throw new Error(`${path} exports no ${name}`);
  return () => {
    const start = performance.now();
    const value = fn();
    return { ms: performance.now() - start, result: () => value };
  };
}

async function afterglowRunner(path) {
  let mem;
  const host = {
    log: () => {},
    random_bytes: (p, n) => randomFillSync(new Uint8Array(mem.buffer, p, n)),
    now_us: () => Date.now() * 1000,
  };
  const { instance } = await WebAssembly.instantiate(fs.readFileSync(path), { host });
  const x = instance.exports;
  mem = x.memory;
  x.init();
  const view = (packed) => {
    const v = BigInt.asUintN(64, packed);
    return new Uint8Array(mem.buffer, Number(v >> 32n), Number(v & 0xffffffffn));
  };
  return () => {
    // rt_begin resets the heap from the previous run; only main() is the workload.
    x.rt_begin(0);
    const start = performance.now();
    const term = x.main();
    const elapsed = performance.now() - start;
    if (term === 0n) throw new Error(Buffer.from(view(x.rt_exception_text())).toString());
    // Read before the next rt_begin wipes the heap the term lives on.
    const value = decodeInteger(view(x.rt_result_etf(term)));
    return { ms: elapsed, result: () => value };
  };
}

// Every workload returns an Int: SMALL_INTEGER_EXT, INTEGER_EXT or SMALL_BIG_EXT.
function decodeInteger(etf) {
  const data = new DataView(etf.buffer, etf.byteOffset, etf.byteLength);
  if (etf[0] !== 131) throw new Error("not ETF");
  switch (etf[1]) {
    case 97:
      return BigInt(etf[2]);
    case 98:
      return BigInt(data.getInt32(2));
    case 110: {
      const length = etf[2];
      let n = 0n;
      for (let i = length - 1; i >= 0; i--) n = (n << 8n) | BigInt(etf[4 + i]);
      return etf[3] ? -n : n;
    }
    default:
      throw new Error(`workload returned ETF tag ${etf[1]}, not an Int`);
  }
}
