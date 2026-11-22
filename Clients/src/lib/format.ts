const pad = (n: number) => String(n).padStart(2, "0");

/** "10:45 AM": upper-case day period like the reference, whatever the locale prints. */
export function clock(ts: number): string {
  return new Date(ts).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }).replace(/\b(am|pm)\b/i, (m) => m.toUpperCase());
}

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** Chat list time: "10:45 AM", "Yesterday", "Monday", "Jul 14". */
export function listTime(ts: number, now = Date.now()): string {
  if (!ts) return "";
  const days = Math.round((startOfDay(new Date(now)) - startOfDay(new Date(ts))) / 86_400_000);
  if (days <= 0) return clock(ts);
  if (days === 1) return "Yesterday";
  if (days < 7) return new Date(ts).toLocaleDateString([], { weekday: "long" });
  const d = new Date(ts);
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  return d.toLocaleDateString([], sameYear ? { month: "short", day: "numeric" } : { month: "short", day: "numeric", year: "numeric" });
}

/** Day separator label in a conversation. */
export function dayLabel(ts: number, now = Date.now()): string {
  const days = Math.round((startOfDay(new Date(now)) - startOfDay(new Date(ts))) / 86_400_000);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  const d = new Date(ts);
  const sameYear = d.getFullYear() === new Date(now).getFullYear();
  return d.toLocaleDateString([], sameYear ? { weekday: "long", month: "short", day: "numeric" } : { weekday: "short", month: "short", day: "numeric", year: "numeric" });
}

export const dayKey = (ts: number) => {
  const d = new Date(ts);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};

export function duration(ms: number): string {
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${pad(s % 60)}`;
}

export function fileSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10 * 1024 ? 1 : 0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

export function lastSeen(ts: number | null | undefined, now = Date.now()): string {
  if (!ts) return "";
  const m = Math.round((now - ts) / 60000);
  if (m < 1) return "Active just now";
  if (m < 60) return `Active ${m} min ago`;
  const days = Math.round((startOfDay(new Date(now)) - startOfDay(new Date(ts))) / 86_400_000);
  if (days <= 0) return `Last seen today at ${clock(ts)}`;
  if (days === 1) return `Last seen yesterday at ${clock(ts)}`;
  return `Last seen ${new Date(ts).toLocaleDateString([], { month: "short", day: "numeric" })}`;
}

export const platformLabel = (os: string): string => ({ android: "Android", ios: "iOS", windows: "Windows", linux: "Linux", macos: "macOS" } as Record<string, string>)[os] ?? os;

export function timeAgo(ms: number): string {
  const s = Math.max(0, Math.round((Date.now() - ms) / 1000));
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}
