// The Worker entry this repo's tests and tools run an app.wasm with. It is not shipped:
// the Hex package carries only host.js, and the platform that deploys the app writes
// its own entry around it.
//
// Every string in `env` reaches the app, and Bun and Node have no `env` argument, so
// tests/smoke.test.ts and tools/fetch-worker.ts fall back to process.env.
import wasm from "./app.wasm";
import { instantiate } from "./host.js";

const app = instantiate(wasm);

export default {
  fetch(request, env) {
    return app.handle(request, env ?? process.env);
  },
};
