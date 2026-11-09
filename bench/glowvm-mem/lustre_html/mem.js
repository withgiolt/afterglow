// Memory probe: wraps glowvm's entry unchanged except one line that records the
// instance's linear memory size after _start. Local only, never used for CPU.
import glowvm from "./index.min.js";
export default {
  async fetch(request, env, ctx) {
    globalThis.__glowMem = null;
    const res = await glowvm.fetch(request, env, ctx);
    const out = new Response(res.body, res);
    out.headers.set("x-wasm-memory-bytes", String(globalThis.__glowMem));
    return out;
  },
};
