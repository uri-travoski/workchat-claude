// Minimal QR Code encoder (byte mode, error correction level M, versions 1 to 10), written for the login-link QR.
// No dependencies and no network. The layout and Reed-Solomon steps follow ISO/IEC 18004; decoded in tests with ZXing.

// Per version 1..10 at level M: EC codewords per block and number of blocks.
const ECC_PER_BLOCK = [0, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26];
const NUM_BLOCKS = [0, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5];
export const MAX_VERSION = 10;

function rawModules(v: number): number {
  let r = (16 * v + 128) * v + 64;
  if (v >= 2) { const a = Math.floor(v / 7) + 2; r -= (25 * a - 10) * a - 55; if (v >= 7) r -= 36; }
  return r;
}
function dataCodewords(v: number): number {
  return Math.floor(rawModules(v) / 8) - ECC_PER_BLOCK[v] * NUM_BLOCKS[v];
}

// GF(256) with the QR polynomial 0x11D
function gfMul(x: number, y: number): number {
  let z = 0;
  for (let i = 7; i >= 0; i--) { z = (z << 1) ^ ((z >>> 7) * 0x11d); z ^= ((y >>> i) & 1) * x; }
  return z;
}
function rsDivisor(degree: number): number[] {
  const r: number[] = new Array(degree).fill(0); r[degree - 1] = 1;
  let root = 1;
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < r.length; j++) { r[j] = gfMul(r[j], root); if (j + 1 < r.length) r[j] ^= r[j + 1]; }
    root = gfMul(root, 2);
  }
  return r;
}
function rsRemainder(data: number[], div: number[]): number[] {
  const r = div.map(() => 0);
  for (const b of data) {
    const f = b ^ (r.shift() as number); r.push(0);
    div.forEach((c, i) => { r[i] ^= gfMul(c, f); });
  }
  return r;
}

const bit = (x: number, i: number) => ((x >>> i) & 1) !== 0;

