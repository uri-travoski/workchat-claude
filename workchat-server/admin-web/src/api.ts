// REST client for the admin panel. Types come from the generated schema (api-types.ts).
import type { ErrorBody, LoginResponse, ServerMeta, TokenPair } from "./api-types";

export class ApiError extends Error {
  constructor(public code: string, message: string, public status: number) {
    super(message);
  }
}

const KEY = "workchat-admin-session";

interface Saved { access: string; refresh: string; expires: number; username: string; uid?: string; role?: string }

let saved: Saved | null = null;
try { saved = JSON.parse(sessionStorage.getItem(KEY) || "null"); } catch { saved = null; }

export function signedIn(): boolean { return !!saved; }
export function username(): string { return saved?.username ?? ""; }
export function myId(): string { return saved?.uid ?? ""; }
export function amOwner(): boolean { return saved?.role === "owner"; }

function persist() {
  try {
    if (saved) sessionStorage.setItem(KEY, JSON.stringify(saved)); else sessionStorage.removeItem(KEY);
  } catch { /* private mode */ }
}

export function signOutLocal() { saved = null; persist(); }

export function useLogin(r: LoginResponse) {
  saved = { access: r.access_token, refresh: r.refresh_token, expires: Date.now() + r.expires_in * 1000, username: r.me.user.username, uid: r.me.user.id, role: r.me.user.role };
  persist();
}

async function parse(res: Response): Promise<any> {
  const text = await res.text();
  let body: any = null;
  try { body = text ? JSON.parse(text) : null; } catch { /* not json */ }
  if (!res.ok) {
    const e = (body as ErrorBody | null)?.error;
    throw new ApiError(e?.code ?? "internal", e?.message ?? `HTTP ${res.status}`, res.status);
  }
  return body;
}

let refreshing: Promise<void> | null = null;
async function refresh() {
  if (!saved) throw new ApiError("unauthorized", "signed out", 401);
  refreshing ??= (async () => {
    const res = await fetch("/api/v1/refresh", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ refresh_token: saved!.refresh }) });
    const t: TokenPair = await parse(res);
    saved = { ...saved!, access: t.access_token, expires: Date.now() + t.expires_in * 1000 };
    persist();
  })().finally(() => { refreshing = null; });
  return refreshing;
}

export async function api<T = any>(method: string, path: string, body?: unknown): Promise<T> {
  if (saved && saved.expires - Date.now() < 20000) await refresh();
  const go = () => fetch("/api/v1" + path, {
    method,
    headers: { ...(body !== undefined ? { "Content-Type": "application/json" } : {}), ...(saved ? { Authorization: "Bearer " + saved.access } : {}) },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  let res = await go();
  if (res.status === 401 && saved) {
    try { await refresh(); res = await go(); } catch (e) { signOutLocal(); throw e; }
  }
  return parse(res);
}

export const meta = () => api<ServerMeta>("GET", "/meta");

export async function login(user: string, password: string): Promise<LoginResponse> {
  const res = await fetch("/api/v1/login", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ username: user, password, device_name: "Admin panel", client_version: "1.0.0" }) });
  const r: LoginResponse = await parse(res);
  useLogin(r);
  return r;
}

/** Authenticated download (e.g. backups) returned as a Blob. */
export async function apiBlob(path: string): Promise<Blob> {
  if (saved && saved.expires - Date.now() < 20000) await refresh();
  const res = await fetch("/api/v1" + path, { headers: saved ? { Authorization: "Bearer " + saved.access } : {} });
  if (!res.ok) { await parse(res); }
  return res.blob();
}
