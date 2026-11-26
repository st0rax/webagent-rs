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
pub mod api_bridge;
pub mod browser_inference;
pub mod comms;
pub mod fakebrain;
pub mod free_cloud_chat;
pub mod group_run;
pub mod https_client;
pub mod memory;
pub mod oobe;
pub mod session;
pub mod source_scope;
pub mod startup;
pub mod web_ui;
pub mod web_ui_api;

// ── brain: Gehirn-Abstraktion + Browser-Anbindung ──
pub mod bin_hooks;
pub mod brain_probe;
pub mod browser;
pub mod browser_pool;
pub mod login;
pub mod mock_page;
pub mod page_driver;
pub mod panel_sink;
#[cfg(any(feature = "webview", test))]
mod upload_spa_js;
pub mod webview_reveal;

#[cfg(feature = "webview")]
pub mod webview_runtime;

// ── agent: Steuerung & Agent-Schleife ──
// `counting` calls WebBackendFactory::create_brain_backend; that BackendFactory
// impl lives in browser/backend.rs (root crate), so the module stays here until
// the factory is injected as a dependency (separate task).
pub mod controller;
pub mod counting;
pub mod knockout;
pub mod relay;
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

// T-1011: the platform-neutral modules now live in webagent-core. Re-export
// them here so the root crate keeps its public API and crate::<module> paths.
pub use webagent_core::*;
// Browser-local factory marker; re-exported at the root so callers need no
// transport-layer path (the architecture ratchet counts module edges).
pub use crate::browser::backend::WebBackendFactory;
