// Press-and-hold gesture for the pinned rows of the phone chat list.
//   1. finger down: wait HOLD_MS. Moving more than SLOP before that is a scroll and the gesture is dropped (the list
//      scrolls as usual).
//   2. after HOLD_MS still inside SLOP: the row is picked up (haptic, lift and shadow).
//   3. a lifted row that moves more than SLOP follows the finger (drag); released without moving it opens the chat's
//      action menu instead.
export const HOLD_MS = 450;
export const SLOP = 9;

export type HoldStep = "wait" | "scroll" | "lift" | "hold" | "drag";

/** What the gesture does now, given the time since the finger went down and how far it has moved. */
export function holdStep(lifted: boolean, elapsedMs: number, dist: number): HoldStep {
  if (!lifted) return dist > SLOP ? "scroll" : elapsedMs >= HOLD_MS ? "lift" : "wait";
  return dist > SLOP ? "drag" : "hold";
}
