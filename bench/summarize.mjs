// Summarize one run: tail events (cpuTime, wallTime) + oha latency.
// Usage: node bench/summarize.mjs bench/results/<name>-<label>
import fs from "node:fs";

const base = process.argv[2];
const text = fs.readFileSync(`${base}.tail.json`, "utf8");
const events = [];
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
  else if (c === "}") { depth--; if (!depth) events.push(JSON.parse(text.slice(start, i + 1))); }
}

const pct = (a, p) => a[Math.min(a.length - 1, Math.floor((p / 100) * a.length))];
const outcomes = {};
for (const e of events) outcomes[e.outcome] = (outcomes[e.outcome] ?? 0) + 1;
const ok = events.filter((e) => e.outcome === "ok");
const cpu = ok.map((e) => e.cpuTime).sort((a, b) => a - b);
const wall = ok.map((e) => e.wallTime).sort((a, b) => a - b);

const oha = JSON.parse(fs.readFileSync(`${base}.oha.json`, "utf8"));
const lat = oha.latencyPercentiles ?? {};
const summary = {
  run: base.split("/").pop(),
  tailEvents: events.length,
  outcomes,
  colos: [...new Set(events.map((e) => e.event?.request?.cf?.colo))],
  cpuMs: { p50: pct(cpu, 50), p90: pct(cpu, 90), p99: pct(cpu, 99), max: cpu.at(-1), mean: +(cpu.reduce((a, b) => a + b, 0) / cpu.length).toFixed(3) },
  workerWallMs: { p50: pct(wall, 50), p99: pct(wall, 99) },
  clientLatencyMs: { p50: +(lat.p50 * 1000).toFixed(1), p99: +(lat.p99 * 1000).toFixed(1) },
  rps: +oha.summary.requestsPerSec.toFixed(1),
  statusCodes: oha.statusCodeDistribution,
};
fs.writeFileSync(`${base}.summary.json`, JSON.stringify(summary, null, 2) + "\n");
const csv = ["ts,outcome,cpu_ms,wall_ms,colo", ...events.map((e) => [e.eventTimestamp, e.outcome, e.cpuTime, e.wallTime, e.event?.request?.cf?.colo].join(","))];
fs.writeFileSync(`${base}.csv`, csv.join("\n") + "\n");
console.log(JSON.stringify(summary));
