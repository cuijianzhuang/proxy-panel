//! Xray-core adapter: render `Listener` → inbound JSON.
//!
//! Coverage (v1, enough to validate the abstraction):
//!   protocol × transport × tls_mode
//!   - vless × tcp × {none, tls, reality}
//!   - vless × ws  × {none, tls}
//!   - vless × grpc × {none, tls}
//!   - vmess × {tcp, ws} × {none, tls}
//!   - trojan × {tcp, ws} × {tls}
//!
//! Other combos return `UnsupportedCombination` — extend as needed; the
//! match arms are intentionally explicit so unsupported cases fail loudly
//! rather than producing a half-rendered config that xray would reject.

use panel_core::{
    opt_str, require_str, CoreAdapter, InboundContext, NodeConfigContext, RenderError, Result,
};
use panel_domain::{
    ChainProxy, ChainProxyType, CoreKind, Listener, Protocol, ProxyUser, TlsMode, Transport,
};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct XrayAdapter;

impl CoreAdapter for XrayAdapter {
    fn kind(&self) -> CoreKind {
        CoreKind::Xray
    }

    fn render_inbound(&self, ctx: &InboundContext<'_>) -> Result<Value> {
        self.check_core(ctx.listener)?;

        let settings = render_settings(ctx)?;
        let stream = render_stream_settings(ctx.listener)?;

        Ok(json!({
            "tag":            ctx.listener.name,
            "listen":         "0.0.0.0",
            "port":           ctx.listener.port,
            "protocol":       ctx.listener.protocol.as_str(),
            "settings":       settings,
            "streamSettings": stream,
            "sniffing":       sniffing_defaults(),
        }))
    }

    fn wrap_config(
        &self,
        ctx:          &NodeConfigContext<'_>,
        mut inbounds: Vec<Value>,
    ) -> Value {
        let node = ctx.node;

        // Optional gRPC stats endpoint, bound on 127.0.0.1:mgmt_port. The panel
        // will eventually reach in via a WireGuard tunnel or SSH local-forward.
        if node.mgmt_port > 0 {
            inbounds.push(json!({
                "tag":      "api",
                "listen":   "127.0.0.1",
                "port":     node.mgmt_port,
                "protocol": "dokodemo-door",
                "settings": { "address": "127.0.0.1" },
            }));
        }

        // Built-in outbounds.
        let mut outbounds = vec![
            json!({ "tag": "direct",  "protocol": "freedom"   }),
            json!({ "tag": "blocked", "protocol": "blackhole" }),
        ];

        // Routing rules — api self-loop first, then per-listener chain rules.
        let mut routing_rules = vec![];
        if node.mgmt_port > 0 {
            routing_rules.push(json!({
                "type":        "field",
                "inboundTag":  ["api"],
                "outboundTag": "api",
            }));
        }

        // Chain proxies: dedupe by id so two inbounds using the same chain
        // share a single outbound, then add an `inboundTag → chain-N` rule
        // for each listener that wants to be routed through it.
        let mut seen_chains: std::collections::BTreeSet<i64> = Default::default();
        for inb in ctx.inbounds {
            if let Some(chain) = inb.chain {
                let tag = format!("chain-{}", chain.id);
                if seen_chains.insert(chain.id) {
                    outbounds.push(xray_chain_outbound(chain, &tag));
                }
                routing_rules.push(json!({
                    "type":        "field",
                    "inboundTag":  [inb.listener.name.clone()],
                    "outboundTag": tag,
                }));
            }
        }

        json!({
            "log": { "loglevel": "warning" },
            "api": {
                "tag":      "api",
                "services": ["StatsService", "HandlerService"],
            },
            "stats": {},
            "policy": {
                "levels": {
                    "0": {
                        "statsUserUplink":   true,
                        "statsUserDownlink": true,
                    }
                },
                "system": {
                    "statsInboundUplink":   true,
                    "statsInboundDownlink": true,
                },
            },
            "inbounds":  inbounds,
            "outbounds": outbounds,
            "routing": {
                "domainStrategy": "AsIs",
                "rules":          routing_rules,
            },
        })
    }
}

/// Render one xray outbound for a chain proxy. socks ⇒ `protocol: socks` with
/// a `servers[]` entry; http ⇒ `protocol: http`. Auth fields are omitted when
/// blank so xray doesn't choke on empty user/pass tuples.
fn xray_chain_outbound(chain: &ChainProxy, tag: &str) -> Value {
    let mut server = json!({
        "address": chain.address.clone(),
        "port":    chain.port,
    });
    if let (Some(u), Some(p)) = (chain.username.as_deref(), chain.password.as_deref()) {
        if !u.is_empty() && !p.is_empty() {
            server["users"] = json!([{ "user": u, "pass": p }]);
        }
    }
    let protocol = match chain.proxy_type {
        ChainProxyType::Socks5 => "socks",
        ChainProxyType::Http   => "http",
    };
    json!({
        "tag":      tag,
        "protocol": protocol,
        "settings": { "servers": [server] },
    })
}

// ---------------------------------------------------------------------------
// `settings`: protocol-specific user/decryption block
// ---------------------------------------------------------------------------

