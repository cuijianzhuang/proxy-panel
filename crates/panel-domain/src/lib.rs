//! Domain model + repositories for proxy-panel.

mod cdn_endpoint;
mod chain_proxy;
mod error;
mod listener;
mod node;
mod notification;
mod plan;
mod proxy_user;
mod stats;

pub use cdn_endpoint::{CdnEndpoint, CdnEndpointRepo, CdnKind, CreateCdnEndpoint, UpdateCdnEndpoint};
pub use chain_proxy::{
    ChainProxy, ChainProxyRepo, ChainProxyType, CreateChainProxy, UpdateChainProxy,
};
pub use error::{Error, Result};
pub use notification::{
    ChannelType, CreateChannel, NotificationChannel, NotificationChannelRepo, NotificationRule,
    NotificationRuleRepo, UpdateChannel, UpsertRule, EVENT_TYPES,
};
pub use listener::{
    CoreKind, CreateListener, Listener, ListenerRepo, Protocol, TlsMode, Transport,
    UpdateListener,
};
pub use node::{CreateNode, Node, NodeRepo, NodeStatus, UpdateNode};
pub use plan::{CreatePlan, Plan, PlanRepo, QuotaType, UpdatePlan};
pub use proxy_user::{CreateProxyUser, ProxyUser, ProxyUserRepo, UpdateProxyUser};
pub use stats::{DailyPoint, NewSample, StatsRepo, UserTotal};
