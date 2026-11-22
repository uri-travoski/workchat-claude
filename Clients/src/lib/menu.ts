export interface MenuItem {
  label: string;
  icon?: string;
  danger?: boolean;
  disabled?: boolean;
  /** A caption row (not clickable). */
  heading?: boolean;
  /** Radio choice: true shows a check, false an empty mark. */
  checked?: boolean;
  run: () => void;
}

/** The lock entry of the phone's top-right menu. "Lock" while the locked area is open (or no PIN exists yet: the first
 *  use then sets one up), "Unlock" while locked chats are hidden behind the PIN. */
export function lockEntry(lock: { pin_set: boolean; open: boolean }): { label: "Lock" | "Unlock"; act: "lock" | "unlock" | "setup" } {
  if (!lock.pin_set) return { label: "Lock", act: "setup" };
  return lock.open ? { label: "Lock", act: "lock" } : { label: "Unlock", act: "unlock" };
}
