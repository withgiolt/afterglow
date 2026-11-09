// P5: a chat room process with two WebSocket clients. Between messages the DO instance
// is dropped while the sockets stay open (locally with "__evict"; deployed by waiting
// for hibernation), and the room's state must come back from the checkpoint.
// Usage: node tools/do/p5-test.mjs [CONFIG] [WS_BASE] [IDLE_MS]
import { unstable_startWorker } from "wrangler";

const [config = "out/do-p5/wrangler.jsonc", base, idleArg] = process.argv.slice(2);
const IDLE = Number(idleArg || 0);
let w = null, wsBase = base;
if (!wsBase) {
  w = await unstable_startWorker({ config, dev: { persist: false, server: { port: 0 }, inspector: false } });
  await w.ready;
  wsBase = (await w.url).toString().replace(/^http/, "ws").replace(/\/$/, "");
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
function client(name) {
  const ws = new WebSocket(`${wsBase}/?node=room1&ws=chat&mod=chat`);
  const got = [];
  ws.addEventListener("message", (e) => got.push(String(e.data)));
  return new Promise((res, rej) => { ws.addEventListener("open", () => res({ ws, got, name })); ws.addEventListener("error", rej); });
}
const c1 = await client("c1");
await sleep(300);
const c2 = await client("c2");
await sleep(300);
c1.ws.send("hi");
await sleep(500);
if (IDLE) await sleep(IDLE); else { c1.ws.send("__evict"); await sleep(200); }
c2.ws.send("yo");
await sleep(1000);
c1.ws.close(); c2.ws.close();
await sleep(200);
console.log(JSON.stringify({ c1: c1.got, c2: c2.got }, null, 1));
if (w) await w.dispose();
const want1 = "1: hi (members 2, messages 1)", want2 = "2: yo (members 2, messages 2)";
const ok = c1.got.includes(want1) && c2.got.includes(want1) && c1.got.includes(want2) && c2.got.includes(want2)
  && c2.got.some((m) => m.startsWith("__boot") && /"restored":[1-9]/.test(m));
console.log(ok ? "PASS p5 chat room state survived dropping the DO instance" : "FAIL p5");
process.exit(ok ? 0 : 1);
