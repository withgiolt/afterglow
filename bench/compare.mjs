// Writes compare.html: glowvm vs glowc, from the result files in bench/results.
// Missing results show as "pending", so it can be re-run as runs land.
// Usage: node bench/compare.mjs
import fs from "node:fs";

const R = "bench/results";
const read = (f) => (fs.existsSync(f) ? JSON.parse(fs.readFileSync(f, "utf8")) : null);
const median = (a) => {
  const s = a.filter((x) => x != null).sort((x, y) => x - y);
  return s.length ? (s[(s.length - 1) >> 1] + s[s.length >> 1]) / 2 : null;
};

// Deployed CPU: median over the 3 runs of each run's p50 and p99 (benchmark plan, step 4).
function deployed(prefix) {
  const runs = [1, 2, 3].map((i) => read(`${R}/${prefix}-r${i}.analytics.json`)).filter(Boolean);
  if (!runs.length) return null;
  const ok = (r) => r.byStatus.find((s) => s.status === "success");
  const others = runs.flatMap((r) => r.byStatus.filter((s) => s.status !== "success"));
  return {
    runs: runs.length,
    p50: median(runs.map((r) => ok(r)?.cpuMs.p50)),
    p99: median(runs.map((r) => ok(r)?.cpuMs.p99)),
    perRun: runs.map((r) => ({ p50: ok(r)?.cpuMs.p50, p99: ok(r)?.cpuMs.p99, n: ok(r)?.requests })),
    requests: runs.reduce((n, r) => n + r.byStatus.reduce((m, s) => m + s.requests, 0), 0),
    failures: others.reduce((n, s) => n + s.requests, 0),
    failureKinds: [...new Set(others.map((s) => s.status))],
  };
}
// Cold start: the side-by-side 10-deploy series (2026-11-09 23:35 UTC, 60 s wait, versions
// checked in analytics), not the Phase 0 5-deploy series.
const cold = (prefix) => read(`${R}/${prefix}-cold10.cpu.json`)?.median ?? null;
const local = (rt, f) => {
  const j = read(`${R}/local/local-${rt}-${f}.json`);
  if (!j) return null;
  return { p50: j.latencyPercentiles.p50 * 1000, p99: j.latencyPercentiles.p99 * 1000, n: Object.values(j.statusCodeDistribution).reduce((a, b) => a + b, 0) };
};

const F = [
  { id: "basic_get", name: "basic_get", vm: "glowvm-baseline-basic-get", gc: "glowc-m5-basic-get", vm2: "glowvm-m5-basic-get", factor: 3 },
  { id: "lustre_html", name: "lustre_html", vm: "glowvm-baseline-lustre-html", gc: "glowc-m5-lustre-html", vm2: "glowvm-m5-lustre-html", factor: 3 },
  { id: "json_roundtrip", name: "json_roundtrip", vm: null, gc: "glowc-m5-json-roundtrip", vm2: null, factor: 5 },
];
// Fixed facts measured outside these files (PROGRESS.md, M5).
const size = { basic_get: [298.73, 180.97], lustre_html: [316.5, 187.5], json_roundtrip: [308.37, 205.73] };
const memory = { basic_get: [576, 512], lustre_html: [640, 512], json_roundtrip: [1280, 4992] };

const fmt = (x, d = 2) => (x == null ? "pending" : x.toFixed(d));
const ratio = (a, b) => (a == null || b == null ? null : a / b);
const cls = (ok) => (ok == null ? "pend" : ok ? "pass" : "fail");
const word = (ok) => (ok == null ? "pending" : ok ? "pass" : "fail");

const rows = F.map((f) => {
  const vm = f.vm ? deployed(f.vm) : null, gc = deployed(f.gc), vm2 = f.vm2 ? deployed(f.vm2) : null;
  return { ...f, vm, gc, vm2, lvm: local("vm", f.id), lgc: local("gc", f.id), cvm: f.vm ? cold(f.vm) : null, cgc: cold(f.gc) };
});

