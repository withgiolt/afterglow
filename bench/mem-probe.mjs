import { unstable_startWorker } from "wrangler";
const deadline = setTimeout(() => { console.error("deadline"); process.exit(2); }, 60_000);
const worker = await unstable_startWorker({ config: process.argv[2], dev: { persist: false, server: { port: 0 }, inspector: false } });
await worker.ready;
for (let i = 0; i < 5; i++) {
  const r = await worker.fetch("http://x/");
  const body = await r.text();
  console.log(r.status, r.headers.get("x-wasm-memory-bytes"), body.length);
}
await worker.dispose(); clearTimeout(deadline); process.exit(0);
