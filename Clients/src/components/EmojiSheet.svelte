<script lang="ts">
  import { app, closeSheet, fail } from "../lib/app.svelte";
  import { core, type MessageView } from "../lib/core";
  import EmojiPicker from "./EmojiPicker.svelte";
  import Sheet from "./Sheet.svelte";

  let { msg }: { msg: MessageView } = $props();
  async function pick(e: string) {
    closeSheet();
    const had = msg.reactions.find((r) => r.emoji === e)?.user_ids.includes(app.me!.user.id) ?? false;
    try {
      await core.react(msg.id, e, !had);
    } catch (err) {
      fail(err);
    }
  }
</script>

<Sheet title="React" onclose={closeSheet} plain>
  <EmojiPicker inline onpick={pick} onclose={closeSheet} />
</Sheet>
