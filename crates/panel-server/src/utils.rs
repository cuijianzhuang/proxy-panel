//! Small utility endpoints used by the UI to fill in listener forms without
//! the user typing cryptographic blobs by hand.
//!
//! All admin-only — they're cheap surfaces that don't need to be hammered
//! by anonymous traffic.

use axum::Json;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::rngs::OsRng;
use rand::Rng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::auth::RequireAdmin;
use crate::error::ApiError;

// ---------------------------------------------------------------------------
// Reality x25519 keypair
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RealityKeyPair {
    pub private_key: String,
    pub public_key:  String,
}

/// Generate a fresh Reality keypair. Returned in the base64url-no-pad form
/// Xray expects in its `realitySettings.{privateKey,publicKey}` fields.
pub async fn reality_keypair(_: RequireAdmin) -> Result<Json<RealityKeyPair>, ApiError> {
    let secret = StaticSecret::random_from_rng(OsRng);
    let public = PublicKey::from(&secret);
    Ok(Json(RealityKeyPair {
        private_key: URL_SAFE_NO_PAD.encode(secret.to_bytes()),
        public_key:  URL_SAFE_NO_PAD.encode(public.as_bytes()),
    }))
}

// ---------------------------------------------------------------------------
// Random hex id (default 8 bytes → 16 hex chars; usable as short_id, salt…)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RandomIdQuery {
    /// Bytes of entropy. Clamped to 1..=32. Result has `bytes*2` hex chars.
    #[serde(default)]
    pub bytes: Option<usize>,
}

#[derive(Serialize)]
pub struct RandomIdOut {
    pub value: String,
}

pub async fn random_id(
    _: RequireAdmin,
    axum::extract::Query(q): axum::extract::Query<RandomIdQuery>,
) -> Result<Json<RandomIdOut>, ApiError> {
    let n = q.bytes.unwrap_or(8).clamp(1, 32);
    let mut bytes = vec![0u8; n];
    OsRng.fill_bytes(&mut bytes);
    Ok(Json(RandomIdOut { value: hex::encode(bytes) }))
}

// ---------------------------------------------------------------------------
// Random port suggestion (10000..=60000 by default, avoiding the well-known range)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct RandomPortQuery {
    pub min: Option<u16>,
    pub max: Option<u16>,
}

#[derive(Serialize)]
pub struct RandomPortOut {
    pub port: u16,
}

pub async fn random_port(
    _: RequireAdmin,
    axum::extract::Query(q): axum::extract::Query<RandomPortQuery>,
) -> Result<Json<RandomPortOut>, ApiError> {
    let min = q.min.unwrap_or(10000).max(1);
    let max = q.max.unwrap_or(60000).min(65535).max(min);
    let port: u16 = OsRng.gen_range(min..=max);
    Ok(Json(RandomPortOut { port }))
}
