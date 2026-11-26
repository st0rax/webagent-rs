//! Runtime provider for the concrete browser backend factory.
//!
//! T-1013: the api crate owns `relay` but must not depend on `transport`, where
//! the concrete `WebBackendFactory` lives (the trait is in core, the impl needs
//! transport). The binary installs one factory type at startup; callers in
//! core/api reach it through this module without naming a transport type.
//! Library users and unit tests that need a real browser must install it too;
//! without installation the accessors return an explicit error instead of
//! silently constructing a different backend.

use std::sync::OnceLock;

use crate::brain::{BackendFactory, BrainBackend, ImageGenerationBackend, RelayBackend};

/// Returned when a second factory tries to replace the installed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactoryAlreadyInstalled;

impl std::fmt::Display for FactoryAlreadyInstalled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "backend factory already installed")
    }
}

impl std::error::Error for FactoryAlreadyInstalled {}

struct Constructors {
    brain: fn(&str) -> Result<Box<dyn BrainBackend>, String>,
    relay: fn(&str) -> Result<Box<dyn RelayBackend>, String>,
    image: fn(&str) -> Result<Box<dyn ImageGenerationBackend>, String>,
}

static FACTORY: OnceLock<Constructors> = OnceLock::new();

/// Installs the process-wide factory type. Returns `Err` if one is already
/// set; the binary treats that as a no-op, the same way it treats a second
/// `panel_sink` installation.
pub fn install<F: BackendFactory + 'static>() -> Result<(), FactoryAlreadyInstalled> {
    FACTORY
        .set(Constructors {
            brain: F::create_brain_backend,
            relay: F::create_relay_backend,
            image: F::create_image_backend,
        })
        .map_err(|_| FactoryAlreadyInstalled)
}

/// `true` when a factory has been installed.
pub fn is_installed() -> bool {
    FACTORY.get().is_some()
}

fn constructors() -> Result<&'static Constructors, String> {
    FACTORY
        .get()
        .ok_or_else(|| "backend factory is not installed".to_string())
}

/// Plain backend for benchmark/counting turns.
pub fn brain_backend(brain_id: &str) -> Result<Box<dyn BrainBackend>, String> {
    (constructors()?.brain)(brain_id)
}

/// Backend with the relay-specific turn operations.
pub fn relay_backend(brain_id: &str) -> Result<Box<dyn RelayBackend>, String> {
    (constructors()?.relay)(brain_id)
}

/// Backend for image-generation turns (T-1009).
pub fn image_backend(brain_id: &str) -> Result<Box<dyn ImageGenerationBackend>, String> {
    (constructors()?.image)(brain_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct EmptyFactory;

    impl BackendFactory for EmptyFactory {
        fn create_brain_backend(_brain_id: &str) -> Result<Box<dyn BrainBackend>, String> {
            Err("no backend".to_string())
        }
        fn create_relay_backend(_brain_id: &str) -> Result<Box<dyn RelayBackend>, String> {
            Err("no backend".to_string())
        }
        fn create_image_backend(
            _brain_id: &str,
        ) -> Result<Box<dyn ImageGenerationBackend>, String> {
            Err("no backend".to_string())
        }
    }

    #[test]
    fn provider_install_is_single_assignment() {
        // The process-wide slot may already be occupied by another test; the
        // invariant holds either way and never replaces an installed factory.
        let _ = install::<EmptyFactory>();
        assert!(install::<EmptyFactory>().is_err());
        assert!(is_installed());
    }

    #[test]
    fn accessors_return_the_factory_error() {
        if install::<EmptyFactory>().is_ok() {
            assert!(matches!(brain_backend("x"), Err(e) if e == "no backend"));
            assert!(matches!(relay_backend("x"), Err(e) if e == "no backend"));
            assert!(matches!(image_backend("x"), Err(e) if e == "no backend"));
        }
    }
}
