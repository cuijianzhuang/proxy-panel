/*
 * Auth context. `/api/me` is the source of truth — on mount we probe it once
 * and cache the result. `login()` and `logout()` re-fetch so the UI stays
 * in sync without manual cache busting.
 */
import {
  createContext,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { api, ApiError, PanelUser } from "./api";

type AuthState =
  | { status: "loading" }
  | { status: "anon" }
  | { status: "authed"; user: PanelUser };

type Ctx = {
  state: AuthState;
  login: (username: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
  refresh: () => Promise<void>;
};

const AuthCtx = createContext<Ctx | null>(null);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<AuthState>({ status: "loading" });

  const refresh = useCallback(async () => {
    try {
      const me = await api.get<PanelUser | null>("/api/me");
      setState(me ? { status: "authed", user: me } : { status: "anon" });
    } catch (err) {
      if (err instanceof ApiError && err.status === 401) {
        setState({ status: "anon" });
      } else {
        // Network/server issue — treat as anonymous so the login screen renders.
        setState({ status: "anon" });
      }
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const login = useCallback(
    async (username: string, password: string) => {
      const user = await api.post<PanelUser>("/api/login", { username, password });
      setState({ status: "authed", user });
    },
    []
  );

  const logout = useCallback(async () => {
    try {
      await api.post("/api/logout");
    } finally {
      setState({ status: "anon" });
    }
  }, []);

  const value = useMemo<Ctx>(() => ({ state, login, logout, refresh }), [state, login, logout, refresh]);
  return <AuthCtx.Provider value={value}>{children}</AuthCtx.Provider>;
}

export function useAuth() {
  const ctx = useContext(AuthCtx);
  if (!ctx) throw new Error("useAuth must be used inside <AuthProvider>");
  return ctx;
}
