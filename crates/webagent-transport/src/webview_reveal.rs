//! Reveal/Park fuer WebView-Fenster: offscreen by default, onscreen nur bei
//! Nutzeraktion (Login/Captcha), danach wieder parken.
//!
//! `set_bounds(Some(rect)|None)` + `focus_view` leben in
//! [`crate::webview_runtime::WebViewRuntime`]. Dieses Modul kapselt die
//! Aufrufreihenfolge und ist mit einem Mock unit-testbar — ohne echten
//! WebView-Thread.

use crate::panel_sink::{panel_sink, reveal_rect_for_work_area, Rect};
use std::cell::RefCell;

/// Steuerflaeche, die Reveal/Park auf ein Fenster anwendet.
///
/// Produktion: Wrapper um `WebViewRuntime::set_bounds` / `focus_view`.
/// Tests: [`RecordingViewControl`].
pub trait ViewWindowControl {
    fn set_bounds(&self, bounds: Option<Rect>) -> Result<(), String>;
    fn focus_view(&self, focus: bool) -> Result<(), String>;
}

/// Holts das Fenster auf den Bildschirm und gibt Fokus.
pub fn reveal_onscreen(ctrl: &dyn ViewWindowControl, rect: Rect) -> Result<(), String> {
    ctrl.set_bounds(Some(rect))?;
    ctrl.focus_view(true)?;
    Ok(())
}

/// Parkt das Fenster wieder offscreen und gibt den Fokus ab.
pub fn park_offscreen(ctrl: &dyn ViewWindowControl) -> Result<(), String> {
    ctrl.set_bounds(None)?;
    ctrl.focus_view(false)?;
    Ok(())
}

/// Einzel-Brain-Rechteck: zentriert im Arbeitsbereich, sonst Fallback 1280x900.
///
/// Der Arbeitsbereich kommt vom installierten [`crate::panel_sink::PanelSink`],
/// nicht aus der Root-Crate. Ohne Installation gilt derselbe Fallback wie
/// bisher bei nicht ermittelbarem Arbeitsbereich.
pub fn reveal_rect() -> Rect {
    reveal_rect_for_work_area(panel_sink().work_area())
}

/// Aufzeichnung fuer Unit-Tests (Reihenfolge reveal → park).
#[derive(Default)]
pub struct RecordingViewControl {
    pub calls: RefCell<Vec<&'static str>>,
    pub bounds: RefCell<Vec<Option<Rect>>>,
    pub focuses: RefCell<Vec<bool>>,
}

impl ViewWindowControl for RecordingViewControl {
    fn set_bounds(&self, bounds: Option<Rect>) -> Result<(), String> {
        self.calls.borrow_mut().push(if bounds.is_some() {
            "set_bounds(Some)"
        } else {
            "set_bounds(None)"
        });
        self.bounds.borrow_mut().push(bounds);
        Ok(())
    }

    fn focus_view(&self, focus: bool) -> Result<(), String> {
        self.calls
            .borrow_mut()
            .push(if focus { "focus(true)" } else { "focus(false)" });
        self.focuses.borrow_mut().push(focus);
        Ok(())
    }
}

#[cfg(feature = "webview")]
pub struct RuntimeViewControl<'a> {
    pub runtime: &'a crate::webview_runtime::WebViewRuntime,
    pub view_id: u64,
}

#[cfg(feature = "webview")]
impl ViewWindowControl for RuntimeViewControl<'_> {
    fn set_bounds(&self, bounds: Option<Rect>) -> Result<(), String> {
        self.runtime
            .set_bounds(self.view_id, bounds)
            .map_err(|e| e.to_string())
    }

    fn focus_view(&self, focus: bool) -> Result<(), String> {
        self.runtime
            .focus_view(self.view_id, focus)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_then_park_calls_bounds_and_focus() {
        let ctrl = RecordingViewControl::default();
        let rect = Rect::new(10, 20, 800, 600);
        reveal_onscreen(&ctrl, rect).unwrap();
        park_offscreen(&ctrl).unwrap();
        assert_eq!(
            *ctrl.calls.borrow(),
            vec![
                "set_bounds(Some)",
                "focus(true)",
                "set_bounds(None)",
                "focus(false)",
            ]
        );
        assert_eq!(ctrl.bounds.borrow()[0], Some(rect));
        assert_eq!(ctrl.bounds.borrow()[1], None);
        assert_eq!(*ctrl.focuses.borrow(), vec![true, false]);
    }

    #[test]
    fn reveal_rect_fallback_ohne_work_area_ist_nicht_leer() {
        let r = reveal_rect();
        assert!(!r.is_empty());
        assert!(r.width >= 800);
        assert!(r.height >= 600);
    }
}
