import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, ApiError, ChannelType, NotificationChannel, NotificationRule } from "../lib/api";
import { Modal } from "../components/Modal";
import { SearchSelect } from "../components/SearchSelect";

/*
 * 告警通知 — two halves:
 *   1. Channels: telegram / webhook / smtp endpoints (type-specific config).
 *   2. Rules: per-event-type checkbox grid mapping events → channels.
 *
 * The "测试" button on a channel hits /api/notifications/:id/test which fires
 * a real sample message and reports success/failure inline.
 */

const EVENT_LABELS: Record<string, string> = {
  node_offline:  "节点离线",
  node_deployed: "节点部署完成",
  quota_exceed:  "用户超额",
  backup:        "备份完成",
  task_failed:   "任务失败",
  cert_expiring: "证书即将过期",
};

export function Notifications() {
  const [channels, setChannels] = useState<NotificationChannel[]>([]);
  const [rules, setRules] = useState<NotificationRule[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const [ch, rl] = await Promise.all([
        api.get<NotificationChannel[]>("/api/notifications"),
        api.get<NotificationRule[]>("/api/notification-rules"),
      ]);
      setChannels(ch);
      setRules(rl);
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);
  useEffect(() => { reload(); }, [reload]);

  function flash(msg: string) {
    setToast(msg);
    setTimeout(() => setToast(null), 5000);
  }

  async function testChannel(id: number) {
    try {
      const r = await api.post<{ ok: boolean; detail: string }>(`/api/notifications/${id}/test`);
      flash(r.ok ? "✓ 测试消息已发送" : `✗ 发送失败:${r.detail}`);
    } catch (e) {
      flash(e instanceof ApiError ? e.message : String(e));
    }
  }
  async function toggleChannel(c: NotificationChannel) {
    try { await api.put(`/api/notifications/${c.id}`, { enabled: !c.enabled }); reload(); }
    catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }
  async function removeChannel(id: number) {
    if (!confirm("删除该通道?引用它的规则会自动忽略。")) return;
    try { await api.del(`/api/notifications/${id}`); reload(); }
    catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }
  async function toggleRuleChannel(rule: NotificationRule, channelId: number) {
    const has = rule.channel_ids.includes(channelId);
    const next = has
      ? rule.channel_ids.filter((x) => x !== channelId)
      : [...rule.channel_ids, channelId];
    try {
      await api.put(`/api/notification-rules/${rule.event_type}`, {
        channel_ids: next, enabled: rule.enabled,
      });
      reload();
    } catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }

  return (
    <div>
      <header className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-semibold">告警通知</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            通道发消息,规则决定哪些事件发到哪些通道。
          </p>
        </div>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">＋ 新建通道</button>
      </header>

      {toast && <div className="card p-3 mb-3 text-sm" style={{ color: "var(--accent)" }}>{toast}</div>}
      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      {/* Channels */}
      <div className="card overflow-hidden mb-6">
        <table className="table">
          <thead>
            <tr><th>ID</th><th>名称</th><th>类型</th><th>启用</th><th className="text-right">操作</th></tr>
          </thead>
          <tbody>
            {channels.map((c) => (
              <tr key={c.id}>
                <td className="font-mono text-xs">{c.id}</td>
                <td>{c.name}</td>
                <td><span className="badge">{c.type}</span></td>
                <td>
                  <button onClick={() => toggleChannel(c)}
                          className={`badge ${c.enabled ? "badge-ok" : "badge-err"}`}
                          style={{ cursor: "pointer", border: "none" }}>
                    {c.enabled ? "✓ 启用" : "✗ 停用"}
                  </button>
                </td>
                <td className="text-right">
                  <div className="flex gap-2 justify-end">
                    <button className="btn btn-ghost" onClick={() => testChannel(c.id)}>测试</button>
                    <button className="btn btn-danger" onClick={() => removeChannel(c.id)}>删除</button>
                  </div>
                </td>
              </tr>
            ))}
            {channels.length === 0 && (
              <tr><td colSpan={5} className="text-center py-6 text-sm" style={{ color: "var(--fg-muted)" }}>
                还没有通道 — 新建一个 Telegram / Webhook / SMTP 通道。
              </td></tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Rules grid */}
      <h2 className="font-semibold mb-2">事件路由</h2>
      <div className="card overflow-x-auto">
        <table className="table">
          <thead>
            <tr>
              <th>事件</th>
              {channels.map((c) => <th key={c.id} className="text-center">{c.name}</th>)}
            </tr>
          </thead>
          <tbody>
            {rules.map((r) => (
              <tr key={r.event_type}>
                <td>
                  <div>{EVENT_LABELS[r.event_type] ?? r.event_type}</div>
                  <div className="text-xs font-mono" style={{ color: "var(--fg-muted)" }}>{r.event_type}</div>
                </td>
                {channels.map((c) => (
                  <td key={c.id} className="text-center">
                    <input type="checkbox"
                           checked={r.channel_ids.includes(c.id)}
                           onChange={() => toggleRuleChannel(r, c.id)} />
                  </td>
                ))}
              </tr>
            ))}
            {channels.length === 0 && (
              <tr><td className="py-4 text-sm" style={{ color: "var(--fg-muted)" }}>先建通道,这里才能勾选路由。</td></tr>
            )}
          </tbody>
        </table>
      </div>

      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建通知通道">
        <NewChannelForm onCreated={() => { setShowNew(false); reload(); }} />
      </Modal>
    </div>
  );
}

function NewChannelForm({ onCreated }: { onCreated: () => void }) {
  const [name, setName] = useState("");
  const [type, setType] = useState<ChannelType>("telegram");
  const [cfg, setCfg] = useState<Record<string, string>>({});
  const [err, setErr] = useState<string | null>(null);

  const set = (k: string) => (e: { target: { value: string } }) =>
    setCfg((prev) => ({ ...prev, [k]: e.target.value }));

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    // Coerce smtp port to a number if present.
    const config: Record<string, unknown> = { ...cfg };
    if (type === "smtp" && cfg.port) config.port = Number(cfg.port);
    try {
      await api.post("/api/notifications", { name, type, config, enabled: true });
      onCreated();
    } catch (e) { setErr(e instanceof ApiError ? e.message : String(e)); }
  }

  return (
    <form onSubmit={onSubmit} className="space-y-3">
      <div className="grid grid-cols-[1fr_auto] gap-3">
        <label className="block">
          <span className="text-sm mb-1 block">名称</span>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} required />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">类型</span>
          <SearchSelect
            value={type}
            onChange={(v) => { setType(v as ChannelType); setCfg({}); }}
            options={[
              { value: "telegram", label: "Telegram Bot", sub: "Bot API 推送消息" },
              { value: "webhook",  label: "Webhook",      sub: "HTTP POST 自定义 URL" },
              { value: "smtp",     label: "Email (SMTP)", sub: "发送邮件通知" },
            ]}
          />
        </label>
      </div>

      {type === "telegram" && (
        <>
          <Field label="Bot Token"><input className="input font-mono text-xs" onChange={set("bot_token")} placeholder="123456:ABC-DEF..." required /></Field>
          <Field label="Chat ID"><input className="input font-mono text-xs" onChange={set("chat_id")} placeholder="-1001234567890" required /></Field>
        </>
      )}
      {type === "webhook" && (
        <>
          <Field label="URL"><input className="input font-mono text-xs" onChange={set("url")} placeholder="https://example.com/hook" required /></Field>
          <div className="grid grid-cols-2 gap-3">
            <Field label="自定义 Header 名 (可选)"><input className="input font-mono text-xs" onChange={set("header_name")} placeholder="Authorization" /></Field>
            <Field label="Header 值 (可选)"><input className="input font-mono text-xs" onChange={set("header_value")} placeholder="Bearer ..." /></Field>
          </div>
        </>
      )}
      {type === "smtp" && (
        <>
          <div className="grid grid-cols-[1fr_120px] gap-3">
            <Field label="SMTP 主机"><input className="input" onChange={set("host")} placeholder="smtp.gmail.com" required /></Field>
            <Field label="端口"><input className="input" type="number" onChange={set("port")} placeholder="587" /></Field>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <Field label="用户名"><input className="input font-mono text-xs" onChange={set("username")} /></Field>
            <Field label="密码"><input className="input font-mono text-xs" type="password" onChange={set("password")} /></Field>
          </div>
          <div className="grid grid-cols-2 gap-3">
            <Field label="发件人 (from)"><input className="input font-mono text-xs" onChange={set("from")} placeholder="bot@example.com" required /></Field>
            <Field label="收件人 (to)"><input className="input font-mono text-xs" onChange={set("to")} placeholder="me@example.com" required /></Field>
          </div>
        </>
      )}

      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary">创建</button>
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
