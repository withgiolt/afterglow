// Tail Worker: records per-invocation CPU and wall time from every producer that
// lists it in `tail_consumers`, into Durable Object SQLite. Read-only endpoints
// return stats and CSV; there is no arbitrary SQL.
import { DurableObject } from "cloudflare:workers";

export class Store extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    ctx.storage.sql.exec(`CREATE TABLE IF NOT EXISTS ev (
      id INTEGER PRIMARY KEY, ts INTEGER, script TEXT, outcome TEXT,
      cpu REAL, wall REAL, path TEXT, phase TEXT, colo TEXT, raw TEXT)`);
  }
  add(rows) {
    for (const r of rows) {
      this.ctx.storage.sql.exec(
        "INSERT INTO ev (ts, script, outcome, cpu, wall, path, phase, colo, raw) VALUES (?,?,?,?,?,?,?,?,?)",
        r.ts, r.script, r.outcome, r.cpu, r.wall, r.path, r.phase, r.colo, r.raw,
      );
    }
    return rows.length;
  }
  raw(limit) {
    return this.ctx.storage.sql.exec("SELECT * FROM ev ORDER BY id DESC LIMIT ?", limit).toArray();
  }
  rows(script, phase, since) {
    return this.ctx.storage.sql
      .exec(
        "SELECT ts, script, outcome, cpu, wall, path, phase, colo FROM ev WHERE script = ? AND phase = ? AND ts >= ? ORDER BY id",
        script, phase, since,
      )
      .toArray();
  }
}

const pct = (sorted, p) => (sorted.length ? sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))] : null);

export default {
  async tail(events, env) {
    const rows = events.map((e) => {
      const req = e.event?.request;
      let path = null, phase = "measure";
      try {
        const u = new URL(req.url);
        path = u.pathname;
        phase = u.searchParams.get("phase") ?? "measure";
      } catch {}
      const { logs, ...rest } = e;
      return {
        ts: e.eventTimestamp ?? Date.now(),
        script: e.scriptName ?? null,
        outcome: e.outcome ?? null,
        cpu: e.cpuTime ?? null,
        wall: e.wallTime ?? null,
        path,
        phase,
        colo: req?.cf?.colo ?? null,
        raw: JSON.stringify({ ...rest, logs: (logs ?? []).slice(0, 5) }).slice(0, 4000),
      };
    });
    if (rows.length) await env.STORE.get(env.STORE.idFromName("all")).add(rows);
  },

  async fetch(request, env) {
    const url = new URL(request.url);
    const store = env.STORE.get(env.STORE.idFromName("all"));
    const script = url.searchParams.get("script");
    const phase = url.searchParams.get("phase") ?? "measure";
    const since = Number(url.searchParams.get("since") ?? 0);
    switch (url.pathname) {
      case "/raw":
        return Response.json(await store.raw(Number(url.searchParams.get("limit") ?? 3)));
      case "/stats": {
        const rows = await store.rows(script, phase, since);
        const ok = rows.filter((r) => r.outcome === "ok");
        const cpu = ok.map((r) => r.cpu).sort((a, b) => a - b);
        const wall = ok.map((r) => r.wall).sort((a, b) => a - b);
        const outcomes = {};
        for (const r of rows) outcomes[r.outcome] = (outcomes[r.outcome] ?? 0) + 1;
        return Response.json({
          script, phase, since, n: rows.length, outcomes,
          cpu: { p50: pct(cpu, 50), p90: pct(cpu, 90), p99: pct(cpu, 99), max: cpu.at(-1) ?? null },
          wall: { p50: pct(wall, 50), p99: pct(wall, 99) },
          colos: [...new Set(rows.map((r) => r.colo))],
        });
      }
      case "/csv": {
        const rows = await store.rows(script, phase, since);
        const lines = ["ts,script,outcome,cpu,wall,path,phase,colo", ...rows.map((r) => [r.ts, r.script, r.outcome, r.cpu, r.wall, r.path, r.phase, r.colo].join(","))];
        return new Response(lines.join("\n") + "\n", { headers: { "content-type": "text/csv" } });
      }
      default:
        return new Response("not found", { status: 404 });
    }
  },
};
