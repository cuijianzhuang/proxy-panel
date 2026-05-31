import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { api, ApiError, Node, relativeTime } from "../lib/api";
import { Modal } from "../components/Modal";
import { badgeClass } from "./Dashboard";
import { useI18n } from "../lib/i18n";
import { SearchSelect } from "../components/SearchSelect";

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
      setToast(`已入队同步任务 #${r.task_id} — 去「任务」页查看日志`);
    } catch (e) {
      setToast(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusyId(null);
      setTimeout(() => setToast(null), 5000);
    }
  }

  async function provisionNow(id: number) {
    if (!confirm("将通过 SSH 连接节点，安装内核并部署配置。\n首次部署约需 1–3 分钟（取决于网速），可在「任务」页实时查看日志。\n\n确认初始化部署？")) return;
    setBusyId(id);
    try {
      const r = await api.post<{ task_id: number }>(`/api/nodes/${id}/provision`);
      setToast(`已入队初始化任务 #${r.task_id} — 去「任务」页查看部署日志`);
    } catch (e) {
      setToast(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusyId(null);
      setTimeout(() => setToast(null), 8000);
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

  const { t } = useI18n();

  return (
    <div>
      <header className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">{t.nodes.title}</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            {t.nodes.subtitle(totalCount, onlineCount)}
          </p>
        </div>
        <button onClick={() => setShowNew(true)} className="btn btn-primary">{t.nodes.addNode}</button>
      </header>

      <input
        className="input mb-3"
        placeholder={`🔍 ${t.common.search}…`}
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
              <th>{t.nodes.status}</th>
              <th>{t.nodes.core}</th>
              <th>{t.nodes.ssh}</th>
              <th>{t.nodes.lastSeen}</th>
              <th>{t.nodes.createdAt}</th>
              <th className="text-right">{t.nodes.actions}</th>
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
                    {n.status === "online"  ? t.nodes.online  :
                     n.status === "offline" ? t.nodes.offline :
                     n.status === "pending" ? t.nodes.pending : n.status}
                  </span>
                </td>
                <td>
                  <span className="badge">{n.core === "singbox" ? "sing-box" : "Xray"}</span>
                </td>
                <td>
                  <div className="text-xs">{n.ssh_user}:{n.ssh_port}</div>
                  <div className="text-xs" style={{ color: "var(--fg-muted)" }}>
                    {n.ssh_auth_method === "password" ? "🔑 密码" :
                     n.ssh_auth_method === "key" ? "🔑 私钥" : "🌐 全局密钥"}
                    {n.ssh_auth_method !== "global" && !n.has_ssh_credential && (
                      <span style={{ color: "#b45309" }}> ⚠ 未配置</span>
                    )}
                  </div>
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
                  <div className="flex gap-2 justify-end flex-wrap">
                    {/* 初始化部署: install core + start service */}
                    <button
                      className="btn btn-primary btn-sm"
                      onClick={() => provisionNow(n.id)}
                      disabled={busyId === n.id}
                      title={t.nodes.provision}
                      style={{ background: "#7c3aed" }}
                    >
                      {busyId === n.id ? "…" : t.nodes.provision}
                    </button>
                    <button
                      className="btn btn-primary btn-sm"
                      onClick={() => applyNow(n.id)}
                      disabled={busyId === n.id}
                      title={t.nodes.applyConfig}
                    >
                      {busyId === n.id ? "…" : t.nodes.applyConfig}
                    </button>
                    <button className="btn btn-ghost btn-sm" onClick={() => setEditing(n)}>{t.common.edit}</button>
                    <button className="btn btn-danger btn-sm" onClick={() => remove(n.id)}>{t.common.delete}</button>
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
  // SSH auth: how this node authenticates. Secrets are write-only — on edit we
  // start blank and only send a new value if the operator types one.
  const [authMethod, setAuthMethod] =
    useState<"global" | "password" | "key">(editing?.ssh_auth_method ?? "global");
  const [sshPassword, setSshPassword] = useState("");
  const [sshKey, setSshKey] = useState("");
  const [autoApply, setAutoApply] = useState(!isEdit);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // test-connection state
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; msg: string } | null>(null);
  // Once we've created/loaded a row, reuse its id for subsequent saves so
  // hitting "测试连接" before "创建" can't spawn a duplicate node.
  const [savedId, setSavedId] = useState<number | null>(editing?.id ?? null);

  // Persist current values (PUT if the row already exists, else POST) and
  // remember the id. Shared by both submit and test-connection.
  async function persist(): Promise<Node> {
    const payload = buildPayload();
    const saved = savedId != null
      ? await api.put<Node>(`/api/nodes/${savedId}`, payload)
      : await api.post<Node>("/api/nodes", payload);
    setSavedId(saved.id);
    return saved;
  }

  function buildPayload() {
    const p: Record<string, unknown> = {
      name, addr, core,
      ssh_port: sshPort,
      ssh_user: sshUser,
      mgmt_port: mgmtPort,
      ssh_auth_method: authMethod,
    };
    // Only send a secret when the operator actually entered one (keeps the
    // stored value on edit; matches the backend's absent=leave semantics).
    if (authMethod === "password" && sshPassword) p.ssh_password = sshPassword;
    if (authMethod === "key" && sshKey)           p.ssh_private_key = sshKey;
    return p;
  }

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      const saved = await persist();
      // Auto-sync: push the rendered config right after saving.
      if (autoApply) {
        await api.post(`/api/nodes/${saved.id}/apply`).catch(() => {});
      }
      onSaved();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  // Save current values first (so the credential is on the row), then ping.
  // Keeps the modal open so the operator can read the result and adjust.
  async function testConnection() {
    setTesting(true);
    setTestResult(null);
    setErr(null);
    try {
      const saved = await persist();
      const r = await api.post<{ ok: boolean; identity?: string; error?: string }>(
        `/api/nodes/${saved.id}/test-connection`,
      );
      setTestResult(
        r.ok
          ? { ok: true, msg: `连接成功:${r.identity ?? ""}` }
          : { ok: false, msg: `连接失败:${r.error ?? "未知错误"}` },
      );
    } catch (e) {
      setTestResult({ ok: false, msg: e instanceof ApiError ? e.message : String(e) });
    } finally {
      setTesting(false);
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
          <input className="input" type="number" value={sshPort}
                 onChange={(e) => setSshPort(Number(e.target.value))} min={1} max={65535} />
        </Field>
        <Field label="SSH 用户">
          <input className="input" value={sshUser}
                 onChange={(e) => setSshUser(e.target.value)} required />
        </Field>
      </div>

      {/* ---- SSH 认证 ------------------------------------------------ */}
      <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
        <legend className="text-sm font-semibold px-1">🔑 SSH 认证</legend>
        <Field label="认证方式">
          <SearchSelect
            value={authMethod}
            onChange={(v) => setAuthMethod(v as "global" | "password" | "key")}
            options={[
              { value: "global",   label: "🌐 全局密钥", sub: "使用面板 PANEL_SSH_KEY 环境变量" },
              { value: "password", label: "🔑 本节点密码", sub: "每节点独立密码，不共享" },
              { value: "key",      label: "🔑 本节点私钥", sub: "粘贴 PEM 格式私钥" },
            ]}
          />
        </Field>
        {authMethod === "password" && (
          <Field label={isEdit && editing?.has_ssh_credential ? "密码(留空 = 保持不变)" : "密码"}>
            <input className="input font-mono text-xs" type="password" value={sshPassword}
                   autoComplete="new-password"
                   placeholder={isEdit && editing?.has_ssh_credential ? "已配置,留空保持不变" : "root 密码"}
                   onChange={(e) => setSshPassword(e.target.value)} />
          </Field>
        )}
        {authMethod === "key" && (
          <Field label={isEdit && editing?.has_ssh_credential ? "私钥 PEM(留空 = 保持不变)" : "私钥 PEM"}>
            <textarea className="textarea font-mono text-xs" rows={6} value={sshKey}
                      placeholder={"-----BEGIN OPENSSH PRIVATE KEY-----\n...\n-----END OPENSSH PRIVATE KEY-----"}
                      onChange={(e) => setSshKey(e.target.value)} />
          </Field>
        )}
        <div className="flex items-center gap-3">
          <button type="button" className="btn btn-ghost" onClick={testConnection} disabled={testing}>
            {testing ? "测试中…" : "🔌 测试连接"}
          </button>
          {testResult && (
            <span className="text-xs" style={{ color: testResult.ok ? "#166534" : "#b91c1c" }}>
              {testResult.msg}
            </span>
          )}
        </div>
        <p className="text-xs" style={{ color: "var(--fg-muted)" }}>
          私钥 / 密码仅用于面板连机器,保存后不会再回显。dry-run 模式下「测试连接」返回模拟结果。
        </p>
      </fieldset>

      <div className="grid grid-cols-2 gap-3">
        <Field label="内核">
          <SearchSelect
            value={core}
            onChange={(v) => setCore(v as "xray" | "singbox")}
            options={[
              { value: "xray",    label: "Xray",     sub: "XTLS/Xray-core，支持 Reality" },
              { value: "singbox", label: "sing-box",  sub: "SagerNet，支持 Hysteria2/TUIC" },
            ]}
          />
        </Field>
        <Field label="Stats 端口 (0=禁用)">
          <input className="input" type="number" value={mgmtPort}
                 onChange={(e) => setMgmtPort(Number(e.target.value))} min={0} max={65535} />
        </Field>
      </div>

      <label className="flex items-center gap-2 text-sm cursor-pointer">
        <input type="checkbox" checked={autoApply} onChange={(e) => setAutoApply(e.target.checked)} />
        保存后自动同步配置到 VPS(入队 Apply 任务)
      </label>

      {err && <div className="text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "保存中…" : isEdit ? "保存修改" : "创建"}
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
