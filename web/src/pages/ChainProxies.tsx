import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, ApiError, ChainProxy } from "../lib/api";
import { Modal } from "../components/Modal";

/*
 * 链式代理 — outbound proxy chain. Listeners can route their traffic through
 * one of these (国内中转 → 海外落地 模式)。
 *
 * The row stores the upstream (SOCKS5/HTTP) endpoint + credentials. Wiring
 * the adapter so that selected listeners actually route through a chain
 * proxy is the next render-layer task; today this page is the data plane.
 */
export function ChainProxies() {
  const [rows, setRows] = useState<ChainProxy[] | null>(null);
  const [err, setErr]   = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [editing, setEditing] = useState<ChainProxy | null>(null);

  const reload = useCallback(async () => {
    try { setRows(await api.get<ChainProxy[]>("/api/chain-proxies")); setErr(null); }
    catch (e) { setErr(String(e)); }
  }, []);
  useEffect(() => { reload(); }, [reload]);

  async function toggleEnabled(r: ChainProxy) {
    try { await api.put(`/api/chain-proxies/${r.id}`, { enabled: !r.enabled }); reload(); }
    catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }
  async function remove(id: number) {
    if (!confirm("删除该链式代理?")) return;
    try { await api.del(`/api/chain-proxies/${id}`); reload(); }
    catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }

  return (
    <div>
      <header className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold">链式代理</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            上游代理(国内中转 → 海外落地用)。每个监听器可选择经过其中一条。
          </p>
        </div>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">＋ 新建链式代理</button>
      </header>

      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>ID</th><th>名称</th><th>类型</th><th>地址</th><th>认证</th><th>启用</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {rows?.map((r) => (
              <tr key={r.id}>
                <td className="font-mono text-xs">{r.id}</td>
                <td>{r.name}</td>
                <td><span className="badge">{r.proxy_type}</span></td>
                <td className="font-mono text-xs">{r.address}:{r.port}</td>
                <td className="text-xs" style={{ color: "var(--fg-muted)" }}>
                  {r.username ? `${r.username} / ***` : <i>(无)</i>}
                </td>
                <td>
                  <button
                    onClick={() => toggleEnabled(r)}
                    className={`badge ${r.enabled ? "badge-ok" : "badge-err"}`}
                    style={{ cursor: "pointer", border: "none" }}
                  >
                    {r.enabled ? "✓ 启用" : "✗ 停用"}
                  </button>
                </td>
                <td className="text-right">
                  <div className="flex gap-2 justify-end">
                    <button className="btn btn-ghost" onClick={() => setEditing(r)}>编辑</button>
                    <button className="btn btn-danger" onClick={() => remove(r.id)}>删除</button>
                  </div>
                </td>
              </tr>
            ))}
            {rows && rows.length === 0 && (
              <tr><td colSpan={7} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>
                还没有链式代理。
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建链式代理">
        <ChainForm onSaved={() => { setShowNew(false); reload(); }} />
      </Modal>
      <Modal open={editing !== null} onClose={() => setEditing(null)} title={`编辑 — ${editing?.name ?? ""}`}>
        {editing && (
          <ChainForm key={editing.id} editing={editing} onSaved={() => { setEditing(null); reload(); }} />
        )}
      </Modal>
    </div>
  );
}

function ChainForm({ editing, onSaved }: { editing?: ChainProxy | null; onSaved: () => void }) {
  const isEdit = !!editing;
  const [name,     setName]     = useState(editing?.name ?? "");
  const [type,     setType]     = useState<"socks5" | "http">(editing?.proxy_type ?? "socks5");
  const [address,  setAddress]  = useState(editing?.address ?? "");
  const [port,     setPort]     = useState(editing?.port ?? 1080);
  const [username, setUsername] = useState(editing?.username ?? "");
  const [password, setPassword] = useState(editing?.password ?? "");
  const [note,     setNote]     = useState(editing?.note ?? "");
  const [err,      setErr]      = useState<string | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    const payload = {
      name, proxy_type: type, address, port,
      username: username || null, password: password || null,
      note: note || null,
    };
    try {
      if (isEdit) await api.put(`/api/chain-proxies/${editing!.id}`, payload);
      else        await api.post("/api/chain-proxies", payload);
      onSaved();
    } catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <label className="block">
        <span className="text-sm mb-1 block">名称</span>
        <input className="input" value={name} onChange={(e) => setName(e.target.value)} required
               placeholder="r.example.com" />
      </label>
      <div className="grid grid-cols-3 gap-3">
        <label className="block">
          <span className="text-sm mb-1 block">类型</span>
          <select className="select" value={type} onChange={(e) => setType(e.target.value as never)}>
            <option value="socks5">socks5</option>
            <option value="http">http</option>
          </select>
        </label>
        <label className="block col-span-2">
          <span className="text-sm mb-1 block">地址 + 端口</span>
          <div className="grid grid-cols-[1fr_120px] gap-2">
            <input className="input font-mono text-xs" value={address}
                   onChange={(e) => setAddress(e.target.value)} required placeholder="1.2.3.4 或 r.example.com" />
            <input className="input" type="number" min={1} max={65535}
                   value={port} onChange={(e) => setPort(Number(e.target.value))} />
          </div>
        </label>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <label className="block">
          <span className="text-sm mb-1 block">用户名 (可选)</span>
          <input className="input font-mono text-xs" value={username}
                 onChange={(e) => setUsername(e.target.value)} />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">密码 (可选)</span>
          <input className="input font-mono text-xs" type="password" value={password}
                 onChange={(e) => setPassword(e.target.value)} />
        </label>
      </div>
      <label className="block">
        <span className="text-sm mb-1 block">备注 (可选)</span>
        <input className="input" value={note} onChange={(e) => setNote(e.target.value)} />
      </label>
      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary">{isEdit ? "保存修改" : "创建"}</button>
      </div>
    </form>
  );
}
