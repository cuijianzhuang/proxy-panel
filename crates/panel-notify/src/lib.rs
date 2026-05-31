//! Notification senders + fan-out dispatch.
//!
//! This crate is a *pure sender*: it takes a list of already-resolved
//! `NotificationChannel`s (loaded from the DB by the caller) plus a message,
//! and delivers to each. It owns the protocol details (Telegram Bot API,
//! generic webhook POST, SMTP via lettre) but knows nothing about how
//! channels are stored or which event routed here.
//!
//! All sends are best-effort and independent: one channel failing never
//! blocks the others. `dispatch` returns a per-channel result vector so the
//! caller (e.g. a "test send" endpoint) can surface what happened.

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use panel_domain::{ChannelType, NotificationChannel};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("missing config field `{0}`")]
    MissingField(&'static str),
    #[error("http: {0}")]
    Http(String),
    #[error("smtp: {0}")]
    Smtp(String),
    #[error("invalid config: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// A message to deliver. `event_type` is included so webhook receivers can
/// route, and so the formatted text can carry context.
#[derive(Debug, Clone)]
pub struct Notification {
    pub event_type: String,
    pub title:      String,
    pub body:       String,
}

/// Outcome of sending to one channel.
#[derive(Debug, Clone)]
pub struct SendOutcome {
    pub channel_id:   i64,
    pub channel_name: String,
    pub ok:           bool,
    pub detail:       String,
}

/// Send `msg` to every channel, collecting per-channel outcomes.
pub async fn dispatch(channels: &[NotificationChannel], msg: &Notification) -> Vec<SendOutcome> {
    let mut out = Vec::with_capacity(channels.len());
    for ch in channels {
        let res = send_one(ch, msg).await;
        out.push(match res {
            Ok(()) => SendOutcome {
                channel_id: ch.id, channel_name: ch.name.clone(),
                ok: true, detail: "sent".into(),
            },
            Err(e) => {
                tracing::warn!(channel = %ch.name, error = %e, "notification send failed");
                SendOutcome {
                    channel_id: ch.id, channel_name: ch.name.clone(),
                    ok: false, detail: e.to_string(),
                }
            }
        });
    }
    out
}

/// Send to a single channel by its declared type.
pub async fn send_one(ch: &NotificationChannel, msg: &Notification) -> Result<()> {
    match ch.kind {
        ChannelType::Telegram => send_telegram(&ch.config, msg).await,
        ChannelType::Webhook => send_webhook(&ch.config, msg).await,
        ChannelType::Smtp => send_smtp(&ch.config, msg).await,
    }
}

// ---------------------------------------------------------------------------
// Telegram
// ---------------------------------------------------------------------------

async fn send_telegram(cfg: &serde_json::Value, msg: &Notification) -> Result<()> {
    let token = field(cfg, "bot_token")?;
    let chat_id = field(cfg, "chat_id")?;
    let text = format!("*{}*\n{}", escape_md(&msg.title), escape_md(&msg.body));

    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&json!({ "chat_id": chat_id, "text": text, "parse_mode": "MarkdownV2" }))
        .send()
        .await
        .map_err(|e| Error::Http(e.to_string()))?;
    if !resp.status().is_success() {
        let code = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Http(format!("telegram {code}: {body}")));
    }
    Ok(())
}

/// Telegram MarkdownV2 requires escaping a specific set of chars.
fn escape_md(s: &str) -> String {
    const SPECIAL: &[char] = &[
        '_', '*', '[', ']', '(', ')', '~', '`', '>', '#', '+', '-', '=', '|', '{', '}', '.', '!',
    ];
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if SPECIAL.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------
// Webhook
// ---------------------------------------------------------------------------

async fn send_webhook(cfg: &serde_json::Value, msg: &Notification) -> Result<()> {
    let url = field(cfg, "url")?;
    let client = reqwest::Client::new();
    let mut req = client.post(url).json(&json!({
        "event_type": msg.event_type,
        "title":      msg.title,
        "body":       msg.body,
    }));
    // Optional single custom header (e.g. an auth token).
    if let (Some(name), Some(value)) = (
        cfg.get("header_name").and_then(|v| v.as_str()),
        cfg.get("header_value").and_then(|v| v.as_str()),
    ) {
        if !name.is_empty() {
            req = req.header(name, value);
        }
    }
    let resp = req.send().await.map_err(|e| Error::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(Error::Http(format!("webhook returned {}", resp.status())));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// SMTP
// ---------------------------------------------------------------------------

async fn send_smtp(cfg: &serde_json::Value, msg: &Notification) -> Result<()> {
    let host = field(cfg, "host")?;
    let from = field(cfg, "from")?;
    let to = field(cfg, "to")?;
    let port = cfg.get("port").and_then(|v| v.as_u64()).unwrap_or(587) as u16;
    let username = cfg.get("username").and_then(|v| v.as_str()).unwrap_or("");
    let password = cfg.get("password").and_then(|v| v.as_str()).unwrap_or("");

    let email = Message::builder()
        .from(from.parse().map_err(|e| Error::Invalid(format!("from: {e}")))?)
        .to(to.parse().map_err(|e| Error::Invalid(format!("to: {e}")))?)
        .subject(format!("[proxy-panel] {}", msg.title))
        .header(ContentType::TEXT_PLAIN)
        .body(msg.body.clone())
        .map_err(|e| Error::Invalid(e.to_string()))?;

    let mut builder = AsyncSmtpTransport::<Tokio1Executor>::relay(host)
        .map_err(|e| Error::Smtp(e.to_string()))?
        .port(port);
    if !username.is_empty() {
        builder = builder.credentials(Credentials::new(username.to_string(), password.to_string()));
    }
    let mailer = builder.build();
    mailer.send(email).await.map_err(|e| Error::Smtp(e.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------

fn field<'a>(cfg: &'a serde_json::Value, key: &'static str) -> Result<&'a str> {
    cfg.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or(Error::MissingField(key))
}
