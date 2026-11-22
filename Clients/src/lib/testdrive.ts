// Debug builds only: runs the script named by WORKCHAT_TEST_SCRIPT so a driver can walk the UI and take screenshots
// (each `api.mark(name)` pauses until the driver has captured the window). Release builds never get a script.
import * as appmod from "./app.svelte";
import { core } from "./core";

export async function runTestScript() {
  let script: string | null = null;
  try {
    script = await core.testScript();
  } catch {
    return;
  }
  if (!script) return;
  const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
  const q = (sel: string) => document.querySelector<HTMLElement>(sel);
  const api = {
    ...appmod,
    core,
    wait,
    mark: async (name: string) => {
      await wait(350);
      await core.testMark(name);
    },
    click: async (sel: string, nth = 0) => {
      const all = document.querySelectorAll<HTMLElement>(sel);
      const el = all[nth < 0 ? all.length + nth : nth];
      if (!el) throw new Error("click: nothing matches " + sel);
      el.click();
      await wait(250);
    },
    /** Clicks the first button or link whose visible text or aria-label contains `text`. */
    clickText: async (text: string, scope = "body") => {
      const els = [...document.querySelectorAll<HTMLElement>(`${scope} button, ${scope} [role=tab], ${scope} [role=menuitem]`)];
      const el = els.find((e) => (e.getAttribute("aria-label") ?? e.textContent ?? "").trim().toLowerCase().includes(text.toLowerCase()));
      if (!el) throw new Error("clickText: no button with " + text);
      el.click();
      await wait(250);
    },
    type: async (sel: string, value: string) => {
      const el = q(sel) as HTMLInputElement | HTMLTextAreaElement | null;
      if (!el) throw new Error("type: nothing matches " + sel);
      el.focus();
      el.value = value;
      el.dispatchEvent(new Event("input", { bubbles: true }));
      await wait(250);
    },
    openChat: async (title: string) => {
      const row = Object.values(appmod.app.rows).find((r) => r.title === title);
      if (!row) throw new Error("openChat: no chat " + title);
      appmod.app.tab = "chats";
      await appmod.openConversation(row.id);
      await wait(500);
      return row.id;
    },
    /** Scrolls the nearest scrolling message list so the element matching `sel` (the nth, default last) is centred. */
    scrollTo: async (sel: string, nth = -1, block: ScrollLogicalPosition = "center") => {
      const els = document.querySelectorAll<HTMLElement>(sel);
      const el = els[nth < 0 ? els.length + nth : nth];
      if (!el) throw new Error("scrollTo: nothing matches " + sel);
      el.scrollIntoView({ block });
      await wait(300);
    },
    contextMenu: async (sel: string, nth = -1) => {
      const els = document.querySelectorAll<HTMLElement>(sel);
      const el = els[nth < 0 ? els.length + nth : nth];
      if (!el) throw new Error("contextMenu: nothing matches " + sel);
      const r = el.getBoundingClientRect();
      el.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + r.width / 2, clientY: r.top + r.height / 2 }));
      await wait(300);
    },
    pointer: async (sel: string, type: string, dx = 0, dy = 0) => {
      const el = q(sel);
      if (!el) throw new Error("pointer: nothing matches " + sel);
      const r = el.getBoundingClientRect();
      el.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 7, pointerType: "touch", clientX: r.left + r.width / 2 + dx, clientY: r.top + r.height / 2 + dy }));
      await wait(100);
    },
    exists: (sel: string) => !!q(sel),
  };
  (window as unknown as { __wcApi: typeof api }).__wcApi = api;
  await core.testRun(script);
}
