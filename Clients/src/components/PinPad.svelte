<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { app, fail, refreshLock, toast } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import Icon from "./Icon.svelte";

  // The 6-digit keypad: a full screen on phones, a centred modal on desktop (desktop/4.webp). Used to set a PIN, to
  // unlock the locked area and to change the PIN. The PIN is checked in the core; it never leaves the device.
  const pad = $derived(app.pad!);
  const LEN = 6;
  let entered = $state("");
  let step = $state<"old" | "enter" | "confirm">("enter");
  let first = $state("");
  let old = $state("");
  let msg = $state("");
  let bad = $state(false);
  let busy = $state(false);
  let wait = $state(0);
  let timer: ReturnType<typeof setInterval> | undefined;

  $effect(() => {
    step = pad.mode === "change" ? "old" : "enter";
    entered = first = old = msg = "";
    // Only a new pad resets this; a refreshed lock status (after a wrong PIN) must not wipe the message.
    untrack(() => startWait(app.lock.wait_secs));
  });

  function startWait(secs: number) {
    clearInterval(timer);
    wait = secs;
    if (secs <= 0) return;
    msg = "";
    timer = setInterval(() => {
      wait -= 1;
      if (wait <= 0) {
        clearInterval(timer);
        wait = 0;
      }
    }, 1000);
  }
  onDestroy(() => clearInterval(timer));

  const prompt = $derived(
    wait > 0
      ? `Too many wrong PINs. Try again in ${Math.floor(wait / 60)}:${String(wait % 60).padStart(2, "0")}.`
      : msg ||
        (pad.mode === "set" ? (step === "confirm" ? "Enter the PIN again to confirm." : "Choose a 6-digit PIN. Locked chats stay hidden until you enter it.") : pad.mode === "change" ? (step === "old" ? "Enter your current PIN." : step === "confirm" ? "Enter the new PIN again." : "Enter your new 6-digit PIN.") : "Enter your PIN to unlock your locked chats."),
  );

  function close() {
    app.pad = null;
  }

  function wrong(r: { wait_secs: number; attempts_left: number }) {
    bad = true;
    setTimeout(() => (bad = false), 450);
    entered = "";
    if (r.wait_secs > 0) startWait(r.wait_secs);
    else msg = `Wrong PIN. ${r.attempts_left} ${r.attempts_left === 1 ? "try" : "tries"} left before a wait.`;
    void refreshLock();
  }

  async function submit() {
    busy = true;
    try {
      const p = entered;
      if (pad.mode === "unlock") {
        const r = await core.lockUnlock(p);
        if (!r.ok) return wrong(r);
        await refreshLock();
        const then = pad.then;
        close();
        await then?.();
      } else if (pad.mode === "set") {
        if (step === "enter") {
          first = p;
          entered = "";
          step = "confirm";
        } else if (p !== first) {
          bad = true;
          setTimeout(() => (bad = false), 450);
          msg = "The PINs do not match. Start again.";
          step = "enter";
          first = entered = "";
        } else {
          await core.lockSetPin(first, p);
          await refreshLock();
          const then = pad.then;
          close();
          toast("PIN set.");
          await then?.();
        }
      } else if (pad.mode === "change") {
        if (step === "old") {
          old = p;
          entered = "";
          step = "enter";
        } else if (step === "enter") {
          first = p;
          entered = "";
          step = "confirm";
        } else if (p !== first) {
          bad = true;
          setTimeout(() => (bad = false), 450);
          msg = "The new PINs do not match. Start again.";
          step = "enter";
          first = entered = "";
        } else {
          const r = await core.lockChangePin(old, first, p);
          if (!r.ok) {
            step = "old";
            first = old = "";
            return wrong(r);
          }
          await refreshLock();
          close();
          toast("PIN changed.");
        }
      }
    } catch (e) {
      entered = "";
      fail(e);
    } finally {
      busy = false;
    }
  }

  function digit(d: string) {
    if (busy || wait > 0 || entered.length >= LEN) return;
    msg = "";
    entered += d;
    if (entered.length === LEN) setTimeout(() => void submit(), 120);
  }
  function back() {
    if (!busy) entered = entered.slice(0, -1);
  }
  function key(e: KeyboardEvent) {
    if (/^[0-9]$/.test(e.key)) digit(e.key);
    else if (e.key === "Backspace") back();
    else if (e.key === "Escape") close();
  }
  async function forgot() {
    if (!window.confirm("Forgot your PIN?\n\nThis signs you out of WorkChat on this device and erases everything stored here: messages, files and the list of locked chats. Your chats stay on the server and appear again when you sign in.")) return;
    try {
      close();
      await core.lockForgot();
    } catch (e) {
      fail(e);
    }
  }
