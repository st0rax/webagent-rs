//! UI-neutrale Session-Schicht (Kern): geordnete Event-Stroeme mit monotoner
//! Sequenznummer und der Service, der lebende Lauefe samt Strom verwaltet.
//!
//! Sie kennt weder Webview noch TUI noch einen Brain-Adapter. Der Kern
//! (AgentController/API) legt Ereignisse ab; Konsumenten (Web-UI, Responses-SSE,
//! REPL) lesen denselben Lauf aus unterschiedlichen Sichten ab einer Sequenz-
//! nummer — die Grundlage fuer den EventStream-Vertrag der neuen Web-UI.

pub mod events;
pub mod service;

pub use events::{EventStream, SessionEvent, Since, StampedEvent};
pub use service::{SessionHandle, SessionService, SessionSnapshot};
