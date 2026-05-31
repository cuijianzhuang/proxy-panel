import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, ApiError, CdnEndpoint } from "../lib/api";
import { Modal } from "../components/Modal";
import { SearchSelect } from "../components/SearchSelect";

/*
 * CDN 优选 — a sorted list of domains / IPs that listeners with cdn_enabled
 * rotate through. The "sort_order" column is the priority knob (lower wins).
 * Toggling `enabled` on a row instantly drops it from the rotation pool.
 */
export function CdnEndpoints() {
  const [rows, setRows] = useState<CdnEndpoint[] | null>(null);
  const [err, setErr]   = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [editing, setEditing] = useState<CdnEndpoint | null>(null);

  const reload = useCallback(async () => {
    try { setRows(await api.get<CdnEndpoint[]>("/api/cdn-endpoints")); setErr(null); }
    catch (e) { setErr(String(e)); }
  }, []);
  useEffect(() => { reload(); }, [reload]);

  async function toggleEnabled(r: CdnEndpoint) {
    try {
      await api.put(`/api/cdn-endpoints/${r.id}`, { enabled: !r.enabled });
      reload();
    } catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }
  async function remove(id: number) {
    if (!confirm("删除该 CDN 端点?引用它的监听器不会自动更新。")) return;
    try { await api.del(`/api/cdn-endpoints/${id}`); reload(); }
    catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }

  const enabledCount = rows?.filter((r) => r.enabled).length ?? 0;

  return (
    <div>
      <header className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold">CDN 优选</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            按 sort_order 升序优先级,启用的有 {enabledCount} 个,共 {rows?.length ?? "—"} 个。
          </p>
        </div>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">＋ 新建端点</button>
      </header>

      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>ID</th><th>名称</th><th>地址</th><th>类型</th>
              <th className="text-right">排序</th><th>启用</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {rows?.map((r) => (
              <tr key={r.id}>
                <td className="font-mono text-xs">{r.id}</td>
                <td>{r.name}</td>
                <td className="font-mono text-xs break-all">{r.address}</td>
                <td><span className="badge">{r.kind}</span></td>
                <td className="text-right font-mono">{r.sort_order}</td>
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
                还没有 CDN 端点 — 比如 Cloudflare 优选 IP 或自定义 CNAME。
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建 CDN 端点">
        <CdnForm onSaved={() => { setShowNew(false); reload(); }} />
      </Modal>
      <Modal open={editing !== null} onClose={() => setEditing(null)} title={`编辑 — ${editing?.name ?? ""}`}>
        {editing && (
          <CdnForm key={editing.id} editing={editing} onSaved={() => { setEditing(null); reload(); }} />
        )}
      </Modal>
    </div>
  );
}

function CdnForm({ editing, onSaved }: { editing?: CdnEndpoint | null; onSaved: () => void }) {
  const isEdit = !!editing;
  const [name,      setName]      = useState(editing?.name ?? "");
  const [address,   setAddress]   = useState(editing?.address ?? "");
  const [kind,      setKind]      = useState<"domain" | "ip">(editing?.kind ?? "domain");
  const [sortOrder, setSortOrder] = useState(editing?.sort_order ?? 100);
  const [note,      setNote]      = useState(editing?.note ?? "");
  const [err,       setErr]       = useState<string | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    const payload = { name, address, kind, sort_order: sortOrder, note: note || null };
    try {
      if (isEdit) await api.put(`/api/cdn-endpoints/${editing!.id}`, payload);
      else        await api.post("/api/cdn-endpoints", payload);
      onSaved();
    } catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <label className="block">
        <span className="text-sm mb-1 block">名称</span>
        <input className="input" value={name} onChange={(e) => setName(e.target.value)} required
               placeholder="cf-优选-japan" />
      </label>
      <div className="grid grid-cols-[1fr_auto] gap-3">
        <label className="block">
          <span className="text-sm mb-1 block">地址 (域名或 IP)</span>
          <input className="input font-mono text-xs" value={address}
                 onChange={(e) => setAddress(e.target.value)} required
                 placeholder="cloudflare.example.com 或 104.16.0.0" />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">类型</span>
          <SearchSelect
            value={kind}
            onChange={(v) => setKind(v as never)}
            options={[
              { value: "domain", label: "domain", sub: "域名（CNAME）" },
              { value: "ip",     label: "ip",     sub: "IP 地址" },
            ]}
          />
        </label>
      </div>
      <label className="block">
        <span className="text-sm mb-1 block">排序 (越小越优先)</span>
        <input className="input" type="number" min={0} max={9999}
               value={sortOrder} onChange={(e) => setSortOrder(Number(e.target.value))} />
      </label>
      <label className="block">
        <span className="text-sm mb-1 block">备注 (可选)</span>
        <input className="input" value={note} onChange={(e) => setNote(e.target.value)}
               placeholder="比如:东京 / 来源 yxip.cmliussss.com" />
      </label>
      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary">{isEdit ? "保存修改" : "创建"}</button>
      </div>
    </form>
  );
}
