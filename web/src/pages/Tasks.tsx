import { useEffect, useState } from "react";
import { api, Task } from "../lib/api";
import { badgeClass, fmt } from "./Dashboard";

/*
 * Auto-polling task list. Pending/running tasks make the page refresh every
 * second; once everything is terminal we throttle to every 5s so the panel
 * stays cheap to leave open in a tab.
 */
export function Tasks() {
  const [rows, setRows] = useState<Task[]>([]);
  const [open, setOpen] = useState<Task | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const tick = async () => {
      try {
        const data = await api.get<Task[]>("/api/tasks?limit=100");
        if (cancelled) return;
        setRows(data);
        setErr(null);
        const active = data.some((t) => t.status === "pending" || t.status === "running");
        timer = setTimeout(tick, active ? 1000 : 5000);
      } catch (e) {
        if (!cancelled) {
          setErr(String(e));
          timer = setTimeout(tick, 5000);
        }
      }
    };

    tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, []);

  // Keep the open task in sync with the polled list — always replace,
  // React will skip the re-render if the JSON happens to be identical.
  useEffect(() => {
    if (!open) return;
    const next = rows.find((t) => t.id === open.id);
    if (next) setOpen(next);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows]);

  return (
    <div>
      <header className="mb-4">
        <h1 className="text-2xl font-semibold">任务</h1>
        <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
          每秒自动刷新进行中的任务,空闲时降为 5 秒。
        </p>
      </header>

      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>ID</th>
              <th>类型</th>
              <th>节点</th>
              <th>状态</th>
              <th>开始</th>
              <th>结束</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((t) => (
              <tr key={t.id}>
                <td className="font-mono text-xs">{t.id}</td>
                <td>{t.kind}</td>
                <td>#{t.node_id}</td>
                <td><span className={`badge ${badgeClass(t.status)}`}>{t.status}</span></td>
                <td className="text-xs" style={{ color: "var(--fg-muted)" }}>{fmt(t.started_at)}</td>
                <td className="text-xs" style={{ color: "var(--fg-muted)" }}>{fmt(t.finished_at)}</td>
                <td className="text-right">
                  <button className="btn btn-ghost" onClick={() => setOpen(t)}>查看日志</button>
                </td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr><td colSpan={7} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>暂无任务。</td></tr>
            )}
          </tbody>
        </table>
      </div>

      {open && (
        <div
          className="fixed inset-0 z-50 flex items-end justify-end p-4"
          style={{ background: "rgba(15, 23, 42, 0.45)" }}
          onClick={() => setOpen(null)}
        >
          <div
            className="card w-full max-w-3xl h-[80vh] flex flex-col"
            onClick={(e) => e.stopPropagation()}
          >
            <div
              className="px-5 py-3 border-b flex items-center justify-between"
              style={{ borderColor: "var(--border)" }}
            >
              <div>
                <span className="font-semibold">任务 #{open.id} · {open.kind}</span>{" "}
                <span className={`badge ${badgeClass(open.status)}`}>{open.status}</span>
              </div>
              <button onClick={() => setOpen(null)} className="btn btn-ghost">✕</button>
            </div>
            <pre
              className="flex-1 overflow-auto p-4 font-mono text-xs whitespace-pre-wrap"
              style={{ background: "var(--bg)" }}
            >
{open.log || "(空)"}
            </pre>
            {open.error && (
              <div
                className="px-5 py-3 border-t text-sm"
                style={{ borderColor: "var(--border)", color: "#b91c1c" }}
              >
                {open.error}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

