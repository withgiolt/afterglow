import mod from "../../../build/eh_exnref.wasm";
import modOz from "../../../build/eh_exnref.oz.wasm";
const host = { host: { boom: (x) => x + 1n } };
const run = (m) => { try { return { ok: true, value: String(new WebAssembly.Instance(m, host).exports.eh(41n)) }; } catch (e) { return { ok: false, error: `${e?.constructor?.name}: ${e?.message}` }; } };
export default { fetch: () => Response.json({ plain_wat: run(mod), oz: run(modOz), expected: "42" }) };
