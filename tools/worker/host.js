// Runs an afterglow app.wasm for index.mjs, the Worker entry it ships with. It never
// reads `env` or `process.env`; the entry decides which vars the app sees.
//
// The request goes in as ETF {Method, Url, Headers, Body} and the response comes out
// framed as {Status, Headers, Body} (see gleam/src/afterglow/internal/bridge.gleam).
// One Wasm instance per `instantiate`, reused across requests: afterglow_runtime resets
// the heap, binary arena and process state in rt_begin.

const MAX_BODY_SIZE = 1024 * 1024;

const encoder = new TextEncoder(),
  decoder = new TextDecoder();

export function instantiate(wasmModule) {
  let inst = null;
  let mem = null;

  function start() {
    const instance = new WebAssembly.Instance(wasmModule, {
      host: {
        log: (p, n) => console.log(decoder.decode(new Uint8Array(mem.buffer, p, n))),
        random_bytes: (p, n) => crypto.getRandomValues(new Uint8Array(mem.buffer, p, n)),
        now_us: () => Date.now() * 1000,
      },
    });
    mem = instance.exports.memory;
    instance.exports.init();
    inst = instance.exports;
  }

  function unpack(v) {
    const u = BigInt.asUintN(64, v);
    const p = Number(u >> 32n), n = Number(u & 0xffffffffn);
    return new Uint8Array(mem.buffer, p, n);
  }

  function run(requestEtf, vars) {
    if (!inst) start();
    const x = inst;
    try {
      const buf = x.rt_begin(requestEtf.length);
      new Uint8Array(mem.buffer, buf, requestEtf.length).set(requestEtf);
      const entries = Object.entries(vars).map(([k, v]) => `${k}=${v}\0`).join("");
      const eb = encoder.encode(entries);
      const ep = x.rt_env_buf(eb.length);
      new Uint8Array(mem.buffer, ep, eb.length).set(eb);
      x.rt_env_set();
      const r = x.main();
      if (r === 0n) {
        console.error("uncaught exception: " + decoder.decode(unpack(x.rt_exception_text())));
        const missing = decoder.decode(unpack(x.rt_missing_function()));
        if (missing) {
          const error = `afterglow does not implement ${missing}`;
          console.error(error);
          return { error, status: 500 };
        }
        return { error: "runtime error" };
      }
      return parseOutput(unpack(x.rt_output()).slice());
    } catch (e) {
      // A trap (stack overflow, unreachable) leaves the instance in an unknown state.
      inst = null;
      console.error("trap: " + (e && e.message));
      return { error: "runtime error" };
    }
  }

  // The entry instantiates at module scope, so this runs during startup: V8 compiles
  // Wasm functions lazily, and one throwaway request (the bridge skips the handler for
  // it) compiles the hot ones before the first real request.
  try {
    run(
      encodeRequest("GET", "/", [["host", "localhost"], ["accept", "*/*"], ["user-agent", "afterglow"]], new Uint8Array(0), true),
      {},
    );
  } catch {}

  return { handle: (request, vars) => handle(run, request, vars) };
}

const SENTINEL = encoder.encode("\n__AFTERGLOW__");

function parseOutput(out) {
  const end = lastIndexOf(out, SENTINEL);
  if (end < 4) return { error: "no output" };
  const size = new DataView(out.buffer, out.byteOffset + end - 4, 4).getUint32(0);
  const start = end - 4 - size;
  if (start < 0) return { error: "malformed output" };
  try {
    const [status, headers, body] = decodeEtf(out.subarray(start, end - 4));
    return {
      status,
      headers: headers.map(([k, v]) => [decoder.decode(k), decoder.decode(v)]),
      body,
    };
  } catch {
    return { error: "malformed output" };
  }
}

function lastIndexOf(haystack, needle) {
  outer: for (let i = haystack.length - needle.length; i >= 0; i--) {
    for (let j = 0; j < needle.length; j++) {
      if (haystack[i + j] !== needle[j]) continue outer;
    }
    return i;
  }
  return -1;
}

const ETF_VERSION = 131,
  SMALL_INTEGER_EXT = 97,
  INTEGER_EXT = 98,
  SMALL_TUPLE_EXT = 104,
  SMALL_ATOM_UTF8_EXT = 119,
  NIL_EXT = 106,
  LIST_EXT = 108,
  BINARY_EXT = 109;

