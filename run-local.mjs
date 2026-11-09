// Starts each Phase 0 Worker in local workerd via wrangler and prints what it returns.
import { unstable_startWorker } from "wrangler";
import { setTimeout as sleep } from "node:timers/promises";

async function withWorker(config, fn) {
  const worker = await unstable_startWorker({ config, dev: { persist: false, server: { port: 0 }, inspector: false } });
  try {
    await worker.ready;
    const get = async (path) => {
      const r = await worker.fetch(`http://example.com${path}`);
      const text = await r.text();
      try { return JSON.parse(text); } catch { return { status: r.status, text: text.slice(0, 300) }; }
    };
    await fn(get);
  } catch (e) {
    console.log(config, "FAILED:", e?.message ?? e);
  } finally {
    await worker.dispose();
  }
}

const show = (label, v) => console.log(label.padEnd(22), JSON.stringify(v));

await withWorker("checks/core/wrangler.jsonc", async (get) => {
  show("env", await get("/env"));
  show("tail 10M", await get("/tail?n=10000000"));
  show("plaincall 10M (ctrl)", await get("/plaincall?n=10000000"));
  show("plaincall 1k (ctrl)", await get("/plaincall?n=1000"));
  show("features", await get("/features"));
  show("i64", await get("/i64"));
  for (let i = 0; i < 5; i++) show(`bump #${i + 1}`, await get("/bump"));
});
for (const v of ["exnref", "legacy"]) {
  await withWorker(`checks/eh/${v}/wrangler.jsonc`, async (get) => show(`eh ${v}`, await get("/")));
}
await withWorker("checks/do/wrangler.jsonc", async (get) => {
  show("do sql #1", await get("/sql"));
  show("do sql #2", await get("/sql"));
  show("do rpc", await get("/rpc"));
  show("do alarm armed", await get("/alarm"));
  await sleep(2000);
  show("do alarms fired", await get("/alarms"));
});
