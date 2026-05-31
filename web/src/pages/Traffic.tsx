import { useCallback, useEffect, useState } from "react";
import {
  api, ApiError, DailyPoint, formatBytes, relativeTime, TrafficSummary,
} from "../lib/api";

/*
 * 流量统计 — three pieces:
 *   1. KPI row: grand total + last-collected time + 手动采集 button.
 *   2. Daily up/down bar chart (hand-rolled SVG, no chart lib).
 *   3. Per-user table with a used / quota progress bar.
 *
 * The backend collector runs every 60s; this page also lets an admin trigger
 * a collection on demand and refreshes.
 */
export function Traffic() {
  const [summary, setSummary] = useState<TrafficSummary | null>(null);
  const [series, setSeries] = useState<DailyPoint[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      const [s, ds] = await Promise.all([
        api.get<TrafficSummary>("/api/traffic"),
        api.get<DailyPoint[]>("/api/traffic/series?days=14"),
      ]);
      setSummary(s);
      setSeries(ds);
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);
  useEffect(() => { reload(); }, [reload]);

  async function collectNow() {
    setBusy(true);
    try {
      await api.post("/api/traffic/collect");
      await reload();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div>
      <header className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold">流量统计</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            采集器每 60 秒拉取一次;也可手动触发。
          </p>
        </div>
        <button className="btn btn-primary" onClick={collectNow} disabled={busy}>
          {busy ? "采集中…" : "🔄 立即采集"}
        </button>
      </header>

      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      <section className="grid grid-cols-2 md:grid-cols-3 gap-4 mb-6">
        <Kpi label="累计总流量" value={summary ? formatBytes(summary.grand_total) : "—"} emoji="📊" />
        <Kpi label="代理用户" value={summary ? String(summary.users.length) : "—"} emoji="👥" />
        <Kpi label="最近采集" value={summary ? relativeTime(summary.last_collected) : "—"} emoji="⏱️" />
      </section>

      <section className="card p-4 mb-6">
        <h2 className="font-semibold mb-3">近 14 天流量</h2>
        <DailyChart data={series} />
      </section>

      <section className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>用户</th>
              <th className="text-right">上行</th>
              <th className="text-right">下行</th>
              <th className="text-right">合计</th>
              <th>配额用量</th>
              <th>状态</th>
            </tr>
          </thead>
          <tbody>
            {summary?.users.map((u) => {
              const quotaBytes = u.quota_gb * 1024 ** 3;
              const pct = quotaBytes > 0 ? Math.min(100, (u.used_bytes / quotaBytes) * 100) : 0;
              return (
                <tr key={u.proxy_user_id}>
                  <td>{u.name}</td>
                  <td className="text-right font-mono text-xs">{formatBytes(u.up)}</td>
                  <td className="text-right font-mono text-xs">{formatBytes(u.down)}</td>
                  <td className="text-right font-mono text-xs">{formatBytes(u.total)}</td>
                  <td style={{ minWidth: 180 }}>
                    {u.quota_gb > 0 ? (
                      <div>
                        <div className="flex justify-between text-xs mb-1" style={{ color: "var(--fg-muted)" }}>
                          <span>{formatBytes(u.used_bytes)}</span>
                          <span>{u.quota_gb} GB</span>
                        </div>
                        <div style={{ height: 6, borderRadius: 999, background: "var(--accent-soft)", overflow: "hidden" }}>
                          <div style={{
                            width: `${pct}%`, height: "100%",
                            background: pct >= 90 ? "#dc2626" : pct >= 70 ? "#f59e0b" : "var(--accent)",
                          }} />
                        </div>
                      </div>
                    ) : (
                      <span className="text-xs" style={{ color: "var(--fg-muted)" }}>无限</span>
                    )}
                  </td>
                  <td>
                    <span className={`badge ${u.enabled ? "badge-ok" : "badge-err"}`}>
                      {u.enabled ? "启用" : "停用"}
                    </span>
                  </td>
                </tr>
              );
            })}
            {summary && summary.users.length === 0 && (
              <tr><td colSpan={6} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>
                还没有代理用户。
              </td></tr>
            )}
          </tbody>
        </table>
      </section>
    </div>
  );
}

function Kpi({ label, value, emoji }: { label: string; value: string; emoji: string }) {
  return (
    <div className="card p-4">
      <div className="text-2xl mb-1">{emoji}</div>
      <div className="text-xl font-semibold">{value}</div>
      <div className="text-sm" style={{ color: "var(--fg-muted)" }}>{label}</div>
    </div>
  );
}

/*
 * Stacked up/down daily bars in a single inline SVG. Width scales with the
 * data length; each day is a column with download (bottom) + upload (top).
 */
function DailyChart({ data }: { data: DailyPoint[] }) {
  if (data.length === 0) {
    return <div className="text-sm py-8 text-center" style={{ color: "var(--fg-muted)" }}>暂无数据 — 点「立即采集」生成。</div>;
  }
  const W = Math.max(480, data.length * 48);
  const H = 200;
  const pad = { top: 12, right: 8, bottom: 28, left: 56 };
  const plotW = W - pad.left - pad.right;
  const plotH = H - pad.top - pad.bottom;
  const max = Math.max(1, ...data.map((d) => d.up + d.down));
  const barW = Math.min(28, (plotW / data.length) * 0.6);
  const step = plotW / data.length;

  // y-axis ticks (0, 50%, 100%)
  const ticks = [0, 0.5, 1].map((f) => ({ f, val: max * f, y: pad.top + plotH * (1 - f) }));

  return (
    <div className="overflow-x-auto">
      <svg width={W} height={H} role="img">
        {/* gridlines + y labels */}
        {ticks.map((t, i) => (
          <g key={i}>
            <line x1={pad.left} y1={t.y} x2={W - pad.right} y2={t.y}
                  stroke="var(--border)" strokeWidth={1} />
            <text x={pad.left - 6} y={t.y + 4} textAnchor="end"
                  fontSize={10} fill="var(--fg-muted)">{formatBytes(t.val)}</text>
          </g>
        ))}
        {/* bars */}
        {data.map((d, i) => {
          const x = pad.left + step * i + (step - barW) / 2;
          const downH = (d.down / max) * plotH;
          const upH = (d.up / max) * plotH;
          const downY = pad.top + plotH - downH;
          const upY = downY - upH;
          return (
            <g key={d.day}>
              <rect x={x} y={downY} width={barW} height={downH} fill="var(--accent)" rx={2}>
                <title>{`${d.day}\n下行 ${formatBytes(d.down)}`}</title>
              </rect>
              <rect x={x} y={upY} width={barW} height={upH} fill="#f59e0b" rx={2}>
                <title>{`${d.day}\n上行 ${formatBytes(d.up)}`}</title>
              </rect>
              <text x={x + barW / 2} y={H - 10} textAnchor="middle"
                    fontSize={9} fill="var(--fg-muted)">{d.day.slice(5)}</text>
            </g>
          );
        })}
      </svg>
      <div className="flex gap-4 text-xs mt-2" style={{ color: "var(--fg-muted)" }}>
        <span><span style={{ display: "inline-block", width: 10, height: 10, background: "var(--accent)", borderRadius: 2 }} /> 下行</span>
        <span><span style={{ display: "inline-block", width: 10, height: 10, background: "#f59e0b", borderRadius: 2 }} /> 上行</span>
      </div>
    </div>
  );
}
