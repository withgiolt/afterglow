// bun tools/fetch-worker.ts WORKER_DIR [METHOD] PATH [BODY]: one request through an afterglow
// Worker (index.js + app.wasm) in-process, the way tests/smoke.test.ts loads them.
// Prints status, headers and body.
import { plugin } from "bun";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

plugin({
  name: "afterglow-assets",
  setup(build) {
    build.onLoad({ filter: /app\.wasm$/ }, (args) => ({
      exports: { default: new WebAssembly.Module(readFileSync(args.path)) },
      loader: "object",
    }));
  },
});

const [dir, ...rest] = process.argv.slice(2);
const [method, path, body] = rest.length >= 2 ? rest : ["GET", rest[0] ?? "/"];
if (/^[A-Za-z]:[\\/]/.test(path)) {
  // Git Bash turns a bare "/path" argument into a Windows path before bun sees it
  console.error(`path became ${path}: run with MSYS_NO_PATHCONV=1`);
  process.exit(2);
}
const { default: worker } = await import(pathToFileURL(resolve(dir, "index.js")).href);
const res = await worker.fetch(new Request(`http://example.com${path}`, {
  method, headers: { host: "example.com" }, body: method === "GET" ? undefined : body,
}));
console.log(res.status);
for (const [k, v] of res.headers) console.log(`${k}: ${v}`);
console.log();
console.log(await res.text());
