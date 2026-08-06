//! Die Umsetzung der CLI-Befehle, thematisch getrennt.
//!
//! `main.rs` traegt nur noch Start, Konsolen-Setup und `dispatch`; die
//! Handler-Rumpfe lagen dort als zusammenhaengender Block von rund 1500 Zeilen.

pub mod ops;
pub mod research;
pub mod ui;
