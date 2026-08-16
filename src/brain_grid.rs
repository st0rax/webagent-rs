//! Brain-Kachelansicht — Geometrie.
//!
//! Die Brain-Fenster existieren bereits: [`crate::webview_runtime`] oeffnet pro
//! Tab ein echtes Fenster und parkt es nur auf `OFFSCREEN_POS`, damit es
//! sichtbar (gerendert) bleibt — ein `with_visible(false)`-Fenster laesst die
//! Seite nicht laufen, dann geht `press_enter` ins Leere. Die Kachelansicht holt
//! diese Fenster in ein Raster zurueck auf den Bildschirm.
//!
//! Sichtbar heisst hier ausdruecklich **nicht** aktivierbar: die Kacheln tragen
//! `WS_EX_NOACTIVATE` (siehe `webview_runtime::set_no_activate`), sonst wuerde
//! jedes absendende Brain dem Terminal mitten im Tippen den Fokus wegreissen.
//! Fokus bekommt eine Kachel nur auf ausdrueckliche Anforderung (Alt+Nummer),
//! Esc gibt ihn ans Terminalfenster zurueck.
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
/// Mobil-Layouts um und der Inhalt wird unlesbar. Die Kachelansicht verkleinert dann
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

/// Rechteck des Terminalfensters, in dem dieser Prozess laeuft.
///
/// **Nicht** ueber `GetConsoleWindow`: unter Windows Terminal liefert das nur
/// ein verstecktes Pseudokonsolen-Fenster, dessen Rechteck mit dem sichtbaren
/// Fenster nichts zu tun hat. Stattdessen die Elternkette hochlaufen, bis ein
/// Prozess mit sichtbarem Fenster kommt — gemessen am 02.08.2026:
/// `webagent.exe -> pwsh.exe -> WindowsTerminal.exe`, und erst der letzte hat
/// ein Fenster.
#[cfg(all(windows, feature = "webview"))]
pub fn terminal_window_rect() -> Option<Rect> {
    terminal_window_handle().map(|(_, rect)| rect)
}

#[cfg(not(all(windows, feature = "webview")))]
pub fn terminal_window_rect() -> Option<Rect> {
    None
}

/// Fensterhandle desselben Terminalfensters, dessen Rechteck
/// [`terminal_window_rect`] liefert.
///
/// Bewusst aus derselben Suche gespeist statt als zweite Elternketten-Jagd:
/// zwei Suchen koennten auseinanderlaufen, und dann ginge der Fokus an ein
/// anderes Fenster, als die Kacheln umfliessen. Genau das ist beim Merge
/// beinahe passiert — beide Zweige hatten die Suche unabhaengig gebaut.
#[cfg(all(windows, feature = "webview"))]
pub fn terminal_window_hwnd() -> Option<isize> {
    terminal_window_handle().map(|(hwnd, _rect)| hwnd.0 as isize)
}

#[cfg(not(all(windows, feature = "webview")))]
pub fn terminal_window_hwnd() -> Option<isize> {
    None
}

/// Fenster (HWND + Rechteck) des Terminalfensters, in dem dieser Prozess laeuft.
///
/// Der Weg ueber die Elternkette ist der, den auch [`terminal_window_rect`]
/// geht (`webagent.exe -> pwsh.exe -> WindowsTerminal.exe`). Hier kommt aber
/// die HWND mit zurueck — die braucht die Kachelansicht, um das Terminalfenster
/// selbst zu docken, und der Fokusweg, um es wieder nach vorn zu holen.
#[cfg(all(windows, feature = "webview"))]
fn terminal_window_handle() -> Option<(windows::Win32::Foundation::HWND, Rect)> {
    let mut pid = std::process::id();
    for _ in 0..6 {
        if let Some(hit) = visible_terminal_window_of(pid) {
            return Some(hit);
        }
        pid = parent_pid(pid)?;
    }
    None
}

