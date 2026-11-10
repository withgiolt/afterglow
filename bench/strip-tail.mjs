// Strip client IP and geo data from `wrangler tail --format json` output, in place.
// Keeps what bench/summarize.mjs reads (outcome, cpuTime, wallTime, eventTimestamp, cf.colo).
// Usage: node bench/strip-tail.mjs <file.tail.json>...
import fs from "node:fs";

const DROP_HEADERS = ["cf-connecting-ip", "x-real-ip", "x-forwarded-for", "cf-ipcountry", "true-client-ip"];
const KEEP_CF = ["colo", "httpProtocol"];

function events(text) {
  const out = [];
  let depth = 0, start = -1, inStr = false, esc = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inStr) {
      if (esc) esc = false;
      else if (c === "\\") esc = true;
      else if (c === '"') inStr = false;
      continue;
    }
    if (c === '"') inStr = true;
    else if (c === "{") { if (!depth) start = i; depth++; }
    else if (c === "}") { depth--; if (!depth) out.push(JSON.parse(text.slice(start, i + 1))); }
  }
  return out;
}

for (const file of process.argv.slice(2)) {
  const evs = events(fs.readFileSync(file, "utf8"));
  for (const e of evs) {
    const req = e.event?.request;
    if (!req) continue;
    for (const h of DROP_HEADERS) delete req.headers?.[h];
    if (req.cf) req.cf = Object.fromEntries(KEEP_CF.filter((k) => k in req.cf).map((k) => [k, req.cf[k]]));
  }
  fs.writeFileSync(file, evs.map((e) => JSON.stringify(e, null, 4)).join("\n") + "\n");
  console.log(`${file}: ${evs.length} events`);
}
