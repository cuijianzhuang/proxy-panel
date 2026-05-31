//! Public subscription endpoint: `GET /sub/{token}[?format=...]`.
//!
//! The token IS the auth — no session cookie required. Supported formats:
//!   - `base64` (default) — v2rayN / Shadowrocket / NekoBox compatible URI list
//!   - `clash`            — Clash Meta / Mihomo YAML
//!   - `singbox`          — sing-box outbounds JSON
//!
//! All three render off the same `Listener` rows; each format produces what
//! the matching client expects to consume.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use base64::Engine;
use panel_core::opt_str;
use panel_domain::{CdnEndpoint, Listener, Protocol, ProxyUser, TlsMode, Transport};
use serde::Deserialize;
use serde_json::json;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct SubQuery {
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubFormat {
    Base64,
    Clash,
    SingBox,
}

impl SubFormat {
    pub fn parse(s: Option<&str>) -> Self {
        match s.unwrap_or("base64") {
            // Surge / Quantumult X / Loon all consume Mihomo-compatible YAML,
            // so they alias to Clash today. Keeping the alias here means we
            // can hot-swap in a per-client renderer later without touching
            // already-deployed `?format=surge` subscription URLs.
            "clash" | "mihomo" | "yaml"
            | "surge" | "quantumult" | "quantumult-x" | "quanx"
            | "loon" => Self::Clash,
            "singbox" | "sing-box" | "json" => Self::SingBox,
            _ => Self::Base64,
        }
    }
    fn content_type(self) -> &'static str {
        match self {
            Self::Base64 => "text/plain; charset=utf-8",
            Self::Clash => "text/yaml; charset=utf-8",
            Self::SingBox => "application/json; charset=utf-8",
        }
    }
}

pub async fn subscribe(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(q): Query<SubQuery>,
) -> Result<Response, ApiError> {
    let user = state
        .proxy_users
        .find_by_subscription_token(&token)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "subscription not found"))?;

    if !user.enabled {
        return Err(ApiError::new(StatusCode::FORBIDDEN, "subscription disabled"));
    }

    let listener_ids = state.proxy_users.listener_ids_for_user(user.id).await?;
    let mut listeners = Vec::new();
    for lid in listener_ids {
        if let Some(l) = state.listeners.find(lid).await? {
            if l.enabled {
                listeners.push(l);
            }
        }
    }

    // CDN 优选 pool — all *enabled* endpoints, sorted by priority (lowest
    // sort_order first). Listeners flagged `cdn_enabled` get their connect
    // host swapped to one of these; if `params.cdn_endpoint_id` is set, that
    // exact endpoint wins, otherwise the highest-priority one is used.
    // SNI / ws_host are deliberately left untouched — CDN 优选 only changes
    // *which edge* the client dials, not what domain the origin terminates.
    let cdn_pool: Vec<CdnEndpoint> = state
        .cdn_endpoints
        .list()
        .await
        .map(|eps| eps.into_iter().filter(|e| e.enabled).collect())
        .unwrap_or_default();

    let format = SubFormat::parse(q.format.as_deref());
    let body = match format {
        SubFormat::Base64 => render_base64(&listeners, &user, &state.public_host, &cdn_pool),
        SubFormat::Clash => render_clash(&listeners, &user, &state.public_host, &cdn_pool),
        SubFormat::SingBox => render_singbox(&listeners, &user, &state.public_host, &cdn_pool),
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, HeaderValue::from_static(format.content_type()))
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, must-revalidate"),
        )
        .header(
            "Subscription-Userinfo",
            HeaderValue::from_str(&userinfo_header(&user)).unwrap_or(HeaderValue::from_static("")),
        )
        .body(Body::from(body))
        .unwrap())
}

// ---------------------------------------------------------------------------
// Host resolution
// ---------------------------------------------------------------------------

/// Whether this listener opts into CDN優选 (set via `params.cdn_enabled`).
fn cdn_enabled(l: &Listener) -> bool {
    l.params.get("cdn_enabled").and_then(|v| v.as_bool()).unwrap_or(false)
}