/// Sichtbares Terminalfenster des Prozesses `pid`.
///
/// Beim EIGENEN Prozess zaehlen die Brain-WebView-Fenster (gleicher Prozess,
/// sichtbar, gross) nicht als Terminal: die Kachelansicht wuerde sonst ein
/// Brain-Fenster unten andocken, statt des Terminals. Nur das echte
/// Konsolenfenster gilt hier — und das ist unter Windows Terminal ein
/// verstecktes Pseudokonsolen-Fenster, faellt also selbst durch und die Suche
/// wandert zum Parent weiter. Unter einer echten Konsole ist es sichtbar und
/// gross genug und die Wall funktioniert dort genauso.
#[cfg(all(windows, feature = "webview"))]
fn visible_terminal_window_of(
    pid: u32,
) -> Option<(windows::Win32::Foundation::HWND, Rect)> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Console::GetConsoleWindow;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindowVisible};

    if pid == std::process::id() {
        let console = unsafe { GetConsoleWindow() };
        if console == HWND(std::ptr::null_mut())
            || !unsafe { IsWindowVisible(console) }.as_bool()
        {
            return None;
        }
        let mut rect = windows::Win32::Foundation::RECT::default();
        if unsafe { GetWindowRect(console, &mut rect) }.is_err() {
            return None;
        }
        let width = (rect.right - rect.left).max(0) as u32;
        let height = (rect.bottom - rect.top).max(0) as u32;
        if width < 200 || height < 100 {
            return None;
        }
        return Some((
            console,
            Rect::new(rect.left, rect.top, width, height),
        ));
    }
    visible_window_of(pid)
}

/// Uebersetzt eine Nummerntaste (Alt+1 … Alt+9) in einen Kachelindex.
///
/// Rein rechnend und damit testbar — die Tastenbelegung ist genau die Stelle,
/// an der sich ein Off-by-one einschleicht: der Nutzer zaehlt ab 1, die
/// Kachelliste ab 0. `Alt+0` ist bewusst kein Brain: eine zehnte Kachel passt
/// ohnehin nie auf den Schirm (siehe [`fitting_tile_count`]).
pub fn brain_index_for_digit(c: char) -> Option<usize> {
    match c {
        '1'..='9' => Some(c as usize - '1' as usize),
        _ => None,
    }
}

#[cfg(all(windows, feature = "webview"))]
fn parent_pid(pid: u32) -> Option<u32> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = None;
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                if entry.th32ProcessID == pid {
                    found = Some(entry.th32ParentProcessID);
                    break;
                }
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snapshot);
        found.filter(|p| *p != 0 && *p != pid)
    }
}

#[cfg(all(windows, feature = "webview"))]
struct WindowSearch {
    pid: u32,
    result: Option<(windows::Win32::Foundation::HWND, Rect)>,
}

#[cfg(all(windows, feature = "webview"))]
fn visible_window_of(
    pid: u32,
) -> Option<(windows::Win32::Foundation::HWND, Rect)> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    };

    unsafe extern "system" fn callback(window: HWND, param: LPARAM) -> BOOL {
        let search = &mut *(param.0 as *mut WindowSearch);
        let mut owner = 0u32;
        GetWindowThreadProcessId(window, Some(&mut owner));
        if owner != search.pid || !IsWindowVisible(window).as_bool() {
            return BOOL(1);
        }
        let mut rect = RECT::default();
        if GetWindowRect(window, &mut rect).is_err() {
            return BOOL(1);
        }
        let width = (rect.right - rect.left).max(0) as u32;
        let height = (rect.bottom - rect.top).max(0) as u32;
        // Werkzeug- und Nachrichtenfenster aussortieren: nur etwas, das als
        // Terminalfenster durchgeht, ist hier gemeint.
        if width < 200 || height < 100 {
            return BOOL(1);
        }
        search.result = Some((
            window,
            Rect::new(rect.left, rect.top, width, height),
        ));
        BOOL(0)
    }

    let mut search = WindowSearch { pid, result: None };
    unsafe {
        let _ = EnumWindows(
            Some(callback),
            LPARAM(&mut search as *mut WindowSearch as isize),
        );
    }
    search.result
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

/// Nutzbare Flaeche des Hauptbildschirms (ohne Taskleiste).
///
/// Bewusst das Arbeitsbereichs-Rechteck und nicht die volle Aufloesung: eine
/// Kachel unter der Taskleiste ist eine Kachel, die man nicht sieht.
#[cfg(all(windows, feature = "webview"))]
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

#[cfg(not(all(windows, feature = "webview")))]
pub fn primary_work_area() -> Option<Rect> {
    None
}

/// Obere Bildschirmflaeche: der Bereich, in dem die Brain-Kacheln liegen.
///
/// Die TUI dockt sich selbst darunter an (siehe [`dock_terminal_bottom`]) und
/// die Kachelwand bekommt den Rest — das ist Storax' Wunsch vom 03.08.2026
/// („Wall oben, Terminal darunter").
pub fn wall_area() -> Option<Rect> {
    let work = primary_work_area()?;
    Some(split_areas(work).1)
}

