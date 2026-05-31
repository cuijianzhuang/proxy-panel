import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, ApiError, Plan } from "../lib/api";
import { Modal } from "../components/Modal";
import { SearchSelect } from "../components/SearchSelect";

export function Plans() {
  const [rows, setRows] = useState<Plan[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [editing, setEditing] = useState<Plan | null>(null);

  const reload = useCallback(async () => {
    try { setRows(await api.get<Plan[]>("/api/plans")); setErr(null); }
    catch (e) { setErr(String(e)); }
  }, []);
  useEffect(() => { reload(); }, [reload]);

  async function remove(id: number) {
    if (!confirm("删除该套餐？已关联的用户将失去套餐，流量限额改用户自身设置。")) return;
    try { await api.del(`/api/plans/${id}`); reload(); }
    catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }

  return (
    <div>
      <header className="mb-5 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold">套餐管理</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>共 {rows?.length ?? "—"} 个套餐</p>
        </div>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">＋ 新建套餐</button>
      </header>

      {err && <div className="card p-3 mb-4 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      {/* Card grid view */}
      <div className="grid gap-4" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(280px,1fr))" }}>
        {rows?.map((p) => <PlanCard key={p.id} plan={p} onEdit={setEditing} onDelete={remove} />)}
        {rows?.length === 0 && (
          <div className="card p-8 text-center text-sm col-span-full" style={{ color: "var(--fg-muted)" }}>
            暂无套餐 — 点右上角新建
          </div>
        )}
      </div>

      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建套餐" size="lg">
        <PlanForm onSaved={() => { setShowNew(false); reload(); }} />
      </Modal>
      <Modal open={editing !== null} onClose={() => setEditing(null)} title={`编辑套餐 — ${editing?.name ?? ""}`} size="lg">
        {editing && (
          <PlanForm key={editing.id} editing={editing} onSaved={() => { setEditing(null); reload(); }} />
        )}
      </Modal>
    </div>
  );
}

// ── Plan card ─────────────────────────────────────────────────────────────────
function PlanCard({ plan: p, onEdit, onDelete }: {
  plan: Plan;
  onEdit: (p: Plan) => void;
  onDelete: (id: number) => void;
}) {
  return (
    <div className="card p-4 flex flex-col gap-3">
      {/* header */}
      <div className="flex items-start justify-between">
        <div>
          <div className="font-semibold text-base">{p.name}</div>
          {p.description && (
            <div className="text-xs mt-0.5" style={{ color: "var(--fg-muted)" }}>{p.description}</div>
          )}
        </div>
        {p.price_monthly != null && (
          <div className="text-base font-bold" style={{ color: "var(--accent)" }}>
            ¥{p.price_monthly.toFixed(0)}<span className="text-xs font-normal text-inherit opacity-60">/月</span>
          </div>
        )}
      </div>

      {/* stats */}
      <div className="grid grid-cols-2 gap-2 text-sm">
        <Stat label="流量配额" value={p.quota_gb === 0 ? "无限" : `${p.quota_gb} GB`} />
        <Stat label="类型" value={p.quota_type === "monthly" ? "月重置" : "永久"} />
        <Stat label="设备上限" value={p.device_limit != null ? `${p.device_limit} 台` : "无限"} />
        <Stat label="限速" value={p.speed_limit_mbps != null ? `${p.speed_limit_mbps} Mbps` : "不限"} />
        {p.duration_days != null && <Stat label="有效期" value={`${p.duration_days} 天`} />}
        {p.quota_type === "monthly" && <Stat label="重置日" value={`每月 ${p.quota_reset_day} 日`} />}
      </div>

      {/* actions */}
      <div className="flex gap-2 pt-1" style={{ borderTop: "1px solid var(--border)" }}>
        <button className="btn btn-ghost btn-sm flex-1" onClick={() => onEdit(p)}>编辑</button>
        <button className="btn btn-danger btn-sm flex-1" onClick={() => onDelete(p.id)}>删除</button>
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs" style={{ color: "var(--fg-muted)" }}>{label}</div>
      <div className="font-medium">{value}</div>
    </div>
  );
}

// ── Plan form ─────────────────────────────────────────────────────────────────
function PlanForm({ editing, onSaved }: { editing?: Plan | null; onSaved: () => void }) {
  const isEdit = !!editing;
  const [name,          setName]          = useState(editing?.name ?? "");
  const [description,   setDescription]   = useState(editing?.description ?? "");
  const [quotaType,     setQuotaType]     = useState<"permanent" | "monthly">(editing?.quota_type ?? "monthly");
  const [quotaGb,       setQuotaGb]       = useState(editing?.quota_gb ?? 50);
  const [resetDay,      setResetDay]      = useState(editing?.quota_reset_day ?? 1);
  const [durationDays,  setDurationDays]  = useState(editing?.duration_days != null ? String(editing.duration_days) : "");
  const [deviceLimit,   setDeviceLimit]   = useState(editing?.device_limit != null ? String(editing.device_limit) : "");
  const [speedLimit,    setSpeedLimit]    = useState(editing?.speed_limit_mbps != null ? String(editing.speed_limit_mbps) : "");
  const [priceMonthly,  setPriceMonthly]  = useState(editing?.price_monthly != null ? String(editing.price_monthly) : "");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    const payload = {
      name,
      description: description || null,
      quota_type:  quotaType,
      quota_gb:    quotaGb,
      quota_reset_day: resetDay,
      duration_days:   durationDays ? Number(durationDays) : null,
      device_limit:    deviceLimit  ? Number(deviceLimit)  : null,
      speed_limit_mbps: speedLimit  ? Number(speedLimit)   : null,
      price_monthly:   priceMonthly ? Number(priceMonthly) : null,
    };
    try {
      if (isEdit) await api.put(`/api/plans/${editing!.id}`, payload);
      else        await api.post("/api/plans", payload);
      onSaved();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    } finally { setBusy(false); }
  }

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      {/* Basic */}
      <div className="grid grid-cols-2 gap-3">
        <label className="block col-span-2">
          <span className="text-sm mb-1 block">套餐名称 *</span>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} required placeholder="如: 基础版 / 专业版" />
        </label>
        <label className="block col-span-2">
          <span className="text-sm mb-1 block">描述(可选)</span>
          <input className="input" value={description} onChange={(e) => setDescription(e.target.value)} placeholder="简短说明，展示给用户" />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">月价格(¥)</span>
          <input className="input" type="number" min={0} step="0.01" value={priceMonthly}
                 onChange={(e) => setPriceMonthly(e.target.value)} placeholder="留空=不显示" />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">类型</span>
          <SearchSelect
            value={quotaType}
            onChange={(v) => setQuotaType(v as "permanent" | "monthly")}
            options={[
              { value: "monthly",   label: "月流量",   sub: "每月自动重置" },
              { value: "permanent", label: "永久流量", sub: "不重置，用完为止" },
            ]}
          />
        </label>
      </div>

      {/* Traffic */}
      <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
        <legend className="text-sm font-semibold px-1">📊 流量 &amp; 周期</legend>
        <div className="grid grid-cols-2 gap-3">
          <label className="block">
            <span className="text-sm mb-1 block">流量配额 (GB，0=无限)</span>
            <input className="input" type="number" min={0} step="0.1" value={quotaGb}
                   onChange={(e) => setQuotaGb(Number(e.target.value))} />
          </label>
          {quotaType === "monthly" && (
            <label className="block">
              <span className="text-sm mb-1 block">每月重置日 (1–28)</span>
              <input className="input" type="number" min={1} max={28} value={resetDay}
                     onChange={(e) => setResetDay(Number(e.target.value))} />
            </label>
          )}
          <label className="block">
            <span className="text-sm mb-1 block">有效期(天，留空=永不过期)</span>
            <input className="input" type="number" min={1} value={durationDays}
                   onChange={(e) => setDurationDays(e.target.value)} placeholder="如: 30 / 365" />
          </label>
        </div>
      </fieldset>

      {/* Limits */}
      <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
        <legend className="text-sm font-semibold px-1">🔒 限制</legend>
        <div className="grid grid-cols-2 gap-3">
          <label className="block">
            <span className="text-sm mb-1 block">设备上限(留空=无限)</span>
            <input className="input" type="number" min={1} value={deviceLimit}
                   onChange={(e) => setDeviceLimit(e.target.value)} placeholder="如: 3" />
          </label>
          <label className="block">
            <span className="text-sm mb-1 block">限速 Mbps(留空=不限)</span>
            <input className="input" type="number" min={1} value={speedLimit}
                   onChange={(e) => setSpeedLimit(e.target.value)} placeholder="如: 100" />
          </label>
        </div>
      </fieldset>

      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "保存中…" : isEdit ? "保存修改" : "创建套餐"}
        </button>
      </div>
    </form>
  );
}
