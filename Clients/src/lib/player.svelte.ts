// Audio playback control for the UI. The core decodes and plays (see src-tauri/src/player.rs); this only sends
// commands and reads `app.audio`, which core events keep current.
import { app, fail } from "./app.svelte";
import { core } from "./core";

export const SPEEDS = [1, 1.5, 2] as const;

export const isCurrent = (id: string) => app.audio.id === id && app.audio.state !== "stopped" && app.audio.state !== "error";
export const isPlaying = (id: string) => app.audio.id === id && (app.audio.state === "playing" || app.audio.state === "loading");

export async function togglePlay(id: string) {
  try {
    if (isPlaying(id)) await core.audioCtl("pause");
    else if (isCurrent(id) && app.audio.state === "paused") await core.audioCtl("resume");
    else {
      app.audio.id = id;
      app.audio.state = "loading";
      app.audio.pos = 0;
      await core.audioPlay(id, app.audio.speed || 1, 0);
    }
  } catch (e) {
    fail(e);
  }
}

export async function seek(id: string, ms: number, durMs: number) {
  try {
    if (isCurrent(id) && app.audio.state !== "ended") await core.audioCtl("seek", ms);
    else {
      app.audio.id = id;
      app.audio.state = "loading";
      app.audio.dur = durMs;
      await core.audioPlay(id, app.audio.speed || 1, ms);
    }
  } catch (e) {
    fail(e);
  }
}

export async function cycleSpeed() {
  const i = SPEEDS.indexOf(app.audio.speed as (typeof SPEEDS)[number]);
  const next = SPEEDS[(i + 1) % SPEEDS.length];
  app.audio.speed = next;
  try {
    await core.audioCtl("speed", next);
  } catch (e) {
    fail(e);
  }
}

export function stopAudio() {
  void core.audioCtl("stop").catch(() => {});
}
