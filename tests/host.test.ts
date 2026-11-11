// tools/worker/host.mjs, which index.mjs runs an app.wasm with.
// Uses the Workers tools/smoke/build.sh writes to out/workers/<fixture>.
//   bun test tests/host.test.ts
import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import * as host from "../tools/worker/host.mjs";

function fixtureModule(name: string) {
  const url = new URL(`../out/workers/${name}/app.wasm`, import.meta.url);
  if (!existsSync(url)) throw new Error(`missing ${url.pathname}: run tools/smoke/build.sh`);
  return new WebAssembly.Module(readFileSync(url));
}

test("host.mjs is not a Worker entry", () => {
  expect("default" in host).toBe(false);
});

test("an app runs a request", async () => {
  const app = host.instantiate(fixtureModule("basic_get"));
  const response = await app.handle(new Request("http://example.com/"), {});
  expect(response.status).toEqual(200);
  expect(await response.text()).toEqual("hello from afterglow");
});

test("the app reads only the vars it is given", async () => {
  process.env.SECRET_KEY = "from process.env";
  try {
    const app = host.instantiate(fixtureModule("env_get"));

    const given = await app.handle(new Request("http://example.com/"), { SECRET_KEY: "given" });
    expect(await given.text()).toEqual("given");

    const none = await app.handle(new Request("http://example.com/"), {});
    expect(await none.text()).toEqual("NONE");
  } finally {
    delete process.env.SECRET_KEY;
  }
});

test("a var that is not a string is dropped", async () => {
  const app = host.instantiate(fixtureModule("env_get"));
  const response = await app.handle(new Request("http://example.com/"), {
    SECRET_KEY: { toString: () => "binding" },
  });
  expect(await response.text()).toEqual("NONE");
});

// Vars reach the runtime as `key=value\0` pairs, so a NUL in a value or an
// `=` in a key would let one var forge another.
test("a var that would forge another is dropped", async () => {
  const app = host.instantiate(fixtureModule("env_get"));
  for (const vars of [{ OTHER: "x\0SECRET_KEY=forged" }, { "SECRET_KEY=forged": "x" }]) {
    const response = await app.handle(new Request("http://example.com/"), vars);
    expect(await response.text()).toEqual("NONE");
  }
});

test("two apps in one isolate each run their own module", async () => {
  const basic = host.instantiate(fixtureModule("basic_get"));
  const env = host.instantiate(fixtureModule("env_get"));
  const a = await basic.handle(new Request("http://example.com/"), {});
  const b = await env.handle(new Request("http://example.com/"), { SECRET_KEY: "b" });
  expect(await a.text()).toEqual("hello from afterglow");
  expect(await b.text()).toEqual("b");
});
