<script lang="ts">
  import { app } from "../lib/app.svelte";
  import AttachSheet from "./AttachSheet.svelte";
  import ChatActionsMenu from "./ChatActionsMenu.svelte";
  import ChatInfoSheet from "./ChatInfoSheet.svelte";
  import EditProfileSheet from "./EditProfileSheet.svelte";
  import LockedChatsSheet from "./LockedChatsSheet.svelte";
  import ShareCardSheet from "./ShareCardSheet.svelte";
  import CropSheet from "./CropSheet.svelte";
  import EmojiSheet from "./EmojiSheet.svelte";
  import ForwardSheet from "./ForwardSheet.svelte";
  import ShareToSheet from "./ShareToSheet.svelte";
  import InfoSheet from "./InfoSheet.svelte";
  import MessageMenu from "./MessageMenu.svelte";
  import NewChatSheet from "./NewChatSheet.svelte";
  import PinsSheet from "./PinsSheet.svelte";
  import PreviewSheet from "./PreviewSheet.svelte";
  import ProfileSheet from "./ProfileSheet.svelte";
  import Viewer from "./Viewer.svelte";
  import OpenFailDialog from "./OpenFailDialog.svelte";

  const s = $derived(app.sheet);
</script>

{#if s}
  {#key s.kind + (s.kind === "profile" ? s.userId : s.kind === "crop" ? s.stagedId : s.kind === "newchat" ? (s.step ?? "") : "")}
    {#if s.kind === "attach"}<AttachSheet conv={s.conv} thread={s.thread} />
    {:else if s.kind === "preview"}<PreviewSheet sheet={s} />
    {:else if s.kind === "forward"}<ForwardSheet msg={s.msg} />
    {:else if s.kind === "shareto"}<ShareToSheet files={s.files} text={s.text} />
    {:else if s.kind === "newchat"}<NewChatSheet step={s.step} />
    {:else if s.kind === "profile"}<ProfileSheet userId={s.userId} />
    {:else if s.kind === "crop"}<CropSheet sheet={s} />
    {:else if s.kind === "pins"}<PinsSheet conv={s.conv} />
    {:else if s.kind === "info"}<InfoSheet msg={s.msg} />
    {:else if s.kind === "msgmenu"}<MessageMenu msg={s.msg} x={s.x} y={s.y} inThread={s.inThread} onedit={s.onedit} />
    {:else if s.kind === "emoji"}<EmojiSheet msg={s.msg} />
    {:else if s.kind === "lockedchats"}<LockedChatsSheet />
    {:else if s.kind === "editprofile"}<EditProfileSheet />
    {:else if s.kind === "sharecard"}<ShareCardSheet />
    {:else if s.kind === "chatactions"}<ChatActionsMenu conv={s.conv} x={s.x} y={s.y} />
    {:else if s.kind === "channelinfo" || s.kind === "chatmenu"}<ChatInfoSheet conv={s.conv} />
    {/if}
  {/key}
{/if}
<Viewer />
<OpenFailDialog />
