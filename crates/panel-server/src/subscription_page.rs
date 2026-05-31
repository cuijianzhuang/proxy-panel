//! `GET /s/{token}` — a public, no-auth HTML landing page for a proxy user.
//!
//! The page is fully self-contained:
//!   - inline CSS (sakura palette)
//!   - server-rendered SVG QR code (no JS deps, no external resources)
//!
//! It pairs with `/sub/{token}` which serves the raw subscription content.
//! End users scan the QR with v2rayN / NekoBox / sing-box / Shadowrocket etc.

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use chrono::{DateTime, Utc};
use panel_domain::ProxyUser;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};

use crate::error::ApiError;
use crate::state::AppState;

pub async fn page(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response, ApiError> {
    let user = state
        .proxy_users
        .find_by_subscription_token(&token)
        .await?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "subscription not found"))?;

    let token = &user.subscription_token;
    // Order mirrors the production reference panel (base64 → singbox → clash
    // → surge → quantumult → loon). Backend currently aliases surge / quan /
    // loon to clash since they all consume Mihomo-compatible YAML; the URLs
    // stay distinct so we can specialise per-client later without breaking
    // already-deployed subscription links.
    let cards = vec![
        SubCard {
            title:   "通用订阅",
            tag:     "Base64",
            url:     format!("/sub/{token}"),
            caption: "扫码或点击导入",
        },
        SubCard {
            title:   "sing-box 配置",
            tag:     "JSON",
            url:     format!("/sub/{token}?format=singbox"),
            caption: "扫码或点击导入",
        },
        SubCard {
            title:   "Clash / Mihomo",
            tag:     "YAML",
            url:     format!("/sub/{token}?format=clash"),
            caption: "扫码或点击导入",
        },
        SubCard {
            title:   "Surge",
            tag:     "CONF",
            url:     format!("/sub/{token}?format=surge"),
            caption: "扫码或点击导入",
        },
        SubCard {
            title:   "Quantumult X",
            tag:     "CONF",
            url:     format!("/sub/{token}?format=quantumult"),
            caption: "扫码或点击导入",
        },
        SubCard {
            title:   "Loon",
            tag:     "CONF",
            url:     format!("/sub/{token}?format=loon"),
            caption: "扫码或点击导入",
        },
    ];

    let body = render(&user, &cards);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        )
        .header(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, must-revalidate"),
        )
        .body(axum::body::Body::from(body))
        .unwrap())
}

// ---------------------------------------------------------------------------
// QR generation
// ---------------------------------------------------------------------------

fn qr_svg(text: &str) -> Option<String> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M).ok()?;
    Some(
        code.render::<svg::Color>()
            .min_dimensions(220, 220)
            .quiet_zone(true)
            .dark_color(svg::Color("#1f2937"))
            .light_color(svg::Color("#ffffff"))
            .build(),
    )
}

// ---------------------------------------------------------------------------
// Rendering (hand-written templating to avoid an extra dependency)
// ---------------------------------------------------------------------------

pub struct SubCard {
    pub title:   &'static str,
    pub tag:     &'static str,
    pub url:     String,
    pub caption: &'static str,
}

