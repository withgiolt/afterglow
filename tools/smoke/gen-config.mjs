// gen-config.mjs OUT.capnp FIXTURE...: workerd config with every fixture on both
// runtimes. glowvm: ../glowvm/fixtures/app/dist (read-only; json_roundtrip from
// fixtures/app/dist). glowc: out/workers/<fixture>. Ports: glowvm 9400+i, glowc 9500+i.
import fs from 'node:fs';
import path from 'node:path';
const [out, ...fixtures] = process.argv.slice(2);
const root = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../..');
const glowvmDist = (f) => fs.existsSync(path.join(root, '../glowvm/fixtures/app/dist', f))
  ? path.join(root, '../glowvm/fixtures/app/dist', f) : path.join(root, 'fixtures/app/dist', f);
const ident = (s) => s.replace(/[^a-zA-Z0-9]+(.)/g, (_, c) => c.toUpperCase());
const rel = (p) => path.relative(path.dirname(path.resolve(out)), p);
let services = [], sockets = [], workers = [];
fixtures.forEach((f, i) => {
  const bindings = f === 'env_get' ? '[ (name = "SECRET_KEY", text = "secret") ]' : '[]';
  const vm = rel(glowvmDist(f)), gc = rel(path.join(root, 'out/workers', f));
  workers.push(`const vm${ident('_' + f)} :Workerd.Worker = (
  modules = [ (name = "index.min.js", esModule = embed "${vm}/index.min.js"),
              (name = "glowvm.wasm", wasm = embed "${vm}/glowvm.wasm"),
              (name = "app.avm", data = embed "${vm}/app.avm") ],
  compatibilityDate = "2025-08-06", bindings = ${bindings});
const gc${ident('_' + f)} :Workerd.Worker = (
  modules = [ (name = "index.js", esModule = embed "${gc}/index.js"),
              (name = "app.wasm", wasm = embed "${gc}/app.wasm") ],
  compatibilityDate = "2025-08-06", bindings = ${bindings});`);
  for (const [k, port] of [['vm', 9400 + i], ['gc', 9500 + i]]) {
    services.push(`(name = "${k}_${f}", worker = .${k}${ident('_' + f)})`);
    sockets.push(`(name = "s_${k}_${f}", address = "127.0.0.1:${port}", http = (), service = "${k}_${f}")`);
  }
});
fs.writeFileSync(out, `using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [ ${services.join(',\n    ')} ],
  sockets = [ ${sockets.join(',\n    ')} ]
);
${workers.join('\n')}
`);