// Verdict per row of SPIKE.md's first table.
const cpuFixed = rows.slice(0, 2).map((r) => {
  const t50 = r.vm && r.vm.p50 / 3, t99 = r.vm && r.vm.p99 / 3;
  const ok = r.gc && r.vm ? r.gc.p50 <= t50 && r.gc.p99 <= t99 : null;
  return { r, t50, t99, ok };
});
const j = rows[2];
const heavyOk = j.lgc && j.lvm ? j.lgc.p50 * 5 <= j.lvm.p50 && j.lgc.p99 * 5 <= j.lvm.p99 : null;
const coldOk = rows.slice(0, 2).every((r) => r.cgc) ? rows.slice(0, 2).every((r) => r.cgc.startupMs <= r.cvm.startupMs && r.cgc.serverMs <= r.cvm.serverMs && r.cgc.cpuMs <= r.cvm.cpuMs) : null;
const sizeOk = Object.values(size).every(([v, g]) => g <= v);
const memOk = ["basic_get", "lustre_html"].every((k) => memory[k][1] < memory[k][0]);
const correctness = rows.slice(0, 2).map((r) => {
  const f = `${R}/m5-${r.id.replace("_", "-")}.log`;
  if (!fs.existsSync(f)) return null;
  const l = fs.readFileSync(f, "utf8");
  return /deployed correctness: identical/.test(l) ? true : /deployed correctness: DIFFERS/.test(l) ? false : null;
});
const corrOk = correctness.every((c) => c === true) ? true : correctness.some((c) => c === false) ? false : null;

const verdictRows = [
  ["CPU, fixed overhead (basic_get, lustre_html)", "≥ 3× lower p50 and p99 (deployed)",
    cpuFixed.map(({ r, t50, t99 }) => `${r.name}: p50 ${fmt(r.gc?.p50)} ≤ ${fmt(t50)}, p99 ${fmt(r.gc?.p99)} ≤ ${fmt(t99)} ms`).join("<br>"),
    cpuFixed.every((c) => c.ok === true) ? true : cpuFixed.some((c) => c.ok === false) ? false : null],
  ["CPU, heavy handler (json_roundtrip)", "≥ 5× lower p50 and p99 (local workerd: glowvm exceeds Free's CPU limit deployed)",
    `p50 ${fmt(j.lvm?.p50)} → ${fmt(j.lgc?.p50)} ms (${fmt(ratio(j.lvm?.p50, j.lgc?.p50), 0)}×), p99 ${fmt(j.lvm?.p99)} → ${fmt(j.lgc?.p99)} ms (${fmt(ratio(j.lvm?.p99, j.lgc?.p99), 0)}×)`, heavyOk],
  ["Isolate startup", "no worse (startup, first request after deploy; glowc and glowvm side by side, 10 deploys each)",
    rows.slice(0, 2).map((r) => `${r.name}: startup ${fmt(r.cvm?.startupMs, 0)} → ${fmt(r.cgc?.startupMs, 0)} ms, server ${fmt(r.cvm?.serverMs, 1)} → ${fmt(r.cgc?.serverMs, 1)} ms, CPU ${fmt(r.cvm?.cpuMs)} → ${fmt(r.cgc?.cpuMs)} ms`).join("<br>"), coldOk],
  ["Bundle size", "≤ glowvm (gzip upload)", Object.entries(size).map(([k, [v, g]]) => `${k}: ${v} → ${g} KiB`).join("<br>"), sizeOk],
  ["Peak memory", "lower (basic_get, lustre_html)", Object.entries(memory).map(([k, [v, g]]) => `${k}: ${v} → ${g} KiB`).join("<br>") + "<br><span class=dim>json_roundtrip is higher on glowc; not a fixture of this row, flagged below</span>", memOk],
  ["Correctness", "byte-identical status, headers, body", `30/30 smoke requests identical on local workerd; deployed check: ${correctness.map((c, i) => `${rows[i].name} ${word(c)}`).join(", ")}`, corrOk],
];
const all = verdictRows.map((v) => v[3]);
const overall = all.every((x) => x === true) ? "pass" : all.some((x) => x === false) ? "fail" : "pending";

