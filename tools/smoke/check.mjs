// check.mjs FIXTURE...: sends tests/smoke.test.ts's requests to each fixture's Worker
// on workerd (ports from gen-config.mjs) and checks the responses.
const fixtures = process.argv.slice(2);
const marker = [...new TextEncoder().encode('\n__AFTERGLOW__')];
const cases = {
  basic_get: [['/', {}, (r) => r.status === 200 && r.text === 'hello from afterglow']],
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
  lustre_html: [['/', {}, (r) => r.status === 200 && r.text.includes('<h1>hello from afterglow</h1>')]],
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
    const r = await get(9400 + i, p, init);
    const ok = check(r);
    if (!ok) fail++;
    console.log(`${ok ? 'PASS' : 'FAIL'} ${f} ${init.method ?? 'GET'} ${p} status ${r.status} body ${r.bytes.length} B` +
      (ok ? '' : ` [${r.hdrs.replace(/\n/g, '; ')}] ${r.text.slice(0, 120)}`));
  }
}
process.exit(fail ? 1 : 0);
