/*
 * Minimal fetch client. Every call sends/receives the `__Host-vpspanel_session`
 * cookie (or its dev-friendly variant), so 401 simply means "log in again".
 *
 * Errors thrown by this module are always `ApiError` — pages can match on
 * `.status` for 401/403/404/422 to render targeted messages.
 */

export class ApiError extends Error {
  status: number;
  body: unknown;
  constructor(status: number, message: string, body: unknown) {
    super(message);
    this.status = status;
    this.body = body;
  }
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    credentials: "include",
    headers: body !== undefined ? { "Content-Type": "application/json" } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  const ct = res.headers.get("content-type") ?? "";
  const isJson = ct.includes("application/json");
  const data: unknown = isJson ? await res.json().catch(() => null) : await res.text();
  if (!res.ok) {
    const msg =
      (isJson && data && typeof data === "object" && "error" in (data as object)
        ? String((data as { error: unknown }).error)
        : null) ?? `${res.status} ${res.statusText}`;
    throw new ApiError(res.status, msg, data);
  }
  return data as T;
}

export const api = {
  get:  <T,>(path: string)               => request<T>("GET",    path),
  post: <T,>(path: string, body?: unknown) => request<T>("POST",   path, body),
  put:  <T,>(path: string, body?: unknown) => request<T>("PUT",    path, body),
  del:  <T,>(path: string)               => request<T>("DELETE", path),
};

// ============================================================================
// Backend types — only the shapes the UI consumes.
// ============================================================================

export type Health = {
  status: string;
  name: string;
  version: string;
  db: { kind: string; ping: string };
};

export type PanelUser = {
  id: number;
  username: string;
  role: string;
  is_admin: boolean;
  active: boolean;
  last_login_at: string | null;
  created_at: string;
};

export type Listener = {
  id: number;
  node_id: number | null;
  name: string;
  core: "xray" | "singbox";
  protocol: string;
  transport: string;
  tls_mode: "none" | "tls" | "reality";
  port: number;
  params: Record<string, unknown>;
  enabled: boolean;
  source_listener_id: number | null;
  created_at: string;
  updated_at: string;
};

export type Plan = {
  id: number;
  name: string;
  quota_type: "permanent" | "monthly";
  quota_gb: number;
  quota_reset_day: number;
  duration_days: number | null;
  device_limit: number | null;
  speed_limit_mbps: number | null;
};

export type ProxyUser = {
  id: number;
  name: string;
  uuid: string;
  password: string;
  plan_id: number | null;
  enabled: boolean;
  quota_type: "permanent" | "monthly";
  quota_gb: number;
  used_bytes: number;
  expires_at: string | null;
  subscription_token: string;
  note: string | null;
  tags: string[];
  last_seen_at: string | null;
};

export type Node = {
  id: number;
  name: string;
  addr: string;
  public_host: string | null;
  core: "xray" | "singbox";
  core_version: string | null;
  mgmt_port: number;
  mgmt_secret: string | null;
  ssh_port: number;
  ssh_user: string;
  status: "pending" | "provisioning" | "online" | "offline" | "failed";
  last_seen_at: string | null;
  note: string | null;
  created_at: string;
  updated_at: string;
};

export type Task = {
  id: number;
  node_id: number;
  kind: "apply_config" | "restart" | "check_health";
  status: "pending" | "running" | "success" | "failed";
  log: string;
  error: string | null;
  started_at: string | null;
  finished_at: string | null;
  created_at: string;
};

export type AuditEntry = {
  id: number;
  actor_id: number | null;
  actor_name: string | null;
  method: string;
  path: string;
  status: number;
  ip: string | null;
  user_agent: string | null;
  ts: string;
};

export type Backup = {
  id: number;
  filename: string;
  size_bytes: number;
  kind: "manual" | "auto";
  created_at: string;
};

export type CdnEndpoint = {
  id: number;
  name: string;
  address: string;
  kind: "domain" | "ip";
  enabled: boolean;
  sort_order: number;
  note: string | null;
  created_at: string;
  updated_at: string;
};

export type ChainProxy = {
  id: number;
  name: string;
  proxy_type: "socks5" | "http";
  address: string;
  port: number;
  username: string | null;
  password: string | null;
  enabled: boolean;
  note: string | null;
  created_at: string;
  updated_at: string;
};

export type ChannelType = "telegram" | "webhook" | "smtp";

export type NotificationChannel = {
  id: number;
  name: string;
  type: ChannelType;
  config: Record<string, unknown>;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type NotificationRule = {
  event_type: string;
  channel_ids: number[];
  enabled: boolean;
};

export type UserTraffic = {
  proxy_user_id: number;
  name: string;
  up: number;
  down: number;
  total: number;
  used_bytes: number;
  quota_gb: number;
  enabled: boolean;
};

export type TrafficSummary = {
  users: UserTraffic[];
  grand_total: number;
  last_collected: string | null;
};

export type DailyPoint = { day: string; up: number; down: number };

/** Human byte size with binary units. */
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const kib = n / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  const mib = kib / 1024;
  if (mib < 1024) return `${mib.toFixed(2)} MiB`;
  const gib = mib / 1024;
  if (gib < 1024) return `${gib.toFixed(2)} GiB`;
  return `${(gib / 1024).toFixed(2)} TiB`;
}

/** Compact "x 秒前 / x 分钟前 / x 小时前" helper. Falls back to local datetime. */
export function relativeTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const ms = Date.now() - new Date(iso).getTime();
  if (Number.isNaN(ms)) return iso;
  const s = Math.floor(ms / 1000);
  if (s < 60)        return `${s} 秒前`;
  const m = Math.floor(s / 60);
  if (m < 60)        return `${m} 分钟前`;
  const h = Math.floor(m / 60);
  if (h < 24)        return `${h} 小时前`;
  const d = Math.floor(h / 24);
  if (d < 30)        return `${d} 天前`;
  return new Date(iso).toLocaleDateString();
}
