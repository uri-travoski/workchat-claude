import qrcode from "qrcode-generator";

/** An SVG data URL of a QR code for `text` (error correction M). */
export function qrSvg(text: string, size = 220): string {
  const qr = qrcode(0, "M");
  qr.addData(text);
  qr.make();
  const n = qr.getModuleCount();
  const quiet = 3;
  let d = "";
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (qr.isDark(y, x)) d += `M${x + quiet} ${y + quiet}h1v1h-1z`;
  const full = n + quiet * 2;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${full} ${full}" width="${size}" height="${size}" shape-rendering="crispEdges"><rect width="${full}" height="${full}" fill="#fff"/><path d="${d}" fill="#000"/></svg>`;
  return "data:image/svg+xml;utf8," + encodeURIComponent(svg);
}

/** A vCard 3.0 for the person's contact details (what "Share Card" encodes and copies). */
export function vcard(p: { name: string; title?: string | null; phone?: string | null; username: string; server: string }): string {
  const esc = (s: string) => s.replace(/([,;\\])/g, "\\$1").replace(/\n/g, "\\n");
  const [first, ...rest] = p.name.split(" ");
  const lines = ["BEGIN:VCARD", "VERSION:3.0", `N:${esc(rest.join(" "))};${esc(first)};;;`, `FN:${esc(p.name)}`];
  if (p.title) lines.push(`TITLE:${esc(p.title)}`);
  if (p.phone) lines.push(`TEL;TYPE=WORK:${esc(p.phone)}`);
  lines.push(`NOTE:${esc("WorkChat @" + p.username + " on " + p.server)}`, "END:VCARD");
  return lines.join("\r\n");
}
