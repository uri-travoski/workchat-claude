<script lang="ts">
  import { afterLogin, app } from "../lib/app.svelte";
  import { core, errorCode, errorMessage, type ServerCheck } from "../lib/core";
  import Icon from "./Icon.svelte";

  const KEY = "workchat.server";
  const saved = (() => {
    try {
      return localStorage.getItem(KEY) ?? "";
    } catch {
      return "";
    }
  })();

  let mode = $state<"password" | "link">("password");
  let server = $state(saved || app.server);
  let username = $state("");
  let password = $state("");
  let link = $state("");
  let busy = $state(false);
  let error = $state("");
  let check = $state<ServerCheck | null>(null);
  let checking = $state(false);
  let showPw = $state(false);

  $effect(() => {
    if (app.pendingLink) {
      mode = "link";
      link = app.pendingLink;
      app.pendingLink = "";
    }
  });

  // A pasted link that names the server needs no server field.
  const linkHasServer = $derived(/^(https?|workchat):\/\//i.test(link.trim()));

  async function verify() {
    check = null;
    if (!server.trim()) return;
    checking = true;
    try {
      check = await core.checkServer(server);
      error = "";
    } catch (e) {
      error = errorMessage(e);
    } finally {
      checking = false;
    }
  }

  function friendly(e: unknown): string {
    switch (errorCode(e)) {
      case "invalid_credentials": return "Wrong username or password.";
      case "locked_out": return "Too many failed attempts. Wait a few minutes and try again.";
      case "client_too_old": return "This version of WorkChat is too old for that server. Please update the app.";
      case "not_found": return "This login link was not found. Ask your admin for a new one.";
      case "validation": return errorMessage(e);
      default: return errorMessage(e);
    }
  }

  async function submit(ev: Event) {
    ev.preventDefault();
    if (busy) return;
    busy = true;
    error = "";
    try {
      if (mode === "password") await core.loginPassword(server, username, password);
      else await core.loginLink(link, linkHasServer ? undefined : server);
      try {
        localStorage.setItem(KEY, (check?.url ?? server).trim());
      } catch {
        /* optional */
      }
      password = "";
      await afterLogin();
    } catch (e) {
      error = friendly(e);
    } finally {
      busy = false;
    }
  }
</script>

<main>
  <form class="card" onsubmit={submit} novalidate>
    <img class="logo" src="/app-icon.svg" alt="" width="72" height="72" />
    <h1>WorkChat</h1>
    <p class="sub">Sign in to your team's server</p>

    {#if app.signedOutNote}
      <div class="note" role="alert"><Icon name="alert" size={18} /><span>{app.signedOutNote}</span></div>
    {/if}

    <div class="seg" role="tablist">
      <button type="button" role="tab" aria-selected={mode === "password"} class:on={mode === "password"} onclick={() => { mode = "password"; error = ""; }}>Password</button>
      <button type="button" role="tab" aria-selected={mode === "link"} class:on={mode === "link"} onclick={() => { mode = "link"; error = ""; }}>Login link</button>
    </div>

    {#if mode === "link"}
      <label>
        <span>Login link</span>
        <textarea rows="3" placeholder="https://chat.example.com/l/…" bind:value={link} autocomplete="off" autocapitalize="off" spellcheck="false"></textarea>
      </label>
    {/if}

    {#if mode === "password" || !linkHasServer}
      <label>
        <span>Server address</span>
        <input placeholder="chat.example.com or 192.168.1.20:8080" bind:value={server} onblur={verify} autocomplete="url" autocapitalize="off" autocorrect="off" spellcheck="false" inputmode="url" />
        {#if checking}<small class="hint">Checking…</small>{:else if check}<small class="hint ok"><Icon name="check" size={14} stroke={2.4} /> {check.name} · {check.url}</small>{:else}<small class="hint">https is required, except on your local network.</small>{/if}
      </label>
    {/if}

    {#if mode === "password"}
      <label>
        <span>Username</span>
        <input bind:value={username} autocomplete="username" autocapitalize="off" autocorrect="off" spellcheck="false" />
      </label>
      <label>
        <span>Password</span>
        <span class="pw">
          <input type={showPw ? "text" : "password"} bind:value={password} autocomplete="current-password" />
          <button type="button" class="eye" aria-label={showPw ? "Hide password" : "Show password"} onclick={() => (showPw = !showPw)}>{showPw ? "Hide" : "Show"}</button>
        </span>
      </label>
    {/if}

    {#if error}<div class="err" role="alert"><Icon name="alert" size={18} /><span>{error}</span></div>{/if}

    <button class="go" type="submit" disabled={busy || (mode === "password" ? !server.trim() || !username.trim() || !password : !link.trim() || (!linkHasServer && !server.trim()))}>
      {busy ? "Signing in…" : mode === "password" ? "Sign in" : "Use login link"}
    </button>
    <p class="foot">Accounts are created by your server's admin. Ask them for a password or a one-time login link.</p>
  </form>
</main>

<style>
  main { height: 100%; display: flex; align-items: center; justify-content: center; padding: calc(var(--safe-top) + 16px) 16px calc(var(--safe-bottom) + 16px); overflow-y: auto; background: var(--page); }
  .card { width: 100%; max-width: 420px; background: var(--surface); border-radius: 20px; padding: 28px 24px 22px; box-shadow: var(--shadow-pop); display: flex; flex-direction: column; gap: 14px; margin: auto; }
  .logo { align-self: center; border-radius: 18px; }
  h1 { margin: 0; text-align: center; font-size: 26px; letter-spacing: -0.02em; }
  .sub { margin: -8px 0 4px; text-align: center; color: var(--muted); }
  .seg { display: flex; background: var(--chip); border-radius: 12px; padding: 3px; }
  .seg button { flex: 1; padding: 9px; border-radius: 9px; font-weight: 600; font-size: 14px; color: var(--muted); }
  .seg button.on { background: var(--surface); color: var(--text); box-shadow: var(--shadow); }
  label { display: flex; flex-direction: column; gap: 6px; }
  label > span:first-child { font-size: 13px; font-weight: 600; color: var(--text-2); }
  input, textarea { width: 100%; background: var(--surface-2); border: 1px solid var(--line); border-radius: 12px; padding: 12px 14px; font-size: 16px; outline: none; resize: none; }
  input:focus, textarea:focus { border-color: var(--blue); box-shadow: 0 0 0 3px color-mix(in srgb, var(--blue) 18%, transparent); }
  .pw { position: relative; display: block; }
  .pw input { padding-right: 64px; }
  .eye { position: absolute; right: 12px; top: 50%; transform: translateY(-50%); font-size: 13px; font-weight: 600; color: var(--blue); }
  .hint { color: var(--muted); font-size: 12.5px; display: inline-flex; align-items: center; gap: 5px; }
  .hint.ok { color: #0c8f67; }
  .err, .note { display: flex; gap: 10px; align-items: flex-start; padding: 11px 13px; border-radius: 12px; font-size: 14px; }
  .err { background: color-mix(in srgb, var(--danger) 11%, var(--surface)); color: var(--danger); }
  .note { background: color-mix(in srgb, var(--blue) 12%, var(--surface)); color: var(--text); }
  .go { background: linear-gradient(145deg, var(--accent), var(--accent-2)); color: #fff; border-radius: 14px; padding: 14px; font-size: 16px; font-weight: 700; margin-top: 4px; }
  .go:disabled { opacity: 0.5; cursor: default; }
  .foot { margin: 0; text-align: center; font-size: 12.5px; color: var(--muted); }
</style>
