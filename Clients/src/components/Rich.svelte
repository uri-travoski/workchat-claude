<script lang="ts">
  import { app } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { parse, type Span } from "../lib/markup";

  let { body, preview = false }: { body: string; preview?: boolean } = $props();
  const blocks = $derived(parse(body));

  const known = $derived(new Set(Object.values(app.users).map((u) => u.username.toLowerCase())));
  const myName = $derived(app.me?.user.username.toLowerCase() ?? "");
  const isGroup = (n: string) => ["all", "channel", "here"].includes(n.toLowerCase());

  function open(e: MouseEvent, url: string) {
    e.preventDefault();
    void core.openUrl(url).catch(() => {});
  }
</script>

{#snippet sp(s: Span)}
  {#if s.link}
    <a href={s.link} class:b={s.bold} class:i={s.italic} class:s={s.strike} onclick={(e) => open(e, s.link!)} rel="noreferrer">{s.text}</a>
  {:else if s.mention}
    <span class="mention" class:me={s.mention.toLowerCase() === myName || isGroup(s.mention)} class:unknown={!known.has(s.mention.toLowerCase()) && !isGroup(s.mention)} class:b={s.bold} class:i={s.italic} class:s={s.strike}>{s.text}</span>
  {:else if s.code}
    <code class:b={s.bold} class:i={s.italic} class:s={s.strike}>{s.text}</code>
  {:else}
    <span class:b={s.bold} class:i={s.italic} class:s={s.strike}>{s.text}</span>
  {/if}
{/snippet}

<div class="rich" class:preview>
  {#each blocks as b, bi (bi)}
    {#if b.t === "p"}
      <p>{#each b.spans as s}{@render sp(s)}{/each}</p>
    {:else if b.t === "quote"}
      <blockquote>{#each b.spans as s}{@render sp(s)}{/each}</blockquote>
    {:else if b.t === "code"}
      <pre><code>{b.text}</code></pre>
    {:else if b.ordered}
      <ol>{#each b.items as it}<li>{#each it as s}{@render sp(s)}{/each}</li>{/each}</ol>
    {:else}
      <ul>{#each b.items as it}<li>{#each it as s}{@render sp(s)}{/each}</li>{/each}</ul>
    {/if}
  {/each}
</div>

<style>
  .rich { font-size: var(--msg-size); font-weight: var(--msg-weight); line-height: 1.38; overflow-wrap: anywhere; word-break: break-word; }
  p { margin: 0; white-space: pre-wrap; }
  p + p, p + ul, p + ol, p + blockquote, p + pre, ul + p, ol + p, blockquote + p, pre + p, ul + ul, ul + blockquote, blockquote + ul { margin-top: 6px; }
  ul, ol { margin: 0; padding-left: 1.3em; }
  li { padding-left: 2px; white-space: pre-wrap; }
  blockquote { margin: 0; padding: 2px 0 2px 10px; border-left: 3px solid currentColor; opacity: 0.85; white-space: pre-wrap; }
  pre { margin: 0; background: rgba(127, 127, 127, 0.16); border-radius: 8px; padding: 8px 10px; overflow-x: auto; font-size: 0.9em; }
  code { font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace; font-size: 0.9em; background: rgba(127, 127, 127, 0.18); padding: 1px 5px; border-radius: 5px; }
  pre code { background: none; padding: 0; white-space: pre; }
  .b { font-weight: 700; }
  .i { font-style: italic; }
  .s { text-decoration: line-through; }
  a { text-decoration: underline; text-underline-offset: 2px; color: inherit; }
  .mention { font-weight: 600; background: rgba(127, 127, 127, 0.18); border-radius: 5px; padding: 0 3px; }
  .mention.me { background: color-mix(in srgb, #f5b73b 38%, transparent); }
  :global(.bubble.in) a { color: var(--blue); }
</style>