fn render(user: &ProxyUser, cards: &[SubCard]) -> String {
    let used = format_bytes(user.used_bytes);
    let quota = if user.quota_gb == 0.0 {
        "无限".to_string()
    } else {
        format!("{:.2} GB", user.quota_gb)
    };
    let percent = traffic_percent(user.used_bytes, user.quota_gb);
    let expire = format_expire(user.expires_at);
    let status_badge = if user.enabled {
        r#"<span class="badge badge-ok">已启用</span>"#
    } else {
        r#"<span class="badge badge-err">已停用</span>"#
    };

    // Render each card with its own QR, using `data-url` so the page-side JS
    // can rewrite the relative path into an absolute one (with the real origin).
    let mut grid = String::new();
    for (i, c) in cards.iter().enumerate() {
        let qr = qr_svg(&c.url).unwrap_or_default();
        let target_id = format!("sub-url-{i}");
        grid.push_str(&format!(
            r##"      <article class="card sub-card">
        <header>
          <div class="card-title">{title}</div>
          <span class="format-tag">{tag}</span>
        </header>
        <div class="qr" data-qr-id="qr-{i}">{qr}</div>
        <footer>
          <span>{caption}</span>
          <button class="copy-btn" data-target="{target_id}">复制链接</button>
        </footer>
        <code id="{target_id}" class="hidden">{url}</code>
      </article>
"##,
            title = escape_html(c.title),
            tag = escape_html(c.tag),
            caption = escape_html(c.caption),
            url = escape_html(&c.url),
            qr = qr,
            i = i,
            target_id = target_id,
        ));
    }

    format!(
        r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no" />
  <meta name="referrer" content="no-referrer" />
  <title>{name} · 订阅</title>
  <style>{css}</style>
</head>
<body>
  <main>
    <header class="hero card">
      <div>
        <div class="eyebrow">SUBSCRIPTION</div>
        <h1>{name}</h1>
        <p>扫码导入客户端,或点击卡片复制订阅地址。</p>
      </div>
      <aside>
        <div class="meta">
          <div class="meta-label">流量用量</div>
          <div class="meta-value">{used} <span class="meta-divider">/</span> {quota}</div>
          <div class="bar"><div class="bar-fill" style="width: {percent}%"></div></div>
        </div>
        <div class="meta">
          <div class="meta-label">账号</div>
          <div class="meta-value-sm">
            {status_badge}
            <span class="muted">· 到期 {expire}</span>
          </div>
        </div>
      </aside>
    </header>

    <section class="grid">
{grid}    </section>

    <p class="hint">泄露这些链接相当于泄露完整账号,请妥善保管。</p>
  </main>

  <script>
    // Backfill every relative URL with the page origin, so copy + QR scan land
    // at the absolute address.
    document.querySelectorAll('code.hidden').forEach((node) => {{
      const t = node.textContent || '';
      if (t.startsWith('/')) node.textContent = location.origin + t;
    }});

    document.querySelectorAll('button.copy-btn').forEach((btn) => {{
      btn.addEventListener('click', async () => {{
        const node = document.getElementById(btn.dataset.target);
        if (!node) return;
        const text = node.textContent || '';
        try {{
          if (navigator.clipboard && window.isSecureContext) {{
            await navigator.clipboard.writeText(text);
          }} else {{
            const ta = document.createElement('textarea');
            ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';
            document.body.appendChild(ta); ta.select();
            document.execCommand('copy'); document.body.removeChild(ta);
          }}
          const prev = btn.textContent;
          btn.textContent = '已复制 ✓';
          btn.classList.add('ok');
          setTimeout(() => {{ btn.textContent = prev; btn.classList.remove('ok'); }}, 1600);
        }} catch (e) {{
          btn.textContent = '复制失败';
        }}
      }});
    }});
  </script>
</body>
</html>
"##,
        name = escape_html(&user.name),
        used = used,
        quota = quota,
        percent = percent,
        expire = expire,
        status_badge = status_badge,
        grid = grid,
        css = include_str!("subscription_page.css"),
    )
}

fn format_bytes(b: i64) -> String {
    let b = b as f64;
    if b < 1024.0 {
        return format!("{b:.0} B");
    }
    let kib = b / 1024.0;
    if kib < 1024.0 {
        return format!("{kib:.1} KiB");
    }
    let mib = kib / 1024.0;
    if mib < 1024.0 {
        return format!("{mib:.2} MiB");
    }
    format!("{:.2} GiB", mib / 1024.0)
}

fn traffic_percent(used_bytes: i64, quota_gb: f64) -> i64 {
    if quota_gb <= 0.0 {
        return 0;
    }
    let total = quota_gb * 1024.0 * 1024.0 * 1024.0;
    ((used_bytes as f64 / total) * 100.0).clamp(0.0, 100.0) as i64
}

fn format_expire(expires_at: Option<DateTime<Utc>>) -> String {
    match expires_at {
        None => "永不过期".to_string(),
        Some(t) => t.format("%Y-%m-%d").to_string(),
    }
}

/// Tiny HTML-escape covering the cases we plug into the template.
fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
