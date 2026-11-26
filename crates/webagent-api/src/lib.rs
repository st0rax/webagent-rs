//! WebAgent API library.
//!
//! Local OpenAI-compatible bridge, relay and session modules extracted in
//! T-1013. The root crate re-exports everything, so existing
//! `crate::<module>` paths keep working.

pub mod api_bridge;
pub mod browser_inference;
pub mod comms;
pub mod free_cloud_chat;
pub mod https_client;
pub mod oobe;
pub mod relay;
pub mod session;
pub mod source_scope;
