import { useEffect, useState } from "react";
import { api, AuditEntry, relativeTime } from "../lib/api";

const METHOD_COLORS: Record<string, string> = {
  POST:   "#1d4ed8",
  PUT:    "#92400e",
  DELETE: "#b91c1c",
  PATCH:  "#7c3aed",
};

export function Audit() {
  const [rows, setRows] = useState<AuditEntry[]>([]);
  const [filter, setFilter] = useState("");
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        setRows(await api.get<AuditEntry[]>("/api/audit"));
      } catch (e) {
        setErr(String(e));
      }
    })();
  }, []);

  const filtered = rows.filter((r) => {
    if (!filter) return true;
    const q = filter.toLowerCase();
    return (
      r.path.toLowerCase().includes(q) ||
      (r.actor_name ?? "").toLowerCase().includes(q) ||
      r.method.toLowerCase().includes(q) ||
      String(r.status).includes(q)
    );
  });

  return (
    <div>
      <header className="mb-4 flex items-center justify-between gap-3">
        <h1 className="text-2xl font-semibold">审计日志</h1>
        <input
          className="input max-w-xs"
          placeholder="按路径 / 用户 / 方法过滤…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </header>
      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>时间</th>
              <th>操作者</th>
              <th>方法</th>
              <th>路径</th>
              <th>状态</th>
              <th>IP</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((r) => (
              <tr key={r.id}>
                <td className="text-xs whitespace-nowrap" title={r.ts}>
                  {relativeTime(r.ts)}
                </td>
                <td className="text-sm">{r.actor_name ?? <i style={{ color: "var(--fg-muted)" }}>(匿名)</i>}</td>
                <td>
                  <span className="font-mono text-xs font-semibold"
                        style={{ color: METHOD_COLORS[r.method] ?? "var(--fg)" }}>
                    {r.method}
                  </span>
                </td>
                <td className="font-mono text-xs">{r.path}</td>
                <td>
                  <span className="badge" style={{
                    background: r.status < 300 ? "#dcfce7" :
                                r.status < 500 ? "#fef3c7" : "#fee2e2",
                    color:      r.status < 300 ? "#166534" :
                                r.status < 500 ? "#92400e" : "#991b1b",
                    borderColor:r.status < 300 ? "#bbf7d0" :
                                r.status < 500 ? "#fde68a" : "#fecaca",
                  }}>
                    {r.status}
                  </span>
                </td>
                <td className="font-mono text-xs">{r.ip ?? "—"}</td>
              </tr>
            ))}
            {filtered.length === 0 && (
              <tr>
                <td colSpan={6} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>
                  {rows.length === 0 ? "暂无写操作记录。" : "没有匹配项。"}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
