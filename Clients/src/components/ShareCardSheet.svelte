<script lang="ts">
  import { app, closeSheet, fail, toast } from "../lib/app.svelte";
  import { qrSvg, vcard } from "../lib/qr";
  import Icon from "./Icon.svelte";
  import Sheet from "./Sheet.svelte";

  const me = $derived(app.me!.user);
  const card = $derived(vcard({ name: me.display_name, title: me.title, phone: me.phone, username: me.username, server: app.server }));
  const text = $derived([me.display_name, me.title, me.phone, `@${me.username} on ${app.server}`].filter(Boolean).join("\n"));
  const qr = $derived(qrSvg(card, 232));

  async function copy(what: string, label: string) {
    try {
      await navigator.clipboard.writeText(what);
      toast(`${label} copied.`);
    } catch (e) {
      fail(e);
    }
  }
</script>

<Sheet title="Share Card" onclose={closeSheet}>
  <div class="c">
    <img src={qr} alt="QR code with {me.display_name}'s contact card" width="232" height="232" />
    <h3>{me.display_name}</h3>
    {#if me.title}<p class="t">{me.title}</p>{/if}
    {#if me.phone}<p class="l"><Icon name="phone" size={15} /> {me.phone}</p>{/if}
    <p class="l"><Icon name="at" size={15} /> {me.username}</p>
    <p class="st-note">Scan with a phone camera to add the contact. The code holds name, title and phone as a standard vCard.</p>
    <div class="st-btns">
      <button class="st-btn pri" onclick={() => copy(text, "Contact details")}><Icon name="copy" size={16} /> Copy details</button>
      <button class="st-btn" onclick={() => copy(card, "vCard")}>Copy vCard</button>
    </div>
  </div>
</Sheet>

<style>
  .c { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 6px; }
  img { border-radius: 14px; border: 1px solid var(--line); }
  h3 { margin: 10px 0 0; font-size: 19px; }
  .t { margin: 0; color: var(--blue); font-weight: 600; }
  .l { margin: 0; display: inline-flex; align-items: center; gap: 6px; color: var(--text-2); }
  .c :global(.st-note) { margin: 10px 4px; }
</style>