</script>

<svelte:window onkeydown={key} />
{#if !app.phone}<div class="back" role="presentation" onclick={close}></div>{/if}
<div class="pad" class:phone={app.phone} role="dialog" aria-modal="true" aria-label={pad.title ?? "PIN"}>
  <header>
    <h2>{pad.title ?? "Enter your PIN"} <Icon name="lock" size={17} /></h2>
    <button class="x" aria-label="Cancel" onclick={close}><Icon name="x" size={20} /></button>
  </header>
  <div class="main">
    <span class="ring"><Icon name="lock" size={40} stroke={1.7} /></span>
    <p class="msg" class:err={!!msg || wait > 0} aria-live="polite">{prompt}</p>
    <div class="dots" class:bad aria-label={`${entered.length} of ${LEN} digits entered`}>
      {#each Array(LEN) as _, i}<i class:on={i < entered.length}></i>{/each}
    </div>
    <div class="keys">
      {#each ["1", "2", "3", "4", "5", "6", "7", "8", "9"] as d}
        <button class="k" disabled={busy || wait > 0} onclick={() => digit(d)}>{d}</button>
      {/each}
      <button class="k flat" aria-label="Delete" onclick={back}><Icon name="left" size={24} /></button>
      <button class="k" disabled={busy || wait > 0} onclick={() => digit("0")}>0</button>
      <button class="k flat txt" onclick={close}>Cancel</button>
    </div>
    {#if pad.mode === "unlock" || pad.mode === "change"}<button class="forgot" onclick={forgot}>Forgot PIN?</button>{/if}
  </div>
</div>

<style>
  .back { position: fixed; inset: 0; background: rgba(12, 18, 32, 0.5); z-index: 80; }
  .pad { position: fixed; z-index: 81; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(380px, calc(100vw - 32px)); background: var(--surface); border-radius: 16px; border: 1px solid var(--line); box-shadow: var(--shadow-pop); overflow: hidden; display: flex; flex-direction: column; }
  .pad.phone { inset: 0; left: 0; top: 0; transform: none; width: 100%; height: 100%; border-radius: 0; border: 0; padding-top: var(--safe-top); padding-bottom: var(--safe-bottom); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 12px 12px 10px 20px; background: var(--page); border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 17px; display: inline-flex; align-items: center; gap: 8px; color: var(--text-2); }
  .x { width: 34px; height: 34px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; color: var(--muted); }
  .x:hover { background: var(--hover); }
  .main { display: flex; flex-direction: column; align-items: center; padding: 22px 22px 18px; gap: 14px; flex: 1; justify-content: center; }
  .ring { width: 84px; height: 84px; border-radius: 50%; border: 4px solid var(--accent); color: var(--accent); display: inline-flex; align-items: center; justify-content: center; }
  .msg { margin: 0; text-align: center; font-size: 14.5px; color: var(--text-2); min-height: 40px; max-width: 290px; }
  .msg.err { color: var(--danger); font-weight: 600; }
  .dots { display: flex; gap: 16px; padding: 4px 0 6px; }
  .dots i { width: 17px; height: 17px; border-radius: 50%; border: 2px solid var(--accent); }
  .dots i.on { background: var(--accent); }
  .dots.bad { animation: shake 0.4s; }
  .keys { display: grid; grid-template-columns: repeat(3, 70px); gap: 14px 22px; margin-top: 4px; }
  .pad.phone .keys { grid-template-columns: repeat(3, 78px); gap: 18px 28px; }
  .k { height: 62px; border-radius: 50%; background: var(--chip); font-size: 26px; font-weight: 500; display: inline-flex; align-items: center; justify-content: center; color: var(--text); }
  .pad.phone .k { height: 78px; }
  .k:hover:not(:disabled) { background: var(--hover); filter: brightness(0.97); }
  .k:active:not(:disabled) { background: var(--accent); color: #fff; }
  .k:disabled { opacity: 0.4; }
  .k.flat { background: transparent; color: var(--text-2); }
  .k.txt { font-size: 15px; font-weight: 600; }
  .forgot { color: var(--blue); font-weight: 600; font-size: 14px; padding: 6px 10px; }
  @keyframes shake { 20% { transform: translateX(-9px); } 40% { transform: translateX(8px); } 60% { transform: translateX(-6px); } 80% { transform: translateX(4px); } }
</style>
