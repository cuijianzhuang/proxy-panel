import { ReactNode } from "react";
import { Link, NavLink } from "react-router-dom";
import { useAuth } from "../lib/auth";
import { useI18n, Lang, NavKey } from "../lib/i18n";
import { ThemeSwitch } from "./ThemeSwitch";

/*
 * Two-column shell: persistent sidebar + scrollable main pane.
 * The sidebar nav labels are driven by the active locale.
 */

const NAV: { to: string; key: NavKey; emoji: string }[] = [
  { to: "/",              key: "dashboard",     emoji: "📊" },
  { to: "/nodes",         key: "nodes",         emoji: "🖥️" },
  { to: "/listeners",     key: "listeners",     emoji: "📡" },
  { to: "/plans",         key: "plans",         emoji: "📦" },
  { to: "/proxy-users",   key: "proxy_users",   emoji: "👥" },
  { to: "/cdn-endpoints", key: "cdn",           emoji: "☁️" },
  { to: "/chain-proxies", key: "chain",         emoji: "🔗" },
  { to: "/notifications", key: "notifications", emoji: "🔔" },
  { to: "/traffic",       key: "traffic",       emoji: "📈" },
  { to: "/tasks",         key: "tasks",         emoji: "⏱️" },
  { to: "/audit",         key: "audit",         emoji: "📜" },
  { to: "/backups",       key: "backups",       emoji: "💾" },
  { to: "/account",       key: "account",       emoji: "⚙️" },
];

/** Pill-style 中 / EN language toggle */
function LangSwitch() {
  const { lang, setLang } = useI18n();
  const opts: { l: Lang; label: string }[] = [
    { l: "zh", label: "中" },
    { l: "en", label: "EN" },
  ];
  return (
    <div
      className="flex rounded-lg overflow-hidden"
      style={{ border: "1.5px solid var(--border)", fontSize: 12 }}
    >
      {opts.map(({ l, label }) => {
        const active = lang === l;
        return (
          <button
            key={l}
            onClick={() => setLang(l)}
            style={{
              padding: "3px 10px",
              fontWeight:  active ? 700 : 400,
              background:  active ? "var(--accent)" : "transparent",
              color:       active ? "var(--accent-fg)" : "var(--fg-muted)",
              border:      "none",
              cursor:      "pointer",
              transition:  "background 120ms ease, color 120ms ease",
              letterSpacing: "0.03em",
            }}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}

export function AppShell({ children }: { children: ReactNode }) {
  const { state, logout } = useAuth();
  const { t } = useI18n();
  const user = state.status === "authed" ? state.user : null;

  return (
    <div className="min-h-screen flex" style={{ background: "var(--bg)" }}>
      {/* Sidebar */}
      <aside
        className="w-60 shrink-0 flex flex-col border-r"
        style={{ background: "var(--bg-sidebar)", borderColor: "var(--border)" }}
      >
        {/* Logo */}
        <Link to="/" className="px-5 py-5 flex items-center gap-2">
          <span className="text-2xl">🌸</span>
          <span className="font-semibold tracking-wide">Proxy Panel</span>
        </Link>

        {/* Nav */}
        <nav className="flex-1 overflow-y-auto py-2">
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === "/"}
              className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}
            >
              <span className="w-5 text-center shrink-0">{item.emoji}</span>
              <span className="truncate">{t.nav[item.key]}</span>
            </NavLink>
          ))}
        </nav>

        {/* Theme + Lang row */}
        <div
          className="border-t p-3 flex items-center gap-2"
          style={{ borderColor: "var(--border)" }}
        >
          <div className="flex-1">
            <ThemeSwitch />
          </div>
          <LangSwitch />
        </div>

        {/* User info */}
        {user && (
          <div className="border-t p-4" style={{ borderColor: "var(--border)" }}>
            <div className="text-sm font-medium">{user.username}</div>
            <div className="text-xs" style={{ color: "var(--fg-muted)" }}>
              {user.is_admin ? t.common.admin : user.role}
            </div>
            <button onClick={() => logout()} className="btn btn-ghost mt-3 w-full">
              {t.common.logout}
            </button>
          </div>
        )}
      </aside>

      {/* Main */}
      <main className="flex-1 overflow-y-auto">
        <div className="max-w-6xl mx-auto px-8 py-6">{children}</div>
      </main>
    </div>
  );
}
