use std::sync::Arc;

use panel_auth::{PanelUserRepo, SessionRepo};
use panel_domain::{
    CdnEndpointRepo, ChainProxyRepo, ListenerRepo, NewSample, NodeRepo, NotificationChannelRepo,
    NotificationRuleRepo, PlanRepo, ProxyUserRepo, StatsRepo,
};
use panel_node::{NodeRemote, StatUser};
use panel_notify::Notification;
use panel_persistence::Database;
use panel_task::TaskRepo;

use crate::adapters::Adapters;

#[derive(Clone)]
pub struct AppState {
    pub db:                 Database,
    pub users:              PanelUserRepo,
    pub sessions:           SessionRepo,
    pub listeners:          ListenerRepo,
    pub plans:              PlanRepo,
    pub proxy_users:        ProxyUserRepo,
    pub nodes:              NodeRepo,
    pub tasks:              TaskRepo,
    pub cdn_endpoints:      CdnEndpointRepo,
    pub chain_proxies:      ChainProxyRepo,
    pub channels:           NotificationChannelRepo,
    pub notification_rules: NotificationRuleRepo,
    pub stats:              StatsRepo,
    pub remote:             Arc<dyn NodeRemote>,
    pub adapters:           Adapters,
    pub cookie_secure:      bool,
    pub public_host:        String,
}

/// Summary returned by one traffic-collection pass.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CollectSummary {
    pub nodes_polled:   usize,
    pub samples_written: usize,
    pub users_cut_off:  usize,
}

impl AppState {
    /// Fire an event through the notification system: look up the rule for
    /// `event_type`, resolve its enabled channels, and fan out. Best-effort —
    /// failures are logged, never propagated to the triggering operation.
    pub async fn notify(&self, event_type: &str, title: impl Into<String>, body: impl Into<String>) {
        let rule = match self.notification_rules.get(event_type).await {
            Ok(r) if r.enabled && !r.channel_ids.is_empty() => r,
            _ => return,
        };
        let channels = match self.channels.find_enabled_by_ids(&rule.channel_ids).await {
            Ok(c) if !c.is_empty() => c,
            _ => return,
        };
        let msg = Notification {
            event_type: event_type.to_string(),
            title:      title.into(),
            body:       body.into(),
        };
        let outcomes = panel_notify::dispatch(&channels, &msg).await;
        let failed = outcomes.iter().filter(|o| !o.ok).count();
        if failed > 0 {
            tracing::warn!(event_type, failed, "some notification channels failed");
        }
    }

    /// One traffic-collection pass over every node.
    ///
    /// For each node: gather the proxy users attached to its listeners, pull
    /// per-user byte deltas from the node, append `stats_samples`, bump each
    /// user's `used_bytes`, and — if a user with a non-zero quota crossed it —
    /// disable the user and fire a `quota_exceed` notification.
    pub async fn collect_traffic(&self) -> CollectSummary {
        let mut summary = CollectSummary { nodes_polled: 0, samples_written: 0, users_cut_off: 0 };

        let nodes = match self.nodes.list().await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(%e, "collect_traffic: list nodes failed");
                return summary;
            }
        };

        for node in &nodes {
            // Build the unique set of users attached to this node's listeners.
            let listeners = self.listeners.list_for_node(node.id).await.unwrap_or_default();
            let mut seen = std::collections::HashMap::<i64, String>::new();
            for l in &listeners {
                if let Ok(clients) = self.proxy_users.list_for_listener(l.id).await {
                    for c in clients {
                        seen.entry(c.id).or_insert(c.name);
                    }
                }
            }
            if seen.is_empty() {
                continue;
            }
            let stat_users: Vec<StatUser> =
                seen.iter().map(|(id, name)| StatUser { id: *id, email: name.clone() }).collect();

            let deltas = match self.remote.fetch_stats(node, &stat_users).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(node = %node.name, %e, "fetch_stats failed");
                    continue;
                }
            };
            summary.nodes_polled += 1;

            let samples: Vec<NewSample> = deltas
                .iter()
                .map(|d| NewSample {
                    node_id: node.id,
                    proxy_user_id: d.id,
                    up_delta: d.up,
                    down_delta: d.down,
                })
                .collect();
            if self.stats.insert_samples(&samples).await.is_ok() {
                summary.samples_written += samples.iter().filter(|s| s.up_delta + s.down_delta > 0).count();
            }

            // Apply to used_bytes + quota enforcement.
            for d in &deltas {
                let bytes = d.up + d.down;
                if bytes <= 0 {
                    continue;
                }
                let _ = self.proxy_users.add_used_bytes(d.id, bytes).await;

                if let Ok(Some(u)) = self.proxy_users.find(d.id).await {
                    let quota_bytes = (u.quota_gb * 1024.0 * 1024.0 * 1024.0) as i64;
                    if u.enabled && quota_bytes > 0 && u.used_bytes >= quota_bytes {
                        let _ = self.proxy_users.set_enabled(d.id, false).await;
                        summary.users_cut_off += 1;
                        self.notify(
                            "quota_exceed",
                            format!("用户 {} 超额停用", u.name),
                            format!(
                                "用户 {} 已用 {:.2} GB / 配额 {:.2} GB,已自动停用。",
                                u.name,
                                u.used_bytes as f64 / 1024.0 / 1024.0 / 1024.0,
                                u.quota_gb,
                            ),
                        )
                        .await;
                    }
                }
            }
        }
        summary
    }
}
