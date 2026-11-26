//! WebAgent — lokaler, browserbasierter Agent (Rust-Port des Python-Originals).
//!
//! Plattformunabhängiger Kern: Windows, Linux, Android (Browser via Embedded WebView).
//! Prozess-Liveness liefert dieses Crate selbst (siehe [`ProcessSnapshot`]), damit der
//! Kern überall baut. Zeitstempel werden *formatiert* über [`civil_utc`] (Python-kompatibel,
//! siehe dort) und *geparst* über `time` — das ist ohnehin Dependency.

// Modul-Übersicht, gruppiert nach Schicht (siehe docs/ARCHITECTURE.md).
// Alle Module `pub`: die echte API-Fläche ist kleiner (nur das Binary nutzt
// ~31 Module direkt) — Verschlankung ist als P1 notiert, sie erfordert das
// Löschen toter Items (siehe docs/ARCHITECTURE.md → "API-Fläche").

// ── core: plattformreiner Kern (keine UI/Browser-Abhängigkeiten) ──
// T-1013: api_bridge, browser_inference, comms, free_cloud_chat, https_client,
// oobe, session and source_scope now live in webagent-api; `relay` moved with
// them and is re-exported below. Existing `crate::<module>` paths stay valid.
pub mod fakebrain;
pub mod group_run;
pub mod memory;
pub mod startup;
pub mod web_ui;
pub mod web_ui_api;

// ── brain: Gehirn-Abstraktion + Browser-Anbindung ──
// T-1012: the browser/page/panel modules live in webagent-transport and are
// re-exported below, so `crate::<module>` paths in the binary and tests keep
// resolving exactly as before.
pub mod bin_hooks;

// ── agent: Steuerung & Agent-Schleife ──
// `counting` calls WebBackendFactory::create_brain_backend; that BackendFactory
// impl lives in webagent-transport/browser/backend.rs, so the module stays here
// until the factory is injected as a trait object (separate task).
pub mod controller;
pub mod counting;
pub mod knockout;
pub mod welcome;

// Autoresearch: Canary-Tests für Brain-Erreichbarkeit
pub mod canary;

// ── bench: Messung & Selbst-Verbesserung ──
pub mod autoresearch;
pub mod bench_harvest;
pub mod bench_scoring;
pub mod benchmark;
pub mod brains_health;
pub mod design_vote;
pub mod round_tally;
pub mod runs_report;
pub mod self_research;

// ── workers: Parallelität & Gesundheit ──
pub mod bot2bot_worker;

// ── ui: TUI, REPL ──
pub mod brain_grid;
pub mod brain_wall;
pub mod repl;
pub mod target_check;
pub mod tui;
pub mod tui_ansi;
#[cfg(feature = "tui")]
pub mod tui_bench;
pub mod tui_config;
#[cfg(feature = "tui")]
pub mod tui_footer;
#[cfg(feature = "tui")]
pub mod tui_keys;
pub mod tui_load;
#[cfg(feature = "tui")]
pub mod tui_mouse;
#[cfg(feature = "tui")]
pub mod tui_render;
pub mod tui_state;
#[cfg(feature = "tui")]
pub mod tui_widgets;

// T-1011/T-1012/T-1013: platform-neutral modules live in webagent-core,
// browser/page modules in webagent-transport, api/relay modules in webagent-api.
// Re-export all three so the root crate keeps its public API and
// crate::<module> paths. The concrete WebBackendFactory marker is re-exported
// by webagent-transport itself (see its lib.rs).
pub use webagent_api::*;
pub use webagent_core::*;
pub use webagent_transport::*;
