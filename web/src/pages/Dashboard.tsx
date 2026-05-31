import { useEffect, useState } from "react";
import { api, Health, Listener, Node, ProxyUser, Task } from "../lib/api";

type Counts = {
  listeners: number;
  proxyUsers: number;
  nodes: number;
  recentTasks: Task[];
};

export function Dashboard() {
  const [health, setHealth] = useState<Health | null>(null);
  const [counts, setCounts] = useState<Counts | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [h, listeners, users, nodes, tasks] = await Promise.all([
          api.get<Health>("/api/healthz"),
          api.get<Listener[]>("/api/listeners"),
          api.get<ProxyUser[]>("/api/proxy-users"),
          api.get<Node[]>("/api/nodes"),
          api.get<Task[]>("/api/tasks?limit=5"),
        ]);
        if (cancelled) return;
        setHealth(h);
        setCounts({
          listeners:   listeners.length,
          proxyUsers:  users.length,
          nodes:       nodes.length,
          recentTasks: tasks,
        });
      } catch (e) {
        if (!cancelled) setErr(String(e));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div>
      <header className="mb-6">
        <h1 className="text-2xl font-semibold">仪表盘</h1>
        <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
          系统运行状态总览
        </p>
      </header>

      {err && (
        <div className="card p-4 mb-4" style={{ color: "#b91c1c" }}>
          {err}
        </div>
      )}

      <section className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6">
        <StatCard label="VPS 节点"  value={counts?.nodes ?? "—"} emoji="🖥️" />
        <StatCard label="监听器"    value={counts?.listeners ?? "—"} emoji="📡" />
        <StatCard label="代理用户"  value={counts?.proxyUsers ?? "—"} emoji="👥" />
        <StatCard
          label="数据库"
          value={health ? `${health.db.kind} · ${health.db.ping}` : "—"}
          emoji="💾"
        />
      </section>

      <section className="card p-4">
        <h2 className="font-semibold mb-3">最近任务</h2>
        {counts?.recentTasks.length ? (
          <table className="table">
            <thead>
              <tr>
                <th>ID</th>
                <th>类型</th>
                <th>节点</th>
                <th>状态</th>
                <th>创建</th>
              </tr>
            </thead>
            <tbody>
              {counts.recentTasks.map((t) => (
                <tr key={t.id}>
                  <td className="font-mono text-xs">{t.id}</td>
                  <td>{t.kind}</td>
                  <td>#{t.node_id}</td>
                  <td>
                    <span className={`badge ${badgeClass(t.status)}`}>{t.status}</span>
                  </td>
                  <td className="text-xs" style={{ color: "var(--fg-muted)" }}>
                    {fmt(t.created_at)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <div className="text-sm" style={{ color: "var(--fg-muted)" }}>
            还没有任务
          </div>
        )}
      </section>
    </div>
  );
}

function StatCard({
  label,
  value,
  emoji,
}: {
  label: string;
  value: string | number;
  emoji: string;
}) {
  return (
    <div className="card p-4">
      <div className="text-3xl mb-1">{emoji}</div>
      <div className="text-2xl font-semibold">{value}</div>
      <div className="text-sm" style={{ color: "var(--fg-muted)" }}>
        {label}
      </div>
    </div>
  );
}

export function badgeClass(status: string): string {
  switch (status) {
    case "success":
    case "online":
      return "badge-ok";
    case "running":
    case "provisioning":
      return "badge-running";
    case "failed":
    case "offline":
      return "badge-err";
    case "pending":
      return "badge-warn";
    default:
      return "";
  }
}

export function fmt(iso: string | null): string {
  if (!iso) return "—";
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}
