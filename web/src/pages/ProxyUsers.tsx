import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, ApiError, Listener, Plan, ProxyUser } from "../lib/api";
import { Modal } from "../components/Modal";
import { SearchSelect } from "../components/SearchSelect";

// ── helpers ──────────────────────────────────────────────────────────────────

function fmtBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  const k = b / 1024;
  if (k < 1024) return `${k.toFixed(1)} KB`;
  const m = k / 1024;
  if (m < 1024) return `${m.toFixed(2)} MB`;
  const g = m / 1024;
  return `${g.toFixed(2)} GB`;
}

function fmtDate(iso: string | null): string {
  if (!iso) return "—";
  return new Date(iso).toLocaleDateString("zh-CN", { year:"numeric", month:"2-digit", day:"2-digit" });
}

function relTime(iso: string | null): string {
  if (!iso) return "从未";
  const d = (Date.now() - new Date(iso).getTime()) / 1000;
  if (d < 60) return `${Math.floor(d)} 秒前`;
  if (d < 3600) return `${Math.floor(d/60)} 分钟前`;
  if (d < 86400) return `${Math.floor(d/3600)} 小时前`;
  return `${Math.floor(d/86400)} 天前`;
}

function TrafficBar({ used, quota }: { used: number; quota: number }) {
  const pct = quota > 0 ? Math.min((used / (quota * 1073741824)) * 100, 100) : 0;
  const color = pct > 90 ? "#ef4444" : pct > 70 ? "#f59e0b" : "#22c55e";
  return (
    <div>
      <div className="text-xs font-mono">{fmtBytes(used)} / {quota === 0 ? "∞" : `${quota} GB`}</div>
      {quota > 0 && (
        <div style={{ height: 4, background: "var(--border)", borderRadius: 99, marginTop: 3, overflow: "hidden" }}>
          <div style={{ width: `${pct}%`, height: "100%", background: color, borderRadius: 99, transition: "width 400ms ease" }} />
        </div>
      )}
    </div>
  );
}

// ── Main list ─────────────────────────────────────────────────────────────────

