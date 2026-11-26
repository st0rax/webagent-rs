//! WebAgent transport library.
//!
//! Browser, page and panel modules extracted in T-1012. The root crate
//! re-exports everything, so existing `crate::<module>` paths keep working.

pub mod brain_probe;
pub mod browser;
pub mod browser_pool;
pub mod login;
pub mod mock_page;
pub mod page_driver;
pub mod panel_sink;
pub mod webview_reveal;

#[cfg(any(feature = "webview", test))]
mod upload_spa_js;
#[cfg(feature = "webview")]
pub mod webview_runtime;

/// Concrete [`webagent_core::brain::BackendFactory`] marker. The trait lives
/// in core; the impl lives next to this type in `browser::backend.rs` so the
/// orphan rule stays satisfied after the move.
pub use browser::backend::WebBackendFactory;
