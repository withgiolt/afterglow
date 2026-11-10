// The few ETF terms the DO test scripts send to an afterglow node, and a decoder for
// the replies they read (small and 32-bit integers, small bigs, atoms, tuples).
const enc = new TextEncoder();
export const atom = (s) => { const b = enc.encode(s); return [119, b.length, ...b]; };
export const int = (n) => { const b = Buffer.alloc(5); b[0] = 98; b.writeInt32BE(n, 1); return [...b]; };
export const list = (xs) => xs.length ? [108, 0, 0, 0, xs.length, ...xs.flat(), 106] : [106];
// {apply, Mod, F, Args}: the node runs Mod:F(Args...) and replies with the result.
export const apply = (mod, f, args) => Uint8Array.from([131, 104, 4, ...atom("apply"), ...atom(mod), ...atom(f), ...list(args)]);

export function decode(b) {
  let o = 1; const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const t = () => { const g = b[o++];
    if (g === 97) return b[o++];
    if (g === 98) { const v = dv.getInt32(o); o += 4; return v; }
    if (g === 110) { const n = b[o++]; const s = b[o++]; let v = 0; for (let i = n - 1; i >= 0; i--) v = v * 256 + b[o + i]; o += n; return s ? -v : v; }
    if (g === 119) { const n = b[o++]; const s = Buffer.from(b.subarray(o, o + n)).toString(); o += n; return s; }
    if (g === 104) { const n = b[o++]; const xs = []; for (let i = 0; i < n; i++) xs.push(t()); return xs; }
    return "?" + g; };
  return t();
}
