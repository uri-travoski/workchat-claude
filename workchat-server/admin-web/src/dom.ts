// Tiny DOM helpers. Text is always set via textContent, never innerHTML.
type Child = Node | string | null | undefined | false;

export function h<K extends keyof HTMLElementTagNameMap>(tag: K, attrs: Record<string, any> = {}, ...children: Child[]): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === undefined || v === null || v === false) continue;
    if (k === "class") el.className = v;
    else if (k.startsWith("on") && typeof v === "function") el.addEventListener(k.slice(2), v);
    else if (k === "value") (el as any).value = v;
    else if (k === "checked") (el as any).checked = !!v;
    else el.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    el.append(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return el;
}

export function toast(msg: string, err = false) {
  const t = h("div", { class: "toast" + (err ? " err" : "") }, msg);
  document.body.append(t);
  setTimeout(() => t.remove(), err ? 6000 : 3000);
}

export function dialog(title: string, body: Node, buttons: { label: string; primary?: boolean; danger?: boolean; action: (close: () => void) => void | Promise<void> }[]): HTMLDialogElement {
  const d = h("dialog", {});
  // Close from anywhere: the X, Cancel, Esc (native), a click on the backdrop. The element is removed whichever way it closes.
  const close = () => { if (d.open) d.close(); d.remove(); };
  d.addEventListener("close", () => d.remove());
  d.addEventListener("click", (ev) => { if (ev.target === d) close(); });
  d.append(
    h("div", { class: "dlg-head" }, h("h2", {}, title), h("button", { class: "dlg-x", type: "button", "aria-label": "Close", title: "Close", onclick: close }, "\u00d7")),
    h("div", { class: "dlg-body" }, body),
    h("div", { class: "row dlg-foot" },
      h("button", { type: "button", class: "dlg-cancel", onclick: close }, "Cancel"),
      ...buttons.map((b) => h("button", { type: "button", class: b.primary ? "primary" : b.danger ? "danger" : "", onclick: async (ev: Event) => {
        const btn = ev.currentTarget as HTMLButtonElement; btn.disabled = true;
        try { await b.action(close); } catch (e: any) { toast(e.message ?? String(e), true); } finally { btn.disabled = false; }
      } }, b.label))));
  document.body.append(d);
  d.showModal();
  return d;
}

export function field(label: string, input: HTMLElement): HTMLElement {
  return h("div", {}, h("label", {}, label), input);
}

export function fmtDate(ms?: number | null): string {
  if (!ms) return "never";
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  let hr = d.getHours(); const ap = hr >= 12 ? "pm" : "am"; hr = hr % 12 || 12;
  return `${p(d.getDate())}/${p(d.getMonth() + 1)}/${d.getFullYear()} ${hr}:${p(d.getMinutes())} ${ap}`;
}

export function fmtBytes(n: number): string {
  if (n < 1024) return n + " B";
  const u = ["KB", "MB", "GB", "TB"]; let i = -1;
  do { n /= 1024; i++; } while (n >= 1024 && i < u.length - 1);
  return n.toFixed(n < 10 ? 1 : 0) + " " + u[i];
}

/** `navigator.clipboard` and `crypto.randomUUID` exist only in secure contexts (https or localhost); the panel also runs on plain
 *  http://<LAN address>, so both have fallbacks. */
export async function copy(text: string) {
  try { await navigator.clipboard.writeText(text); toast("Copied"); return; } catch { /* not a secure context or denied: try the old way */ }
  try {
    const ta = document.createElement("textarea");
    ta.value = text; ta.setAttribute("readonly", ""); ta.style.cssText = "position:fixed;top:0;left:0;opacity:0";
    document.body.appendChild(ta); ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    if (ok) { toast("Copied"); return; }
  } catch { /* fall through */ }
  toast("Select the text and copy it manually", true);
}

/** 32 hex characters, unique enough for an idempotency key; works without a secure context. */
export function newKey(): string {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}
