//! sing-box adapter: render `Listener` → inbound JSON.
//!
//! sing-box's schema is flatter than xray's (no separate `streamSettings` —
//! transport / tls live as siblings on the inbound). Field names use
//! `snake_case` instead of xray's `camelCase`.
//!
//! Coverage (v1):
//!   - vless × {tcp, ws, grpc} × {none, tls, reality}      (reality only on tcp/grpc)
//!   - trojan × {tcp, ws}      × {tls}
//!   - vmess × {tcp, ws}       × {none, tls}
//!   - hysteria2 × quic        × {tls}                     (sing-box only)
//!   - tuic × quic             × {tls}                     (sing-box only)

use panel_core::{
    opt_str, require_str, CoreAdapter, InboundContext, NodeConfigContext, RenderError, Result,
};
use panel_domain::{ChainProxy, ChainProxyType, CoreKind, Listener, Protocol, TlsMode, Transport};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct SingBoxAdapter;

impl CoreAdapter for SingBoxAdapter {
    fn kind(&self) -> CoreKind {
        CoreKind::Singbox
    }

    fn render_inbound(&self, ctx: &InboundContext<'_>) -> Result<Value> {
        self.check_core(ctx.listener)?;

        let mut inbound = serde_json::Map::new();
        inbound.insert(
            "type".to_string(),
            json!(protocol_type(ctx.listener.protocol)),
        );
        inbound.insert("tag".to_string(), json!(ctx.listener.name));
        inbound.insert("listen".to_string(), json!("0.0.0.0"));
        inbound.insert("listen_port".to_string(), json!(ctx.listener.port));

        protocol_specific(ctx, &mut inbound)?;
        transport_specific(ctx.listener, &mut inbound)?;
        tls_specific(ctx.listener, &mut inbound)?;

        // sniffing equivalent
        inbound.insert("sniff".to_string(), json!(true));
        inbound.insert("sniff_override_destination".to_string(), json!(true));

        Ok(Value::Object(inbound))
    }

    fn wrap_config(&self, ctx: &NodeConfigContext<'_>, inbounds: Vec<Value>) -> Value {
        let node = ctx.node;

        // Built-in outbounds.
        let mut outbounds = vec![
            json!({ "type": "direct", "tag": "direct" }),
            json!({ "type": "block",  "tag": "block"  }),
        ];

        // route.rules — DNS short-circuit first, then per-listener chain rules.
        let mut rules: Vec<Value> = vec![json!({ "protocol": "dns", "outbound": "direct" })];

        let mut seen_chains: std::collections::BTreeSet<i64> = Default::default();
        for inb in ctx.inbounds {
            if let Some(chain) = inb.chain {
                let tag = format!("chain-{}", chain.id);
                if seen_chains.insert(chain.id) {
                    outbounds.push(singbox_chain_outbound(chain, &tag));
                }
                // sing-box uses `inbound` (singular) + `outbound`. The inbound
                // is matched by its tag, which we set to the listener name.
                rules.push(json!({
                    "inbound":  [inb.listener.name.clone()],
                    "outbound": tag,
                }));
            }
        }

        let mut cfg = json!({
            "log":      { "level": "warn", "timestamp": true },
            "inbounds": inbounds,
            "outbounds": outbounds,
            "route":    { "rules": rules },
        });

        // Stats API. We expose sing-box's `v2ray_api` (xray-compatible gRPC
        // StatsService) on the management port, rather than the Clash API,
        // because that's what gives per-user up/down counters — the panel's
        // traffic collector queries it with `xray api statsquery` exactly like
        // an xray node. The `users` list (all distinct client emails on this
        // node) is required for sing-box to track per-user counters.
        if node.mgmt_port > 0 {
            let mut emails: Vec<String> = ctx
                .inbounds
                .iter()
                .flat_map(|inb| inb.clients.iter().map(|u| u.name.clone()))
                .collect();
            emails.sort();
            emails.dedup();

            cfg["experimental"] = json!({
                "v2ray_api": {
                    "listen": format!("127.0.0.1:{}", node.mgmt_port),
                    "stats": {
                        "enabled": true,
                        "users":   emails,
                    }
                }
            });
        }
        cfg
    }
}

/// Render one sing-box outbound for a chain proxy. socks5 ⇒ `type: socks`
/// with `version: 5`; http ⇒ `type: http`. Auth fields are omitted when blank.
fn singbox_chain_outbound(chain: &ChainProxy, tag: &str) -> Value {
    let mut o = match chain.proxy_type {
        ChainProxyType::Socks5 => json!({
            "type":    "socks",
            "version": "5",
            "tag":     tag,
            "server":      chain.address.clone(),
            "server_port": chain.port,
        }),
        ChainProxyType::Http => json!({
            "type":    "http",
            "tag":     tag,
            "server":      chain.address.clone(),
            "server_port": chain.port,
        }),
    };
    if let (Some(u), Some(p)) = (chain.username.as_deref(), chain.password.as_deref()) {
        if !u.is_empty() && !p.is_empty() {
            o["username"] = json!(u);
            o["password"] = json!(p);
        }
    }
    o
}

fn protocol_type(p: Protocol) -> &'static str {
    match p {
        Protocol::Vless => "vless",
        Protocol::Vmess => "vmess",
        Protocol::Trojan => "trojan",
        Protocol::Shadowsocks => "shadowsocks",
        Protocol::Hysteria2 => "hysteria2",
        Protocol::Tuic => "tuic",
    }
}

