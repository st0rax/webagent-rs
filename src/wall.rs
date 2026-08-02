//! Brain-Wall — Geometrie.
//!
//! Die Brain-Fenster existieren bereits: [`crate::webview_runtime`] oeffnet pro
//! Tab ein echtes Fenster und parkt es nur auf `OFFSCREEN_POS`, damit es
//! fokussierbar bleibt (ein `with_visible(false)`-Fenster bekommt keinen Fokus,
//! dann laeuft `press_enter` ins Leere). Die Wall holt diese Fenster in ein
//! Raster zurueck auf den Bildschirm.
//!
//! Dieses Modul rechnet ausschliesslich — es fasst kein Fenster an. Genau
//! deshalb ist es testbar: die Kachelaufteilung ist die Stelle, an der sich
//! Ueberlappungen, Rundungsfehler und leere Faelle einschleichen, und keine
//! davon braucht eine UI, um sichtbar zu werden.

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

    pub fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Ueberlappen sich zwei Kacheln? Beruehrende Kanten zaehlen nicht.
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
/// Mobil-Layouts um und der Inhalt wird unlesbar. Die Wall verkleinert dann
/// lieber die Zahl der Spalten, statt weiter zu schrumpfen.
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
pub fn wall_layout(area: Rect, n: usize) -> Vec<Rect> {
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

/// Wohin die Wall relativ zum Terminalfenster gehoert.
///
/// Storax' Wunsch ist „oberhalb der TUI angedockt". Reicht der Platz oberhalb
/// nicht, ist ein zu flacher Streifen schlechter als die Alternative: dann
/// bekommt die Wall den groesseren der beiden freien Bereiche.
pub fn dock_area(screen: Rect, terminal: Rect) -> Rect {
    let above_height = (terminal.y - screen.y).max(0) as u32;
    let below_height = (screen.bottom() - terminal.bottom()).max(0) as u32;

    if above_height >= MIN_TILE_HEIGHT || above_height >= below_height {
        Rect::new(screen.x, screen.y, screen.width, above_height)
    } else {
        Rect::new(screen.x, terminal.bottom(), screen.width, below_height)
    }
}

/// Passt die Kachelzahl an, wenn `n` Kacheln zu klein wuerden.
///
/// Gibt zurueck, wie viele Kacheln gleichzeitig sinnvoll darstellbar sind. Der
/// Aufrufer entscheidet, was mit dem Rest passiert (blaettern, weglassen) —
/// aber er soll es bewusst entscheiden und nicht acht unleserliche Briefmarken
/// auf den Schirm legen.
pub fn fitting_tile_count(area: Rect, desired: usize) -> usize {
    for count in (1..=desired).rev() {
        let tiles = wall_layout(area, count);
        if tiles
            .iter()
            .all(|t| t.width >= MIN_TILE_WIDTH && t.height >= MIN_TILE_HEIGHT)
        {
            return count;
        }
    }
    0
}

/// Nutzbare Flaeche des Hauptbildschirms (ohne Taskleiste).
///
/// Bewusst das Arbeitsbereichs-Rechteck und nicht die volle Aufloesung: eine
/// Kachel unter der Taskleiste ist eine Kachel, die man nicht sieht.
#[cfg(windows)]
pub fn primary_work_area() -> Option<Rect> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
    let mut rect = RECT::default();
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut rect as *mut RECT as *mut core::ffi::c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_err() {
        return None;
    }
    let width = (rect.right - rect.left).max(0) as u32;
    let height = (rect.bottom - rect.top).max(0) as u32;
    if width == 0 || height == 0 {
        return None;
    }
    Some(Rect::new(rect.left, rect.top, width, height))
}

#[cfg(not(windows))]
pub fn primary_work_area() -> Option<Rect> {
    None
}

/// Anteil der Bildschirmhoehe, den die Wall belegt, solange sie nicht an ein
/// konkretes Terminalfenster angedockt ist.
const WALL_HEIGHT_SHARE: u32 = 60;

