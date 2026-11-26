//! WebAgent core library.
//!
//! Platform-neutral modules extracted in T-1011. The root crate re-exports all
//! of them, so existing `crate::<module>` paths in the binary and tests keep working.

pub mod support;
pub use support::*;

pub mod acceptance;
pub mod bench_events;
pub mod brain;
pub mod brain_limits;
pub mod brain_score;
pub mod capability;
pub mod capability_proof;
pub mod capability_types;
pub mod circuit_breaker;
pub mod code_score;
pub mod config;
pub mod contract;
pub mod doctor;
pub mod executor;
pub mod file_actions;
pub mod goal_plan;
pub mod inference_types;
pub mod loop_guard;
pub mod network_observation;
pub mod observer;
pub mod pool_failover;
pub mod pool_state;
pub mod prompts;
pub mod protocol;
pub mod run_store;
pub mod scoring;
pub mod shell_policy;
pub mod taskboard;
pub mod timeouts;
pub mod tools;
pub mod transcript;
pub mod watchdog;
pub mod wiki_memory;
pub mod worker_pool;
