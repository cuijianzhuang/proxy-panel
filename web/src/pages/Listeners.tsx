import { FormEvent, useCallback, useEffect, useState } from "react";
import { api, ApiError, CdnEndpoint, ChainProxy, Listener, Node } from "../lib/api";
import { SearchSelect } from "../components/SearchSelect";
import { useI18n } from "../lib/i18n";
import { Modal } from "../components/Modal";

export function Listeners() {
  const [rows, setRows] = useState<Listener[] | null>(null);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);

  const reload = useCallback(async () => {
    try {
      const [ls, ns] = await Promise.all([
        api.get<Listener[]>("/api/listeners"),
        api.get<Node[]>("/api/nodes"),
      ]);
      setRows(ls);
      setNodes(ns);
      setErr(null);
    } catch (e) {
      setErr(String(e));
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  async function remove(id: number) {
    if (!confirm("删除该监听器?")) return;
    try {
      await api.del(`/api/listeners/${id}`);
      reload();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    }
  }

  return (
    <ListenersView
      rows={rows} nodes={nodes} err={err}
      showNew={showNew} setShowNew={setShowNew}
      remove={remove} reload={reload}
    />
  );
}

/*
 * Per-node grouped layout. Listeners with `node_id == null` get bucketed into
 * a "(未绑定节点)" group; each group is a collapsible card showing the
 * enabled/total count, mirroring the production panel's layout.
 */
function ListenersView({
  rows, nodes, err, showNew, setShowNew, remove, reload,
}: {
  rows: Listener[] | null;
  nodes: Node[];
  err: string | null;
  showNew: boolean;
  setShowNew: (v: boolean) => void;
  remove: (id: number) => void | Promise<void>;
  reload: () => void;
}) {
  const [filterNode, setFilterNode] = useState<number | "all">("all");
  const [collapsed, setCollapsed]   = useState<Set<number | "_">>(new Set());
  const [editing, setEditing]       = useState<Listener | null>(null);

  const toggle = (key: number | "_") => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key); else next.add(key);
      return next;
    });
  };

  const grouped = new Map<number | "_", Listener[]>();
  (rows ?? []).forEach((l) => {
    const k: number | "_" = l.node_id ?? "_";
    const bucket = grouped.get(k) ?? [];
    bucket.push(l);
    grouped.set(k, bucket);
  });

  const visibleNodes = nodes.filter((n) => filterNode === "all" || filterNode === n.id);
  const showUnbound  = filterNode === "all" && grouped.has("_");

  const total = rows?.length ?? 0;
  const enabledTotal = (rows ?? []).filter((l) => l.enabled).length;
  const { t } = useI18n();

  return (
    <div>
      <header className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold">{t.listeners.title}</h1>
          <p className="text-sm" style={{ color: "var(--fg-muted)" }}>
            {t.nav.listeners} · {total} {t.common.enabled}/{total}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <div style={{ minWidth: 180 }}>
            <SearchSelect
              value={filterNode === "all" ? "all" : String(filterNode)}
              onChange={(v) => setFilterNode(v === "all" ? "all" : Number(v))}
              searchPlaceholder="搜索节点…"
              options={[
                { value: "all", label: "全部节点" },
                ...nodes.map((n) => ({
                  value: String(n.id),
                  label: n.name,
                  sub:   n.addr,
                })),
              ]}
            />
          </div>
          <button
            onClick={() => reload()}
            className="btn btn-ghost btn-sm"
            title="刷新列表"
            style={{ fontSize: "1rem", lineHeight: 1, padding: "0.4rem 0.55rem" }}
          >⟳</button>
          <button
            onClick={() => setShowNew(true)}
            className="btn btn-primary"
            style={{ paddingLeft: "0.9rem", paddingRight: "1rem", whiteSpace: "nowrap" }}
          >
            <svg width="13" height="13" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" style={{ flexShrink: 0 }}>
              <path d="M6 1v10M1 6h10"/>
            </svg>
            {t.listeners.addListener}
          </button>
        </div>
      </header>

      {err && <div className="card p-3 mb-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}

      <div className="space-y-3">
        {visibleNodes.map((n) => (
          <NodeGroup
            key={n.id}
            title={n.name}
            subtitle={n.addr}
            core={n.core}
            listeners={grouped.get(n.id) ?? []}
            collapsed={collapsed.has(n.id)}
            onToggle={() => toggle(n.id)}
            onRemove={remove}
            onEdit={setEditing}
          />
        ))}
        {showUnbound && (
          <NodeGroup
            title="(未绑定节点)"
            subtitle="这些监听器没有 node_id,不会被 /api/nodes/:id/config 渲染。"
            core={null}
            listeners={grouped.get("_") ?? []}
            collapsed={collapsed.has("_")}
            onToggle={() => toggle("_")}
            onRemove={remove}
            onEdit={setEditing}
          />
        )}
        {rows && rows.length === 0 && (
          <div className="card p-6 text-center text-sm" style={{ color: "var(--fg-muted)" }}>
            暂无监听器 — 先建节点,再点右上角「＋ 新建监听器」。
          </div>
        )}
      </div>

      {/* Create */}
      <Modal open={showNew} onClose={() => setShowNew(false)} title="新建监听器" size="xl">
        <ListenerForm nodes={nodes} onSaved={() => { setShowNew(false); reload(); }} />
      </Modal>

      {/* Edit — keyed by id so the form remounts (re-inits state) per listener */}
      <Modal open={editing !== null} onClose={() => setEditing(null)} title={`编辑监听器 — ${editing?.name ?? ""}`} size="xl">
        {editing && (
          <ListenerForm
            key={editing.id}
            nodes={nodes}
            editing={editing}
            onSaved={() => { setEditing(null); reload(); }}
          />
        )}
      </Modal>
    </div>
  );
}

