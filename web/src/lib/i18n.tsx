/**
 * Minimal i18n — Chinese ↔ English.
 * Language preference is persisted in localStorage ("panel_lang").
 * A React context distributes the active locale so any component can read it.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  ReactNode,
} from "react";

// ============================================================================
// Types
// ============================================================================

export type Lang = "zh" | "en";

// Nav labels used in AppShell
export type NavKey =
  | "dashboard" | "nodes" | "listeners" | "plans" | "proxy_users"
  | "cdn" | "chain" | "notifications" | "traffic" | "tasks"
  | "audit" | "backups" | "account";

// Page / section strings used across pages
export type T = {
  nav: Record<NavKey, string>;

  common: {
    save:     string;
    create:   string;
    edit:     string;
    delete:   string;
    cancel:   string;
    refresh:  string;
    search:   string;
    enabled:  string;
    disabled: string;
    loading:  string;
    logout:   string;
    admin:    string;
    confirm:  string;
  };

  nodes: {
    title:       string;
    subtitle:    (total: number, online: number) => string;
    addNode:     string;
    provision:   string;
    applyConfig: string;
    ssh:         string;
    status:      string;
    core:        string;
    lastSeen:    string;
    createdAt:   string;
    actions:     string;
    pending:     string;
    online:      string;
    offline:     string;
  };

  listeners: {
    title:      string;
    addListener: string;
    protocol:   string;
    transport:  string;
    tls:        string;
    port:       string;
    node:       string;
  };
};

// ============================================================================
// Translations
// ============================================================================

const ZH: T = {
  nav: {
    dashboard:     "仪表盘",
    nodes:         "VPS 管理",
    listeners:     "监听器",
    plans:         "套餐管理",
    proxy_users:   "代理用户",
    cdn:           "CDN 优选",
    chain:         "链式代理",
    notifications: "告警通知",
    traffic:       "流量统计",
    tasks:         "任务",
    audit:         "审计日志",
    backups:       "备份管理",
    account:       "账户设置",
  },
  common: {
    save:     "保存",
    create:   "创建",
    edit:     "编辑",
    delete:   "删除",
    cancel:   "取消",
    refresh:  "刷新",
    search:   "搜索",
    enabled:  "启用",
    disabled: "停用",
    loading:  "加载中…",
    logout:   "退出登录",
    admin:    "管理员",
    confirm:  "确认",
  },
  nodes: {
    title:       "VPS 管理",
    subtitle:    (total, online) => `共 ${total} 台 VPS，${online} 台在线`,
    addNode:     "＋ 新建节点",
    provision:   "🚀 初始化",
    applyConfig: "↑ 同步配置",
    ssh:         "SSH",
    status:      "状态",
    core:        "地区 / 内核",
    lastSeen:    "最近在线",
    createdAt:   "创建时间",
    actions:     "操作",
    pending:     "待机",
    online:      "在线",
    offline:     "离线",
  },
  listeners: {
    title:       "监听器",
    addListener: "＋ 新建监听器",
    protocol:    "协议",
    transport:   "传输",
    tls:         "TLS",
    port:        "端口",
    node:        "所属节点",
  },
};

const EN: T = {
  nav: {
    dashboard:     "Dashboard",
    nodes:         "VPS Nodes",
    listeners:     "Listeners",
    plans:         "Plans",
    proxy_users:   "Proxy Users",
    cdn:           "CDN Endpoints",
    chain:         "Chain Proxies",
    notifications: "Notifications",
    traffic:       "Traffic",
    tasks:         "Tasks",
    audit:         "Audit Log",
    backups:       "Backups",
    account:       "Account",
  },
  common: {
    save:     "Save",
    create:   "Create",
    edit:     "Edit",
    delete:   "Delete",
    cancel:   "Cancel",
    refresh:  "Refresh",
    search:   "Search",
    enabled:  "Enabled",
    disabled: "Disabled",
    loading:  "Loading…",
    logout:   "Log out",
    admin:    "Administrator",
    confirm:  "Confirm",
  },
  nodes: {
    title:       "VPS Nodes",
    subtitle:    (total, online) => `${total} nodes · ${online} online`,
    addNode:     "+ Add Node",
    provision:   "🚀 Initialize",
    applyConfig: "↑ Deploy Config",
    ssh:         "SSH",
    status:      "Status",
    core:        "Region / Core",
    lastSeen:    "Last Seen",
    createdAt:   "Created",
    actions:     "Actions",
    pending:     "Pending",
    online:      "Online",
    offline:     "Offline",
  },
  listeners: {
    title:       "Listeners",
    addListener: "+ New Listener",
    protocol:    "Protocol",
    transport:   "Transport",
    tls:         "TLS",
    port:        "Port",
    node:        "Node",
  },
};

const TRANSLATIONS: Record<Lang, T> = { zh: ZH, en: EN };

// ============================================================================
// Context
// ============================================================================

type I18nCtx = {
  lang: Lang;
  t:    T;
  setLang: (l: Lang) => void;
};

const Ctx = createContext<I18nCtx>({
  lang: "zh",
  t:    ZH,
  setLang: () => {},
});

const LS_KEY = "panel_lang";

function storedLang(): Lang {
  try {
    const v = localStorage.getItem(LS_KEY);
    if (v === "en" || v === "zh") return v;
  } catch {}
  return "zh";
}

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(storedLang);

  const setLang = useCallback((l: Lang) => {
    setLangState(l);
    try { localStorage.setItem(LS_KEY, l); } catch {}
  }, []);

  const value = useMemo<I18nCtx>(
    () => ({ lang, t: TRANSLATIONS[lang], setLang }),
    [lang, setLang],
  );

  // Sync <html lang=""> attribute
  useEffect(() => {
    document.documentElement.lang = lang;
  }, [lang]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

/** Hook to consume locale strings + switch language. */
export function useI18n() {
  return useContext(Ctx);
}
