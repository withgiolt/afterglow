// The Worker entry an afterglow build ships next to app.wasm. A platform deploying the
// app wraps this module in its own entry and decides what `fetch` gets as its second
// argument; only the string values in it reach the app as vars. There is no
// `process.env` fallback, so the app sees no vars unless the caller passes them.
import wasm from "./app.wasm";
import { instantiate } from "./host.js";

const app = instantiate(wasm);

export default {
  fetch(request, vars) {
    return app.handle(request, vars);
  },
};
