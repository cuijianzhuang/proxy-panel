import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { api, ApiError, Node, relativeTime } from "../lib/api";
import { Modal } from "../components/Modal";
import { badgeClass } from "./Dashboard";

export function Nodes() {
  const [rows, setRows] = useState<Node[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [editing, setEditing] = useState<Node | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      setRows(await api.get<Node[]>("/api/nodes"));
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  async function applyNow(id: number) {
    setBusyId(id);
    try {
      const r = await api.post<{ task_id: number }>(`/api/nodes/${id}/apply`);
      setToast(`已入队任务 #${r.task_id} — 去「任务」页查看`);
    } catch (e) {
      setToast(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusyId(null);
      setTimeout(() => setToast(null), 5000);
    }
  }

  async function remove(id: number) {
    if (!confirm("确认删除该节点?所有附着的监听器关联会被级联清空。")) return;
    try {
      await api.del(`/api/nodes/${id}`);
      reload();
    } catch (e) {
      setToast(e instanceof ApiError ? e.message : String(e));
    }
  }

  const [filter, setFilter] = useState("");
  const filtered = useMemo(() => {
    if (!rows) return null;
    const q = filter.toLowerCase().trim();
    if (!q) return rows;
    return rows.filter((n) =>
      n.name.toLowerCase().includes(q) ||
      n.addr.toLowerCase().includes(q) ||
      String(n.id).includes(q) ||
      n.core.includes(q)
    );
  }, [rows, filter]);

  const onlineCount = rows?.filter((n) => n.status === "online").length ?? 0;
  const totalCount  = rows?.length ?? 0;

  return (
    <div>
      <header className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">VPS 管理</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            共 {totalCount} 台 VPS,{onlineCount} 台在线
          </p>
        </div>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">＋ 新建节点</button>
      </header>

      <input
        className="input mb-3"
        placeholder="🔍 搜索名称、主机或内核…"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
      />

      {toast && (
        <div className="card p-3 mb-3 text-sm" style={{ color: "var(--accent)" }}>{toast}</div>
      )}
      {err && (
        <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>
      )}

      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>VPS</th>
              <th>状态</th>
              <th>地区 / 内核</th>
              <th>SSH</th>
              <th>最近在线</th>
              <th>创建时间</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {filtered?.map((n) => (
              <tr key={n.id}>
                {/* Collapse name + addr into a two-line cell so the table
                  * reads like a stack of cards (matching the reference UI).
                  * Primary identity is the friendly name; the host string
                  * sits underneath in muted mono. */}
                <td>
                  <div className="font-medium">{n.name}</div>
                  <div className="font-mono text-xs" style={{ color: "var(--fg-muted)" }}>
                    #{n.id} · {n.addr}
                  </div>
                </td>
                <td>
                  <span className={`badge ${badgeClass(n.status)}`}>
                    {n.status === "online" ? "在线" : n.status === "offline" ? "离线" : n.status}
                  </span>
                </td>
                <td>
                  <span className="badge">{n.core === "singbox" ? "sing-box" : "Xray"}</span>
                </td>
                <td>
                  <div className="text-xs">{n.ssh_user}</div>
                  <div className="text-xs" style={{ color: "var(--fg-muted)" }}>:{n.ssh_port}</div>
                </td>
                <td className="text-xs" title={n.last_seen_at ?? ""} style={{ color: "var(--fg-muted)" }}>
                  {relativeTime(n.last_seen_at)}
                </td>
                <td className="text-xs" style={{ color: "var(--fg-muted)" }}>
                  {n.created_at
                    ? new Date(n.created_at).toLocaleString("zh-CN", { hour12: false }).replace(/\//g, "/")
                    : "—"}
                </td>
                <td className="text-right">
                  <div className="flex gap-2 justify-end">
                    <button className="btn btn-primary" onClick={() => applyNow(n.id)} disabled={busyId === n.id}>
                      {busyId === n.id ? "…" : "Apply"}
                    </button>
                    <button className="btn btn-ghost" onClick={() => setEditing(n)}>编辑</button>
                    <button className="btn btn-danger" onClick={() => remove(n.id)}>删除</button>
                  </div>
                </td>
              </tr>
            ))}
            {filtered && filtered.length === 0 && (
              <tr>
                <td colSpan={7} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>
                  {totalCount === 0 ? "还没有节点 — 点右上角新建。" : "没有匹配的节点。"}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建节点">
        <NewNodeForm onSaved={() => { setShowNew(false); reload(); }} />
      </Modal>
      <Modal open={editing !== null} onClose={() => setEditing(null)} title={`编辑节点 — ${editing?.name ?? ""}`}>
        {editing && (
          <NewNodeForm key={editing.id} editing={editing} onSaved={() => { setEditing(null); reload(); }} />
        )}
      </Modal>
    </div>
  );
}

function Header({ title, onNew }: { title: string; onNew: () => void }) {
  return (
    <header className="mb-4 flex items-center justify-between">
      <h1 className="text-2xl font-semibold">{title}</h1>
      <button onClick={onNew} className="btn btn-primary">
        ＋ 新建
      </button>
    </header>
  );
}

function NewNodeForm({ editing, onSaved }: { editing?: Node | null; onSaved: () => void }) {
  const isEdit = !!editing;
  const [name, setName] = useState(editing?.name ?? "");
  const [addr, setAddr] = useState(editing?.addr ?? "");
  const [core, setCore] = useState<"xray" | "singbox">(editing?.core ?? "xray");
  const [sshPort, setSshPort] = useState(editing?.ssh_port ?? 22);
  const [sshUser, setSshUser] = useState(editing?.ssh_user ?? "root");
  const [mgmtPort, setMgmtPort] = useState(editing?.mgmt_port ?? 0);
  const [err, setErr] = useState<string | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    const payload = {
      name,
      addr,
      core,
      ssh_port: sshPort,
      ssh_user: sshUser,
      mgmt_port: mgmtPort,
    };
    try {
      if (isEdit) await api.put(`/api/nodes/${editing!.id}`, payload);
      else        await api.post("/api/nodes", payload);
      onSaved();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    }
  }

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <Field label="名称">
        <input className="input" value={name} onChange={(e) => setName(e.target.value)} required />
      </Field>
      <Field label="地址 (IP / 域名)">
        <input className="input" value={addr} onChange={(e) => setAddr(e.target.value)} required />
      </Field>
      <div className="grid grid-cols-2 gap-3">
        <Field label="SSH 端口">
          <input
            className="input"
            type="number"
            value={sshPort}
            onChange={(e) => setSshPort(Number(e.target.value))}
            min={1}
            max={65535}
          />
        </Field>
        <Field label="SSH 用户">
          <input
            className="input"
            value={sshUser}
            onChange={(e) => setSshUser(e.target.value)}
            required
          />
        </Field>
      </div>
      <div className="grid grid-cols-2 gap-3">
        <Field label="内核">
          <select className="select" value={core} onChange={(e) => setCore(e.target.value as never)}>
            <option value="xray">xray</option>
            <option value="singbox">sing-box</option>
          </select>
        </Field>
        <Field label="Stats 端口 (0=禁用)">
          <input
            className="input"
            type="number"
            value={mgmtPort}
            onChange={(e) => setMgmtPort(Number(e.target.value))}
            min={0}
            max={65535}
          />
        </Field>
      </div>
      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary">
          {isEdit ? "保存修改" : "创建"}
        </button>
      </div>
    </form>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="text-sm mb-1 block">{label}</span>
      {children}
    </label>
  );
}