export function ProxyUsers() {
  const [rows, setRows] = useState<ProxyUser[] | null>(null);
  const [plans, setPlans] = useState<Plan[]>([]);
  const [listeners, setListeners] = useState<Listener[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [editing, setEditing] = useState<ProxyUser | null>(null);
  const [attach, setAttach] = useState<ProxyUser | null>(null);
  const [detail, setDetail] = useState<ProxyUser | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  const reload = useCallback(async () => {
    try {
      const [us, ps, ls] = await Promise.all([
        api.get<ProxyUser[]>("/api/proxy-users"),
        api.get<Plan[]>("/api/plans"),
        api.get<Listener[]>("/api/listeners"),
      ]);
      setRows(us); setPlans(ps); setListeners(ls); setErr(null);
    } catch (e) { setErr(String(e)); }
  }, []);
  useEffect(() => { reload(); }, [reload]);

  function showToast(msg: string) {
    setToast(msg); setTimeout(() => setToast(null), 4000);
  }

  async function remove(id: number) {
    if (!confirm("删除该用户？操作不可撤销，订阅链接立即失效。")) return;
    try { await api.del(`/api/proxy-users/${id}`); reload(); }
    catch (e) { showToast(e instanceof ApiError ? e.message : String(e)); }
  }

  async function kickUser(id: number) {
    if (!confirm("强制踢出并停用该用户？其订阅链接将立即失效，需手动重新启用。")) return;
    try {
      await api.post(`/api/proxy-users/${id}/kick`);
      showToast("已停用并踢出该用户"); reload();
    } catch (e) { showToast(String(e)); }
  }

  async function enableUser(id: number) {
    try {
      await api.post(`/api/proxy-users/${id}/enable`);
      showToast("已重新启用"); reload();
    } catch (e) { showToast(String(e)); }
  }

  async function resetTraffic(id: number) {
    if (!confirm("重置该用户流量统计（used_bytes → 0）？")) return;
    try {
      await api.post(`/api/proxy-users/${id}/reset-traffic`);
      showToast("流量已重置"); reload();
    } catch (e) { showToast(String(e)); }
  }

  const subUrl = (token: string) => `${location.protocol}//${location.host}/sub/${token}`;
  const planName = (id: number | null) => plans.find(p => p.id === id)?.name ?? "—";

  const filtered = (rows ?? []).filter(u =>
    !filter ||
    u.name.toLowerCase().includes(filter.toLowerCase()) ||
    u.uuid.startsWith(filter) ||
    u.tags.some(t => t.includes(filter))
  );

  const total   = rows?.length ?? 0;
  const active  = rows?.filter(u => u.enabled).length ?? 0;

  return (
    <div>
      <header className="mb-5 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">代理用户</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            共 {total} 个用户 · {active} 个启用
          </p>
        </div>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">＋ 新建用户</button>
      </header>

      {toast && <div className="card p-3 mb-3 text-sm" style={{ color: "var(--accent)" }}>{toast}</div>}
      {err   && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      <input className="input mb-3" placeholder="🔍 搜索名称 / UUID / 标签…"
             value={filter} onChange={(e) => setFilter(e.target.value)} />

      <div className="card overflow-hidden">
        <table className="table">
          <thead>
            <tr>
              <th>用户</th>
              <th>套餐</th>
              <th>流量</th>
              <th>设备/速度</th>
              <th>到期</th>
              <th>最近活跃</th>
              <th>状态</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((u) => (
              <tr key={u.id}>
                {/* User col */}
                <td>
                  <button
                    className="text-left hover:underline font-medium"
                    style={{ color: "var(--accent)", cursor: "pointer", border: "none", background: "none" }}
                    onClick={() => setDetail(u)}
                  >{u.name}</button>
                  <div className="text-xs font-mono" style={{ color: "var(--fg-muted)" }}>
                    {u.uuid.slice(0, 8)}…
                  </div>
                  {u.tags.length > 0 && (
                    <div className="flex flex-wrap gap-1 mt-1">
                      {u.tags.map(t => (
                        <span key={t} className="badge" style={{ fontSize: 10, padding: "1px 6px" }}>{t}</span>
                      ))}
                    </div>
                  )}
                </td>
                {/* Plan */}
                <td className="text-sm">{planName(u.plan_id)}</td>
                {/* Traffic */}
                <td><TrafficBar used={u.used_bytes} quota={u.quota_gb} /></td>
                {/* Device / speed */}
                <td>
                  <div className="text-xs">
                    {u.device_limit != null ? `≤${u.device_limit} 台` : "不限台数"}
                  </div>
                  <div className="text-xs" style={{ color: "var(--fg-muted)" }}>
                    {u.speed_limit_mbps != null ? `${u.speed_limit_mbps} Mbps` : "不限速"}
                  </div>
                </td>
                {/* Expires */}
                <td className="text-xs">
                  {u.expires_at ? (
                    <span style={{ color: new Date(u.expires_at) < new Date() ? "#ef4444" : "inherit" }}>
                      {fmtDate(u.expires_at)}
                    </span>
                  ) : "永不过期"}
                </td>
                {/* Last seen */}
                <td className="text-xs" style={{ color: "var(--fg-muted)" }}>
                  <div>{relTime(u.last_seen_at)}</div>
                  {u.last_seen_ip && <div className="font-mono">{u.last_seen_ip}</div>}
                </td>
                {/* Status */}
                <td>
                  <span style={{
                    width: 8, height: 8, borderRadius: "50%", display: "inline-block",
                    background: u.enabled ? "#22c55e" : "#94a3b8",
                    boxShadow: u.enabled ? "0 0 0 2px #bbf7d0" : undefined,
                  }} />
                </td>
                {/* Actions */}
                <td className="text-right">
                  <div className="flex gap-1 justify-end flex-wrap">
                    <button className="btn btn-ghost btn-sm" onClick={() => setEditing(u)}>编辑</button>
                    <button className="btn btn-ghost btn-sm" onClick={() => setAttach(u)}>关联</button>
                    <button className="btn btn-ghost btn-sm" onClick={() => {
                      navigator.clipboard.writeText(subUrl(u.subscription_token)).catch(() => {});
                    }} title={subUrl(u.subscription_token)}>复制订阅</button>
                    <button className="btn btn-ghost btn-sm"
                            onClick={() => resetTraffic(u.id)} title="重置流量">↺</button>
                    {u.enabled
                      ? <button className="btn btn-danger btn-sm" onClick={() => kickUser(u.id)}>踢出</button>
                      : <button className="btn btn-ghost btn-sm" onClick={() => enableUser(u.id)}>启用</button>
                    }
                    <button className="btn btn-danger btn-sm" onClick={() => remove(u.id)}>删除</button>
                  </div>
                </td>
              </tr>
            ))}
            {filtered.length === 0 && (
              <tr><td colSpan={8} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>
                {rows?.length === 0 ? "暂无用户" : "没有匹配的用户"}
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Modals */}
      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建代理用户" size="lg">
        <UserForm plans={plans} onSaved={() => { setShowNew(false); reload(); }} />
      </Modal>
      <Modal open={editing !== null} onClose={() => setEditing(null)} title={`编辑用户 — ${editing?.name ?? ""}`} size="lg">
        {editing && <UserForm key={editing.id} editing={editing} plans={plans} onSaved={() => { setEditing(null); reload(); }} />}
      </Modal>
      <Modal open={detail !== null} onClose={() => setDetail(null)} title={`用户详情 — ${detail?.name ?? ""}`} size="lg">
        {detail && <UserDetail user={detail} plans={plans} onClose={() => setDetail(null)} onRefresh={() => { setDetail(null); reload(); }} />}
      </Modal>
      <Modal open={attach !== null} onClose={() => setAttach(null)} title={`关联监听器 — ${attach?.name ?? ""}`}>
        {attach && <AttachForm user={attach} listeners={listeners} onClose={() => setAttach(null)} />}
      </Modal>
    </div>
  );
}

// ── User detail modal ─────────────────────────────────────────────────────────
function UserDetail({ user: u, plans, onClose, onRefresh }: {
  user: ProxyUser; plans: Plan[];
  onClose: () => void; onRefresh: () => void;
}) {
  const plan = plans.find(p => p.id === u.plan_id);
  const subUrl = `${location.protocol}//${location.host}/sub/${u.subscription_token}`;

  async function kick() {
    if (!confirm("踢出并停用？")) return;
    await api.post(`/api/proxy-users/${u.id}/kick`).catch(console.error);
    onRefresh();
  }
  async function resetTraffic() {
    await api.post(`/api/proxy-users/${u.id}/reset-traffic`).catch(console.error);
    onRefresh();
  }
  async function rotateToken() {
    await api.put(`/api/proxy-users/${u.id}`, { rotate_subscription_token: true }).catch(console.error);
    onRefresh();
  }

  const Row = ({ label, children }: { label: string; children: React.ReactNode }) => (
    <div className="flex justify-between py-2" style={{ borderBottom: "1px solid var(--border)" }}>
      <span className="text-sm" style={{ color: "var(--fg-muted)" }}>{label}</span>
      <span className="text-sm font-medium">{children}</span>
    </div>
  );

  return (
    <div className="space-y-4">
      {/* info grid */}
      <div className="card p-4">
        <Row label="UUID"><code className="text-xs font-mono">{u.uuid}</code></Row>
        <Row label="套餐">{plan?.name ?? "— 无套餐"}</Row>
        <Row label="启用状态">
          <span style={{ color: u.enabled ? "#16a34a" : "#dc2626" }}>
            {u.enabled ? "✓ 启用" : "✗ 停用"}
          </span>
        </Row>
        <Row label="流量已用 / 配额">
          {fmtBytes(u.used_bytes)} / {u.quota_gb === 0 ? "无限" : `${u.quota_gb} GB`}
        </Row>
        <Row label="设备上限">{u.device_limit != null ? `${u.device_limit} 台` : "不限"}</Row>
        <Row label="限速">{u.speed_limit_mbps != null ? `${u.speed_limit_mbps} Mbps` : "不限"}</Row>
        <Row label="到期时间">{u.expires_at ? new Date(u.expires_at).toLocaleDateString("zh-CN") : "永不过期"}</Row>
        <Row label="最近在线">{relTime(u.last_seen_at)}{u.last_seen_ip ? ` (${u.last_seen_ip})` : ""}</Row>
        <Row label="最近重置">{relTime(u.last_reset_at)}</Row>
        <Row label="创建时间">{new Date(u.created_at).toLocaleString("zh-CN", { hour12: false })}</Row>
        {u.note && <Row label="备注">{u.note}</Row>}
        {u.tags.length > 0 && (
          <Row label="标签">
            <div className="flex flex-wrap gap-1 justify-end">
              {u.tags.map(t => <span key={t} className="badge text-xs">{t}</span>)}
            </div>
          </Row>
        )}
      </div>

      {/* subscription */}
      <div className="card p-4 space-y-2">
        <div className="text-sm font-semibold">订阅链接</div>
        <div className="font-mono text-xs break-all p-2 rounded-md" style={{ background: "var(--bg)" }}>
          {subUrl}
        </div>
        <div className="flex gap-2">
          <button className="btn btn-ghost btn-sm flex-1"
                  onClick={() => navigator.clipboard.writeText(subUrl).catch(() => {})}>
            复制链接
          </button>
          <button className="btn btn-danger btn-sm flex-1" onClick={rotateToken}>
            轮换 token (旧链接失效)
          </button>
        </div>
      </div>

      {/* actions */}
      <div className="flex gap-2">
        <button className="btn btn-ghost btn-sm flex-1" onClick={resetTraffic}>↺ 重置流量</button>
        {u.enabled
          ? <button className="btn btn-danger btn-sm flex-1" onClick={kick}>⛔ 踢出 &amp; 停用</button>
          : <button className="btn btn-ghost btn-sm flex-1"
                    onClick={() => api.post(`/api/proxy-users/${u.id}/enable`).then(onRefresh)}>
              ✓ 重新启用
            </button>
        }
        <button className="btn btn-ghost btn-sm" onClick={onClose}>关闭</button>
      </div>
    </div>
  );
}

// ── User form ─────────────────────────────────────────────────────────────────
function UserForm({ editing, plans, onSaved }: {
  editing?: ProxyUser | null; plans: Plan[]; onSaved: () => void;
}) {
  const isEdit = !!editing;
  const [name,         setName]         = useState(editing?.name ?? "");
  const [planId,       setPlanId]       = useState<number | null>(editing?.plan_id ?? null);
  const [quotaGb,      setQuotaGb]      = useState(editing?.quota_gb ?? 0);
  const [quotaType,    setQuotaType]    = useState<"permanent"|"monthly">(editing?.quota_type ?? "monthly");
  const [resetDay,     setResetDay]     = useState(editing?.quota_reset_day ?? 1);
  const [deviceLimit,  setDeviceLimit]  = useState(editing?.device_limit != null ? String(editing.device_limit) : "");
  const [speedLimit,   setSpeedLimit]   = useState(editing?.speed_limit_mbps != null ? String(editing.speed_limit_mbps) : "");
  const [expiresAt,    setExpiresAt]    = useState(
    editing?.expires_at ? new Date(editing.expires_at).toISOString().slice(0, 10) : ""
  );
  const [enabled,      setEnabled]      = useState(editing?.enabled ?? true);
  const [note,         setNote]         = useState(editing?.note ?? "");
  const [tagsStr,      setTagsStr]      = useState(editing?.tags.join(", ") ?? "");
  const [rotate,       setRotate]       = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // When a plan is selected, auto-fill quota from plan
  function selectPlan(id: number | null) {
    setPlanId(id);
    const p = plans.find(pl => pl.id === id);
    if (p) {
      setQuotaGb(p.quota_gb);
      setQuotaType(p.quota_type);
      setResetDay(p.quota_reset_day);
      if (p.device_limit != null) setDeviceLimit(String(p.device_limit));
      if (p.speed_limit_mbps != null) setSpeedLimit(String(p.speed_limit_mbps));
      if (p.duration_days != null) {
        const exp = new Date();
        exp.setDate(exp.getDate() + p.duration_days);
        setExpiresAt(exp.toISOString().slice(0, 10));
      }
    }
  }

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    const tags = tagsStr.split(",").map(t => t.trim()).filter(Boolean);
    const payload: Record<string, unknown> = {
      name,
      plan_id:          planId,
      quota_gb:         quotaGb,
      quota_type:       quotaType,
      quota_reset_day:  resetDay,
      device_limit:     deviceLimit  ? Number(deviceLimit)  : null,
      speed_limit_mbps: speedLimit   ? Number(speedLimit)   : null,
      expires_at:       expiresAt    ? new Date(expiresAt).toISOString() : null,
      enabled,
      note: note || null,
      tags,
    };
    if (isEdit) payload.rotate_subscription_token = rotate;
    try {
      if (isEdit) await api.put(`/api/proxy-users/${editing!.id}`, payload);
      else        await api.post("/api/proxy-users", payload);
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
          <span className="text-sm mb-1 block">用户名 *</span>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} required />
        </label>

        <label className="block col-span-2">
          <span className="text-sm mb-1 block">绑定套餐（自动填充配额）</span>
          <SearchSelect
            value={planId != null ? String(planId) : ""}
            onChange={(v) => selectPlan(v ? Number(v) : null)}
            placeholder="— 无套餐，手动配置 —"
            searchPlaceholder="搜索套餐名称…"
            options={[
              { value: "", label: "— 无套餐 —", sub: "手动配置所有参数" },
              ...plans.map(p => ({
                value: String(p.id),
                label: p.name,
                sub: [
                  p.description,
                  p.quota_gb > 0 ? `${p.quota_gb}GB` : "无限流量",
                  p.price_monthly != null ? `¥${p.price_monthly}/月` : null,
                  p.device_limit != null ? `${p.device_limit}台` : null,
                ].filter(Boolean).join(" · "),
              })),
            ]}
          />
        </label>
      </div>

      {/* Traffic / limits */}
      <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
        <legend className="text-sm font-semibold px-1">📊 流量配置</legend>
        <div className="grid grid-cols-2 gap-3">
          <label className="block">
            <span className="text-sm mb-1 block">流量配额 GB (0=无限)</span>
            <input className="input" type="number" min={0} step="0.1" value={quotaGb}
                   onChange={(e) => setQuotaGb(Number(e.target.value))} />
          </label>
          <label className="block">
            <span className="text-sm mb-1 block">流量类型</span>
            <SearchSelect
              value={quotaType}
              onChange={(v) => setQuotaType(v as "permanent" | "monthly")}
              options={[
                { value: "monthly",   label: "月流量",   sub: "每月自动重置" },
                { value: "permanent", label: "永久流量", sub: "不重置，用完为止" },
              ]}
            />
          </label>
          {quotaType === "monthly" && (
            <label className="block">
              <span className="text-sm mb-1 block">重置日 (1–28)</span>
              <input className="input" type="number" min={1} max={28} value={resetDay}
                     onChange={(e) => setResetDay(Number(e.target.value))} />
            </label>
          )}
          <label className="block">
            <span className="text-sm mb-1 block">到期日 (留空=永不)</span>
            <input className="input" type="date" value={expiresAt}
                   onChange={(e) => setExpiresAt(e.target.value)} />
          </label>
        </div>
      </fieldset>

      {/* Limits */}
      <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
        <legend className="text-sm font-semibold px-1">🔒 限制</legend>
        <div className="grid grid-cols-2 gap-3">
          <label className="block">
            <span className="text-sm mb-1 block">设备上限 (留空=无限)</span>
            <input className="input" type="number" min={1} value={deviceLimit}
                   onChange={(e) => setDeviceLimit(e.target.value)} placeholder="如: 3" />
          </label>
          <label className="block">
            <span className="text-sm mb-1 block">限速 Mbps (留空=不限)</span>
            <input className="input" type="number" min={1} value={speedLimit}
                   onChange={(e) => setSpeedLimit(e.target.value)} placeholder="如: 100" />
          </label>
        </div>
      </fieldset>

      {/* Misc */}
      <div className="grid grid-cols-2 gap-3">
        <label className="block col-span-2">
          <span className="text-sm mb-1 block">标签 (逗号分隔)</span>
          <input className="input" value={tagsStr} onChange={(e) => setTagsStr(e.target.value)}
                 placeholder="vip, trial, reseller" />
        </label>
        <label className="block col-span-2">
          <span className="text-sm mb-1 block">备注</span>
          <input className="input" value={note} onChange={(e) => setNote(e.target.value)} />
        </label>
      </div>

      <label className="flex items-center gap-2 text-sm cursor-pointer">
        <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
        启用
      </label>

      {isEdit && (
        <>
          <label className="flex items-center gap-2 text-sm cursor-pointer">
            <input type="checkbox" checked={rotate} onChange={(e) => setRotate(e.target.checked)} />
            轮换订阅 token（旧链接立即失效）
          </label>
          {isEdit && (
            <div className="text-xs font-mono break-all" style={{ color: "var(--fg-muted)" }}>
              UUID: {editing!.uuid}
            </div>
          )}
        </>
      )}

      {!isEdit && (
        <p className="text-xs" style={{ color: "var(--fg-muted)" }}>
          UUID、密码、订阅 token 自动生成；可在编辑页查看。
        </p>
      )}

      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "保存中…" : isEdit ? "保存修改" : "创建用户"}
        </button>
      </div>
    </form>
  );
}

