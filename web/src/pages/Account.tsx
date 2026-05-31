import { FormEvent, useState } from "react";
import { api, ApiError } from "../lib/api";
import { useAuth } from "../lib/auth";

/*
 * Self-service account settings. Today only password change; profile fields
 * (name/email/2FA) will land here as those endpoints come online.
 */
export function Account() {
  const { state } = useAuth();
  const user = state.status === "authed" ? state.user : null;

  return (
    <div>
      <header className="mb-4">
        <h1 className="text-2xl font-semibold">账户设置</h1>
        <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
          管理你的登录信息。
        </p>
      </header>

      {user && (
        <section className="card p-5 mb-4">
          <div className="grid grid-cols-2 gap-3 text-sm">
            <Row label="用户名" value={user.username} />
            <Row label="角色"   value={user.is_admin ? "管理员" : user.role} />
            <Row label="状态"   value={user.active ? "已启用" : "已停用"} />
            <Row label="上次登录" value={user.last_login_at ?? "—"} />
          </div>
        </section>
      )}

      <ChangePasswordCard />
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-xs" style={{ color: "var(--fg-muted)" }}>{label}</div>
      <div className="font-mono text-xs mt-1 break-all">{value}</div>
    </div>
  );
}

function ChangePasswordCard() {
  const [oldPw, setOldPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirm, setConfirm] = useState("");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ type: "ok" | "err"; text: string } | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setMsg(null);
    if (newPw.length < 8) {
      setMsg({ type: "err", text: "新密码至少 8 位。" });
      return;
    }
    if (newPw !== confirm) {
      setMsg({ type: "err", text: "两次输入的新密码不一致。" });
      return;
    }
    setBusy(true);
    try {
      await api.post("/api/me/password", { old_password: oldPw, new_password: newPw });
      setMsg({ type: "ok", text: "密码已更新,其他会话已被踢出。" });
      setOldPw(""); setNewPw(""); setConfirm("");
    } catch (e) {
      setMsg({ type: "err", text: e instanceof ApiError ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="card p-5">
      <h2 className="font-semibold mb-3">修改密码</h2>
      <form onSubmit={onSubmit} className="space-y-3 max-w-md">
        <label className="block">
          <span className="text-sm mb-1 block">当前密码</span>
          <input className="input" type="password" autoComplete="current-password"
                 value={oldPw} onChange={(e) => setOldPw(e.target.value)} required />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">新密码 (≥ 8 位)</span>
          <input className="input" type="password" autoComplete="new-password" minLength={8}
                 value={newPw} onChange={(e) => setNewPw(e.target.value)} required />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">再次输入新密码</span>
          <input className="input" type="password" autoComplete="new-password"
                 value={confirm} onChange={(e) => setConfirm(e.target.value)} required />
        </label>
        {msg && (
          <div className="text-sm" style={{ color: msg.type === "ok" ? "#166534" : "#b91c1c" }}>
            {msg.text}
          </div>
        )}
        <div>
          <button type="submit" className="btn btn-primary" disabled={busy}>
            {busy ? "保存中…" : "保存"}
          </button>
        </div>
      </form>
    </section>
  );
}