fn render_settings(ctx: &InboundContext<'_>) -> Result<Value> {
    let listener = ctx.listener;
    match listener.protocol {
        Protocol::Vless => {
            let flow = opt_str(&listener.params, "flow"); // e.g. "xtls-rprx-vision"
            let clients: Vec<Value> = ctx
                .clients
                .iter()
                .map(|u| vless_client(u, flow))
                .collect();
            Ok(json!({
                "clients":    clients,
                "decryption": "none",
            }))
        }
        Protocol::Vmess => {
            let clients: Vec<Value> =
                ctx.clients.iter().map(vmess_client).collect();
            Ok(json!({ "clients": clients }))
        }
        Protocol::Trojan => {
            let clients: Vec<Value> =
                ctx.clients.iter().map(trojan_client).collect();
            Ok(json!({ "clients": clients }))
        }
        Protocol::Shadowsocks => Ok(json!({
            "method":   opt_str(&listener.params, "method").unwrap_or("aes-128-gcm"),
            "password": opt_str(&listener.params, "password").unwrap_or(""),
            "network":  "tcp,udp",
        })),
        // Xray-core doesn't speak Hysteria2 / TUIC.
        Protocol::Hysteria2 | Protocol::Tuic => Err(unsupported(listener)),
    }
}

fn vless_client(u: &ProxyUser, flow: Option<&str>) -> Value {
    let mut obj = json!({
        "id":    u.uuid,
        "email": u.name,
    });
    if let Some(f) = flow {
        obj["flow"] = json!(f);
    }
    obj
}

fn vmess_client(u: &ProxyUser) -> Value {
    json!({
        "id":      u.uuid,
        "email":   u.name,
        "alterId": 0,
    })
}

fn trojan_client(u: &ProxyUser) -> Value {
    json!({
        "password": u.password,
        "email":    u.name,
    })
}

// ---------------------------------------------------------------------------
// `streamSettings`: transport + TLS / Reality
// ---------------------------------------------------------------------------

fn render_stream_settings(listener: &Listener) -> Result<Value> {
    let mut stream = serde_json::Map::new();

    // network = transport
    stream.insert("network".to_string(), json!(transport_name(listener.transport)));

    // transport-specific sub-block
    if let Some((field, value)) = transport_block(listener)? {
        stream.insert(field.to_string(), value);
    }

    // security + sub-block
    match listener.tls_mode {
        TlsMode::None => {
            stream.insert("security".to_string(), json!("none"));
        }
        TlsMode::Tls => {
            stream.insert("security".to_string(), json!("tls"));
            stream.insert("tlsSettings".to_string(), tls_block(listener)?);
        }
        TlsMode::Reality => {
            // Reality is only valid combined with specific protocols & transports.
            if !matches!(listener.protocol, Protocol::Vless) {
                return Err(unsupported(listener));
            }
            if !matches!(listener.transport, Transport::Tcp | Transport::Grpc) {
                return Err(unsupported(listener));
            }
            stream.insert("security".to_string(), json!("reality"));
            stream.insert("realitySettings".to_string(), reality_block(listener)?);
        }
    }

    Ok(Value::Object(stream))
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

fn transport_block(listener: &Listener) -> Result<Option<(&'static str, Value)>> {
    match listener.transport {
        Transport::Tcp => Ok(None),
        Transport::Ws => {
            let path = opt_str(&listener.params, "ws_path").unwrap_or("/");
            let host = opt_str(&listener.params, "ws_host");
            let mut block = json!({ "path": path });
            if let Some(h) = host {
                block["headers"] = json!({ "Host": h });
            }
            Ok(Some(("wsSettings", block)))
        }
        Transport::Grpc => {
            let svc = opt_str(&listener.params, "grpc_service_name").unwrap_or("");
            Ok(Some(("grpcSettings", json!({ "serviceName": svc }))))
        }
        Transport::Xhttp => {
            let path = opt_str(&listener.params, "xhttp_path").unwrap_or("/");
            Ok(Some(("xhttpSettings", json!({ "path": path }))))
        }
        Transport::Quic => Ok(None),
    }
}

fn tls_block(listener: &Listener) -> Result<Value> {
    let cert = opt_str(&listener.params, "tls_cert_path");
    let key = opt_str(&listener.params, "tls_key_path");
    let mut tls = json!({
        "alpn": ["h2", "http/1.1"],
    });
    if let (Some(c), Some(k)) = (cert, key) {
        tls["certificates"] = json!([{
            "certificateFile": c,
            "keyFile":         k,
        }]);
    }
    if let Some(sni) = opt_str(&listener.params, "sni") {
        tls["serverName"] = json!(sni);
    }
    Ok(tls)
}

fn reality_block(listener: &Listener) -> Result<Value> {
    let server_name = require_str(&listener.params, "reality_server_name")?;
    let private_key = require_str(&listener.params, "reality_private_key")?;
    let short_id = require_str(&listener.params, "reality_short_id")?;

    // `dest` defaults to `<server_name>:443` if not set explicitly.
    let dest = opt_str(&listener.params, "reality_dest")
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{server_name}:443"));

    Ok(json!({
        "show":         false,
        "dest":         dest,
        "xver":         0,
        "serverNames":  [server_name],
        "privateKey":   private_key,
        "shortIds":     [short_id],
    }))
}

fn sniffing_defaults() -> Value {
    json!({
        "enabled":       true,
        "destOverride":  ["http", "tls"],
        "routeOnly":     false,
    })
}

fn unsupported(listener: &Listener) -> RenderError {
    RenderError::UnsupportedCombination {
        core:      "xray",
        protocol:  listener.protocol.as_str(),
        transport: listener.transport.as_str(),
        tls_mode:  listener.tls_mode.as_str(),
    }
}