// ── Attach form ───────────────────────────────────────────────────────────────
function AttachForm({ user, listeners, onClose }: {
  user: ProxyUser; listeners: Listener[]; onClose: () => void;
}) {
  const [attached, setAttached] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const out = new Set<number>();
        for (const l of listeners) {
          const us = await api.get<ProxyUser[]>(`/api/listeners/${l.id}/clients`);
          if (us.find(u => u.id === user.id)) out.add(l.id);
        }
        setAttached(out);
      } catch (e) { setErr(String(e)); }
    })();
  }, [listeners, user.id]);

  async function toggle(lid: number, on: boolean) {
    setBusy(true);
    try {
      if (on) await api.post(`/api/listeners/${lid}/clients`, { proxy_user_id: user.id });
      else    await api.del(`/api/listeners/${lid}/clients/${user.id}`);
      setAttached(prev => { const n = new Set(prev); if (on) n.add(lid); else n.delete(lid); return n; });
    } catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
    finally { setBusy(false); }
  }

  return (
    <div className="space-y-2">
      {listeners.length === 0 && <p className="text-sm" style={{ color: "var(--fg-muted)" }}>还没有监听器。</p>}
      {listeners.map(l => (
        <label key={l.id} className="flex items-center gap-2 p-2 rounded hover:bg-black/5 cursor-pointer">
          <input type="checkbox" checked={attached.has(l.id)} disabled={busy}
                 onChange={(e) => toggle(l.id, e.target.checked)} />
          <span className="text-sm">
            #{l.id} {l.name}
            <span style={{ color: "var(--fg-muted)" }}> · {l.protocol}/{l.transport} · :{l.port}</span>
          </span>
        </label>
      ))}
      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end pt-3">
        <button className="btn btn-ghost" onClick={onClose}>完成</button>
      </div>
    </div>
  );
}
