// tools/worker/host.js, the library a platform's Worker entry uses to run an app.wasm.
// Uses the Workers tools/smoke/build.sh writes to out/workers/<fixture>.
//   bun test tests/host.test.ts
import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import * as host from "../tools/worker/host.js";

function fixtureModule(name: string) {
  const url = new URL(`../out/workers/${name}/app.wasm`, import.meta.url);
  if (!existsSync(url)) throw new Error(`missing ${url.pathname}: run tools/smoke/build.sh`);
  return new WebAssembly.Module(readFileSync(url));
}

const EMPTY_WASM_MODULE = new WebAssembly.Module(
  Uint8Array.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]),
);

test("host.js is a library, not a Worker entry", () => {
  expect("default" in host).toBe(false);
});

test("an app built for this ABI runs a request", async () => {
  const app = host.instantiate(fixtureModule("basic_get"));
  const response = await app.handle(new Request("http://example.com/"), {});
  expect(response.status).toEqual(200);
  expect(await response.text()).toEqual("hello from afterglow");
});

test("a module without the ABI export is refused before it runs", () => {
  expect(() => host.instantiate(EMPTY_WASM_MODULE)).toThrow(
    "this app.wasm was not built for afterglow host ABI 1",
  );
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

test("two apps in one isolate each run their own module", async () => {
  const basic = host.instantiate(fixtureModule("basic_get"));
  const env = host.instantiate(fixtureModule("env_get"));
  const a = await basic.handle(new Request("http://example.com/"), {});
  const b = await env.handle(new Request("http://example.com/"), { SECRET_KEY: "b" });
  expect(await a.text()).toEqual("hello from afterglow");
  expect(await b.text()).toEqual("b");
});
