// compare.mjs FIXTURE...: sends glowvm's smoke.test.ts requests to both runtimes
// (ports from gen-config.mjs) and compares status, headers and body byte for byte.
// Also checks the smoke test's own expectations against the glowc response.
const fixtures = process.argv.slice(2);
const marker = [...new TextEncoder().encode('\n__GLOWVM__')];
const cases = {
  basic_get: [['/', {}, (r) => r.status === 200 && r.text === 'hello from glowvm']],
  not_found: [['/anything', {}, (r) => r.status === 404]],
  env_get: [['/', {}, (r) => r.status === 200 && r.text === 'secret']],
  query_param: [['/?msg=hi', {}, (r) => r.status === 200 && r.text === 'hi']],
  bytes_body: [['/', {}, (r) => r.status === 200 && r.bytes.length === 1 && r.bytes[0] === 0xff]],
  echo_body: [
    ['/', { method: 'POST', body: new Uint8Array([0, 0xff, 131, ...marker, 0, 0, 0, 1, ...marker]) },
      (r) => r.status === 200 && r.headers.get('x-echo') === '1'],
    ['/', { method: 'POST', body: new Uint8Array(300_000).map((_, i) => i % 251) }, (r) => r.status === 200 && r.bytes.length === 300_000],
    ['/', { method: 'POST', body: new TextEncoder().encode('ünïcödé '.repeat(20000)) }, (r) => r.status === 200],
  ],
  no_content: [['/', {}, (r) => r.status === 204 && r.bytes.length === 0]],
  file_body: [['/', {}, (r) => r.status === 501]],
  lustre_html: [['/', {}, (r) => r.status === 200 && r.text.includes('<h1>hello from glowvm</h1>')]],
  json_roundtrip: [['/', {}, (r) => r.status === 200 && r.bytes.length > 50000]],
};
const get = async (port, p, init) => {
  const res = await fetch(`http://127.0.0.1:${port}${p}`, { headers: { host: 'example.com' }, ...init });
  const bytes = new Uint8Array(await res.arrayBuffer());
  return { status: res.status, headers: res.headers, hdrs: [...res.headers].map(([k, v]) => `${k}: ${v}`).join('\n'), bytes, text: new TextDecoder().decode(bytes) };
};
let fail = 0;
for (const [i, f] of fixtures.entries()) {
  const list = cases[f] ?? [['/', {}, (r) => r.status === 200 && r.text === 'OK']];
  for (const [p, init, check] of list) {
    const vm = await get(9400 + i, p, init), gc = await get(9500 + i, p, init);
    const same = vm.status === gc.status && vm.hdrs === gc.hdrs && Buffer.compare(Buffer.from(vm.bytes), Buffer.from(gc.bytes)) === 0;
    const ok = same && check(gc);
    if (!ok) fail++;
    console.log(`${ok ? 'PASS' : 'FAIL'} ${f} ${init.method ?? 'GET'} ${p} status ${gc.status} body ${gc.bytes.length} B` +
      (same ? ' (identical)' : ` DIFFERS: glowvm ${vm.status} ${vm.bytes.length} B [${vm.hdrs.replace(/\n/g, '; ')}] vs glowc [${gc.hdrs.replace(/\n/g, '; ')}] ${gc.text.slice(0, 120)}`) +
      (check(gc) ? '' : ' smoke-check FAILED'));
  }
}
process.exit(fail ? 1 : 0);