/// Dockt das Terminalfenster dieses Prozesses unterhalb der Kachelwand.
///
/// Idempotent: ist das Terminal bereits angedockt (Snapshot vorhanden), passiert
/// nichts — die Auto-Wall ruft das bei jedem Nachziehen neuer Brains erneut,
/// ohne das Terminal jedes Mal neu zu platzieren.
///
/// Merkt sich das alte Rechteck (inkl. Maximier-Status), damit
/// [`restore_terminal`] den Zustand beim Ausschalten der Kachelansicht wieder
/// herstellt.
#[cfg(all(windows, feature = "webview"))]
pub fn dock_terminal_bottom() -> Result<(), String> {
    if TERMINAL_SNAPSHOT.lock().unwrap().is_some() {
        return Ok(());
    }
    use windows::Win32::UI::WindowsAndMessaging::{
        IsZoomed, SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOZORDER, SW_RESTORE,
    };
    let Some((hwnd, old_rect)) = terminal_window_handle() else {
        // Im --force-tui-Modus gibt es kein Konsolenfenster (opencode,
        // Start-Process). Kacheln laufen dann auf dem gesamten Bildschirm
        // statt nur auf der unteren Haelfte — das ist akzeptabel.
        if is_force_tui() {
            return Ok(());
        }
        return Err("Terminalfenster nicht gefunden".into());
    };
    let maximized = unsafe { IsZoomed(hwnd).as_bool() };
    let work = primary_work_area().ok_or("Arbeitsbereich nicht ermittelbar")?;
    let (terminal_area, _) = split_areas(work);
    // Maximierte Fenster ignorieren Groessenangaben von SetWindowPos — erst auf
    // „normal" zuruecksetzen, dann andocken.
    if maximized {
        let _ = unsafe { ShowWindow(hwnd, SW_RESTORE) };
    }
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOP,
            terminal_area.x,
            terminal_area.y,
            terminal_area.width as i32,
            terminal_area.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
        .map_err(|_| "SetWindowPos fehlgeschlagen".to_string())?;
    }
    *TERMINAL_SNAPSHOT.lock().unwrap() = Some(TerminalSnapshot {
        rect: old_rect,
        maximized,
    });
    Ok(())
}

#[cfg(not(all(windows, feature = "webview")))]
pub fn dock_terminal_bottom() -> Result<(), String> {
    Err("ohne webview-Feature nicht verfuegbar".into())
}

/// Stellt das Terminalfenster nach dem Ausschalten der Kachelansicht wieder her.
#[cfg(all(windows, feature = "webview"))]
pub fn restore_terminal() -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_TOP, SWP_NOACTIVATE, SWP_NOZORDER, SW_MAXIMIZE,
    };
    let snapshot = TERMINAL_SNAPSHOT.lock().unwrap().take();
    let Some(snapshot) = snapshot else {
        return Ok(());
    };
    let Some((hwnd, _)) = terminal_window_handle() else {
        return Err("Terminalfenster nicht gefunden".into());
    };
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOP,
            snapshot.rect.x,
            snapshot.rect.y,
            snapshot.rect.width as i32,
            snapshot.rect.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
        .map_err(|_| "SetWindowPos fehlgeschlagen".to_string())?;
        if snapshot.maximized {
            let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        }
    }
    Ok(())
}

#[cfg(not(all(windows, feature = "webview")))]
pub fn restore_terminal() -> Result<(), String> {
    Ok(())
}

#[cfg(all(windows, feature = "webview"))]
struct TerminalSnapshot {
    rect: Rect,
    maximized: bool,
}

#[cfg(all(windows, feature = "webview"))]
static TERMINAL_SNAPSHOT: std::sync::Mutex<Option<TerminalSnapshot>> =
    std::sync::Mutex::new(None);