/// Wall-Bereich ohne Terminal-Kopplung: oberer Teil des Arbeitsbereichs.
///
/// Erstes Inkrement bewusst ohne Fensterjagd: Windows Terminal liefert ueber
/// `GetConsoleWindow` nur ein verstecktes Pseudokonsolen-Fenster, dessen Rect
/// nichts mit dem sichtbaren Fenster zu tun hat. Lieber ein verlaesslicher
/// oberer Streifen als eine Andockung, die je nach Terminal falsch liegt.
pub fn default_wall_area() -> Option<Rect> {
    let work = primary_work_area()?;
    let height = work.height * WALL_HEIGHT_SHARE / 100;
    if height == 0 {
        return None;
    }
    Some(Rect::new(work.x, work.y, work.width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        width: 2560,
        height: 1440,
    };

    #[test]
    fn grid_ist_moeglichst_quadratisch() {
        assert_eq!(grid_dimensions(0), (0, 0));
        assert_eq!(grid_dimensions(1), (1, 1));
        assert_eq!(grid_dimensions(2), (2, 1));
        assert_eq!(grid_dimensions(4), (2, 2));
        assert_eq!(grid_dimensions(8), (3, 3), "8 Brains: 3x3 mit einer Luecke");
        assert_eq!(grid_dimensions(9), (3, 3));
    }

    #[test]
    fn kacheln_ueberlappen_nie_und_bleiben_im_bereich() {
        // Der eigentliche Zweck des Moduls. Ueber mehrere Kachelzahlen, weil
        // sich Rundungsfehler nur bei bestimmten Teilern zeigen.
        for n in 1..=12 {
            let tiles = wall_layout(SCREEN, n);
            assert_eq!(tiles.len(), n, "n={n}");
            for (i, a) in tiles.iter().enumerate() {
                assert!(!a.is_empty(), "n={n}, Kachel {i} ist leer");
                assert!(
                    a.contained_in(&SCREEN),
                    "n={n}, Kachel {i} {a:?} ragt aus dem Bereich"
                );
                for (j, b) in tiles.iter().enumerate().skip(i + 1) {
                    assert!(!a.overlaps(b), "n={n}: Kachel {i} {a:?} und {j} {b:?}");
                }
            }
        }
    }

    #[test]
    fn restpixel_werden_verteilt_statt_am_rand_zu_verfallen() {
        // 2560 / 3 geht nicht auf. Ohne Verteilung klafft rechts eine Luecke.
        let tiles = wall_layout(Rect::new(0, 0, 2560, 1440), 9);
        let rightmost = tiles.iter().map(|t| t.right()).max().unwrap();
        assert_eq!(rightmost, 2560, "rechter Rand muss ausgenutzt sein");
        let bottom = tiles.iter().map(|t| t.bottom()).max().unwrap();
        assert_eq!(bottom, 1440, "unterer Rand muss ausgenutzt sein");
    }

    #[test]
    fn negative_koordinaten_bleiben_erhalten() {
        // Zweiter Monitor links vom Hauptbildschirm.
        let left_screen = Rect::new(-1920, 0, 1920, 1080);
        let tiles = wall_layout(left_screen, 4);
        assert!(tiles.iter().all(|t| t.contained_in(&left_screen)));
        assert!(tiles.iter().any(|t| t.x < 0));
    }

    #[test]
    fn leere_faelle_liefern_nichts_statt_zu_panicken() {
        assert!(wall_layout(SCREEN, 0).is_empty());
        assert!(wall_layout(Rect::new(0, 0, 0, 1080), 4).is_empty());
        assert!(wall_layout(Rect::new(0, 0, 1920, 0), 4).is_empty());
        // Flaeche kleiner als die Zwischenraeume: kein Panic, keine Kachel.
        assert!(wall_layout(Rect::new(0, 0, 4, 4), 9).is_empty());
    }

    #[test]
    fn dock_area_liegt_oberhalb_des_terminals() {
        // Terminal im unteren Drittel -> Wall darueber.
        let terminal = Rect::new(0, 900, 2560, 540);
        let area = dock_area(SCREEN, terminal);
        assert_eq!(area, Rect::new(0, 0, 2560, 900));
        assert!(area.bottom() <= terminal.y, "Wall darf die TUI nicht ueberdecken");
    }

    #[test]
    fn dock_area_weicht_nach_unten_aus_wenn_oben_kein_platz_ist() {
        // Terminal klebt oben: oberhalb bleiben 40 px, das ist als Wall nutzlos.
        let terminal = Rect::new(0, 40, 2560, 700);
        let area = dock_area(SCREEN, terminal);
        assert_eq!(area.y, terminal.bottom(), "Wall weicht unter das Terminal");
        assert!(area.height > MIN_TILE_HEIGHT);
    }

    #[test]
    fn fitting_tile_count_verkleinert_statt_briefmarken_zu_bauen() {
        // Ein schmaler Streifen traegt keine acht Kacheln.
        let strip = Rect::new(0, 0, 700, 300);
        let fits = fitting_tile_count(strip, 8);
        assert!(fits < 8, "8 Kacheln passen hier nicht: {fits}");
        assert!(fits >= 1);
        for tile in wall_layout(strip, fits) {
            assert!(tile.width >= MIN_TILE_WIDTH && tile.height >= MIN_TILE_HEIGHT);
        }
        // Grosser Bereich: alle acht passen.
        assert_eq!(fitting_tile_count(SCREEN, 8), 8);
        // Gar kein Platz.
        assert_eq!(fitting_tile_count(Rect::new(0, 0, 100, 100), 8), 0);
    }
}
