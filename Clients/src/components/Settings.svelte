<script lang="ts">
  import { onMount } from "svelte";
  import { app } from "../lib/app.svelte";
  import { core } from "../lib/core";
  import { fileSize } from "../lib/format";
  import Content from "./settings/Content.svelte";
  import Home from "./settings/Home.svelte";
  import "./settings/st.css";

  // Phone: the list, then one page at a time. Desktop shows Nav (left) and Content (right) from App.svelte.
  let notif = $state("");
  let storage = $state("");
  let sessions = $state("");
  onMount(() => {
    void core.getNotifySettings().then((s) => (notif = !s.enabled ? "Off" : s.dnd ? "DND schedule" : "All ON")).catch(() => {});
    void core.storageInfo().then((i) => (storage = fileSize(i.total) + " used")).catch(() => {});
    void core.listSessions().then((l) => (sessions = `${l.length} active ${l.length === 1 ? "session" : "sessions"}`)).catch(() => {});
  });
</script>

{#if app.spage === ""}<Home {notif} {storage} {sessions} />{:else}<Content />{/if}
