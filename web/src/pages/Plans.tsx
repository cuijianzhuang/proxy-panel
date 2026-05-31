import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, ApiError, Plan } from "../lib/api";
import { Modal } from "../components/Modal";

export function Plans() {
  const [rows, setRows] = useState<Plan[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [editing, setEditing] = useState<Plan | null>(null);

  const reload = useCallback(async () => {
    try {
      setRows(await api.get<Plan[]>("/api/plans"));
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  async function remove(id: number) {
    if (!confirm("删除该套餐?")) return;
    try {
      await api.del(`/api/plans/${id}`);
      reload();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    }
  }

  return (
    <div>
      <header className="mb-4 flex items-center justify-between">
        <h1 className="text-2xl font-semibold">套餐管理</h1>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">＋ 新建</button>
      </header>
      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>ID</th>
              <th>名称</th>
              <th>类型</th>
              <th>配额 (GB)</th>
              <th>重置日</th>
              <th>有效期 (天)</th>
              <th>限速 (Mbps)</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {rows?.map((p) => (
              <tr key={p.id}>
                <td className="font-mono text-xs">{p.id}</td>
                <td>{p.name}</td>
                <td>
                  <span className="badge">{p.quota_type}</span>
                </td>
                <td className="font-mono">{p.quota_gb}</td>
                <td>{p.quota_reset_day}</td>
                <td>{p.duration_days ?? "—"}</td>
                <td>{p.speed_limit_mbps ?? "—"}</td>
                <td className="text-right">
                  <div className="flex gap-2 justify-end">
                    <button className="btn btn-ghost" onClick={() => setEditing(p)}>编辑</button>
                    <button className="btn btn-danger" onClick={() => remove(p.id)}>删除</button>
                  </div>
                </td>
              </tr>
            ))}
            {rows && rows.length === 0 && (
              <tr><td colSpan={8} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>暂无套餐。</td></tr>
            )}
          </tbody>
        </table>
      </div>

      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建套餐">
        <PlanForm onSaved={() => { setShowNew(false); reload(); }} />
      </Modal>
      <Modal open={editing !== null} onClose={() => setEditing(null)} title={`编辑套餐 — ${editing?.name ?? ""}`}>
        {editing && (
          <PlanForm key={editing.id} editing={editing} onSaved={() => { setEditing(null); reload(); }} />
        )}
      </Modal>
    </div>
  );
}

function PlanForm({ editing, onSaved }: { editing?: Plan | null; onSaved: () => void }) {
  const isEdit = !!editing;
  const [name, setName] = useState(editing?.name ?? "");
  const [quotaType, setQuotaType] = useState<"permanent" | "monthly">(editing?.quota_type ?? "permanent");
  const [quotaGb, setQuotaGb] = useState(editing?.quota_gb ?? 100);
  const [resetDay, setResetDay] = useState(editing?.quota_reset_day ?? 1);
  const [durationDays, setDurationDays] = useState(editing?.duration_days != null ? String(editing.duration_days) : "");
  const [speedLimit, setSpeedLimit] = useState(editing?.speed_limit_mbps != null ? String(editing.speed_limit_mbps) : "");
  const [err, setErr] = useState<string | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    const payload = {
      name,
      quota_type: quotaType,
      quota_gb: quotaGb,
      quota_reset_day: resetDay,
      duration_days: durationDays ? Number(durationDays) : null,
      speed_limit_mbps: speedLimit ? Number(speedLimit) : null,
    };
    try {
      if (isEdit) await api.put(`/api/plans/${editing!.id}`, payload);
      else        await api.post("/api/plans", payload);
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
          <span className="text-sm mb-1 block">类型</span>
          <select className="select" value={quotaType} onChange={(e) => setQuotaType(e.target.value as never)}>
            <option value="permanent">permanent</option>
            <option value="monthly">monthly</option>
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
      <div className="grid grid-cols-3 gap-3">
        <label className="block">
          <span className="text-sm mb-1 block">月重置日</span>
          <input
            className="input"
            type="number"
            min={1}
            max={28}
            value={resetDay}
            onChange={(e) => setResetDay(Number(e.target.value))}
          />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">有效期 (天)</span>
          <input className="input" value={durationDays} onChange={(e) => setDurationDays(e.target.value)} placeholder="留空=不过期" />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">限速 (Mbps)</span>
          <input className="input" value={speedLimit} onChange={(e) => setSpeedLimit(e.target.value)} placeholder="留空=不限" />
        </label>
      </div>
      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary">{isEdit ? "保存修改" : "创建"}</button>
      </div>
    </form>
  );
}