function encodeRequest(method, url, headers, body, warmup = false) {
  const chunks = [];
  const tag = (...bytes) => chunks.push(Uint8Array.from(bytes));
  const u32 = (n) => {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, n);
    chunks.push(b);
  };
  const binary = (b) => {
    tag(BINARY_EXT);
    u32(b.length);
    chunks.push(b);
  };
  if (warmup) {
    const name = encoder.encode("warmup");
    tag(ETF_VERSION, SMALL_TUPLE_EXT, 5, SMALL_ATOM_UTF8_EXT, name.length);
    chunks.push(name);
  } else {
    tag(ETF_VERSION, SMALL_TUPLE_EXT, 4);
  }
  binary(encoder.encode(method));
  binary(encoder.encode(url));
  if (headers.length) {
    tag(LIST_EXT);
    u32(headers.length);
    for (const [k, v] of headers) {
      tag(SMALL_TUPLE_EXT, 2);
      binary(encoder.encode(k));
      binary(encoder.encode(v));
    }
  }
  tag(NIL_EXT);
  binary(body);
  const out = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0));
  let o = 0;
  for (const c of chunks) {
    out.set(c, o);
    o += c.length;
  }
  return out;
}

function decodeEtf(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes[0] !== ETF_VERSION) throw new Error("not ETF");
  let o = 1;
  const term = () => {
    const tag = bytes[o++];
    switch (tag) {
      case SMALL_INTEGER_EXT:
        return bytes[o++];
      case INTEGER_EXT: {
        const n = view.getInt32(o);
        o += 4;
        return n;
      }
      case SMALL_TUPLE_EXT: {
        const arity = bytes[o++], items = [];
        for (let i = 0; i < arity; i++) items.push(term());
        return items;
      }
      case NIL_EXT:
        return [];
      case LIST_EXT: {
        const n = view.getUint32(o), items = [];
        o += 4;
        for (let i = 0; i < n; i++) items.push(term());
        if (bytes[o++] !== NIL_EXT) throw new Error("improper list");
        return items;
      }
      case BINARY_EXT: {
        const n = view.getUint32(o);
        const b = bytes.slice(o + 4, o + 4 + n);
        o += 4 + n;
        return b;
      }
      default:
        throw new Error("unsupported ETF tag " + tag);
    }
  };
  return term();
}

// Only string vars reach the app: anything else (a binding object, a number) would be
// stringified into the env buffer, and a binding's `toString` is not a value to expose.
function stringVars(vars) {
  const out = {};
  for (const [key, val] of Object.entries(vars ?? {})) {
    if (typeof val === "string") out[key] = val;
  }
  return out;
}

async function handle(run, request, vars) {
  try {
    const url = new URL(request.url);
    const headers = [];
    request.headers.forEach((val, key) => headers.push([key, val]));
    let body = new Uint8Array(0);
    if (request.body) {
      const contentLength = request.headers.get("content-length");
      if (contentLength && parseInt(contentLength, 10) > MAX_BODY_SIZE) {
        return new Response(JSON.stringify({ error: "payload too large" }), {
          status: 413,
          headers: { "content-type": "application/json" },
        });
      }
      body = new Uint8Array(await request.arrayBuffer());
      if (body.length > MAX_BODY_SIZE) {
        return new Response(JSON.stringify({ error: "payload too large" }), {
          status: 413,
          headers: { "content-type": "application/json" },
        });
      }
    }
    const result = run(encodeRequest(request.method, url.pathname + url.search, headers, body), stringVars(vars));
    if (result.error) {
      return new Response(JSON.stringify({ error: result.error }), {
        status: result.status ?? 502,
        headers: { "content-type": "application/json" },
      });
    }
    const nullBody = [101, 204, 205, 304].includes(result.status);
    return new Response(nullBody ? null : result.body, {
      status: result.status,
      headers: result.headers,
    });
  } catch (e) {
    console.error("Worker error:", e.message || "unknown", e.stack || "");
    return new Response(JSON.stringify({ error: "internal server error" }), {
      status: 500,
      headers: { "content-type": "application/json" },
    });
  }
}