// Bars: two series per group, linear or log scale.
function bars(groups, unit, { log = false } = {}) {
  const W = 760, rowH = 22, gap = 18, labelW = 130, valW = 90;
  const vals = groups.flatMap((g) => g.items.map((i) => i.v)).filter((v) => v != null && v > 0);
  const max = Math.max(...vals), min = Math.min(...vals);
  const scale = (v) => {
    if (v == null || v <= 0) return 0;
    if (!log) return (v / max) * (W - labelW - valW);
    const lo = Math.log10(min) - 0.3, hi = Math.log10(max);
    return ((Math.log10(v) - lo) / (hi - lo)) * (W - labelW - valW);
  };
  let y = 0, out = "";
  for (const g of groups) {
    out += `<text x="0" y="${y + 15}">${g.label}</text>`;
    for (const it of g.items) {
      const w = Math.max(scale(it.v), it.v == null ? 0 : 2);
      out += `<rect x="${labelW}" y="${y}" width="${w.toFixed(1)}" height="${rowH - 6}" rx="3" fill="var(--${it.c})"/>`;
      out += `<text class="val" x="${labelW + w + 6}" y="${y + 12}">${it.v == null ? "pending" : it.v.toFixed(it.v < 10 ? 2 : it.v < 100 ? 1 : 0) + " " + unit}</text>`;
      y += rowH;
    }
    y += gap;
  }
  return `<svg viewBox="0 0 ${W} ${y}" width="100%" role="img">${out}</svg>`;
}
const pair = (label, a, b) => ({ label, items: [{ v: a, c: "a" }, { v: b, c: "b" }] });

const cpuChart = bars(rows.slice(0, 2).flatMap((r) => [pair(`${r.name} p50`, r.vm?.p50, r.gc?.p50), pair(`${r.name} p99`, r.vm?.p99, r.gc?.p99)]), "ms");
const localChart = bars(rows.flatMap((r) => [pair(`${r.name} p50`, r.lvm?.p50, r.lgc?.p50)]), "ms", { log: true });
const sizeChart = bars(Object.entries(size).map(([k, [v, g]]) => pair(k, v, g)), "KiB");
const memChart = bars(Object.entries(memory).map(([k, [v, g]]) => pair(k, v, g)), "KiB");

const runTable = rows.map((r) => {
  const cell = (d) => (d ? d.perRun.map((p) => `${fmt(p.p50)} / ${fmt(p.p99)}`).join("<br>") : "pending");
  return `<tr><td>${r.name}</td><td>${r.vm ? cell(r.vm) : r.id === "json_roundtrip" ? "exceeds CPU limit (see below)" : "pending"}</td><td>${cell(r.gc)}</td>
    <td class=num>${r.gc ? r.gc.requests : "–"}</td><td class=num>${r.gc ? r.gc.failures + (r.gc.failureKinds.length ? " (" + r.gc.failureKinds.join(", ") + ")" : "") : "–"}</td></tr>`;
}).join("");

const sideBySide = rows.slice(0, 2).filter((r) => r.vm2).map((r) =>
  `<tr><td>${r.name}</td><td class=num>${fmt(r.vm.p50)} / ${fmt(r.vm.p99)}</td><td class=num>${fmt(r.vm2.p50)} / ${fmt(r.vm2.p99)}</td><td class=num>${fmt(r.gc?.p50)} / ${fmt(r.gc?.p99)}</td></tr>`).join("");

