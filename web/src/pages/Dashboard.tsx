import { useCallback, useEffect, useState } from "react";
import { api, Health, Listener, Node, Plan, ProxyUser, Task } from "../lib/api";

// ============================================================================
// Types
// ============================================================================
type TrafficSummary = {
  total_up_bytes:   number;
  total_down_bytes: number;
  users_active:     number;
  nodes_polled:     number;
};

type DashData = {
  health:    Health;
  nodes:     Node[];
  listeners: Listener[];
  users:     ProxyUser[];
  plans:     Plan[];
  tasks:     Task[];
  traffic:   TrafficSummary | null;
};

// ============================================================================
// Helpers
// ============================================================================
function fmtBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  const k = b / 1024;
  if (k < 1024) return `${k.toFixed(1)} KB`;
  const m = k / 1024;
  if (m < 1024) return `${m.toFixed(2)} MB`;
  const g = m / 1024;
  if (g < 1024) return `${g.toFixed(2)} GB`;
  return `${(g / 1024).toFixed(2)} TB`;
}

function relTime(iso: string | null): string {
  if (!iso) return "—";
  const diff = (Date.now() - new Date(iso).getTime()) / 1000;
  if (diff < 60) return `${Math.floor(diff)} 秒前`;
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

export function badgeClass(s: string): string {
  if (s === "success" || s === "online")               return "badge-ok";
  if (s === "running" || s === "provisioning")         return "badge-running";
  if (s === "failed"  || s === "offline")              return "badge-err";
  if (s === "pending")                                 return "badge-warn";
  return "";
}

export function fmt(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleString("zh-CN", { hour12: false });
}

// ============================================================================
// Arc gauge  (SVG circle dash — same technique as 3X-UI)
// ============================================================================
function ArcGauge({
  pct, label, sub, color = "var(--accent)",
}: {
  pct: number; label: string; sub?: string; color?: string;
}) {
  const r = 44;
  const circ = 2 * Math.PI * r;
  // 270° arc: starts at 225° (bottom-left), sweeps 270°
  const arcLen  = circ * 0.75;
  const offset  = circ - (arcLen * Math.min(pct, 100) / 100);
  const trackO  = circ - arcLen;

  return (
    <div className="flex flex-col items-center gap-2">
      <div style={{ position: "relative", width: 110, height: 110 }}>
        <svg width="110" height="110" viewBox="0 0 100 100">
          {/* track */}
          <circle
            cx="50" cy="50" r={r}
            fill="none"
            stroke="var(--border)"
            strokeWidth="8"
            strokeDasharray={`${arcLen} ${circ - arcLen}`}
            strokeDashoffset={-(circ * 0.625 - arcLen / 2)}
            strokeLinecap="round"
            transform="rotate(-225 50 50)"
          />
          {/* fill */}
          <circle
            cx="50" cy="50" r={r}
            fill="none"
            stroke={color}
            strokeWidth="8"
            strokeDasharray={`${arcLen - offset} ${circ - (arcLen - offset)}`}
            strokeDashoffset={-(circ * 0.625 - arcLen / 2)}
            strokeLinecap="round"
            transform="rotate(-225 50 50)"
            style={{ transition: "stroke-dasharray 600ms ease" }}
          />
        </svg>
        {/* center text */}
        <div style={{
          position: "absolute", inset: 0,
          display: "flex", flexDirection: "column",
          alignItems: "center", justifyContent: "center",
        }}>
          <span style={{ fontSize: 17, fontWeight: 700, letterSpacing: "-0.02em" }}>
            {pct.toFixed(pct < 10 ? 2 : 1)}%
          </span>
        </div>
      </div>
      <div className="text-center">
        <div className="text-sm font-semibold">{label}</div>
        {sub && <div className="text-xs" style={{ color: "var(--fg-muted)" }}>{sub}</div>}
      </div>
    </div>
  );
}

// ============================================================================
// Node status card (like the Xray card in 3X-UI)
// ============================================================================
function NodeCard({ node, onApply, onRestart, onProvision }: {
  node: Node;
  onApply:     (id: number) => void;
  onRestart:   (id: number) => void;
  onProvision: (id: number) => void;
}) {
  const online = node.status === "online";
  const coreName = node.core === "singbox" ? "sing-box" : "Xray";
  return (
    <div className="card p-4">
      <div className="flex items-start justify-between mb-3">
        <div>
          <div className="font-semibold text-base">{node.name}</div>
          <div className="text-xs mt-0.5" style={{ color: "var(--fg-muted)" }}>
            {coreName} · {node.addr}
          </div>
        </div>
        <div className="flex items-center gap-1.5">
          <span
            style={{
              width: 8, height: 8, borderRadius: "50%", display: "inline-block",
              background: online ? "#22c55e" : "#94a3b8",
              boxShadow: online ? "0 0 0 2px #bbf7d0" : undefined,
            }}
          />
          <span className="text-xs font-medium" style={{ color: online ? "#16a34a" : "var(--fg-muted)" }}>
            {online ? "在线" : node.status === "offline" ? "离线" : node.status}
          </span>
        </div>
      </div>

      {/* action row */}
      <div className="flex gap-1.5 pt-2 flex-wrap" style={{ borderTop: "1px solid var(--border)" }}>
        <button
          className="btn btn-sm flex-1"
          onClick={() => onProvision(node.id)}
          title="安装内核 + 部署配置 + 启动服务"
          style={{ background: "#7c3aed", color: "#fff", minWidth: 80 }}
        >🚀 初始化</button>
        <button className="btn btn-ghost btn-sm flex-1" onClick={() => onRestart(node.id)} style={{ minWidth: 60 }}>
          <svg width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 7A5 5 0 1 1 7 2h2M9 2l2 0 0 2"/>
          </svg>
          重启
        </button>
        <button className="btn btn-ghost btn-sm flex-1" onClick={() => onApply(node.id)} style={{ minWidth: 80 }}>
          <svg width="10" height="10" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
            <path d="M2 7h10M8 3l4 4-4 4"/>
          </svg>
          同步配置
        </button>
      </div>

      {/* meta */}
      <div className="flex justify-between mt-3 text-xs" style={{ color: "var(--fg-muted)" }}>
        <span>最近在线: {relTime(node.last_seen_at)}</span>
        <span>{node.ssh_auth_method === "global" ? "🌐 全局密钥" : node.ssh_auth_method === "password" ? "🔑 密码" : "🔑 私钥"}</span>
      </div>
    </div>
  );
}

// ============================================================================
// Stat info tile  (used in the bottom grid)
// ============================================================================
function InfoTile({ icon, label, value, sub }: {
  icon: React.ReactNode; label: string; value: string; sub?: string;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
      <div className="text-xs font-medium" style={{ color: "var(--fg-muted)", textTransform: "uppercase", letterSpacing: "0.06em" }}>{label}</div>
      <div className="flex items-center gap-1.5">
        <span style={{ color: "var(--accent)" }}>{icon}</span>
        <span className="text-base font-semibold">{value}</span>
      </div>
      {sub && <div className="text-xs" style={{ color: "var(--fg-muted)" }}>{sub}</div>}
    </div>
  );
}

// ============================================================================
// Main Dashboard
// ============================================================================
export function Dashboard() {
  const [data, setData] = useState<DashData | null>(null);
  const [err,  setErr]  = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [health, nodes, listeners, users, plans, tasks, traffic] = await Promise.all([
        api.get<Health>("/api/healthz"),
        api.get<Node[]>("/api/nodes"),
        api.get<Listener[]>("/api/listeners"),
        api.get<ProxyUser[]>("/api/proxy-users"),
        api.get<Plan[]>("/api/plans"),
        api.get<Task[]>("/api/tasks?limit=8"),
        api.get<TrafficSummary>("/api/traffic").catch(() => null),
      ]);
      setData({ health, nodes, listeners, users, plans, tasks, traffic });
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  // auto-refresh every 30 s
  useEffect(() => {
    const t = setInterval(load, 30_000);
    return () => clearInterval(t);
  }, [load]);

  async function handleApply(nodeId: number) {
    try {
      const r = await api.post<{ task_id: number }>(`/api/nodes/${nodeId}/apply`);
      showToast(`已入队同步任务 #${r.task_id}`);
      setTimeout(load, 1500);
    } catch (e) { showToast(String(e)); }
  }

  async function handleRestart(nodeId: number) {
    try {
      const r = await api.post<{ task_id: number }>(`/api/nodes/${nodeId}/restart`);
      showToast(`已入队重启任务 #${r.task_id}`);
      setTimeout(load, 1500);
    } catch (e) { showToast(String(e)); }
  }

  async function handleProvision(nodeId: number) {
    try {
      const r = await api.post<{ task_id: number }>(`/api/nodes/${nodeId}/provision`);
      showToast(`🚀 已入队初始化任务 #${r.task_id} — 在「任务」页可看实时日志`);
      setTimeout(load, 2000);
    } catch (e) { showToast(String(e)); }
  }

  function showToast(msg: string) {
    setToast(msg);
    setTimeout(() => setToast(null), 4000);
  }

  // ---- derived stats ----
  const nodes      = data?.nodes     ?? [];
  const listeners  = data?.listeners ?? [];
  const users      = data?.users     ?? [];
  const tasks      = data?.tasks     ?? [];
  const traffic    = data?.traffic;

  const onlineNodes    = nodes.filter(n => n.status === "online").length;
  const enabledListeners = listeners.filter(l => l.enabled).length;
  const activeUsers    = users.filter(u => u.enabled).length;
  const totalTrafficBytes = (traffic?.total_up_bytes ?? 0) + (traffic?.total_down_bytes ?? 0);

  // gauge: use ratio as percent (nodes online / total, etc.)
  const nodesPct  = nodes.length     ? (onlineNodes / nodes.length) * 100       : 0;
  const listPct   = listeners.length ? (enabledListeners / listeners.length) * 100 : 0;
  const userPct   = users.length     ? (activeUsers / users.length) * 100        : 0;
  // traffic gauge: rough "fullness" — cap display at some reference (e.g. 1 TB total)
  const REF_TB    = 1024 * 1024 * 1024 * 1024;
  const trafficPct = Math.min((totalTrafficBytes / REF_TB) * 100, 100);

  return (
    <div className="space-y-5">
      {/* header */}
      <header className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold">仪表盘</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>系统运行状态</p>
        </div>
        <button className="btn btn-ghost btn-sm" onClick={load} title="刷新">⟳ 刷新</button>
      </header>

      {err   && <div className="card p-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      {toast && <div className="card p-3 text-sm" style={{ color: "var(--accent)" }}>{toast}</div>}

      {/* ---- 4 arc gauges ---- */}
      <div className="card p-5">
        <div className="grid grid-cols-2 md:grid-cols-4 gap-6 justify-items-center">
          <ArcGauge
            pct={nodesPct}
            label="VPS 节点"
            sub={`${onlineNodes} / ${nodes.length} 在线`}
            color="#3b82f6"
          />
          <ArcGauge
            pct={listPct}
            label="监听器"
            sub={`${enabledListeners} / ${listeners.length} 启用`}
            color="var(--accent)"
          />
          <ArcGauge
            pct={userPct}
            label="代理用户"
            sub={`${activeUsers} / ${users.length} 活跃`}
            color="#10b981"
          />
          <ArcGauge
            pct={trafficPct}
            label="累计流量"
            sub={fmtBytes(totalTrafficBytes)}
            color="#f59e0b"
          />
        </div>
      </div>

      {/* ---- Node status cards + quick stats ---- */}
      <div className="grid gap-4" style={{ gridTemplateColumns: nodes.length > 0 ? "1fr 280px" : "1fr" }}>
        {/* node cards */}
        <div className="space-y-3">
          <div className="text-sm font-semibold" style={{ color: "var(--fg-muted)" }}>VPS 节点状态</div>
          {nodes.length === 0 ? (
            <div className="card p-6 text-center text-sm" style={{ color: "var(--fg-muted)" }}>
              还没有节点 — 前往「VPS 管理」添加。
            </div>
          ) : (
            <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(260px,1fr))" }}>
              {nodes.map(n => (
                <NodeCard key={n.id} node={n} onApply={handleApply} onRestart={handleRestart} onProvision={handleProvision} />
              ))}
            </div>
          )}
        </div>

        {/* quick stats column */}
        <div className="space-y-3">
          {/* panel info card */}
          <div className="card p-4 space-y-3">
            <div className="text-sm font-semibold" style={{ color: "var(--fg-muted)" }}>面板概况</div>
            <div className="grid grid-cols-2 gap-4">
              <InfoTile
                icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><rect x="2" y="2" width="10" height="10" rx="2"/><path d="M5 7h4M7 5v4"/></svg>}
                label="套餐"
                value={String(data?.plans.length ?? "—")}
              />
              <InfoTile
                icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><path d="M7 2a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM3 12c0-2 1.8-4 4-4s4 2 4 4"/></svg>}
                label="管理员"
                value={data?.health.status === "ok" ? "在线" : "—"}
              />
              <InfoTile
                icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><circle cx="7" cy="7" r="5"/><path d="M7 4v3l2 2"/></svg>}
                label="数据库"
                value={data?.health.db.kind ?? "—"}
                sub={data?.health.db.ping}
              />
              <InfoTile
                icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><path d="M2 4h10M2 7h10M2 10h10"/></svg>}
                label="任务"
                value={String(tasks.filter(t => t.status === "running" || t.status === "pending").length)}
                sub="运行中/等待"
              />
            </div>
          </div>

          {/* traffic card */}
          <div className="card p-4 space-y-3">
            <div className="text-sm font-semibold" style={{ color: "var(--fg-muted)" }}>流量统计</div>
            <div className="grid grid-cols-2 gap-4">
              <InfoTile
                icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><path d="M7 12V2M3 6l4-4 4 4"/></svg>}
                label="已发送"
                value={fmtBytes(traffic?.total_up_bytes ?? 0)}
              />
              <InfoTile
                icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><path d="M7 2v10M3 8l4 4 4-4"/></svg>}
                label="已接收"
                value={fmtBytes(traffic?.total_down_bytes ?? 0)}
              />
              <InfoTile
                icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><circle cx="7" cy="7" r="5"/><path d="M5 9V5l4 4H5"/></svg>}
                label="已采集节点"
                value={String(traffic?.nodes_polled ?? 0)}
              />
              <InfoTile
                icon={<svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"><path d="M2 7c0-3 2-5 5-5s5 2 5 5-2 5-5 5-5-2-5-5z"/><path d="M9 7H5M7 5v4"/></svg>}
                label="活跃用户"
                value={String(traffic?.users_active ?? activeUsers)}
              />
            </div>
          </div>
        </div>
      </div>

      {/* ---- Recent tasks ---- */}
      <div className="card overflow-hidden">
        <div className="px-4 py-3 flex items-center justify-between" style={{ borderBottom: "1.5px solid var(--border)" }}>
          <span className="font-semibold text-sm">最近任务</span>
          <a href="/tasks" style={{ color: "var(--accent)", fontSize: 12 }}>查看全部 →</a>
        </div>
        {tasks.length === 0 ? (
          <div className="p-6 text-center text-sm" style={{ color: "var(--fg-muted)" }}>还没有任务</div>
        ) : (
          <table className="table">
            <thead>
              <tr>
                <th>ID</th><th>类型</th><th>节点</th><th>状态</th><th>创建时间</th>
              </tr>
            </thead>
            <tbody>
              {tasks.map(t => (
                <tr key={t.id}>
                  <td className="font-mono text-xs">{t.id}</td>
                  <td>
                    <span className="text-xs font-medium">
                      {t.kind === "apply_config" ? "同步配置" : t.kind === "restart" ? "重启" : t.kind}
                    </span>
                  </td>
                  <td className="text-xs" style={{ color: "var(--fg-muted)" }}>#{t.node_id}</td>
                  <td>
                    <span className={`badge ${badgeClass(t.status)}`}>
                      {t.status === "success" ? "成功" : t.status === "failed" ? "失败" :
                       t.status === "running" ? "运行中" : t.status === "pending" ? "等待" : t.status}
                    </span>
                  </td>
                  <td className="text-xs" style={{ color: "var(--fg-muted)" }}>{fmt(t.created_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
