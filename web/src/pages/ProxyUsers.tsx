import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, ApiError, Listener, Plan, ProxyUser } from "../lib/api";
import { Modal } from "../components/Modal";

export function ProxyUsers() {
  const [rows, setRows] = useState<ProxyUser[] | null>(null);
  const [plans, setPlans] = useState<Plan[]>([]);
  const [listeners, setListeners] = useState<Listener[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [attach, setAttach] = useState<ProxyUser | null>(null);
  const [editing, setEditing] = useState<ProxyUser | null>(null);

  const reload = useCallback(async () => {
    try {
      const [us, ps, ls] = await Promise.all([
        api.get<ProxyUser[]>("/api/proxy-users"),
        api.get<Plan[]>("/api/plans"),
        api.get<Listener[]>("/api/listeners"),
      ]);
      setRows(us);
      setPlans(ps);
      setListeners(ls);
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  async function remove(id: number) {
    if (!confirm("删除该代理用户?")) return;
    try {
      await api.del(`/api/proxy-users/${id}`);
      reload();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    }
  }

  const subUrl = (token: string) =>
    `${location.protocol}//${location.host}/sub/${token}`;

  return (
    <div>
      <header className="mb-4 flex items-center justify-between">
        <h1 className="text-2xl font-semibold">代理用户</h1>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">＋ 新建</button>
      </header>
      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>ID</th>
              <th>名称</th>
              <th>UUID</th>
              <th>套餐</th>
              <th>已用 / 配额</th>
              <th>启用</th>
              <th>订阅</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {rows?.map((u) => (
              <tr key={u.id}>
                <td className="font-mono text-xs">{u.id}</td>
                <td>{u.name}</td>
                <td className="font-mono text-xs">{u.uuid.slice(0, 8)}…</td>
                <td>{u.plan_id ? `#${u.plan_id}` : "—"}</td>
                <td className="font-mono text-xs">
                  {formatBytes(u.used_bytes)} / {u.quota_gb} GB
                </td>
                <td>{u.enabled ? "✓" : "✗"}</td>
                <td>
                  <button
                    className="btn btn-ghost text-xs"
                    onClick={() => {
                      navigator.clipboard.writeText(subUrl(u.subscription_token));
                    }}
                    title={subUrl(u.subscription_token)}
                  >
                    复制 URL
                  </button>
                </td>
                <td className="text-right">
                  <div className="flex gap-2 justify-end">
                    <button className="btn btn-ghost" onClick={() => setAttach(u)}>关联监听器</button>
                    <button className="btn btn-ghost" onClick={() => setEditing(u)}>编辑</button>
                    <button className="btn btn-danger" onClick={() => remove(u.id)}>删除</button>
                  </div>
                </td>
              </tr>
            ))}
            {rows && rows.length === 0 && (
              <tr><td colSpan={8} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>暂无用户。</td></tr>
            )}
          </tbody>
        </table>
      </div>

      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建代理用户">
        <NewUserForm plans={plans} onCreated={() => { setShowNew(false); reload(); }} />
      </Modal>

      <Modal open={editing !== null} onClose={() => setEditing(null)} title={`编辑用户 — ${editing?.name ?? ""}`}>
        {editing && (
          <EditUserForm key={editing.id} user={editing} plans={plans}
                        onSaved={() => { setEditing(null); reload(); }} />
        )}
      </Modal>

      <Modal
        open={attach !== null}
        onClose={() => setAttach(null)}
        title={attach ? `关联监听器 — ${attach.name}` : ""}
      >
        {attach && (
          <AttachForm
            user={attach}
            listeners={listeners}
            onClose={() => setAttach(null)}
          />
        )}
      </Modal>
    </div>
  );
}

function NewUserForm({ plans, onCreated }: { plans: Plan[]; onCreated: () => void }) {
  const [name, setName] = useState("");
  const [planId, setPlanId] = useState<number | null>(null);
  const [quotaGb, setQuotaGb] = useState(0);
  const [err, setErr] = useState<string | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    try {
      await api.post("/api/proxy-users", { name, plan_id: planId, quota_gb: quotaGb });
      onCreated();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    }
  }

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <label className="block">
        <span className="text-sm mb-1 block">名称</span>
        <input className="input" value={name} onChange={(e) => setName(e.target.value)} required />
      </label>
      <div className="grid grid-cols-2 gap-3">
        <label className="block">
          <span className="text-sm mb-1 block">套餐</span>
          <select
            className="select"
            value={planId ?? ""}
            onChange={(e) => setPlanId(e.target.value ? Number(e.target.value) : null)}
          >
            <option value="">— 无 —</option>
            {plans.map((p) => (
              <option key={p.id} value={p.id}>#{p.id} {p.name}</option>
            ))}
          </select>
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">配额 (GB,0=无限)</span>
          <input
            className="input"
            type="number"
            min={0}
            step="0.1"
            value={quotaGb}
            onChange={(e) => setQuotaGb(Number(e.target.value))}
          />
        </label>
      </div>
      <p className="text-xs" style={{ color: "var(--fg-muted)" }}>
        UUID、密码、订阅 token 会在创建时自动生成。
      </p>
      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary">创建</button>
      </div>
    </form>
  );
}

function EditUserForm({
  user, plans, onSaved,
}: {
  user: ProxyUser;
  plans: Plan[];
  onSaved: () => void;
}) {
  const [name, setName] = useState(user.name);
  const [planId, setPlanId] = useState<number | null>(user.plan_id);
  const [quotaGb, setQuotaGb] = useState(user.quota_gb);
  const [enabled, setEnabled] = useState(user.enabled);
  const [note, setNote] = useState(user.note ?? "");
  const [rotate, setRotate] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    try {
      await api.put(`/api/proxy-users/${user.id}`, {
        name,
        plan_id: planId,
        quota_gb: quotaGb,
        enabled,
        note: note || null,
        rotate_subscription_token: rotate,
      });
      onSaved();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    }
  }

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <label className="block">
        <span className="text-sm mb-1 block">名称</span>
        <input className="input" value={name} onChange={(e) => setName(e.target.value)} required />
      </label>
      <div className="grid grid-cols-2 gap-3">
        <label className="block">
          <span className="text-sm mb-1 block">套餐</span>
          <select className="select" value={planId ?? ""}
                  onChange={(e) => setPlanId(e.target.value ? Number(e.target.value) : null)}>
            <option value="">— 无 —</option>
            {plans.map((p) => <option key={p.id} value={p.id}>#{p.id} {p.name}</option>)}
          </select>
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">配额 (GB,0=无限)</span>
          <input className="input" type="number" min={0} step="0.1"
                 value={quotaGb} onChange={(e) => setQuotaGb(Number(e.target.value))} />
        </label>
      </div>
      <label className="block">
        <span className="text-sm mb-1 block">备注</span>
        <input className="input" value={note} onChange={(e) => setNote(e.target.value)} />
      </label>
      <label className="flex items-center gap-2 text-sm cursor-pointer">
        <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
        启用(取消勾选立即停用,客户端断连)
      </label>
      <label className="flex items-center gap-2 text-sm cursor-pointer">
        <input type="checkbox" checked={rotate} onChange={(e) => setRotate(e.target.checked)} />
        轮换订阅 token(旧订阅链接立即失效)
      </label>
      <div className="text-xs font-mono break-all" style={{ color: "var(--fg-muted)" }}>
        UUID: {user.uuid}
      </div>
      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary">保存修改</button>
      </div>
    </form>
  );
}

function AttachForm({
  user,
  listeners,
  onClose,
}: {
  user: ProxyUser;
  listeners: Listener[];
  onClose: () => void;
}) {
  const [attached, setAttached] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  // For each listener, query its clients[] to figure out which contain `user`.
  useEffect(() => {
    (async () => {
      try {
        const out = new Set<number>();
        for (const l of listeners) {
          const us = await api.get<ProxyUser[]>(`/api/listeners/${l.id}/clients`);
          if (us.find((u) => u.id === user.id)) out.add(l.id);
        }
        setAttached(out);
      } catch (e) {
        setErr(String(e));
      }
    })();
  }, [listeners, user.id]);

  async function toggle(listenerId: number, on: boolean) {
    setBusy(true);
    try {
      if (on) {
        await api.post(`/api/listeners/${listenerId}/clients`, {
          proxy_user_id: user.id,
        });
      } else {
        await api.del(`/api/listeners/${listenerId}/clients/${user.id}`);
      }
      setAttached((prev) => {
        const next = new Set(prev);
        if (on) next.add(listenerId);
        else next.delete(listenerId);
        return next;
      });
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-2">
      {listeners.length === 0 && (
        <p className="text-sm" style={{ color: "var(--fg-muted)" }}>还没有监听器。</p>
      )}
      {listeners.map((l) => {
        const on = attached.has(l.id);
        return (
          <label key={l.id} className="flex items-center gap-2 p-2 rounded hover:bg-black/5">
            <input
              type="checkbox"
              checked={on}
              disabled={busy}
              onChange={(e) => toggle(l.id, e.target.checked)}
            />
            <span className="text-sm">
              #{l.id} {l.name}
              <span style={{ color: "var(--fg-muted)" }}> · {l.protocol}/{l.transport} · :{l.port}</span>
            </span>
          </label>
        );
      })}
      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end pt-3">
        <button className="btn btn-ghost" onClick={onClose}>完成</button>
      </div>
    </div>
  );
}

function formatBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  const kib = b / 1024;
  if (kib < 1024) return `${kib.toFixed(1)} KiB`;
  const mib = kib / 1024;
  if (mib < 1024) return `${mib.toFixed(1)} MiB`;
  return `${(mib / 1024).toFixed(2)} GiB`;
}