fn protocol_specific(
    ctx: &InboundContext<'_>,
    out: &mut serde_json::Map<String, Value>,
) -> Result<()> {
    let listener = ctx.listener;
    match listener.protocol {
        Protocol::Vless => {
            let flow = opt_str(&listener.params, "flow");
            let users: Vec<Value> = ctx
                .clients
                .iter()
                .map(|u| {
                    let mut o = json!({ "name": u.name, "uuid": u.uuid });
                    if let Some(f) = flow {
                        o["flow"] = json!(f);
                    }
                    o
                })
                .collect();
            out.insert("users".to_string(), json!(users));
        }
        Protocol::Vmess => {
            let users: Vec<Value> = ctx
                .clients
                .iter()
                .map(|u| json!({ "name": u.name, "uuid": u.uuid, "alterId": 0 }))
                .collect();
            out.insert("users".to_string(), json!(users));
        }
        Protocol::Trojan => {
            let users: Vec<Value> = ctx
                .clients
                .iter()
                .map(|u| json!({ "name": u.name, "password": u.password }))
                .collect();
            out.insert("users".to_string(), json!(users));
        }
        Protocol::Shadowsocks => {
            out.insert(
                "method".to_string(),
                json!(opt_str(&listener.params, "method").unwrap_or("aes-128-gcm")),
            );
            out.insert(
                "password".to_string(),
                json!(opt_str(&listener.params, "password").unwrap_or("")),
            );
        }
        Protocol::Hysteria2 => {
            if !matches!(listener.transport, Transport::Quic) {
                return Err(unsupported(listener));
            }
            let users: Vec<Value> = ctx
                .clients
                .iter()
                .map(|u| json!({ "name": u.name, "password": u.password }))
                .collect();
            out.insert("users".to_string(), json!(users));
            if let Some(obfs) = opt_str(&listener.params, "obfs_password") {
                out.insert(
                    "obfs".to_string(),
                    json!({ "type": "salamander", "password": obfs }),
                );
            }
        }
        Protocol::Tuic => {
            if !matches!(listener.transport, Transport::Quic) {
                return Err(unsupported(listener));
            }
            let users: Vec<Value> = ctx
                .clients
                .iter()
                .map(|u| json!({ "name": u.name, "uuid": u.uuid, "password": u.password }))
                .collect();
            out.insert("users".to_string(), json!(users));
            out.insert(
                "congestion_control".to_string(),
                json!(opt_str(&listener.params, "congestion").unwrap_or("bbr")),
            );
        }
    }
    Ok(())
}

fn transport_specific(listener: &Listener, out: &mut serde_json::Map<String, Value>) -> Result<()> {
    let value = match listener.transport {
        Transport::Tcp | Transport::Quic => return Ok(()), // no transport block needed
        Transport::Ws => {
            let mut t = json!({
                "type": "ws",
                "path": opt_str(&listener.params, "ws_path").unwrap_or("/"),
            });
            if let Some(h) = opt_str(&listener.params, "ws_host") {
                t["headers"] = json!({ "Host": h });
            }
            t
        }
        Transport::Grpc => json!({
            "type":         "grpc",
            "service_name": opt_str(&listener.params, "grpc_service_name").unwrap_or(""),
        }),
        Transport::Xhttp => json!({
            "type": "http",
            "path": opt_str(&listener.params, "xhttp_path").unwrap_or("/"),
        }),
    };
    out.insert("transport".to_string(), value);
    Ok(())
}

fn tls_specific(listener: &Listener, out: &mut serde_json::Map<String, Value>) -> Result<()> {
    match listener.tls_mode {
        TlsMode::None => Ok(()),
        TlsMode::Tls => {
            let mut tls = json!({ "enabled": true });
            if let Some(sni) = opt_str(&listener.params, "sni") {
                tls["server_name"] = json!(sni);
            }
            if let (Some(c), Some(k)) = (
                opt_str(&listener.params, "tls_cert_path"),
                opt_str(&listener.params, "tls_key_path"),
            ) {
                tls["certificate_path"] = json!(c);
                tls["key_path"] = json!(k);
            }
            out.insert("tls".to_string(), tls);
            Ok(())
        }
        TlsMode::Reality => {
            if !matches!(listener.protocol, Protocol::Vless) {
                return Err(unsupported(listener));
            }
            if !matches!(listener.transport, Transport::Tcp | Transport::Grpc) {
                return Err(unsupported(listener));
            }

            let server_name = require_str(&listener.params, "reality_server_name")?;
            let private_key = require_str(&listener.params, "reality_private_key")?;
            let short_id = require_str(&listener.params, "reality_short_id")?;
            let dest = opt_str(&listener.params, "reality_dest")
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{server_name}:443"));
            let (dest_host, dest_port) = split_host_port(&dest, 443);

            out.insert(
                "tls".to_string(),
                json!({
                    "enabled":     true,
                    "server_name": server_name,
                    "reality": {
                        "enabled": true,
                        "handshake": {
                            "server":      dest_host,
                            "server_port": dest_port,
                        },
                        "private_key": private_key,
                        "short_id":    [short_id],
                    }
                }),
            );
            Ok(())
        }
    }
}

fn split_host_port(s: &str, default_port: u16) -> (String, u16) {
    if let Some((h, p)) = s.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            return (h.to_string(), port);
        }
    }
    (s.to_string(), default_port)
}

fn unsupported(listener: &Listener) -> RenderError {
    RenderError::UnsupportedCombination {
        core:      "singbox",
        protocol:  listener.protocol.as_str(),
        transport: listener.transport.as_str(),
        tls_mode:  listener.tls_mode.as_str(),
    }
}
