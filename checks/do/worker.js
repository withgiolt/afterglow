// Two DO classes: Counter (SQLite storage + alarm) and Caller (RPC into Counter).
import { DurableObject } from "cloudflare:workers";

export class Counter extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    ctx.storage.sql.exec("CREATE TABLE IF NOT EXISTS kv (k TEXT PRIMARY KEY, v INTEGER)");
  }
  increment() {
    const sql = this.ctx.storage.sql;
    sql.exec("INSERT INTO kv VALUES ('count', 1) ON CONFLICT(k) DO UPDATE SET v = v + 1");
    return sql.exec("SELECT v FROM kv WHERE k = 'count'").one().v;
  }
  async armAlarm(ms) {
    await this.ctx.storage.setAlarm(Date.now() + ms);
    return await this.ctx.storage.getAlarm();
  }
  async alarm() {
    this.ctx.storage.sql.exec("INSERT INTO kv VALUES ('alarms', 1) ON CONFLICT(k) DO UPDATE SET v = v + 1");
  }
  alarms() {
    return this.ctx.storage.sql.exec("SELECT v FROM kv WHERE k = 'alarms'").toArray()[0]?.v ?? 0;
  }
}

export class Caller extends DurableObject {
  async callCounter(name) {
    const stub = this.env.COUNTER.get(this.env.COUNTER.idFromName(name));
    const t0 = performance.now();
    const count = await stub.increment();
    return { count, rpcMs: performance.now() - t0 };
  }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const counter = env.COUNTER.get(env.COUNTER.idFromName("c"));
    switch (url.pathname) {
      case "/sql": return Response.json({ count: await counter.increment() });
      case "/alarm": return Response.json({ armedFor: await counter.armAlarm(500) });
      case "/alarms": return Response.json({ fired: await counter.alarms() });
      case "/rpc": {
        const caller = env.CALLER.get(env.CALLER.idFromName("a"));
        return Response.json(await caller.callCounter("c"));
      }
    }
    return new Response("not found", { status: 404 });
  },
};
