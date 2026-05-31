//! Core abstraction: render panel `Listener` rows into the JSON shape that
//! Xray-core or sing-box expects.
//!
//! The trait is intentionally narrow for now — one `Listener` → one inbound
//! object. Whole-node config rendering (combining listeners + outbounds +
//! routing) will land alongside the node-management module.

use panel_domain::{ChainProxy, CoreKind, Listener, Node, ProxyUser};

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("unsupported combination for {core}: protocol={protocol} transport={transport} tls_mode={tls_mode}")]
    UnsupportedCombination {
        core:      &'static str,
        protocol:  &'static str,
        transport: &'static str,
        tls_mode:  &'static str,
    },
    #[error("missing required param `{0}` for this protocol/tls combination")]
    MissingParam(&'static str),
    #[error("invalid param `{name}`: {reason}")]
    InvalidParam { name: &'static str, reason: String },
    #[error("wrong adapter: listener.core = {found:?}, adapter is {expected:?}")]
    WrongAdapter { expected: CoreKind, found: CoreKind },
}

pub type Result<T> = std::result::Result<T, RenderError>;

/// All inputs an adapter needs to render one inbound.
///
/// `clients` is the materialised list of `proxy_users` attached to the
/// listener (already filtered by `enabled = true`). The adapter takes a
/// borrow so callers can reuse the same slice across multiple listeners.
pub struct InboundContext<'a> {
    pub listener: &'a Listener,
    pub clients:  &'a [ProxyUser],
    /// Optional upstream proxy this listener's traffic should be routed
    /// through (国内中转 → 海外落地). When set, the node config gets a matching
    /// outbound + a routing rule keyed on the listener's tag.
    pub chain:    Option<&'a ChainProxy>,
}

/// Inputs for rendering a whole-node config.
pub struct NodeConfigContext<'a> {
    pub node:     &'a Node,
    pub inbounds: &'a [InboundContext<'a>],
}

/// Render a single `Listener` to the inbound-object shape its core expects.
///
/// The output is a fully self-contained JSON `Value` — caller can plug it
/// directly into Xray's `inbounds[]` or sing-box's `inbounds[]` array.
pub trait CoreAdapter: Send + Sync {
    fn kind(&self) -> CoreKind;

    fn render_inbound(&self, ctx: &InboundContext<'_>) -> Result<serde_json::Value>;

    /// Render the full `config.json` for one node. The default impl walks
    /// `ctx.inbounds`, calls `render_inbound` for each, and delegates the
    /// wrapping shell to `wrap_config` — adapters typically only override
    /// `wrap_config` rather than reimplement the loop.
    fn render_node_config(&self, ctx: &NodeConfigContext<'_>) -> Result<serde_json::Value> {
        if ctx.node.core != self.kind() {
            return Err(RenderError::WrongAdapter {
                expected: self.kind(),
                found:    ctx.node.core,
            });
        }
        let mut inbounds = Vec::with_capacity(ctx.inbounds.len());
        for inb in ctx.inbounds {
            if inb.listener.core != self.kind() {
                return Err(RenderError::WrongAdapter {
                    expected: self.kind(),
                    found:    inb.listener.core,
                });
            }
            inbounds.push(self.render_inbound(inb)?);
        }
        Ok(self.wrap_config(ctx, inbounds))
    }

    /// Wrap rendered inbounds into the core-specific top-level config object
    /// (log / outbounds / routing / stats-or-clash-api / …). Adapters read
    /// `ctx.inbounds[i].chain` to materialise chain-proxy outbounds plus the
    /// matching routing rule (xray: `inboundTag` → `outboundTag`; sing-box:
    /// `route.rules` with `inbound` → `outbound`).
    fn wrap_config(
        &self,
        ctx:      &NodeConfigContext<'_>,
        inbounds: Vec<serde_json::Value>,
    ) -> serde_json::Value;

    /// Default: enforce that the listener targets this adapter.
    fn check_core(&self, listener: &Listener) -> Result<()> {
        if listener.core != self.kind() {
            return Err(RenderError::WrongAdapter {
                expected: self.kind(),
                found:    listener.core,
            });
        }
        Ok(())
    }
}

// ============================================================================
// Param helpers used by both adapters
// ============================================================================

/// Fetch a required string param from `listener.params`.
pub fn require_str<'a>(
    params: &'a serde_json::Value,
    key: &'static str,
) -> Result<&'a str> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or(RenderError::MissingParam(key))
}

/// Optional string param.
pub fn opt_str<'a>(params: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}
