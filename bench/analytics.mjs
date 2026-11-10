// Full-population CPU quantiles for one measured run, from Workers Analytics
// (GraphQL workersInvocationsAdaptive). Only the measured window is queried, so
// warm-up and other runs are excluded. New scripts show as "__unknown__" in
// analytics, so the Worker is selected by its scriptVersion id.
// Usage: node bench/analytics.mjs bench/results/<name>-<label> <scriptVersion>
// Token: ~/.config/afterglow-phase0/cf-analytics-token (Account Analytics: Read).
import fs from "node:fs";
import os from "node:os";
if (!process.env.CLOUDFLARE_ACCOUNT_ID) throw new Error("set CLOUDFLARE_ACCOUNT_ID");

const [base, version] = process.argv.slice(2);
const token = fs.readFileSync(`${os.homedir()}/.config/afterglow-phase0/cf-analytics-token`, "utf8").trim();
const oha = JSON.parse(fs.readFileSync(`${base}.oha.json`, "utf8"));
const endMs = fs.statSync(`${base}.oha.json`).mtimeMs;
const startMs = endMs - oha.summary.total * 1000;
// Shrink to whole seconds inside the run so no warm-up request is counted.
const start = new Date(Math.ceil(startMs / 1000) * 1000).toISOString();
const end = new Date(Math.floor(endMs / 1000) * 1000).toISOString();

const query = `query($a:String!,$v:String!,$s:Time!,$e:Time!){viewer{accounts(filter:{accountTag:$a}){
  workersInvocationsAdaptive(limit:10,filter:{scriptVersion:$v,datetime_geq:$s,datetime_leq:$e}){
    dimensions{status} sum{requests errors subrequests}
    quantiles{cpuTimeP25 cpuTimeP50 cpuTimeP75 cpuTimeP90 cpuTimeP99 cpuTimeP999 wallTimeP50 wallTimeP99}}}}}`;
const res = await fetch("https://api.cloudflare.com/client/v4/graphql", {
  method: "POST",
  headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
  body: JSON.stringify({ query, variables: { a: process.env.CLOUDFLARE_ACCOUNT_ID, v: version, s: start, e: end } }),
});
const j = await res.json();
if (j.errors) throw new Error(JSON.stringify(j.errors));
const rows = j.data.viewer.accounts[0].workersInvocationsAdaptive;
const us2ms = (x) => +(x / 1000).toFixed(3);
const out = {
  run: base.split("/").pop(),
  scriptVersion: version,
  window: { start, end },
  byStatus: rows.map((r) => ({
    status: r.dimensions.status,
    requests: r.sum.requests,
    errors: r.sum.errors,
    cpuMs: Object.fromEntries(Object.entries(r.quantiles).filter(([k]) => k.startsWith("cpu")).map(([k, v]) => [k.replace("cpuTime", "").toLowerCase(), us2ms(v)])),
    wallMs: { p50: us2ms(r.quantiles.wallTimeP50), p99: us2ms(r.quantiles.wallTimeP99) },
  })),
};
fs.writeFileSync(`${base}.analytics.json`, JSON.stringify(out, null, 2) + "\n");
console.log(JSON.stringify(out));
