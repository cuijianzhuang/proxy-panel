import { FormEvent, useState } from "react";
import { useAuth } from "../lib/auth";
import { ApiError } from "../lib/api";

export function Login() {
  const { login } = useAuth();
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      await login(username, password);
    } catch (error) {
      setErr(error instanceof ApiError ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="min-h-screen flex items-center justify-center px-4" style={{ background: "var(--bg)" }}>
      <form onSubmit={onSubmit} className="card w-full max-w-sm p-6">
        <div className="flex items-center gap-2 justify-center mb-6">
          <span className="text-3xl">🌸</span>
          <h1 className="text-xl font-semibold">Proxy Panel</h1>
        </div>

        <label className="block text-sm mb-1">用户名</label>
        <input
          className="input mb-3"
          value={username}
          onChange={(e) => setUsername(e.target.value)}
          autoFocus
          required
        />

        <label className="block text-sm mb-1">密码</label>
        <input
          className="input mb-3"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
        />

        {err && (
          <div className="text-sm mb-3" style={{ color: "#b91c1c" }}>
            {err}
          </div>
        )}

        <button type="submit" className="btn btn-primary w-full" disabled={busy}>
          {busy ? "登录中…" : "登录"}
        </button>

        <p className="text-xs mt-4 text-center" style={{ color: "var(--fg-muted)" }}>
          Self-hosted Xray / sing-box management
        </p>
      </form>
    </div>
  );
}