/** Returns the module matrix (true = dark) without quiet zone, or null when the text does not fit version 10. */
export function qrEncode(text: string): boolean[][] | null {
  const bytes = Array.from(new TextEncoder().encode(text));
  let ver = 0;
  for (let v = 1; v <= MAX_VERSION; v++) {
    if (4 + (v <= 9 ? 8 : 16) + bytes.length * 8 <= dataCodewords(v) * 8) { ver = v; break; }
  }
  if (!ver) return null;
  const bits: number[] = [];
  const push = (val: number, n: number) => { for (let i = n - 1; i >= 0; i--) bits.push((val >>> i) & 1); };
  push(0b0100, 4); push(bytes.length, ver <= 9 ? 8 : 16); bytes.forEach((b) => push(b, 8));
  const cap = dataCodewords(ver) * 8;
  push(0, Math.min(4, cap - bits.length));
  push(0, (8 - (bits.length % 8)) % 8);
  for (let pad = 0xec; bits.length < cap; pad ^= 0xec ^ 0x11) push(pad, 8);
  const data: number[] = [];
  for (let i = 0; i < bits.length; i += 8) data.push(parseInt(bits.slice(i, i + 8).join(""), 2));

  // split into blocks, add EC, interleave
  const nb = NUM_BLOCKS[ver], ecl = ECC_PER_BLOCK[ver], raw = Math.floor(rawModules(ver) / 8);
  const shortBlocks = nb - (raw % nb), shortLen = Math.floor(raw / nb);
  const div = rsDivisor(ecl);
  const blocks: number[][] = [];
  for (let i = 0, k = 0; i < nb; i++) {
    const len = shortLen - ecl + (i < shortBlocks ? 0 : 1);
    const dat = data.slice(k, k + len); k += len;
    const ecc = rsRemainder(dat, div);
    if (i < shortBlocks) dat.push(0);
    blocks.push(dat.concat(ecc));
  }
  const all: number[] = [];
  for (let i = 0; i < blocks[0].length; i++) blocks.forEach((b, j) => { if (i !== shortLen - ecl || j >= shortBlocks) all.push(b[i]); });

  const size = ver * 4 + 17;
  const mod: boolean[][] = Array.from({ length: size }, () => new Array(size).fill(false));
  const fn: boolean[][] = Array.from({ length: size }, () => new Array(size).fill(false));
  const setFn = (x: number, y: number, dark: boolean) => { mod[y][x] = dark; fn[y][x] = true; };
  for (let i = 0; i < size; i++) { setFn(6, i, i % 2 === 0); setFn(i, 6, i % 2 === 0); }
  const finder = (cx: number, cy: number) => {
    for (let dy = -4; dy <= 4; dy++) for (let dx = -4; dx <= 4; dx++) {
      const d = Math.max(Math.abs(dx), Math.abs(dy)), x = cx + dx, y = cy + dy;
      if (x >= 0 && x < size && y >= 0 && y < size) setFn(x, y, d !== 2 && d !== 4);
    }
  };
  finder(3, 3); finder(size - 4, 3); finder(3, size - 4);
  const na = ver === 1 ? 0 : Math.floor(ver / 7) + 2;
  const step = ver === 1 ? 0 : Math.ceil((ver * 4 + 4) / (na * 2 - 2)) * 2;
  const pos = [6]; for (let p = size - 7; pos.length < na; p -= step) pos.splice(1, 0, p);
  for (let i = 0; i < pos.length; i++) for (let j = 0; j < pos.length; j++) {
    if ((i === 0 && j === 0) || (i === 0 && j === pos.length - 1) || (i === pos.length - 1 && j === 0)) continue;
    for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) setFn(pos[i] + dx, pos[j] + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
  }
  const formatBits = (mask: number) => {
    const d = (0 << 3) | mask; // level M = 0b00
    let rem = d; for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
    return ((d << 10) | rem) ^ 0x5412;
  };
  const drawFormat = (mask: number) => {
    const b = formatBits(mask);
    for (let i = 0; i <= 5; i++) setFn(8, i, bit(b, i));
    setFn(8, 7, bit(b, 6)); setFn(8, 8, bit(b, 7)); setFn(7, 8, bit(b, 8));
    for (let i = 9; i < 15; i++) setFn(14 - i, 8, bit(b, i));
    for (let i = 0; i < 8; i++) setFn(size - 1 - i, 8, bit(b, i));
    for (let i = 8; i < 15; i++) setFn(8, size - 15 + i, bit(b, i));
    setFn(8, size - 8, true);
  };
  drawFormat(0); // reserve the area
  if (ver >= 7) {
    let rem = ver; for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25);
    const b = (ver << 12) | rem;
    for (let i = 0; i < 18; i++) { const a = size - 11 + (i % 3), c = Math.floor(i / 3); setFn(a, c, bit(b, i)); setFn(c, a, bit(b, i)); }
  }
  // zig-zag placement of the codewords
  let k = 0;
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vert = 0; vert < size; vert++) for (let j = 0; j < 2; j++) {
      const x = right - j, up = ((right + 1) & 2) === 0, y = up ? size - 1 - vert : vert;
      if (!fn[y][x] && k < all.length * 8) { mod[y][x] = bit(all[k >>> 3], 7 - (k & 7)); k++; }
    }
  }
  const mask = (m: number) => {
    for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
      let inv = false;
      switch (m) {
        case 0: inv = (x + y) % 2 === 0; break;
        case 1: inv = y % 2 === 0; break;
        case 2: inv = x % 3 === 0; break;
        case 3: inv = (x + y) % 3 === 0; break;
        case 4: inv = (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0; break;
        case 5: inv = ((x * y) % 2) + ((x * y) % 3) === 0; break;
        case 6: inv = (((x * y) % 2) + ((x * y) % 3)) % 2 === 0; break;
        default: inv = (((x + y) % 2) + ((x * y) % 3)) % 2 === 0;
      }
      if (!fn[y][x] && inv) mod[y][x] = !mod[y][x];
    }
  };
  const penalty = () => {
    let p = 0;
    const lines: string[] = [];
    for (let i = 0; i < size; i++) {
      lines.push(mod[i].map((v) => (v ? "1" : "0")).join(""));
      lines.push(mod.map((r) => (r[i] ? "1" : "0")).join(""));
    }
    for (const l of lines) {
      for (const m of l.matchAll(/0{5,}|1{5,}/g)) p += 3 + (m[0].length - 5);
      p += 40 * ((l.match(/(?=00001011101)/g) ?? []).length + (l.match(/(?=10111010000)/g) ?? []).length);
    }
    for (let y = 0; y < size - 1; y++) for (let x = 0; x < size - 1; x++) if (mod[y][x] === mod[y][x + 1] && mod[y][x] === mod[y + 1][x] && mod[y][x] === mod[y + 1][x + 1]) p += 3;
    const dark = mod.flat().filter(Boolean).length, total = size * size;
    p += (Math.ceil(Math.abs(dark * 20 - total * 10) / total) - 1) * 10;
    return p;
  };
  let best = 0, bestP = Infinity;
  for (let m = 0; m < 8; m++) { mask(m); drawFormat(m); const p = penalty(); if (p < bestP) { best = m; bestP = p; } mask(m); }
  mask(best); drawFormat(best);
  return mod;
}

/** An <svg> element (dark modules on a white square with a 4-module quiet zone). */
export function qrSvg(text: string, px = 240): SVGElement | null {
  const m = qrEncode(text);
  if (!m) return null;
  const n = m.length + 8, ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", `0 0 ${n} ${n}`); svg.setAttribute("width", String(px)); svg.setAttribute("height", String(px));
  svg.setAttribute("role", "img"); svg.setAttribute("aria-label", "QR code of the one-time login link"); svg.setAttribute("shape-rendering", "crispEdges");
  svg.setAttribute("class", "qr");
  const bg = document.createElementNS(ns, "rect"); bg.setAttribute("width", String(n)); bg.setAttribute("height", String(n)); bg.setAttribute("fill", "#fff"); svg.append(bg);
  let d = "";
  m.forEach((row, y) => row.forEach((v, x) => { if (v) d += `M${x + 4} ${y + 4}h1v1h-1z`; }));
  const path = document.createElementNS(ns, "path"); path.setAttribute("d", d); path.setAttribute("fill", "#000"); svg.append(path);
  return svg;
}
