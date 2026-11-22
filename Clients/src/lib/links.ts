// Pure link parsing (no UI imports, so it is unit-tested).

/** workchat://<host>/<path>?<query>, split by hand: WebViews differ in how they parse a URL whose scheme is not http(s)
 * (some give host "chat", the Android one gives host "" and path "//chat/<id>"). */
export function parseAppLink(raw: string): { host: string; path: string; params: URLSearchParams } | null {
  const m = /^workchat:\/*([^/?#]*)([^?#]*)(?:\?([^#]*))?/i.exec(raw.trim());
  if (!m) return null;
  return { host: m[1].toLowerCase(), path: m[2].replace(/^\/+/, ""), params: new URLSearchParams(m[3] ?? "") };
}
