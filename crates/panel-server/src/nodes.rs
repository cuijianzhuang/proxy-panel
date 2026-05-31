//! `/api/nodes` CRUD plus `/api/nodes/:id/config` for the full rendered config.

use axum::extract::{Path, State};
use axum::Json;
use panel_core::{InboundContext, NodeConfigContext};
use panel_domain::{CreateNode, Node, UpdateNode};
use serde_json::json;

use crate::auth::{CurrentUser, RequireAdmin};
use crate::error::ApiError;
use crate::state::AppState;

pub async fn list(_: CurrentUser, State(state): State<AppState>) -> Result<Json<Vec<Node>>, ApiError> {
    Ok(Json(state.nodes.list().await?))
}

pub async fn get_one(
    _: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Node>, ApiError> {
    let row = state
        .nodes
        .find(id)
        .await?
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "node not found"))?;
    Ok(Json(row))
}

pub async fn create(
    _: RequireAdmin,
    State(state): State<AppState>,
    Json(input): Json<CreateNode>,
) -> Result<(axum::http::StatusCode, Json<Node>), ApiError> {
    let row = state.nodes.create(input).await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<UpdateNode>,
) -> Result<Json<Node>, ApiError> {
    Ok(Json(state.nodes.update(id, patch).await?))
}

/// `POST /api/nodes/:id/test-connection` — open an SSH session and run a
/// trivial command, returning the remote identity (uname) on success. Lets the
/// operator verify per-node credentials before pushing a real config. In
/// dry-run remote mode this returns the simulated identity.
pub async fn test_connection(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let node = state.nodes.find(id).await?.ok_or_else(|| {
        ApiError::new(axum::http::StatusCode::NOT_FOUND, "node not found")
    })?;
    match state.remote.ping(&node).await {
        Ok(ident) => Ok(Json(json!({ "ok": true, "identity": ident }))),
        Err(e) => Ok(Json(json!({ "ok": false, "error": e.to_string() }))),
    }
}

pub async fn delete(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.nodes.delete(id).await? {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "node not found",
        ));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Render the full `config.json` for a node — combines every enabled listener
/// attached to the node with the matching adapter's wrapping shell.
///
/// Returns 422 if any listener's core mismatches the node's core (callers
/// should normally prevent that at create time, but the renderer enforces it).
pub async fn render_config(
    _: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let node = state.nodes.find(id).await?.ok_or_else(|| {
        ApiError::new(axum::http::StatusCode::NOT_FOUND, "node not found")
    })?;

    // Listeners + their attached users.
    let listeners = state.listeners.list_for_node(node.id).await?;
    let mut owned_clients = Vec::with_capacity(listeners.len());
    for l in &listeners {
        let clients = state.proxy_users.list_for_listener(l.id).await?;
        owned_clients.push(clients);
    }

    // Pre-load all enabled chain proxies once so we can resolve
    // `params.chain_proxy_id` per listener without an N+1 query.
    let chain_pool = state.chain_proxies.list_enabled().await.unwrap_or_default();
    let resolve_chain = |l: &panel_domain::Listener| -> Option<&panel_domain::ChainProxy> {
        let id = l.params.get("chain_proxy_id")?.as_i64()?;
        chain_pool.iter().find(|c| c.id == id)
    };

    let inbounds: Vec<InboundContext> = listeners
        .iter()
        .zip(owned_clients.iter())
        .map(|(l, c)| InboundContext {
            listener: l,
            clients:  c.as_slice(),
            chain:    resolve_chain(l),
        })
        .collect();

    let adapter = state.adapters.for_core(node.core);
    let config = adapter.render_node_config(&NodeConfigContext {
        node:     &node,
        inbounds: &inbounds,
    })?;

    Ok(Json(json!({
        "node_id":       node.id,
        "core":          node.core,
        "inbound_count": listeners.len(),
        "config":        config,
    })))
}