/// The address the client should *connect* to:
///   - cdn_enabled + `cdn_endpoint_id` set → that specific endpoint (if still
///     present + enabled, otherwise fall through to default behaviour)
///   - cdn_enabled + no id pin → first of the enabled pool (highest priority)
///   - else → listener's own `public_host` param, else the panel default
///
/// SNI / ws_host are intentionally left to each builder; this only decides the
/// dial target (URI authority / `server`).
fn effective_host<'a>(
    l:            &'a Listener,
    default_host: &'a str,
    cdn_pool:     &'a [CdnEndpoint],
) -> &'a str {
    if cdn_enabled(l) && !cdn_pool.is_empty() {
        // Pinned endpoint wins when it's still in the enabled pool.
        let pinned = l.params.get("cdn_endpoint_id").and_then(|v| v.as_i64());
        if let Some(id) = pinned {
            if let Some(ep) = cdn_pool.iter().find(|e| e.id == id) {
                return &ep.address;
            }
            // Pinned id no longer enabled → silently fall through to top of
            // pool rather than refusing to swap, so a stale subscription
            // still works.
        }
        return &cdn_pool[0].address;
    }
    opt_str(&l.params, "public_host").unwrap_or(default_host)
}

// ---------------------------------------------------------------------------
// Format renderers
// ---------------------------------------------------------------------------

fn render_base64(
    listeners: &[Listener],
    user: &ProxyUser,
    default_host: &str,
    cdn_pool: &[CdnEndpoint],
) -> String {
    let uris: Vec<String> = listeners
        .iter()
        .filter_map(|l| build_uri(l, user, effective_host(l, default_host, cdn_pool)))
        .collect();
    base64::engine::general_purpose::STANDARD.encode(uris.join("\n").as_bytes())
}

/// Clash Meta / Mihomo YAML — proxies + auto urltest group + minimal rules.
fn render_clash(
    listeners: &[Listener],
    user: &ProxyUser,
    default_host: &str,
    cdn_pool: &[CdnEndpoint],
) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("# generated by proxy-panel\n");
    out.push_str("mixed-port: 7890\n");
    out.push_str("allow-lan: false\n");
    out.push_str("mode: rule\n");
    out.push_str("log-level: info\n\n");
    out.push_str("proxies:\n");

    let mut names = Vec::new();
    for l in listeners {
        let host = effective_host(l, default_host, cdn_pool);
        if let Some(block) = clash_proxy_block(l, user, host) {
            out.push_str(&block);
            names.push(l.name.clone());
        }
    }

    out.push_str("\nproxy-groups:\n");
    out.push_str("  - name: \"AUTO\"\n");
    out.push_str("    type: url-test\n");
    out.push_str("    url: \"https://www.gstatic.com/generate_204\"\n");
    out.push_str("    interval: 300\n");
    out.push_str("    proxies:\n");
    for n in &names {
        out.push_str(&format!("      - \"{}\"\n", yaml_escape(n)));
    }
    out.push_str("  - name: \"PROXY\"\n");
    out.push_str("    type: select\n");
    out.push_str("    proxies:\n");
    out.push_str("      - AUTO\n");
    out.push_str("      - DIRECT\n");
    for n in &names {
        out.push_str(&format!("      - \"{}\"\n", yaml_escape(n)));
    }

    out.push_str("\nrules:\n");
    out.push_str("  - GEOIP,LAN,DIRECT\n");
    out.push_str("  - GEOIP,CN,DIRECT\n");
    out.push_str("  - MATCH,PROXY\n");
    out
}