function NodeGroup({
  title, subtitle, core, listeners, collapsed, onToggle, onRemove, onEdit,
}: {
  title: string;
  subtitle?: string;
  core: "xray" | "singbox" | null;
  listeners: Listener[];
  collapsed: boolean;
  onToggle: () => void;
  onRemove: (id: number) => void | Promise<void>;
  onEdit: (l: Listener) => void;
}) {
  const enabled = listeners.filter((l) => l.enabled).length;
  const usedCores = new Set(listeners.map((l) => l.core));
  return (
    <section className="card overflow-hidden">
      <header
        className="px-4 py-3 cursor-pointer select-none flex items-center justify-between"
        onClick={onToggle}
        style={{ background: "var(--bg-elev)" }}
      >
        <div className="flex items-center gap-3 min-w-0">
          <span className="text-xs w-3" style={{ color: "var(--fg-muted)" }}>{collapsed ? "▶" : "▼"}</span>
          <span className="text-base">📡</span>
          <div className="min-w-0">
            <div className="font-semibold truncate">{title}</div>
            <div className="text-xs truncate" style={{ color: "var(--fg-muted)" }}>
              {subtitle ? <>{subtitle} · </> : null}
              {enabled}/{listeners.length} 启用
            </div>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {/* Cores actually used by inbounds on this node — mirrors the
            * reference panel which shows xray + sing-box pills side-by-side
            * when the node hosts both. */}
          {usedCores.has("xray")    && <span className="badge">Xray</span>}
          {usedCores.has("singbox") && <span className="badge">sing-box</span>}
          {/* Fall back to the node's declared core when there are no
            * listeners yet, so the badge isn't empty for a fresh node. */}
          {usedCores.size === 0 && core && <span className="badge">{core === "singbox" ? "sing-box" : "Xray"}</span>}
          <span className="badge badge-ok">{listeners.length} 个监听器</span>
        </div>
      </header>
      {!collapsed && (
        <table className="table">
          <thead>
            <tr>
              <th>ID</th><th>名称</th><th>协议</th><th>传输</th><th>TLS</th><th>端口</th><th>启用</th>
              <th className="text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {listeners.map((l) => (
              <tr key={l.id}>
                <td className="font-mono text-xs">{l.id}</td>
                <td>{l.name}</td>
                <td><span className="badge">{l.core}/{l.protocol}</span></td>
                <td>{l.transport}</td>
                <td>{l.tls_mode}</td>
                <td className="font-mono">{l.port}</td>
                <td>
                  <span
                    style={{
                      display: "inline-block",
                      width: 8, height: 8,
                      borderRadius: "50%",
                      background: l.enabled ? "#22c55e" : "#e5e7eb",
                      boxShadow: l.enabled ? "0 0 0 2px #bbf7d0" : undefined,
                    }}
                    title={l.enabled ? "已启用" : "已停用"}
                  />
                </td>
                <td className="text-right">
                  <div className="flex gap-1.5 justify-end">
                    <button
                      className="btn btn-ghost btn-sm"
                      onClick={() => onEdit(l)}
                      title="编辑"
                    >
                      <svg width="12" height="12" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M10 2.5l1.5 1.5L4 11.5H2.5V10z"/>
                      </svg>
                      编辑
                    </button>
                    <button
                      className="btn btn-danger btn-sm"
                      onClick={() => onRemove(l.id)}
                      title="删除"
                    >
                      <svg width="12" height="12" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M2 4h10M5 4V2.5h4V4M5.5 6.5v4M8.5 6.5v4M3 4l.8 7.5h6.4L11 4"/>
                      </svg>
                      删除
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {listeners.length === 0 && (
              <tr>
                <td colSpan={8} className="text-center py-5 text-sm" style={{ color: "var(--fg-muted)" }}>
                  该节点暂无监听器。
                </td>
              </tr>
            )}
          </tbody>
        </table>
      )}
    </section>
  );
}

