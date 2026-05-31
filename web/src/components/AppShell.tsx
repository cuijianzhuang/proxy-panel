import { ReactNode } from "react";
import { Link, NavLink } from "react-router-dom";
import { useAuth } from "../lib/auth";
import { ThemeSwitch } from "./ThemeSwitch";

/*
 * Two-column shell: persistent sidebar (sakura-tinted) + scrollable main pane.
 * The 6 routes here are the ones implemented today; the rest of yins.win's
 * 12-item menu will appear as their backing tables/endpoints land.
 */
const NAV: { to: string; label: string; emoji: string }[] = [
  { to: "/",            label: "Dashboard",   emoji: "📊" },
  { to: "/nodes",       label: "VPS 管理",    emoji: "🖥️" },
  { to: "/listeners",   label: "监听器",      emoji: "📡" },
  { to: "/plans",       label: "套餐管理",    emoji: "📦" },
  { to: "/proxy-users", label: "代理用户",    emoji: "👥" },
  { to: "/cdn-endpoints", label: "CDN 优选",  emoji: "☁️" },
  { to: "/chain-proxies", label: "链式代理",  emoji: "🔗" },
  { to: "/notifications", label: "告警通知",  emoji: "🔔" },
  { to: "/traffic",     label: "流量统计",    emoji: "📈" },
  { to: "/tasks",       label: "任务",        emoji: "⏱️" },
  { to: "/audit",       label: "审计日志",    emoji: "📜" },
  { to: "/backups",     label: "备份管理",    emoji: "💾" },
  { to: "/account",     label: "账户设置",    emoji: "⚙️" },
];

export function AppShell({ children }: { children: ReactNode }) {
  const { state, logout } = useAuth();
  const user = state.status === "authed" ? state.user : null;

  return (
    <div className="min-h-screen flex" style={{ background: "var(--bg)" }}>
      {/* Sidebar */}
      <aside
        className="w-60 shrink-0 flex flex-col border-r"
        style={{ background: "var(--bg-sidebar)", borderColor: "var(--border)" }}
      >
        <Link to="/" className="px-5 py-5 flex items-center gap-2">
          <span className="text-2xl">🌸</span>
          <span className="font-semibold tracking-wide">Proxy Panel</span>
        </Link>

        <nav className="flex-1 overflow-y-auto py-2">
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.to === "/"}
              className={({ isActive }) => `nav-item ${isActive ? "active" : ""}`}
            >
              <span className="w-5 text-center">{item.emoji}</span>
              <span>{item.label}</span>
            </NavLink>
          ))}
        </nav>

        <div className="border-t p-3 flex items-center gap-2" style={{ borderColor: "var(--border)" }}>
          <ThemeSwitch />
        </div>

        {user && (
          <div className="border-t p-4" style={{ borderColor: "var(--border)" }}>
            <div className="text-sm font-medium">{user.username}</div>
            <div className="text-xs" style={{ color: "var(--fg-muted)" }}>
              {user.is_admin ? "管理员" : user.role}
            </div>
            <button onClick={() => logout()} className="btn btn-ghost mt-3 w-full">
              退出登录
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
