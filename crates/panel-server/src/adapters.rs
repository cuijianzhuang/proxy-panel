//! Static registry of core adapters. Pick by `Listener.core`.

use std::sync::Arc;

use panel_core::CoreAdapter;
use panel_core_singbox::SingBoxAdapter;
use panel_core_xray::XrayAdapter;
use panel_domain::CoreKind;

#[derive(Clone)]
pub struct Adapters {
    xray:    Arc<XrayAdapter>,
    singbox: Arc<SingBoxAdapter>,
}

impl Adapters {
    pub fn new() -> Self {
        Self {
            xray:    Arc::new(XrayAdapter),
            singbox: Arc::new(SingBoxAdapter),
        }
    }

    pub fn for_core(&self, kind: CoreKind) -> &dyn CoreAdapter {
        match kind {
            CoreKind::Xray => self.xray.as_ref(),
            CoreKind::Singbox => self.singbox.as_ref(),
        }
    }
}