/* =======================================================================
 *  Smart listener form
 *
 *  - Top row of preset chips: one click picks a protocol/transport/tls combo
 *    and pre-fills sensible defaults.
 *  - Protocol-specific fields appear inline, so users don't write JSON.
 *  - "🎲" / "生成" buttons call /api/utils/* so cryptographic blobs and ports
 *    are filled by the server.
 *  - "高级 (raw JSON)" expander reveals the assembled params for inspection.
 * ======================================================================= */

type Tls = "none" | "tls" | "reality";

type Preset = {
  key:       string;
  label:     string;
  hint:      string;
  protocol:  string;
  transport: string;
  tls:       Tls;
  flow?:     string;
};

const PRESETS: Preset[] = [
  { key: "vl-reality-vision", label: "VLESS + Reality + Vision",
    hint: "无 SNI 也能反代,推荐 ✨", protocol: "vless", transport: "tcp", tls: "reality", flow: "xtls-rprx-vision" },
  { key: "vl-ws-tls",         label: "VLESS + WS + TLS",
    hint: "走 CDN 友好",         protocol: "vless", transport: "ws", tls: "tls" },
  { key: "vl-grpc-reality",   label: "VLESS + gRPC + Reality",
    hint: "对抗主动探测",         protocol: "vless", transport: "grpc", tls: "reality" },
  { key: "vmess-ws-tls",      label: "VMess + WS + TLS",
    hint: "老牌兼容",            protocol: "vmess", transport: "ws", tls: "tls" },
  { key: "trojan-ws-tls",     label: "Trojan + WS + TLS",
    hint: "兼容主流客户端",       protocol: "trojan", transport: "ws", tls: "tls" },
  { key: "trojan-tcp-tls",    label: "Trojan + TCP + TLS",
    hint: "最简 Trojan",         protocol: "trojan", transport: "tcp", tls: "tls" },
  { key: "hy2",               label: "Hysteria2",
    hint: "QUIC,抗丢包 (sing-box)", protocol: "hysteria2", transport: "quic", tls: "tls" },
  { key: "tuic",              label: "TUIC v5",
    hint: "QUIC,低延迟 (sing-box)", protocol: "tuic", transport: "quic", tls: "tls" },
  { key: "ss",                label: "Shadowsocks",
    hint: "简单方案",            protocol: "shadowsocks", transport: "tcp", tls: "none" },
  { key: "ss2022",            label: "Shadowsocks 2022",
    hint: "新算法,更安全",        protocol: "shadowsocks", transport: "tcp", tls: "none" },
  { key: "custom",            label: "自定义",
    hint: "我自己拼",            protocol: "vless", transport: "tcp", tls: "none" },
];

// hysteria2/tuic must run over QUIC; some presets imply a default SS method.
const PRESET_SS_METHOD: Record<string, string> = {
  ss:     "aes-128-gcm",
  ss2022: "2022-blake3-aes-128-gcm",
};

const SS_METHODS = [
  "aes-128-gcm", "aes-256-gcm", "chacha20-ietf-poly1305",
  "2022-blake3-aes-128-gcm", "2022-blake3-aes-256-gcm",
];

const COMMON_SNIS = [
  "www.cloudflare.com", "www.microsoft.com", "www.apple.com",
  "addons.mozilla.org", "www.lovelive-anime.jp",
];

