// Requests per scriptVersion for a Worker over a time window, from Workers Analytics.
// Usage: node bench/versions.mjs <scriptName> <startISO> <endISO>
import fs from "node:fs";
import os from "node:os";
const [script, s, e] = process.argv.slice(2);
const token = fs.readFileSync(`${os.homedir()}/.config/glowc-phase0/cf-analytics-token`, "utf8").trim();
const query = `query($a:String!,$n:String!,$s:Time!,$e:Time!){viewer{accounts(filter:{accountTag:$a}){
  workersInvocationsAdaptive(limit:100,filter:{scriptName:$n,datetime_geq:$s,datetime_leq:$e}){
    dimensions{scriptVersion status} sum{requests} quantiles{cpuTimeP50}}}}}`;
const r = await fetch("https://api.cloudflare.com/client/v4/graphql", {
  method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
  body: JSON.stringify({ query, variables: { a: "00000000000000000000000000000000", n: script, s, e } }) });
const j = await r.json();
if (j.errors) throw new Error(JSON.stringify(j.errors));
for (const x of j.data.viewer.accounts[0].workersInvocationsAdaptive)
  console.log(x.dimensions.scriptVersion, x.dimensions.status, x.sum.requests, x.quantiles.cpuTimeP50 / 1000);