fn clash_proxy_block(l: &Listener, user: &ProxyUser, host: &str) -> Option<String> {
    let name = yaml_escape(&l.name);
    let mut out = String::new();
    match l.protocol {
        Protocol::Vless => {
            out.push_str(&format!("  - name: \"{name}\"\n"));
            out.push_str("    type: vless\n");
            out.push_str(&format!("    server: \"{host}\"\n"));
            out.push_str(&format!("    port: {}\n", l.port));
            out.push_str(&format!("    uuid: \"{}\"\n", user.uuid));
            out.push_str(&format!("    network: {}\n", transport_clash(l.transport)));
            if let Some(flow) = opt_str(&l.params, "flow") {
                out.push_str(&format!("    flow: \"{flow}\"\n"));
            }
            match l.tls_mode {
                TlsMode::None => {
                    out.push_str("    tls: false\n");
                }
                TlsMode::Tls => {
                    out.push_str("    tls: true\n");
                    if let Some(sni) = opt_str(&l.params, "sni") {
                        out.push_str(&format!("    servername: \"{sni}\"\n"));
                    }
                }
                TlsMode::Reality => {
                    out.push_str("    tls: true\n");
                    out.push_str("    client-fingerprint: chrome\n");
                    if let Some(sni) = opt_str(&l.params, "reality_server_name") {
                        out.push_str(&format!("    servername: \"{sni}\"\n"));
                    }
                    out.push_str("    reality-opts:\n");
                    if let Some(pbk) = opt_str(&l.params, "reality_public_key") {
                        out.push_str(&format!("      public-key: \"{pbk}\"\n"));
                    }
                    if let Some(sid) = opt_str(&l.params, "reality_short_id") {
                        out.push_str(&format!("      short-id: \"{sid}\"\n"));
                    }
                }
            }
            ws_or_grpc_clash(l, &mut out);
        }
        Protocol::Trojan => {
            out.push_str(&format!("  - name: \"{name}\"\n"));
            out.push_str("    type: trojan\n");
            out.push_str(&format!("    server: \"{host}\"\n"));
            out.push_str(&format!("    port: {}\n", l.port));
            out.push_str(&format!("    password: \"{}\"\n", user.password));
            if let Some(sni) = opt_str(&l.params, "sni") {
                out.push_str(&format!("    sni: \"{sni}\"\n"));
            }
            if matches!(l.transport, Transport::Ws) {
                out.push_str("    network: ws\n");
                ws_or_grpc_clash(l, &mut out);
            }
        }
        Protocol::Shadowsocks => {
            out.push_str(&format!("  - name: \"{name}\"\n"));
            out.push_str("    type: ss\n");
            out.push_str(&format!("    server: \"{host}\"\n"));
            out.push_str(&format!("    port: {}\n", l.port));
            out.push_str(&format!(
                "    cipher: \"{}\"\n",
                opt_str(&l.params, "method").unwrap_or("aes-128-gcm")
            ));
            out.push_str(&format!(
                "    password: \"{}\"\n",
                opt_str(&l.params, "password").unwrap_or("")
            ));
        }
        // VMess / Hysteria2 / TUIC Clash blocks deferred.
        _ => return None,
    }
    Some(out)
}

fn ws_or_grpc_clash(l: &Listener, out: &mut String) {
    match l.transport {
        Transport::Ws => {
            out.push_str("    ws-opts:\n");
            out.push_str(&format!(
                "      path: \"{}\"\n",
                opt_str(&l.params, "ws_path").unwrap_or("/")
            ));
            if let Some(h) = opt_str(&l.params, "ws_host") {
                out.push_str(&format!("      headers:\n        Host: \"{h}\"\n"));
            }
        }
        Transport::Grpc => {
            out.push_str("    grpc-opts:\n");
            out.push_str(&format!(
                "      grpc-service-name: \"{}\"\n",
                opt_str(&l.params, "grpc_service_name").unwrap_or("")
            ));
        }
        _ => {}
    }
}

fn transport_clash(t: Transport) -> &'static str {
    match t {
        Transport::Tcp => "tcp",
        Transport::Ws => "ws",
        Transport::Grpc => "grpc",
        Transport::Xhttp => "http",
        Transport::Quic => "quic",
    }
}

fn yaml_escape(s: &str) -> String {
    s.replace('"', "\\\"")
}

/// sing-box outbounds JSON — a complete client-side config snippet.
fn render_singbox(
    listeners: &[Listener],
    user: &ProxyUser,
    default_host: &str,
    cdn_pool: &[CdnEndpoint],
) -> String {
    let mut outbounds = Vec::new();

    // Direct / block always present.
    let mut names: Vec<&str> = Vec::new();
    for l in listeners {
        let host = effective_host(l, default_host, cdn_pool);
        if let Some(obj) = singbox_outbound(l, user, host) {
            outbounds.push(obj);
            names.push(&l.name);
        }
    }

    // Selector + urltest groups
    let names_json: Vec<serde_json::Value> = names.iter().map(|n| json!(n)).collect();
    let mut all_outbounds_for_groups = vec![json!("AUTO"), json!("direct")];
    all_outbounds_for_groups.extend(names.iter().map(|n| json!(n)));

    outbounds.push(json!({
        "type":      "urltest",
        "tag":       "AUTO",
        "outbounds": names_json.clone(),
        "url":       "https://www.gstatic.com/generate_204",
        "interval":  "5m",
    }));
    outbounds.push(json!({
        "type":      "selector",
        "tag":       "PROXY",
        "outbounds": all_outbounds_for_groups,
        "default":   "AUTO",
    }));
    outbounds.push(json!({ "type": "direct", "tag": "direct" }));
    outbounds.push(json!({ "type": "block",  "tag": "block"  }));
    outbounds.push(json!({ "type": "dns",    "tag": "dns-out" }));

    let cfg = json!({
        "log": { "level": "info", "timestamp": true },
        "dns": {
            "servers": [
                { "tag": "google", "address": "8.8.8.8",   "detour": "PROXY" },
                { "tag": "local",  "address": "223.5.5.5", "detour": "direct" }
            ],
            "rules": [{ "outbound": ["any"], "server": "google" }]
        },
        "inbounds": [{
            "type": "mixed",
            "tag":  "in",
            "listen": "127.0.0.1",
            "listen_port": 7890
        }],
        "outbounds": outbounds,
        "route": {
            "rules": [
                { "protocol": "dns", "outbound": "dns-out" },
                { "ip_is_private": true, "outbound": "direct" }
            ],
            "final": "PROXY",
            "auto_detect_interface": true
        }
    });
    serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".into())
}

