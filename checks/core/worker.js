// Phase 0 core checks. Every .wasm is a module import (precompiled by workerd at
// upload time); nothing is compiled from bytes at runtime.
import tailcall from "../../build/tailcall.wasm";
import tailcallOz from "../../build/tailcall.oz.wasm";
import plaincall from "../../build/plaincall.wasm";
import features from "../../build/features.wasm";
import featuresOz from "../../build/features.oz.wasm";

let twiceSaw = null;
const host = {
  host: {
    twice(x) {
      twiceSaw = typeof x;
      return x * 2n;
    },
  },
};

// Created once at module scope; /bump reports whether the same instance and
// memory survive between requests in this isolate.
const shared = new WebAssembly.Instance(features, host);
// workerd forbids random values at global scope, so the id is made on first use.
let isolateId = null;

function run(fn) {
  try {
    return { ok: true, value: String(fn()) };
  } catch (e) {
    return { ok: false, error: `${e?.constructor?.name}: ${e?.message}` };
  }
}

export default {
  async fetch(request) {
    const url = new URL(request.url);
    const n = BigInt(url.searchParams.get("n") ?? "10000000");
    let body;
    switch (url.pathname) {
      case "/tail":
        body = {
          plain_wat: run(() => new WebAssembly.Instance(tailcall).exports.ping(n, 0n)),
          oz: run(() => new WebAssembly.Instance(tailcallOz).exports.ping(n, 0n)),
        };
        break;
      case "/plaincall":
        body = run(() => new WebAssembly.Instance(plaincall).exports.ping(n, 0n));
        break;
      case "/features":
        body = {
          plain_wat: run(() => new WebAssembly.Instance(features, host).exports.features(21n, 1e30)),
          oz: run(() => new WebAssembly.Instance(featuresOz, host).exports.features(21n, 1e30)),
          expected: String(63n - 1n + 2147483647n + 103n + 7n),
        };
        break;
      case "/bump":
        isolateId ??= crypto.randomUUID();
        body = { isolateId, count: shared.exports.bump() };
        break;
      case "/i64": {
        twiceSaw = null;
        const r = run(() => shared.exports.i64_roundtrip(2n ** 62n));
        body = { result: r, resultType: typeof shared.exports.i64_roundtrip(1n), importArgType: twiceSaw, expected: String((2n ** 62n + 1n) * 2n - 2n ** 64n) };
        break;
      }
      case "/env":
        body = {
          jspi_Suspending: "Suspending" in WebAssembly,
          jspi_promising: "promising" in WebAssembly,
          wasm_Tag: "Tag" in WebAssembly,
          wasm_Exception: "Exception" in WebAssembly,
          process_versions: globalThis.process?.versions ?? null,
          navigator_userAgent: globalThis.navigator?.userAgent ?? null,
          runtimeCompile: run(() => new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]))),
        };
        break;
      default:
        return new Response("not found", { status: 404 });
    }
    return Response.json(body);
  },
};