/// Wenn `true`, wird `dock_terminal_bottom` graceful behandelt: kein
/// Konsolenfenster heisst Skip statt Error. Noetig fuer `--force-tui`-Modus,
/// bei dem die TUI aus einem Kontext gestartet wird, in dem kein echtes
/// Konsolenfenster existiert (z.B. opencode, Start-Process).
static FORCE_TUI: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Schaltet die Force-TUI-Flag. Muss einmalig vor dem ersten
/// `dock_terminal_bottom`-Aufruf gesetzt werden.
pub fn set_force_tui(enabled: bool) {
    FORCE_TUI.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

/// Wenn `true`, wird `dock_terminal_bottom` graceful behandelt.
pub fn is_force_tui() -> bool {
    FORCE_TUI.load(std::sync::atomic::Ordering::Relaxed)
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
            let tiles = grid_layout(SCREEN, n);
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
        let tiles = grid_layout(Rect::new(0, 0, 2560, 1440), 9);
        let rightmost = tiles.iter().map(|t| t.right()).max().unwrap();
        assert_eq!(rightmost, 2560, "rechter Rand muss ausgenutzt sein");
        let bottom = tiles.iter().map(|t| t.bottom()).max().unwrap();
        assert_eq!(bottom, 1440, "unterer Rand muss ausgenutzt sein");
    }

    #[test]
    fn negative_koordinaten_bleiben_erhalten() {
        // Zweiter Monitor links vom Hauptbildschirm.
        let left_screen = Rect::new(-1920, 0, 1920, 1080);
        let tiles = grid_layout(left_screen, 4);
        assert!(tiles.iter().all(|t| t.contained_in(&left_screen)));
        assert!(tiles.iter().any(|t| t.x < 0));
    }

    #[test]
    fn leere_faelle_liefern_nichts_statt_zu_panicken() {
        assert!(grid_layout(SCREEN, 0).is_empty());
        assert!(grid_layout(Rect::new(0, 0, 0, 1080), 4).is_empty());
        assert!(grid_layout(Rect::new(0, 0, 1920, 0), 4).is_empty());
        // Flaeche kleiner als die Zwischenraeume: kein Panic, keine Kachel.
        assert!(grid_layout(Rect::new(0, 0, 4, 4), 9).is_empty());
    }

    #[test]
    fn split_areas_teilt_in_wall_oben_und_terminal_unten() {
        let work = Rect::new(0, 0, 1920, 1032);
        let (terminal, wall) = split_areas(work);
        // Wall: obere 70 %, volle Breite.
        assert_eq!(wall, Rect::new(0, 0, 1920, 723));
        // Terminal: untere 30 %, beginnt dort, wo die Wall aufhoert.
        assert_eq!(terminal, Rect::new(0, 723, 1920, 309));
        assert!(terminal.contained_in(&work));
        assert!(wall.contained_in(&work));
        assert!(!terminal.overlaps(&wall));
        assert_eq!(wall.bottom(), terminal.y);
    }

    #[test]
    fn split_areas_mit_ungerader_hoehe() {
        // 1033 / 10 * 3 = 309, Rest 1 wandert in die Wall.
        let work = Rect::new(0, 0, 1920, 1033);
        let (terminal, wall) = split_areas(work);
        assert_eq!(terminal.height, 309);
        assert_eq!(wall.height, 724);
        assert_eq!(wall.bottom(), terminal.y);
    }

    #[test]
    fn split_areas_auf_monitor_mit_negativem_ursprung() {
        // Zweiter Monitor links: Ursprung bei -1920, aber dieselbe Teilung.
        let work = Rect::new(-1920, 0, 1920, 1080);
        let (terminal, wall) = split_areas(work);
        assert_eq!(wall.x, -1920);
        assert_eq!(terminal.x, -1920);
        assert_eq!(terminal.y, 756);
        assert!(wall.contained_in(&work));
        assert!(terminal.contained_in(&work));
    }

    #[test]
    fn alt_nummer_zaehlt_ab_eins_fuer_den_nutzer_und_ab_null_fuer_die_liste() {
        assert_eq!(brain_index_for_digit('1'), Some(0));
        assert_eq!(brain_index_for_digit('9'), Some(8));
        // Alt+0 waere die zehnte Kachel — die passt nie auf den Schirm.
        assert_eq!(brain_index_for_digit('0'), None);
        assert_eq!(brain_index_for_digit('w'), None);
    }

    #[test]
    fn fitting_tile_count_verkleinert_statt_briefmarken_zu_bauen() {
        // Ein schmaler Streifen traegt keine acht Kacheln.
        let strip = Rect::new(0, 0, 700, 300);
        let fits = fitting_tile_count(strip, 8);
        assert!(fits < 8, "8 Kacheln passen hier nicht: {fits}");
        assert!(fits >= 1);
        for tile in grid_layout(strip, fits) {
            assert!(tile.width >= MIN_TILE_WIDTH && tile.height >= MIN_TILE_HEIGHT);
        }
        // Grosser Bereich: alle acht passen.
        assert_eq!(fitting_tile_count(SCREEN, 8), 8);
        // Gar kein Platz.
        assert_eq!(fitting_tile_count(Rect::new(0, 0, 100, 100), 8), 0);
    }
}
