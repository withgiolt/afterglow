// Minimal probe: start one worker, hit a few paths, always exit.
import { unstable_startWorker } from "wrangler";

const [config, ...paths] = process.argv.slice(2);
const deadline = setTimeout(() => {
  console.error("probe: deadline hit");
  process.exit(2);
}, 60_000);

const t0 = Date.now();
const worker = await unstable_startWorker({
  config,
  dev: { persist: false, server: { port: 0 }, inspector: false },
});
await worker.ready;
console.error(`probe: ready after ${Date.now() - t0} ms`);
for (const p of paths) {
  const t = Date.now();
  if (p.startsWith("sleep:")) {
    await new Promise((r) => setTimeout(r, Number(p.slice(6))));
    continue;
  }
  try {
    const res = await worker.fetch(`http://x${p}`);
    console.log(p, res.status, Date.now() - t, "ms", await res.text());
  } catch (e) {
    console.log(p, "ERROR", Date.now() - t, "ms", String(e));
  }
}
await worker.dispose();
clearTimeout(deadline);
process.exit(0);
