// CPU of the first request after each cold-start deploy, from Workers Analytics.
// Each deploy in <name>-coldstart.csv is its own scriptVersion with exactly one request.
// Usage: node bench/coldcpu.mjs bench/results/<name>-coldstart.csv
import fs from "node:fs";
import os from "node:os";
const [csv] = process.argv.slice(2);
const token = fs.readFileSync(`${os.homedir()}/.config/glowc-phase0/cf-analytics-token`, "utf8").trim();
const rows = fs.readFileSync(csv, "utf8").trim().split("\n").slice(1).map((l) => l.split(","));
const start = new Date(fs.statSync(csv).mtimeMs - 20 * 3600e3).toISOString();
const end = new Date().toISOString();
const query = `query($a:String!,$v:String!,$s:Time!,$e:Time!){viewer{accounts(filter:{accountTag:$a}){
  workersInvocationsAdaptive(limit:10,filter:{scriptVersion:$v,datetime_geq:$s,datetime_leq:$e}){
    sum{requests} quantiles{cpuTimeP50 wallTimeP50}}}}}`;
const out = [];
for (const [sample, version, startup, server, total, status] of rows) {
  const res = await fetch("https://api.cloudflare.com/client/v4/graphql", {
    method: "POST",
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: JSON.stringify({ query, variables: { a: "00000000000000000000000000000000", v: version, s: start, e: end } }),
  });
  const j = await res.json();
  if (j.errors) throw new Error(JSON.stringify(j.errors));
  const r = j.data.viewer.accounts[0].workersInvocationsAdaptive[0];
  out.push({ sample: +sample, version, startupMs: +startup, serverMs: +server, status: +status,
    requests: r?.sum.requests ?? 0,
    // A version with more than a handful of requests also served the benchmark (the last
    // deploy is the measured one), so its p50 is not the first request's CPU: excluded.
    cpuMs: r && r.sum.requests <= 3 ? r.quantiles.cpuTimeP50 / 1000 : null,
    wallMs: r && r.sum.requests <= 3 ? r.quantiles.wallTimeP50 / 1000 : null });
}
const med = (a) => { const s = a.filter((x) => x != null).sort((x, y) => x - y); return s.length ? (s[(s.length - 1) >> 1] + s[s.length >> 1]) / 2 : null; };
const summary = { samples: out, median: { startupMs: med(out.map((o) => o.startupMs)), serverMs: med(out.map((o) => o.serverMs)), cpuMs: med(out.map((o) => o.cpuMs)) } };
fs.writeFileSync(csv.replace(/\.csv$/, ".cpu.json"), JSON.stringify(summary, null, 2));
console.log(JSON.stringify(summary.median), out.map((o) => o.cpuMs));