const kpi = (v, l) => `<div class=kpi><div class=v>${v}</div><div class=l>${l}</div></div>`;
const bg = rows[0], lh = rows[1];
const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>glowvm vs glowc · M5 benchmark gate</title>
<style>
  :root { --bg:#111214; --surface:#1a1b1e; --border:#2a2c30; --text:#e8e6e3; --dim:#a09d98; --faint:#6d6a66;
    --a:#7aa2f7; --b:#d97757; --ok:#7fbf7f; --warn:#e0b04f; --bad:#e06c6c; }
  * { box-sizing: border-box; }
  body { margin:0; background:var(--bg); color:var(--text); font:15px/1.55 -apple-system,BlinkMacSystemFont,"Inter","Segoe UI",sans-serif; }
  main { max-width:860px; margin:0 auto; padding:48px 24px 96px; }
  h1 { font-size:28px; margin:0 0 4px; letter-spacing:-0.01em; }
  h2 { font-size:19px; margin:48px 0 12px; }
  .sub { color:var(--dim); margin:0 0 28px; }
  .card { background:var(--surface); border:1px solid var(--border); border-radius:10px; padding:20px 22px; margin:16px 0; }
  .note, .dim { color:var(--faint); font-size:13px; }
  .kpis { display:grid; grid-template-columns:repeat(4,1fr); gap:12px; }
  .kpi { background:var(--surface); border:1px solid var(--border); border-radius:10px; padding:14px 16px; }
  .kpi .v { font-size:24px; font-weight:600; font-variant-numeric:tabular-nums; }
  .kpi .l { color:var(--dim); font-size:13px; }
  @media (max-width:640px) { .kpis { grid-template-columns:repeat(2,1fr); } }
  table { width:100%; border-collapse:collapse; font-variant-numeric:tabular-nums; }
  th, td { text-align:left; padding:8px 10px; border-bottom:1px solid var(--border); vertical-align:top; }
  th { color:var(--dim); font-weight:500; font-size:13px; }
  td.num, th.num { text-align:right; }
  .pass { color:var(--ok); font-weight:600; } .fail { color:var(--bad); font-weight:600; } .pend { color:var(--warn); font-weight:600; }
  .legend { display:flex; gap:16px; font-size:13px; color:var(--dim); margin:6px 0 10px; }
  .legend i { display:inline-block; width:10px; height:10px; border-radius:2px; margin-right:6px; vertical-align:-1px; }
  .callout { border-left:3px solid var(--warn); background:#e0b04f14; padding:14px 18px; border-radius:6px; margin:16px 0; }
  code { font-family:ui-monospace,"SF Mono",Menlo,monospace; font-size:13px; background:#ffffff0d; padding:1px 5px; border-radius:4px; }
  svg text { fill:var(--dim); font-size:12px; font-family:inherit; }
  svg .val { fill:var(--text); font-variant-numeric:tabular-nums; }
  ul { padding-left:20px; } li { margin:4px 0; }
  footer { color:var(--faint); font-size:13px; margin-top:56px; }
</style>
</head>
<body>
<main>
  <h1>glowvm vs glowc</h1>
  <p class="sub">M5 benchmark gate · Cloudflare Workers Free, Giolt account · glowvm commit <code>7190ebc</code> (AtomVM interpreter, fresh VM per request) vs glowc (BEAM compiled ahead of time to Wasm, one instance per isolate) · generated ${new Date().toISOString().slice(0, 16).replace("T", " ")} UTC</p>

  <div class="kpis">
    ${kpi(`<span class=${overall === "pass" ? "pass" : overall === "fail" ? "fail" : "pend"}>${overall}</span>`, "M5 verdict")}
    ${kpi(bg.gc && bg.vm ? fmt(bg.vm.p50 / bg.gc.p50, 1) + "×" : "pending", "basic_get CPU p50, deployed")}
    ${kpi(lh.gc && lh.vm ? fmt(lh.vm.p50 / lh.gc.p50, 1) + "×" : "pending", "lustre_html CPU p50, deployed")}
    ${kpi(j.lgc && j.lvm ? fmt(j.lvm.p50 / j.lgc.p50, 0) + "×" : "pending", "json_roundtrip p50, local workerd")}
  </div>

  <h2>Verdict against SPIKE.md's success table</h2>
  <div class="card"><table>
    <tr><th>Row</th><th>Target</th><th>glowvm → glowc</th><th>Result</th></tr>
    ${verdictRows.map(([a, b, c, ok]) => `<tr><td>${a}</td><td class=dim>${b}</td><td>${c}</td><td class=${cls(ok)}>${word(ok)}</td></tr>`).join("")}
  </table>
  <p class="note">Tier 2 rows (persistence, messaging) belong to Phase 2 and are not part of the M5 gate.</p></div>

  <h2>CPU per request, deployed</h2>
  <div class="legend"><span><i style="background:var(--a)"></i>glowvm</span><span><i style="background:var(--b)"></i>glowc</span></div>
  <div class="card">${cpuChart}
    <p class="note">Workers Analytics CPU time over every measured request (not sampled): 1,000 warm-up, then 5,000 at concurrency 10 and 25 req/s, 3 runs; the median of the runs' p50 and p99. glowvm numbers are the Phase 0 baselines (2026-11-09), same method.</p></div>
  <div class="card"><table>
    <tr><th>Fixture</th><th>glowvm p50 / p99 per run (ms)</th><th>glowc p50 / p99 per run (ms)</th><th class=num>glowc requests</th><th class=num>glowc non-success</th></tr>
    ${runTable}
  </table></div>
  ${sideBySide ? `<div class="card"><h3>Side by side on the same day</h3><table><tr><th>Fixture</th><th class=num>glowvm Phase 0</th><th class=num>glowvm redeployed</th><th class=num>glowc</th></tr>${sideBySide}</table><p class="note">One run of glowvm redeployed next to glowc, to check that the Phase 0 baseline still holds.</p></div>` : ""}

  <h2>Local workerd, same machine</h2>
  <div class="legend"><span><i style="background:var(--a)"></i>glowvm</span><span><i style="background:var(--b)"></i>glowc</span><span>log scale</span></div>
  <div class="card">${localChart}
    <p class="note">One workerd, oha at concurrency 1, wall time per request (pure compute, so close to CPU). n = ${rows.map((r) => `${r.name} ${r.lvm?.n ?? "?"} / ${r.lgc?.n ?? "?"}`).join(", ")}. glowvm takes about 2.4 s CPU for json_roundtrip, over the Free plan's 10 ms limit, so the heavy row is compared here.</p></div>

  <h2>Cold start</h2>
  <div class="card"><table>
    <tr><th>Fixture</th><th class=num>startup (wrangler)</th><th class=num>first request, server time</th><th class=num>first request CPU</th></tr>
    ${rows.map((r) => `<tr><td>${r.name}</td><td class=num>${r.cvm ? r.cvm.startupMs : "–"} → ${r.cgc ? r.cgc.startupMs : "pending"} ms</td><td class=num>${fmt(r.cvm?.serverMs, 1)} → ${fmt(r.cgc?.serverMs, 1)} ms</td><td class=num>${fmt(r.cvm?.cpuMs)} → ${fmt(r.cgc?.cpuMs)} ms</td></tr>`).join("")}
  </table><p class="note">Five deploys each, the first request after each one; medians. glowvm → glowc.</p></div>

  <h2>Size and memory</h2>
  <div class="legend"><span><i style="background:var(--a)"></i>glowvm</span><span><i style="background:var(--b)"></i>glowc</span></div>
  <div class="card"><h3>Upload size, gzip (KiB)</h3>${sizeChart}</div>
  <div class="card"><h3>Wasm memory high-water (KiB)</h3>${memChart}
    <p class="note">glowvm: one fresh instance per request. glowc: one instance per isolate, 5,000 requests through it, highest value seen.</p></div>

  <h2>Where glowc is not yet a drop-in replacement</h2>
  <div class="callout"><ul>
    <li><b>Recursion depth.</b> Body recursion deeper than about 12,340 frames fails on workerd (502, the next request works). glowvm handles 1,000,000. <code>lists:map</code> over ~12k elements hits it. The benchmark fixtures don't.</li>
    <li><b>json_roundtrip memory.</b> 4,992 KiB vs glowvm's 1,280 KiB, from the 2 MiB GC nursery chosen for speed.</li>
    <li><b>Map order above 32 keys and atom-key order in small maps</b> can differ from ERTS when a map is iterated (lookups and equality are the same).</li>
    <li><b>Stack traces</b> are coarser than ERTS. No processes or <code>receive</code> yet (Phase 2).</li>
  </ul></div>

  <footer>Sources: <code>bench/results/*.analytics.json</code>, <code>*-coldstart.cpu.json</code>, <code>bench/results/local/</code>, PROGRESS.md (M5). Regenerate with <code>node bench/compare.mjs</code>.</footer>
</main>
</body>
</html>
`;
fs.writeFileSync("compare.html", html);
console.log("compare.html written, verdict", overall, all.map(word).join(" "));