function ListenerForm({
  nodes, editing, onSaved,
}: {
  nodes: Node[];
  editing?: Listener | null;
  onSaved: () => void;
}) {
  const isEdit = !!editing;
  const ep = (editing?.params ?? {}) as Record<string, unknown>;
  const str = (k: string, d = "") => (typeof ep[k] === "string" ? (ep[k] as string) : d);

  // ---- shared fields --------------------------------------------------
  // Editing always lands on the "自定义" tab so every field is visible/editable.
  const [presetKey, setPresetKey] = useState<string>(isEdit ? "custom" : "vl-reality-vision");
  const [name,      setName]      = useState(editing?.name ?? "");
  const [nodeId,    setNodeId]    = useState<number | null>(editing?.node_id ?? nodes[0]?.id ?? null);
  const [core,      setCore]      = useState<"xray" | "singbox">(editing?.core ?? nodes[0]?.core ?? "xray");

  // ---- selectors driven by preset (but editable in custom mode) -------
  const [protocol,  setProtocol]  = useState(editing?.protocol ?? "vless");
  const [transport, setTransport] = useState(editing?.transport ?? "tcp");
  const [tlsMode,   setTlsMode]   = useState<Tls>(editing?.tls_mode ?? "reality");
  const [port,      setPort]      = useState(editing?.port ?? 443);

  // ---- protocol-specific param fields ---------------------------------
  // VLESS
  const [flow, setFlow] = useState(str("flow", isEdit ? "" : "xtls-rprx-vision"));
  // Reality
  const [realityServerName, setRealityServerName] = useState(str("reality_server_name", isEdit ? "" : "www.cloudflare.com"));
  const [realityPrivateKey, setRealityPrivateKey] = useState(str("reality_private_key"));
  const [realityPublicKey,  setRealityPublicKey]  = useState(str("reality_public_key"));
  const [realityShortId,    setRealityShortId]    = useState(str("reality_short_id"));
  const [realityDest,       setRealityDest]       = useState(str("reality_dest"));
  // TLS (non-Reality)
  const [sni,         setSni]         = useState(str("sni"));
  const [tlsCertPath, setTlsCertPath] = useState(str("tls_cert_path"));
  const [tlsKeyPath,  setTlsKeyPath]  = useState(str("tls_key_path"));
  // WS
  const [wsPath, setWsPath] = useState(str("ws_path", isEdit ? "" : "/"));
  const [wsHost, setWsHost] = useState(str("ws_host"));
  // gRPC
  const [grpcServiceName, setGrpcServiceName] = useState(str("grpc_service_name", isEdit ? "" : "grpc"));
  // xhttp
  const [xhttpPath, setXhttpPath] = useState(str("xhttp_path", isEdit ? "" : "/"));
  // Shadowsocks
  const [ssMethod,   setSsMethod]   = useState(str("method", "aes-128-gcm"));
  const [ssPassword, setSsPassword] = useState(str("password"));
  // Hysteria2 / TUIC
  const [obfsPassword, setObfsPassword] = useState(str("obfs_password"));
  // CDN 优选 — can pin to a specific endpoint by id, else first-enabled wins.
  const [cdnEnabled, setCdnEnabled] = useState(ep["cdn_enabled"] === true);
  const [cdnEndpointId, setCdnEndpointId] = useState<number | null>(
    typeof ep["cdn_endpoint_id"] === "number" ? (ep["cdn_endpoint_id"] as number) : null,
  );
  const [cdnPool, setCdnPool] = useState<CdnEndpoint[]>([]);
  // 链式代理 (国内中转 → 海外落地)。Renderer writes a matching outbound + an
  // inboundTag→outboundTag routing rule when this is set.
  const [chainProxyId, setChainProxyId] = useState<number | null>(
    typeof ep["chain_proxy_id"] === "number" ? (ep["chain_proxy_id"] as number) : null,
  );
  const [chainPool, setChainPool] = useState<ChainProxy[]>([]);

  const [err,  setErr]  = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [showRaw, setShowRaw] = useState(false);

  // Load both pools in parallel. Disabled rows are kept off so we don't
  // surface stale options the renderer would ignore anyway.
  useEffect(() => {
    api.get<CdnEndpoint[]>("/api/cdn-endpoints")
      .then((eps) => setCdnPool(eps.filter((e) => e.enabled)))
      .catch(() => setCdnPool([]));
    api.get<ChainProxy[]>("/api/chain-proxies")
      .then((cs) => setChainPool(cs.filter((c) => c.enabled)))
      .catch(() => setChainPool([]));
  }, []);
  // Resolve the host that the subscription would actually swap to, so the
  // hint under the CDN switch always reflects the *exact* endpoint that will
  // be used (matching the server-side `effective_host` logic).
  const pinnedCdn = cdnEndpointId != null
    ? cdnPool.find((e) => e.id === cdnEndpointId)
    : undefined;
  const topCdn = pinnedCdn ?? cdnPool[0];

  // Apply preset whenever it changes (not while user edits inside "custom").
  function applyPreset(key: string) {
    setPresetKey(key);
    const p = PRESETS.find((x) => x.key === key);
    if (!p || p.key === "custom") return;
    setProtocol(p.protocol);
    setTransport(p.transport);
    setTlsMode(p.tls);
    setFlow(p.flow ?? "");
    if (p.tls === "reality" && !realityServerName) setRealityServerName("www.cloudflare.com");
    if (p.transport === "ws" && !wsPath) setWsPath("/");
    if (PRESET_SS_METHOD[key]) setSsMethod(PRESET_SS_METHOD[key]);
  }

  // Build the params object the server expects.
  function buildParams(): Record<string, unknown> {
    const p: Record<string, unknown> = {};
    if (protocol === "vless" && flow)            p.flow = flow;
    if (tlsMode === "reality") {
      if (realityServerName) p.reality_server_name = realityServerName;
      if (realityPrivateKey) p.reality_private_key = realityPrivateKey;
      if (realityPublicKey)  p.reality_public_key  = realityPublicKey;
      if (realityShortId)    p.reality_short_id    = realityShortId;
      if (realityDest)       p.reality_dest        = realityDest;
    }
    if (tlsMode === "tls") {
      if (sni)         p.sni           = sni;
      if (tlsCertPath) p.tls_cert_path = tlsCertPath;
      if (tlsKeyPath)  p.tls_key_path  = tlsKeyPath;
    }
    if (transport === "ws") {
      if (wsPath) p.ws_path = wsPath;
      if (wsHost) p.ws_host = wsHost;
    }
    if (transport === "grpc" && grpcServiceName) p.grpc_service_name = grpcServiceName;
    if (transport === "xhttp" && xhttpPath)      p.xhttp_path        = xhttpPath;
    if (protocol === "shadowsocks") {
      p.method   = ssMethod;
      p.password = ssPassword;
    }
    if (protocol === "hysteria2" && obfsPassword) p.obfs_password = obfsPassword;
    if (cdnEnabled) {
      p.cdn_enabled = true;
      if (cdnEndpointId != null) p.cdn_endpoint_id = cdnEndpointId;
    }
    if (chainProxyId != null) p.chain_proxy_id = chainProxyId;
    return p;
  }

  async function generateRealityKeys() {
    try {
      const r = await api.get<{ private_key: string; public_key: string }>("/api/utils/reality-keypair");
      setRealityPrivateKey(r.private_key);
      setRealityPublicKey(r.public_key);
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    }
  }
  async function randomShortId() {
    const r = await api.get<{ value: string }>("/api/utils/random-id?bytes=4");
    setRealityShortId(r.value);
  }
  async function randomPort() {
    const r = await api.get<{ port: number }>("/api/utils/random-port");
    setPort(r.port);
  }
  async function randomSsPassword() {
    const r = await api.get<{ value: string }>("/api/utils/random-id?bytes=16");
    setSsPassword(r.value);
  }

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setErr(null);
    setBusy(true);
    const payload = {
      name,
      core,
      protocol,
      transport,
      tls_mode: tlsMode,
      port,
      node_id: nodeId,
      params: buildParams(),
    };
    try {
      if (isEdit) {
        await api.put(`/api/listeners/${editing!.id}`, payload);
      } else {
        await api.post("/api/listeners", payload);
      }
      onSaved();
    } catch (e) {
      setErr(e instanceof ApiError ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  const isCustom = presetKey === "custom";

  return (
    <form onSubmit={onSubmit} className="space-y-4">
      {/* ---- preset chips ------------------------------------------- */}
      <div>
        <div className="text-xs mb-2" style={{ color: "var(--fg-muted)" }}>常用预设</div>
        <div className="flex flex-wrap gap-2">
          {PRESETS.map((p) => {
            const active = p.key === presetKey;
            return (
              <button
                type="button"
                key={p.key}
                onClick={() => applyPreset(p.key)}
                className="text-left transition-all"
                style={{
                  padding:       "0.45rem 0.75rem",
                  borderRadius:  "10px",
                  border:        active ? "1.5px solid var(--accent)" : "1.5px solid var(--border)",
                  background:    active
                    ? "linear-gradient(135deg, var(--accent-soft) 0%, color-mix(in srgb, var(--accent) 10%, white) 100%)"
                    : "var(--bg-elev)",
                  color:         active ? "var(--accent)" : "var(--fg)",
                  boxShadow:     active
                    ? "0 2px 8px color-mix(in srgb, var(--accent) 20%, transparent)"
                    : "0 1px 2px rgba(0,0,0,.05)",
                  transform:     active ? "translateY(-1px)" : "none",
                }}
              >
                <div className="text-sm font-semibold leading-tight">{p.label}</div>
                <div className="text-xs mt-0.5" style={{ color: active ? "var(--accent)" : "var(--fg-muted)", opacity: active ? .8 : 1 }}>{p.hint}</div>
              </button>
            );
          })}
        </div>
      </div>

      {/* ---- base fields ------------------------------------------- */}
      <div className="grid grid-cols-2 gap-3">
        <label className="block">
          <span className="text-sm mb-1 block">名称</span>
          <input className="input" value={name} onChange={(e) => setName(e.target.value)} required
                 placeholder={`例:vl-reality-${nodes[0]?.name ?? "vps01"}`} />
        </label>
        <label className="block">
          <span className="text-sm mb-1 block">所属节点</span>
          <SearchSelect
            value={nodeId != null ? String(nodeId) : ""}
            onChange={(v) => setNodeId(v ? Number(v) : null)}
            placeholder="— 不绑定 —"
            searchPlaceholder="搜索节点名称或地址…"
            options={[
              { value: "", label: "— 不绑定 —" },
              ...nodes.map((n) => ({
                value: String(n.id),
                label: `${n.name} (${n.core})`,
                sub: `#${n.id} · ${n.addr}`,
              })),
            ]}
          />
        </label>
      </div>

      {/* When custom, expose all three pickers; otherwise show derived chips. */}
      {isCustom ? (
        <div className="grid grid-cols-4 gap-3">
          <label className="block">
            <span className="text-sm mb-1 block">内核</span>
            <SearchSelect
              value={core}
              onChange={(v) => setCore(v as "xray" | "singbox")}
              options={[
                { value: "xray",    label: "Xray" },
                { value: "singbox", label: "sing-box" },
              ]}
            />
          </label>
          <label className="block">
            <span className="text-sm mb-1 block">协议</span>
            <SearchSelect
              value={protocol}
              onChange={setProtocol}
              options={[
                { value: "vless",        label: "VLESS",        sub: "推荐" },
                { value: "vmess",        label: "VMess" },
                { value: "trojan",       label: "Trojan" },
                { value: "shadowsocks",  label: "Shadowsocks",  sub: "SS / SS2022" },
                { value: "hysteria2",    label: "Hysteria2",    sub: "QUIC, sing-box 独有" },
                { value: "tuic",         label: "TUIC v5",      sub: "QUIC, sing-box 独有" },
              ]}
            />
          </label>
          <label className="block">
            <span className="text-sm mb-1 block">传输</span>
            <SearchSelect
              value={transport}
              onChange={setTransport}
              options={[
                { value: "tcp",   label: "TCP" },
                { value: "ws",    label: "WebSocket", sub: "CDN 友好" },
                { value: "grpc",  label: "gRPC",      sub: "抗探测" },
                { value: "xhttp", label: "xhttp",     sub: "xray 专属" },
                { value: "quic",  label: "QUIC",      sub: "Hysteria2/TUIC" },
              ]}
            />
          </label>
          <label className="block">
            <span className="text-sm mb-1 block">TLS</span>
            <SearchSelect
              value={tlsMode}
              onChange={(v) => setTlsMode(v as "none" | "tls" | "reality")}
              options={[
                { value: "none",    label: "none",    sub: "无加密" },
                { value: "tls",     label: "TLS",     sub: "需证书" },
                { value: "reality", label: "Reality", sub: "推荐, 无需证书" },
              ]}
            />
          </label>
        </div>
      ) : (
        <div className="flex flex-wrap gap-2 text-xs">
          <span className="badge">core: {core}</span>
          <span className="badge">protocol: {protocol}</span>
          <span className="badge">transport: {transport}</span>
          <span className="badge">tls: {tlsMode}</span>
        </div>
      )}

      {/* ---- port ---- */}
      <div className="grid grid-cols-[1fr_auto] gap-2 items-end">
        <label className="block">
          <span className="text-sm mb-1 block">端口</span>
          <input className="input" type="number" value={port}
                 onChange={(e) => setPort(Number(e.target.value))} min={1} max={65535} />
        </label>
        <button type="button" className="btn btn-ghost" onClick={randomPort} title="随机一个 10000–60000 的端口">
          🎲 随机
        </button>
      </div>

      {/* ---- CDN 优选 --------------------------------------------- */}
      <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
        <legend className="text-sm font-semibold px-1">☁️ CDN 优选</legend>
        <label className="flex items-start gap-2 cursor-pointer">
          <input type="checkbox" className="mt-1" checked={cdnEnabled}
                 onChange={(e) => setCdnEnabled(e.target.checked)} />
          <span className="text-sm">
            订阅里把<b>连接地址</b>替换成 CDN 端点(SNI / Host 不变,只改连哪个边缘)。
          </span>
        </label>
        {cdnEnabled && (
          <label className="block">
            <span className="text-sm mb-1 block">指定端点</span>
            <SearchSelect
              value={cdnEndpointId != null ? String(cdnEndpointId) : ""}
              onChange={(v) => setCdnEndpointId(v ? Number(v) : null)}
              placeholder="— 自动(优先级最高的) —"
              searchPlaceholder="搜索端点名称或地址…"
              disabled={cdnPool.length === 0}
              options={[
                { value: "", label: "— 自动(sort_order 最小) —" },
                ...cdnPool.map((c) => ({
                  value: String(c.id),
                  label: `${c.name} · ${c.address}`,
                  sub:   `${c.kind} · 优先级 ${c.sort_order}`,
                })),
              ]}
            />
          </label>
        )}
        <p className="text-xs" style={{ color: "var(--fg-muted)" }}>
          {topCdn
            ? <>当前会套用:<code className="kbd">{topCdn.address}</code>（{topCdn.kind}，sort {topCdn.sort_order}）。共 {cdnPool.length} 个启用端点。</>
            : <>⚠️ 还没有启用的 CDN 端点。先去「CDN 优选」页添加,否则勾选无效（回退到节点地址）。</>}
        </p>
      </fieldset>

      {/* ---- 链式代理 (节点出站二级跳板) ---------------------------- */}
      <fieldset className="card p-4 space-y-2" style={{ background: "var(--bg-elev)" }}>
        <legend className="text-sm font-semibold px-1">🔗 链式代理(出站)</legend>
        <label className="block">
          <span className="text-sm mb-1 block">将本入站的流量经由</span>
          <SearchSelect
            value={chainProxyId != null ? String(chainProxyId) : ""}
            onChange={(v) => setChainProxyId(v ? Number(v) : null)}
            placeholder="— 直连(不走链式代理) —"
            searchPlaceholder="搜索代理名称或地址…"
            options={[
              { value: "", label: "— 直连 —", sub: "不走链式代理" },
              ...chainPool.map((c) => ({
                value: String(c.id),
                label: c.name,
                sub:   `${c.proxy_type}://${c.address}:${c.port}`,
              })),
            ]}
          />
        </label>
        <p className="text-xs" style={{ color: "var(--fg-muted)" }}>
          {chainPool.length === 0
            ? <>⚠️ 还没有启用的链式代理。可在「链式代理」页添加 socks5 / http 上游。</>
            : <>已加载 {chainPool.length} 个启用上游。选中后,本节点的 xray/sing-box 会自动加 outbound + 路由规则(<code className="kbd">inboundTag → chain-N</code>)。</>}
        </p>
      </fieldset>

      {/* ---- VLESS + Reality block --------------------------------- */}
      {protocol === "vless" && tlsMode === "reality" && (
        <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
          <legend className="text-sm font-semibold px-1">Reality 配置</legend>
          <label className="block">
            <span className="text-sm mb-1 block">SNI (server_name) — 仿冒的目标域名</span>
            <input className="input" value={realityServerName}
                   onChange={(e) => setRealityServerName(e.target.value)} list="reality-sni-list" />
            <datalist id="reality-sni-list">
              {COMMON_SNIS.map((s) => <option key={s} value={s} />)}
            </datalist>
          </label>
          <div className="grid grid-cols-[1fr_auto] gap-2 items-end">
            <label className="block">
              <span className="text-sm mb-1 block">私钥 / 公钥(base64url,32 字节)</span>
              <input className="input font-mono text-xs" value={realityPrivateKey}
                     onChange={(e) => setRealityPrivateKey(e.target.value)} placeholder="private_key" />
            </label>
            <button type="button" className="btn btn-ghost" onClick={generateRealityKeys}>
              ⚡ 生成密钥对
            </button>
          </div>
          <input className="input font-mono text-xs" value={realityPublicKey}
                 onChange={(e) => setRealityPublicKey(e.target.value)} placeholder="public_key (订阅页用)" />
          <div className="grid grid-cols-[1fr_auto] gap-2 items-end">
            <label className="block">
              <span className="text-sm mb-1 block">short_id (4–16 位 hex)</span>
              <input className="input font-mono text-xs" value={realityShortId}
                     onChange={(e) => setRealityShortId(e.target.value)} placeholder="6ba85179e30d4fc2" />
            </label>
            <button type="button" className="btn btn-ghost" onClick={randomShortId}>🎲 随机</button>
          </div>
          <label className="block">
            <span className="text-sm mb-1 block">dest (可选 — 默认 {realityServerName || "<sni>"}:443)</span>
            <input className="input" value={realityDest}
                   onChange={(e) => setRealityDest(e.target.value)}
                   placeholder={`${realityServerName || "www.cloudflare.com"}:443`} />
          </label>
        </fieldset>
      )}

      {/* ---- TLS block --------------------------------------------- */}
      {tlsMode === "tls" && (
        <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
          <legend className="text-sm font-semibold px-1">TLS 配置</legend>
          <label className="block">
            <span className="text-sm mb-1 block">SNI / serverName</span>
            <input className="input" value={sni} onChange={(e) => setSni(e.target.value)}
                   placeholder="proxy.example.com" />
          </label>
          <div className="grid grid-cols-2 gap-3">
            <label className="block">
              <span className="text-sm mb-1 block">证书路径(节点上)</span>
              <input className="input font-mono text-xs" value={tlsCertPath}
                     onChange={(e) => setTlsCertPath(e.target.value)}
                     placeholder="/etc/letsencrypt/live/.../fullchain.pem" />
            </label>
            <label className="block">
              <span className="text-sm mb-1 block">私钥路径</span>
              <input className="input font-mono text-xs" value={tlsKeyPath}
                     onChange={(e) => setTlsKeyPath(e.target.value)}
                     placeholder="/etc/letsencrypt/live/.../privkey.pem" />
            </label>
          </div>
        </fieldset>
      )}

      {/* ---- flow (VLESS + tcp + reality is the only sensible combo) */}
      {protocol === "vless" && (transport === "tcp" || tlsMode === "reality") && (
        <label className="block">
          <span className="text-sm mb-1 block">flow</span>
          <SearchSelect
            value={flow}
            onChange={setFlow}
            options={[
              { value: "",                 label: "(none)" },
              { value: "xtls-rprx-vision", label: "xtls-rprx-vision", sub: "推荐 Reality / Vision" },
            ]}
          />
        </label>
      )}

      {/* ---- transport-specific ----------------------------------- */}
      {transport === "ws" && (
        <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
          <legend className="text-sm font-semibold px-1">WebSocket 配置</legend>
          <div className="grid grid-cols-2 gap-3">
            <label className="block">
              <span className="text-sm mb-1 block">path</span>
              <input className="input font-mono text-xs" value={wsPath}
                     onChange={(e) => setWsPath(e.target.value)} placeholder="/ws" />
            </label>
            <label className="block">
              <span className="text-sm mb-1 block">Host header (CDN 域名)</span>
              <input className="input" value={wsHost}
                     onChange={(e) => setWsHost(e.target.value)} placeholder="cdn.example.com" />
            </label>
          </div>
        </fieldset>
      )}

      {transport === "grpc" && (
        <label className="block">
          <span className="text-sm mb-1 block">gRPC serviceName</span>
          <input className="input font-mono text-xs" value={grpcServiceName}
                 onChange={(e) => setGrpcServiceName(e.target.value)} placeholder="grpc" />
        </label>
      )}

      {transport === "xhttp" && (
        <label className="block">
          <span className="text-sm mb-1 block">xhttp path</span>
          <input className="input font-mono text-xs" value={xhttpPath}
                 onChange={(e) => setXhttpPath(e.target.value)} placeholder="/xhttp" />
        </label>
      )}

      {/* ---- Shadowsocks ------------------------------------------ */}
      {protocol === "shadowsocks" && (
        <fieldset className="card p-4 space-y-3" style={{ background: "var(--bg-elev)" }}>
          <legend className="text-sm font-semibold px-1">Shadowsocks 配置</legend>
          <div className="grid grid-cols-[1fr_auto] gap-2 items-end">
            <label className="block">
              <span className="text-sm mb-1 block">method (cipher)</span>
              <SearchSelect
                value={ssMethod}
                onChange={setSsMethod}
                options={SS_METHODS.map((m) => ({
                  value: m,
                  label: m,
                  sub: m.startsWith("2022") ? "SS2022, 更安全" : undefined,
                }))}
              />
            </label>
            <div />
          </div>
          <div className="grid grid-cols-[1fr_auto] gap-2 items-end">
            <label className="block">
              <span className="text-sm mb-1 block">password</span>
              <input className="input font-mono text-xs" value={ssPassword}
                     onChange={(e) => setSsPassword(e.target.value)} placeholder="留空 = 自动生成" />
            </label>
            <button type="button" className="btn btn-ghost" onClick={randomSsPassword}>🎲 生成</button>
          </div>
        </fieldset>
      )}

      {/* ---- Hysteria2 obfs --------------------------------------- */}
      {protocol === "hysteria2" && (
        <label className="block">
          <span className="text-sm mb-1 block">obfs password (Salamander,可选)</span>
          <input className="input font-mono text-xs" value={obfsPassword}
                 onChange={(e) => setObfsPassword(e.target.value)} />
        </label>
      )}

      {/* ---- raw json peek --------------------------------------- */}
      <div>
        <button type="button" className="text-xs underline"
                style={{ color: "var(--fg-muted)" }}
                onClick={() => setShowRaw((v) => !v)}>
          {showRaw ? "隐藏" : "查看"} 高级 · 生成的 params JSON
        </button>
        {showRaw && (
          <pre className="text-xs mt-2 p-3 rounded-md overflow-auto"
               style={{ background: "var(--bg)", border: "1px solid var(--border)" }}>
{JSON.stringify(buildParams(), null, 2)}
          </pre>
        )}
      </div>

      {err && <div className="card p-3 text-sm" style={{ color: "#b91c1c" }}>{err}</div>}
      <div className="flex justify-end gap-2">
        <button type="submit" className="btn btn-primary" disabled={busy}>
          {busy ? "保存中…" : isEdit ? "保存修改" : "创建"}
        </button>
      </div>
    </form>
  );
}