fn singbox_outbound(l: &Listener, user: &ProxyUser, host: &str) -> Option<serde_json::Value> {
    let mut o = serde_json::Map::new();
    o.insert("tag".into(), json!(l.name));
    o.insert("server".into(), json!(host));
    o.insert("server_port".into(), json!(l.port));

    match l.protocol {
        Protocol::Vless => {
            o.insert("type".into(), json!("vless"));
            o.insert("uuid".into(), json!(user.uuid));
            if let Some(flow) = opt_str(&l.params, "flow") {
                o.insert("flow".into(), json!(flow));
            }
        }
        Protocol::Trojan => {
            o.insert("type".into(), json!("trojan"));
            o.insert("password".into(), json!(user.password));
        }
        Protocol::Shadowsocks => {
            o.insert("type".into(), json!("shadowsocks"));
            o.insert(
                "method".into(),
                json!(opt_str(&l.params, "method").unwrap_or("aes-128-gcm")),
            );
            o.insert(
                "password".into(),
                json!(opt_str(&l.params, "password").unwrap_or("")),
            );
        }
        _ => return None,
    }

    // tls + transport
    match l.tls_mode {
        TlsMode::None => {}
        TlsMode::Tls => {
            let mut tls = json!({ "enabled": true });
            if let Some(sni) = opt_str(&l.params, "sni") {
                tls["server_name"] = json!(sni);
            }
            o.insert("tls".into(), tls);
        }
        TlsMode::Reality => {
            let sni = opt_str(&l.params, "reality_server_name").unwrap_or("");
            let pbk = opt_str(&l.params, "reality_public_key").unwrap_or("");
            let sid = opt_str(&l.params, "reality_short_id").unwrap_or("");
            o.insert(
                "tls".into(),
                json!({
                    "enabled":     true,
                    "server_name": sni,
                    "utls":        { "enabled": true, "fingerprint": "chrome" },
                    "reality":     { "enabled": true, "public_key": pbk, "short_id": sid }
                }),
            );
        }
    }
    match l.transport {
        Transport::Ws => {
            let mut t = json!({ "type": "ws",
                "path": opt_str(&l.params, "ws_path").unwrap_or("/") });
            if let Some(h) = opt_str(&l.params, "ws_host") {
                t["headers"] = json!({ "Host": h });
            }
            o.insert("transport".into(), t);
        }
        Transport::Grpc => {
            o.insert(
                "transport".into(),
                json!({
                    "type": "grpc",
                    "service_name": opt_str(&l.params, "grpc_service_name").unwrap_or("")
                }),
            );
        }
        _ => {}
    }

    Some(serde_json::Value::Object(o))
}

// ---------------------------------------------------------------------------
// URI builders
// ---------------------------------------------------------------------------

fn build_uri(listener: &Listener, user: &ProxyUser, default_host: &str) -> Option<String> {
    let host = opt_str(&listener.params, "public_host").unwrap_or(default_host);
    match listener.protocol {
        Protocol::Vless => Some(build_vless(listener, user, host)),
        Protocol::Trojan => Some(build_trojan(listener, user, host)),
        Protocol::Shadowsocks => Some(build_ss(listener, host)),
        // Vmess / Hysteria2 / Tuic URI generation deferred; skip those listeners
        // for now rather than emitting broken URIs.
        Protocol::Vmess | Protocol::Hysteria2 | Protocol::Tuic => None,
    }
}

