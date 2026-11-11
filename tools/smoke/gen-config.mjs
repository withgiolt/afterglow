// gen-config.mjs OUT.capnp FIXTURE...: workerd config serving each fixture's afterglow
// Worker from out/workers/<fixture> on port 9400+i.
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import fs from 'node:fs';
const [out, ...fixtures] = process.argv.slice(2);
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const ident = (s) => s.replace(/[^a-zA-Z0-9]+(.)/g, (_, c) => c.toUpperCase());
const rel = (p) => path.relative(path.dirname(path.resolve(out)), p).split(path.sep).join('/');
let services = [], sockets = [], workers = [];
fixtures.forEach((f, i) => {
  const bindings = f === 'env_get' ? '[ (name = "SECRET_KEY", text = "secret") ]' : '[]';
  const dir = rel(path.join(root, 'out/workers', f));
  workers.push(`const w${ident('_' + f)} :Workerd.Worker = (
  modules = [ (name = "index.mjs", esModule = embed "${dir}/index.mjs"),
              (name = "host.js", esModule = embed "${dir}/host.js"),
              (name = "app.wasm", wasm = embed "${dir}/app.wasm") ],
  compatibilityDate = "2025-08-06", bindings = ${bindings});`);
  services.push(`(name = "${f}", worker = .w${ident('_' + f)})`);
  sockets.push(`(name = "s_${f}", address = "127.0.0.1:${9400 + i}", http = (), service = "${f}")`);
});
fs.writeFileSync(out, `using Workerd = import "/workerd/workerd.capnp";
const config :Workerd.Config = (
  services = [ ${services.join(',\n    ')} ],
  sockets = [ ${sockets.join(',\n    ')} ]
);
${workers.join('\n')}
`);
