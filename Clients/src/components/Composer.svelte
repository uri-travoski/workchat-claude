<script lang="ts">
  import { tick, untrack } from "svelte";
  import { app, fail, prefs, toast } from "../lib/app.svelte";
  import { stageBrowserFiles } from "../lib/attach";
  import { activeFormats, applyFormat, shortcutFor, type FormatKind } from "../lib/composer";
  import { core, type MessageView } from "../lib/core";
  import { hasFormatting } from "../lib/markup";
  import EmojiPicker from "./EmojiPicker.svelte";
  import Icon from "./Icon.svelte";
  import Rich from "./Rich.svelte";

  let {
    convId,
    threadRoot = null,
    editing = null,
    oncancelEdit,
    placeholder = "Message…",
    alsoChannel = false,
  }: { convId: string; threadRoot?: string | null; editing?: MessageView | null; oncancelEdit?: () => void; placeholder?: string; alsoChannel?: boolean } = $props();

  // Unsent text per conversation/thread survives switching chats for this session.
  const drafts: Record<string, string> = (globalThis as any).__wcDrafts ?? ((globalThis as any).__wcDrafts = {});
  const key = $derived(`${convId}|${threadRoot ?? ""}`);

  let text = $state("");
  let ta = $state<HTMLTextAreaElement>();
  let sel = $state({ start: 0, end: 0 });
  let toolsOpen = $state(false);
  let emojiOpen = $state(false);
  let linkOpen = $state(false);
  let linkUrl = $state("");
  let busy = $state(false);
  let lastTyping = 0;
  let loadedKey = "";
  let also = $state(false);

  // ---- voice message: hold the mic (release sends, slide left cancels, slide up locks) or tap it for hands-free
  type Rec = { mode: "hold" | "locked"; cancelling: boolean; lockHint: boolean };
  let rec = $state<Rec | null>(null);
  let recBusy = false;
  let micDown: { x: number; y: number; t: number } | null = null;
  const fmtTime = (ms: number) => `${Math.floor(ms / 60000)}:${String(Math.floor(ms / 1000) % 60).padStart(2, "0")}`;

  async function micStart(e: PointerEvent) {
    if (rec || recBusy) return;
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* synthetic pointer (tests) */
    }
    micDown = { x: e.clientX, y: e.clientY, t: Date.now() };
    recBusy = true;
    try {
      app.voice.levels = [];
      app.voice.elapsed = 0;
      await core.voiceStart();
      rec = { mode: "hold", cancelling: false, lockHint: false };
    } catch (err) {
      micDown = null;
      fail(err);
    } finally {
      recBusy = false;
    }
  }
  function micMove(e: PointerEvent) {
    if (!rec || rec.mode !== "hold" || !micDown) return;
    const dx = e.clientX - micDown.x;
    const dy = e.clientY - micDown.y;
    rec.cancelling = dx < -70;
    rec.lockHint = dy < -40;
    if (dy < -80) rec.mode = "locked";
  }
  async function micUp() {
    if (!micDown) return;
    const held = Date.now() - micDown.t;
    micDown = null;
    // The recorder may still be starting when the finger lifts: wait for it.
    for (let i = 0; i < 40 && !rec && recBusy; i++) await new Promise((r) => setTimeout(r, 25));
    if (!rec) return;
    if (rec.mode === "locked") return;
    if (rec.cancelling) return void cancelRec();
    if (held < 350) {
      rec.mode = "locked"; // a tap: keep recording hands-free
      return;
    }
    await sendRec();
  }
  async function cancelRec() {
    rec = null;
    await core.voiceCancel().catch(() => {});
  }
  async function sendRec() {
    rec = null;
    try {
      const st = await core.voiceFinish();
      await core.sendFiles(convId, [st.id], "", threadRoot);
    } catch (err) {
      fail(err);
    }
  }
  $effect(() => {
    // A recording never outlives the screen it was started on.
    key;
    return () => {
      if (rec) void core.voiceCancel().catch(() => {});
      rec = null;
    };
  });

  $effect(() => {
    // Switching conversation: store the old draft, load the new one.
    const k = key;
    if (k !== loadedKey) {
      if (loadedKey) drafts[loadedKey] = text;
      loadedKey = k;
      text = drafts[k] ?? "";
      linkOpen = emojiOpen = false;
    }
  });
  // Editing swaps the box content; the unsent draft is put back when editing ends (saved or cancelled).
  let stash = "";
  let wasEditing = false;
  $effect(() => {
    const e = editing;
    untrack(() => {
      if (e) {
        if (!wasEditing) stash = text;
        wasEditing = true;
        text = e.body;
        void tick().then(() => ta?.focus());
      } else if (wasEditing) {
        wasEditing = false;
        text = stash;
        stash = "";
      }
    });
  });
  $effect(() => {
    const v = text;
    drafts[loadedKey] = v;
    if (ta) {
      ta.style.height = "auto";
      ta.style.height = Math.min(ta.scrollHeight, 150) + "px";
    }
  });

  const desktopTools = $derived(!app.phone);
  const showTools = $derived(desktopTools || toolsOpen);
  const enterSends = $derived(!app.phone || prefs.sendWithReturn);
  const canSend = $derived(text.trim().length > 0);
  const live = $derived(hasFormatting(text));
  const active = $derived(activeFormats(text, Math.min(sel.start, text.length), Math.min(sel.end, text.length)));

  function syncSel() {
    if (ta) sel = { start: ta.selectionStart, end: ta.selectionEnd };
  }

  async function format(kind: FormatKind, url = "") {
    if (!ta) return;
    const r = applyFormat(text, ta.selectionStart, ta.selectionEnd, kind, url);
    text = r.text;
    await tick();
    ta.focus();
    ta.setSelectionRange(r.start, r.end);
    syncSel();
  }

  function openLink() {
    syncSel();
    linkUrl = "";
    linkOpen = !linkOpen;
  }
  function addLink() {
    let u = linkUrl.trim();
    if (!u) return;
    if (!/^https?:\/\//i.test(u)) u = "https://" + u;
    try {
      const parsed = new URL(u);
      if (!/^https?:$/.test(parsed.protocol)) throw new Error();
    } catch {
      toast("That does not look like a web address.", "error");
      return;
    }
    linkOpen = false;
    void format("link", u);
  }

  async function insert(s: string) {
    if (!ta) return;
    const a = ta.selectionStart;
    const b = ta.selectionEnd;
    text = text.slice(0, a) + s + text.slice(b);
    await tick();
    ta.focus();
    ta.setSelectionRange(a + s.length, a + s.length);
    syncSel();
  }

  async function send() {
    const body = text.trim();
    if (!body || busy) return;
    busy = true;
    try {
      if (editing) {
        await core.editMessage(editing.id, body);
        oncancelEdit?.();
      } else {
        // One message: it shows in the thread and, when asked, in the channel's timeline too.
        await core.sendMessage(convId, body, threadRoot, null, !!(threadRoot && alsoChannel && also));
        also = false;
      }
      text = "";
      sel = { start: 0, end: 0 };
      drafts[key] = "";
      await tick();
      ta?.focus();
    } catch (e) {
      fail(e);
    } finally {
      busy = false;
    }
  }

  function onkey(e: KeyboardEvent) {
    if (e.isComposing) return;
    const mod = e.ctrlKey || e.metaKey;
    if (e.key === "Enter" && !e.shiftKey && !mod && enterSends) {
      e.preventDefault();
      void send();
    } else if (e.key === "Enter" && mod) {
      e.preventDefault();
      void send();
    } else if (shortcutFor(e)) {
      e.preventDefault();
      const k = shortcutFor(e)!;
      if (k === "link") openLink();
      else void format(k);
    } else if (e.key === "Escape") {
      if (emojiOpen || linkOpen) emojiOpen = linkOpen = false;
      else if (editing) oncancelEdit?.();
    }
  }

  function oninput() {
    syncSel();
    const now = Date.now();
    if (text && now - lastTyping > 3000 && !editing) {
      lastTyping = now;
      void core.typing(convId).catch(() => {});
    }
  }

  const tools: { kind: FormatKind; icon: string; label: string }[] = [
    { kind: "bold", icon: "bold", label: "Bold" },
    { kind: "italic", icon: "italic", label: "Italic" },
    { kind: "strike", icon: "strike", label: "Strikethrough" },
    { kind: "bullet", icon: "list", label: "Bulleted list" },
    { kind: "numbered", icon: "olist", label: "Numbered list" },
    { kind: "code", icon: "code", label: "Inline code" },
    { kind: "codeblock", icon: "codeblock", label: "Code block" },
    { kind: "quote", icon: "quote", label: "Quote" },
  ];
</script>

<div class="composer">
  {#if editing}
    <div class="editbar"><Icon name="pencil" size={15} /> Editing message <button onclick={oncancelEdit} aria-label="Cancel editing"><Icon name="x" size={16} /></button></div>
  {/if}

  {#if live}
    <div class="preview" aria-label="Formatted preview"><small>Preview</small><Rich body={text} preview /></div>
  {/if}

  {#if alsoChannel && threadRoot}
    <label class="also"><input type="checkbox" bind:checked={also} /> Also send to channel</label>
  {/if}

  {#if showTools && !rec}
    <div class="tools" role="toolbar" aria-label="Formatting">
      {#each tools as t (t.kind)}
        <button class:on={active.has(t.kind)} aria-label={t.label} title={t.label} aria-pressed={active.has(t.kind)} onmousedown={(e) => e.preventDefault()} onclick={() => format(t.kind)}><Icon name={t.icon} size={19} /></button>
        {#if t.kind === "strike" || t.kind === "numbered" || t.kind === "quote"}<span class="sep"></span>{/if}
      {/each}
      <button class:on={linkOpen} aria-label="Link" title="Link" onmousedown={(e) => e.preventDefault()} onclick={openLink}><Icon name="link" size={19} /></button>
    </div>
  {/if}

  {#if linkOpen}
    <form class="linkrow" onsubmit={(e) => { e.preventDefault(); addLink(); }}>
      <Icon name="link" size={17} />
      <!-- svelte-ignore a11y_autofocus -->
      <input autofocus placeholder="https://example.com" bind:value={linkUrl} aria-label="Link address" autocapitalize="off" spellcheck="false" />
      <button type="submit" class="add">Add</button>
      <button type="button" aria-label="Cancel" onclick={() => (linkOpen = false)}><Icon name="x" size={16} /></button>
    </form>
  {/if}

  <div class="row">
    {#if rec}
      <div class="recbar" class:cancelling={rec.cancelling}>
        {#if rec.mode === "locked"}<button class="trash" aria-label="Cancel recording" onclick={cancelRec}><Icon name="trash" size={22} /></button>{/if}
        <span class="reddot"></span><span class="rtime">{fmtTime(app.voice.elapsed)}</span>
        <span class="live" aria-hidden="true">{#each app.voice.levels.slice(-48) as l}<i style="height:{3 + l * 24}px"></i>{/each}</span>
        {#if rec.mode === "hold"}<span class="slide">{rec.cancelling ? "Release to cancel" : "‹ Slide to cancel"}</span>{:else}<span class="slide">Recording…</span>{/if}
      </div>
      {#if rec.mode === "hold"}<span class="lockhint" class:on={rec.lockHint}><Icon name="lock" size={16} /><Icon name="up" size={14} /></span>{/if}
    {:else}
    <button class="ib" aria-label="Attach" onclick={() => (app.sheet = { kind: "attach", conv: convId, thread: threadRoot })}><Icon name="paperclip" size={24} /></button>
    <div class="field">
      <textarea bind:this={ta} bind:value={text} rows="1" {placeholder} aria-label="Message" onkeydown={onkey} oninput={oninput} onselect={syncSel} onkeyup={syncSel} onclick={syncSel}></textarea>
      {#if app.phone}
        <button class="inner" class:on={toolsOpen} aria-label="Formatting toolbar" aria-pressed={toolsOpen} onclick={() => (toolsOpen = !toolsOpen)}><b>Aa</b></button>
      {/if}
      <span class="emojiwrap">
        <button class="inner" aria-label="Emoji" onclick={() => (emojiOpen = !emojiOpen)}><Icon name="smile" size={22} /></button>
        {#if emojiOpen}<EmojiPicker sheet={app.phone} onpick={(e) => void insert(e)} onclose={() => (emojiOpen = false)} />{/if}
      </span>
    </div>
    {/if}
    {#if rec?.mode === "locked"}
      <button class="go" aria-label="Send voice message" onclick={sendRec}><Icon name="send" size={22} /></button>
    {:else if canSend && !rec}
      <button class="go" aria-label={editing ? "Save" : "Send"} onclick={send} disabled={busy}><Icon name={editing ? "check" : "send"} size={22} /></button>
    {:else if !editing}
      <button class="go mic" class:hot={!!rec} aria-label="Record voice message (hold, or tap to record hands-free)" onpointerdown={micStart} onpointermove={micMove} onpointerup={micUp} onpointercancel={micUp} oncontextmenu={(e) => e.preventDefault()}><Icon name="mic" size={24} /></button>
    {:else}
      <span class="go ghost"></span>
    {/if}
  </div>
</div>

<style>
  .composer { background: var(--surface); border-top: 1px solid var(--line); padding: 8px 10px calc(8px + var(--safe-bottom)); display: flex; flex-direction: column; gap: 6px; flex: none; }
  .row { display: flex; align-items: flex-end; gap: 8px; }
  .ib { width: 42px; height: 46px; display: inline-flex; align-items: center; justify-content: center; color: var(--text-2); flex: none; }
  .field { flex: 1; min-width: 0; display: flex; align-items: flex-end; background: var(--surface-2); border: 1px solid var(--line); border-radius: 24px; padding: 0 6px 0 16px; }
  .field:focus-within { border-color: color-mix(in srgb, var(--blue) 55%, var(--line)); }
  textarea { flex: 1; min-width: 0; border: 0; background: transparent; outline: none; resize: none; padding: 12px 0; font-size: 16px; line-height: 1.3; max-height: 150px; }
  .inner { width: 38px; height: 44px; display: inline-flex; align-items: center; justify-content: center; color: var(--text-2); flex: none; }
  .inner b { font-size: 14px; letter-spacing: -0.02em; }
  .inner.on { color: var(--blue); }
  .emojiwrap { position: relative; display: inline-flex; }
  .go { width: 50px; height: 50px; border-radius: 50%; background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; display: inline-flex; align-items: center; justify-content: center; flex: none; box-shadow: 0 3px 10px rgba(25, 39, 63, 0.3); }
  .go:disabled { opacity: 0.6; }
  .go.mic { touch-action: none; user-select: none; }
  .go.hot { transform: scale(1.18); background: #c4261f; }
  .go.ghost { visibility: hidden; }
  .also { display: flex; align-items: center; gap: 8px; font-size: 13.5px; color: var(--text-2); padding: 0 6px; }
  .recbar { position: relative; flex: 1; min-width: 0; height: 50px; display: flex; align-items: center; gap: 10px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 25px; padding: 0 16px 0 10px; }
  .recbar.cancelling { background: color-mix(in srgb, #c4261f 14%, var(--surface-2)); }
  .reddot { width: 11px; height: 11px; border-radius: 50%; background: #d92d20; animation: blink 1s infinite; flex: none; }
  @keyframes blink { 50% { opacity: 0.25; } }
  .rtime { font-variant-numeric: tabular-nums; font-weight: 600; min-width: 38px; }
  .live { flex: 1; min-width: 0; height: 30px; display: flex; align-items: center; justify-content: flex-end; gap: 2px; overflow: hidden; }
  .live i { width: 3px; border-radius: 2px; background: var(--blue); flex: none; min-height: 3px; }
  .slide { color: var(--muted); font-size: 13.5px; white-space: nowrap; }
  .trash { width: 34px; height: 34px; border-radius: 50%; color: var(--danger); display: inline-flex; align-items: center; justify-content: center; margin-left: -4px; }
  .trash:hover { background: var(--hover); }
  .lockhint { position: absolute; right: 12px; bottom: 82px; width: 38px; height: 62px; border-radius: 19px; background: var(--surface); border: 1px solid var(--line); box-shadow: var(--shadow-pop); display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 3px; color: var(--muted); }
  .lockhint.on { color: var(--blue); }
  .composer { position: relative; }
  .tools { display: flex; align-items: center; gap: 2px; padding: 0 4px; overflow-x: auto; scrollbar-width: none; }
  .tools button { width: 38px; height: 36px; border-radius: 9px; display: inline-flex; align-items: center; justify-content: center; color: var(--text); flex: none; }
  .tools button:hover { background: var(--hover); }
  .tools button.on { background: color-mix(in srgb, var(--blue) 16%, var(--surface)); color: var(--blue); }
  .sep { width: 1px; height: 20px; background: var(--line); margin: 0 4px; flex: none; }
  .linkrow { display: flex; align-items: center; gap: 8px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 12px; padding: 4px 8px; color: var(--muted); }
  .linkrow input { flex: 1; min-width: 0; border: 0; background: transparent; outline: none; padding: 7px 4px; font-size: 15px; color: var(--text); }
  .add { color: var(--blue); font-weight: 700; padding: 6px 10px; }
  .preview { background: var(--surface-2); border: 1px dashed var(--line); border-radius: 12px; padding: 6px 12px 8px; max-height: 130px; overflow-y: auto; }
  .preview small { display: block; color: var(--muted); font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; margin-bottom: 2px; }
  .editbar { display: flex; align-items: center; gap: 8px; font-size: 13.5px; color: var(--blue); font-weight: 600; padding: 0 6px; }
  .editbar button { margin-left: auto; color: var(--muted); }
</style>
