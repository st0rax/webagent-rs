//! Gemeinsame Kachel-Geometrie und die Grenze zu Desktop-Fakten.
//!
//! Dieses Modul ist absichtlich abhaengigkeitsfrei: es rechnet mit Rechtecken
//! und kennt ein `PanelSink` fuer die wenigen Desktop-Fragen, die die
//! Transport-Schicht braucht (Arbeitsbereich und Terminalfenster). Die Antworten
//! liefert die Anwendung — bei `webagent` ist das `brain_grid` in der Root-Crate.
//! Fenster werden hier nicht verschoben und nicht fokussiert; diese Logik bleibt
//! bei `brain_grid`, `brain_wall` und `webview_runtime`.
//!
//! Der Installationspunkt ist der Binary-Einstieg. Ohne Installation antwortet
//! [`NoPanelSink`] mit `None`, damit Bibliotheksnutzer und Unit-Tests keine
//! Win32-Abhaengigkeit und keinen versteckten Fensterzugriff bekommen.

use std::error::Error;
use std::fmt;
use std::sync::OnceLock;

/// Rechteck in physischen Bildschirmkoordinaten.
///
/// `x`/`y` sind vorzeichenbehaftet: ein Monitor links vom Hauptbildschirm hat
/// negative Koordinaten, und die Parkposition der Fenster liegt ohnehin bei
/// -32000.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[cfg(test)]
    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    #[cfg(test)]
    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Ueberlappen sich zwei Kacheln? Beruehrende Kanten zaehlen nicht.
    #[cfg(test)]
    pub fn overlaps(&self, other: &Rect) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// Liegt `self` vollstaendig in `outer`?
    #[cfg(test)]
    pub fn contained_in(&self, outer: &Rect) -> bool {
        self.x >= outer.x
            && self.y >= outer.y
            && self.right() <= outer.right()
            && self.bottom() <= outer.bottom()
    }
}

/// Abstand zwischen zwei Kacheln in Pixeln.
const GAP: u32 = 6;

/// Unterhalb dieser Kachelgroesse schalten die Chat-Oberflaechen der Brains in
/// Mobil-Layouts um und der Inhalt wird unlesbar. Die Kachelansicht verkleinert
/// dann lieber die Zahl der Spalten, statt weiter zu schrumpfen.
pub const MIN_TILE_WIDTH: u32 = 320;
pub const MIN_TILE_HEIGHT: u32 = 240;

/// Spalten/Zeilen fuer `n` Kacheln — moeglichst quadratisch, Rest in die
/// letzte Zeile.
pub fn grid_dimensions(n: usize) -> (usize, usize) {
    if n == 0 {
        return (0, 0);
    }
    let columns = (n as f64).sqrt().ceil() as usize;
    let rows = n.div_ceil(columns);
    (columns, rows)
}

/// Teilt `area` in `n` moeglichst gleich grosse Kacheln.
///
/// Die Restpixel der Ganzzahldivision werden auf die vorderen Spalten bzw.
/// Zeilen verteilt, statt sie am Rand liegen zu lassen — sonst klafft rechts
/// und unten je nach Aufloesung eine sichtbare Luecke.
pub fn grid_layout(area: Rect, n: usize) -> Vec<Rect> {
    if n == 0 || area.is_empty() {
        return Vec::new();
    }
    let (columns, rows) = grid_dimensions(n);

    // Zwischenraeume gehen von der verfuegbaren Flaeche ab, bevor geteilt wird.
    let gaps_x = GAP.saturating_mul(columns.saturating_sub(1) as u32);
    let gaps_y = GAP.saturating_mul(rows.saturating_sub(1) as u32);
    let usable_width = area.width.saturating_sub(gaps_x);
    let usable_height = area.height.saturating_sub(gaps_y);
    if usable_width == 0 || usable_height == 0 {
        return Vec::new();
    }

    let base_width = usable_width / columns as u32;
    let base_height = usable_height / rows as u32;
    let extra_width = usable_width % columns as u32;
    let extra_height = usable_height % rows as u32;

    // Spaltenbreiten und Zeilenhoehen vorab, damit die x/y-Positionen exakt
    // aufsummieren und keine Rundung zweimal wirkt.
    let column_widths: Vec<u32> = (0..columns)
        .map(|c| base_width + u32::from((c as u32) < extra_width))
        .collect();
    let row_heights: Vec<u32> = (0..rows)
        .map(|r| base_height + u32::from((r as u32) < extra_height))
        .collect();

    let mut tiles = Vec::with_capacity(n);
    for index in 0..n {
        let column = index % columns;
        let row = index / columns;
        let x = area.x
            + column_widths[..column]
                .iter()
                .map(|w| (*w + GAP) as i32)
                .sum::<i32>();
        let y = area.y
            + row_heights[..row]
                .iter()
                .map(|h| (*h + GAP) as i32)
                .sum::<i32>();
        tiles.push(Rect::new(x, y, column_widths[column], row_heights[row]));
    }
    tiles
}