fn build_vless(listener: &Listener, user: &ProxyUser, host: &str) -> String {
    let mut q = QueryBuilder::new();
    q.push("type", transport_name(listener.transport));

    match listener.tls_mode {
        TlsMode::None => {
            q.push("security", "none");
        }
        TlsMode::Tls => {
            q.push("security", "tls");
            if let Some(sni) = opt_str(&listener.params, "sni") {
                q.push("sni", sni);
            }
        }
        TlsMode::Reality => {
            q.push("security", "reality");
            if let Some(pbk) = opt_str(&listener.params, "reality_public_key") {
                q.push("pbk", pbk);
            }
            if let Some(sid) = opt_str(&listener.params, "reality_short_id") {
                q.push("sid", sid);
            }
            if let Some(sni) = opt_str(&listener.params, "reality_server_name") {
                q.push("sni", sni);
            }
            q.push("fp", opt_str(&listener.params, "reality_fingerprint").unwrap_or("chrome"));
        }
    }

    // transport-specific
    match listener.transport {
        Transport::Ws => {
            q.push("path", opt_str(&listener.params, "ws_path").unwrap_or("/"));
            if let Some(h) = opt_str(&listener.params, "ws_host") {
                q.push("host", h);
            }
        }
        Transport::Grpc => {
            if let Some(s) = opt_str(&listener.params, "grpc_service_name") {
                q.push("serviceName", s);
            }
        }
        Transport::Xhttp => {
            q.push("path", opt_str(&listener.params, "xhttp_path").unwrap_or("/"));
        }
        Transport::Tcp | Transport::Quic => {}
    }

    if let Some(flow) = opt_str(&listener.params, "flow") {
        q.push("flow", flow);
    }

    let frag = url_encode_fragment(&listener.name);
    format!(
        "vless://{uuid}@{host}:{port}?{qs}#{frag}",
        uuid = user.uuid,
        host = host,
        port = listener.port,
        qs = q.build(),
    )
}

fn build_trojan(listener: &Listener, user: &ProxyUser, host: &str) -> String {
    let mut q = QueryBuilder::new();
    q.push("type", transport_name(listener.transport));
    q.push("security", "tls");
    if let Some(sni) = opt_str(&listener.params, "sni") {
        q.push("sni", sni);
    }
    if matches!(listener.transport, Transport::Ws) {
        q.push("path", opt_str(&listener.params, "ws_path").unwrap_or("/"));
        if let Some(h) = opt_str(&listener.params, "ws_host") {
            q.push("host", h);
        }
    }
    let frag = url_encode_fragment(&listener.name);
    format!(
        "trojan://{pw}@{host}:{port}?{qs}#{frag}",
        pw = url_encode_userinfo(&user.password),
        host = host,
        port = listener.port,
        qs = q.build(),
    )
}

fn build_ss(listener: &Listener, host: &str) -> String {
    let method = opt_str(&listener.params, "method").unwrap_or("aes-128-gcm");
    let password = opt_str(&listener.params, "password").unwrap_or("");
    let userinfo = base64::engine::general_purpose::STANDARD
        .encode(format!("{method}:{password}").as_bytes());
    let frag = url_encode_fragment(&listener.name);
    format!(
        "ss://{ui}@{host}:{port}#{frag}",
        ui = userinfo,
        host = host,
        port = listener.port,
    )
}

fn transport_name(t: Transport) -> &'static str {
    match t {
        Transport::Tcp => "tcp",
        Transport::Ws => "ws",
        Transport::Grpc => "grpc",
        Transport::Xhttp => "xhttp",
        Transport::Quic => "quic",
    }
}

/// Encode the `Subscription-Userinfo` header (a de facto standard adopted by
/// most clients). Bytes used / total / expiry, all in seconds.
fn userinfo_header(user: &ProxyUser) -> String {
    let total_bytes: i64 = (user.quota_gb * 1024.0 * 1024.0 * 1024.0) as i64;
    let expire_secs = user.expires_at.map(|t| t.timestamp()).unwrap_or(0);
    format!(
        "upload=0; download={}; total={}; expire={}",
        user.used_bytes, total_bytes, expire_secs
    )
}

// ---------------------------------------------------------------------------
// Tiny query-string builder. Keeps order stable + skips empty values.
// ---------------------------------------------------------------------------

struct QueryBuilder {
    parts: Vec<(String, String)>,
}

impl QueryBuilder {
    fn new() -> Self {
        Self { parts: Vec::new() }
    }
    fn push(&mut self, k: &str, v: impl Into<String>) {
        let v = v.into();
        if !v.is_empty() {
            self.parts.push((k.to_string(), v));
        }
    }
    fn build(&self) -> String {
        url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(self.parts.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .finish()
    }
}

/// Percent-encode a fragment (URL `#...` portion) — used for the human-readable
/// node name displayed in client UIs.
fn url_encode_fragment(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Userinfo escaping for password-bearing schemes (trojan). Avoid `@`, `:`, `/`.
fn url_encode_userinfo(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}
