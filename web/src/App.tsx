import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";
import { AppShell } from "./components/AppShell";
import { AuthProvider, useAuth } from "./lib/auth";
import { Account } from "./pages/Account";
import { Audit } from "./pages/Audit";
import { Backups } from "./pages/Backups";
import { CdnEndpoints } from "./pages/CdnEndpoints";
import { ChainProxies } from "./pages/ChainProxies";
import { Dashboard } from "./pages/Dashboard";
import { Listeners } from "./pages/Listeners";
import { Login } from "./pages/Login";
import { Nodes } from "./pages/Nodes";
import { Notifications } from "./pages/Notifications";
import { Plans } from "./pages/Plans";
import { ProxyUsers } from "./pages/ProxyUsers";
import { Tasks } from "./pages/Tasks";
import { Traffic } from "./pages/Traffic";

export function App() {
  return (
    <AuthProvider>
      <BrowserRouter>
        <Routed />
      </BrowserRouter>
    </AuthProvider>
  );
}

function Routed() {
  const { state } = useAuth();

  if (state.status === "loading") {
    return (
      <div className="min-h-screen flex items-center justify-center text-sm" style={{ color: "var(--fg-muted)" }}>
        加载中…
      </div>
    );
  }

  if (state.status === "anon") {
    return (
      <Routes>
        <Route path="*" element={<Login />} />
      </Routes>
    );
  }

  return (
    <AppShell>
      <Routes>
        <Route path="/"             element={<Dashboard />} />
        <Route path="/nodes"        element={<Nodes />} />
        <Route path="/listeners"    element={<Listeners />} />
        <Route path="/plans"        element={<Plans />} />
        <Route path="/proxy-users"  element={<ProxyUsers />} />
        <Route path="/cdn-endpoints" element={<CdnEndpoints />} />
        <Route path="/chain-proxies" element={<ChainProxies />} />
        <Route path="/notifications" element={<Notifications />} />
        <Route path="/traffic"      element={<Traffic />} />
        <Route path="/tasks"        element={<Tasks />} />
        <Route path="/audit"        element={<Audit />} />
        <Route path="/backups"      element={<Backups />} />
        <Route path="/account"      element={<Account />} />
        <Route path="*"             element={<Navigate to="/" replace />} />
      </Routes>
    </AppShell>
  );
}