/// Teilung des Arbeitsbereichs: Kachelwand oben, Terminal unten.
///
/// Storax' Wunsch (03.08.2026, Nachfolger): die Brain-Wall steht automatisch
/// auf dem oberen Teil des Bildschirms, die TUI dockt sich darunter an. Das
/// Terminal bekommt die unteren ~30 % der Hoehe, die Wall den Rest.
pub fn split_areas(work: Rect) -> (Rect, Rect) {
    let terminal_height = work.height * 3 / 10;
    let wall_height = work.height - terminal_height;
    let wall = Rect::new(work.x, work.y, work.width, wall_height);
    let terminal = Rect::new(
        work.x,
        work.y + wall_height as i32,
        work.width,
        terminal_height,
    );
    (terminal, wall)
}

/// Passt die Kachelzahl an, wenn `n` Kacheln zu klein wuerden.
///
/// Gibt zurueck, wie viele Kacheln gleichzeitig sinnvoll darstellbar sind. Der
/// Aufrufer entscheidet, was mit dem Rest passiert (blaettern, weglassen) —
/// aber er soll es bewusst entscheiden und nicht acht unleserliche Briefmarken
/// auf den Schirm legen.
pub fn fitting_tile_count(area: Rect, desired: usize) -> usize {
    for count in (1..=desired).rev() {
        let tiles = grid_layout(area, count);
        if tiles
            .iter()
            .all(|t| t.width >= MIN_TILE_WIDTH && t.height >= MIN_TILE_HEIGHT)
        {
            return count;
        }
    }
    0
}

/// Einzel-Brain-Rechteck: zentriert im Arbeitsbereich, sonst Fallback 1280x900.
///
/// Die reine Geometrie liegt hier; woher der Arbeitsbereich kommt, entscheidet
/// der `PanelSink` der Anwendung.
pub fn reveal_rect_for_work_area(work: Option<Rect>) -> Rect {
    if let Some(work) = work {
        let width = work.width.clamp(800, 1280);
        let height = work.height.clamp(600, 900);
        let x = work.x + ((work.width.saturating_sub(width)) / 2) as i32;
        let y = work.y + ((work.height.saturating_sub(height)) / 2) as i32;
        Rect::new(x, y, width, height)
    } else {
        Rect::new(80, 80, 1280, 900)
    }
}

/// Schreibgeschuetzte Desktop-Fakten, die die Transport-Schicht braucht.
///
/// Die Root-Anwendung implementiert dieses Trait; Transportmodule fragen nur
/// über [`panel_sink`] und bleiben damit frei von `brain_grid`.
pub trait PanelSink: Send + Sync {
    /// Nutzbare Flaeche des Hauptbildschirms (ohne Taskleiste).
    fn work_area(&self) -> Option<Rect>;

    /// Fensterhandle des Terminalfensters, in dem dieser Prozess laeuft.
    fn terminal_window_hwnd(&self) -> Option<isize>;
}

/// Fallback ohne Desktop-Zugriff (Bibliotheksnutzer und Tests).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoPanelSink;

impl PanelSink for NoPanelSink {
    fn work_area(&self) -> Option<Rect> {
        None
    }

    fn terminal_window_hwnd(&self) -> Option<isize> {
        None
    }
}

/// Ein Sink ist bereits installiert; der erste gewinnt.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AlreadyInstalled;

impl fmt::Display for AlreadyInstalled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ein PanelSink ist bereits installiert")
    }
}

impl Error for AlreadyInstalled {}

static ACTIVE: OnceLock<Box<dyn PanelSink>> = OnceLock::new();
static FALLBACK: NoPanelSink = NoPanelSink;

/// Installiert den Desktop-Sink der Anwendung. Nur einmal pro Prozess.
pub fn install_panel_sink(sink: Box<dyn PanelSink>) -> Result<(), AlreadyInstalled> {
    ACTIVE.set(sink).map_err(|_| AlreadyInstalled)
}

/// Aktiver Desktop-Sink oder der Fallback ohne Fensterzugriff.
pub fn panel_sink() -> &'static dyn PanelSink {
    match ACTIVE.get() {
        Some(sink) => sink.as_ref(),
        None => &FALLBACK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_hat_keine_desktop_fakten() {
        assert_eq!(panel_sink().work_area(), None);
        assert_eq!(panel_sink().terminal_window_hwnd(), None);
    }

    #[test]
    fn reveal_rect_zentriert_und_begrenzt_den_arbeitsbereich() {
        let work = Rect::new(0, 0, 1920, 1032);
        assert_eq!(
            reveal_rect_for_work_area(Some(work)),
            Rect::new(320, 66, 1280, 900)
        );
        let small = Rect::new(-1920, 10, 900, 500);
        assert_eq!(
            reveal_rect_for_work_area(Some(small)),
            Rect::new(-1920, 10, 900, 600)
        );
    }

    #[test]
    fn reveal_rect_ohne_arbeitsbereich_bleibt_beim_fallback() {
        assert_eq!(
            reveal_rect_for_work_area(None),
            Rect::new(80, 80, 1280, 900)
        );
    }
}
