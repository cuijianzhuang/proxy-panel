import { useCallback, useEffect, useState } from "react";
import { api, ApiError, Backup, relativeTime } from "../lib/api";

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const kib = n / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  const mib = kib / 1024;
  if (mib < 1024) return `${mib.toFixed(2)} MiB`;
  return `${(mib / 1024).toFixed(2)} GiB`;
}

export function Backups() {
  const [rows, setRows] = useState<Backup[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      setRows(await api.get<Backup[]>("/api/backups"));
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => { reload(); }, [reload]);

  async function createBackup() {
    setBusy(true);
    try {
      await api.post("/api/backups");
      await reload();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove(id: number) {
    if (!confirm("删除该备份?磁盘上的 .db 文件也会被删除。")) return;
    try {
      await api.del(`/api/backups/${id}`);
      await reload();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    }
  }

  return (
    <div>
      <header className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold">备份管理</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            SQLite 走 <code className="kbd">VACUUM INTO</code>,原子且一致。文件落在 <code className="kbd">data/backups/</code>。
          </p>
        </div>
        <button className="btn btn-primary" onClick={createBackup} disabled={busy}>
          {busy ? "备份中…" : "＋ 立即备份"}
        </button>
      </header>

      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>ID</th>
              <th>文件名</th>
              <th>大小</th>
              <th>类型</th>
              <th>创建时间</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((b) => (
              <tr key={b.id}>
                <td className="font-mono text-xs">{b.id}</td>
                <td className="font-mono text-xs break-all">{b.filename}</td>
                <td className="font-mono text-xs">{formatBytes(b.size_bytes)}</td>
                <td><span className="badge">{b.kind}</span></td>
                <td className="text-xs" title={b.created_at}>{relativeTime(b.created_at)}</td>
                <td className="text-right">
                  <div className="flex gap-2 justify-end">
                    <a
                      className="btn btn-ghost"
                      href={`/api/backups/${b.id}/download`}
                      download
                    >
                      下载
                    </a>
                    <button className="btn btn-danger" onClick={() => remove(b.id)}>删除</button>
                  </div>
                </td>
              </tr>
            ))}
            {rows.length === 0 && (
              <tr>
                <td colSpan={6} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>
                  还没有备份。
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
