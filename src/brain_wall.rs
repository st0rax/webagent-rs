//! Brain-Wall: sichtbare Brain-Fenster finden und als Raster legen.
//!
//! Ein Fenster zaehlt, wenn sein Titel `webagent · <brain>` ist und seine PID
//! im Prozessbaum der TUI liegt (TUI selbst oder ein Kind). Ob der Tab im
//! TUI-Prozess oder in einem Worker lebt, ist fuer das Raster egal.
//! Geometrie: [`crate::brain_grid`] (oben 70 % Wall, unten 30 % Terminal).
//!
//! Sichtbare Kacheln liegen `HWND_TOPMOST` ohne Fokus. `SWP_NOZORDER` wuerde
//! `HWND_TOP` ignorieren — dann bleibt das Raster hinter einem Vollbild
//! (gemessen 17.08.2026). Minimierte Kacheln haben dieselbe HWND-Liste;
//! [`WallState::needs_relayout`] legt sie trotzdem neu.
//!
//! Titelparsing, PID-Filter und Wall-Zustand sind ohne Win32 testbar.

use std::collections::HashSet;

use crate::brain_grid::{self, Rect};

/// Off-Screen-Parkplatz — dieselbe Signatur wie `webview_runtime::OFFSCREEN_POS`.
pub const PARK_X: i32 = -32000;
pub const PARK_Y: i32 = -32000;

/// Erkannter Fenstertitel eines WebAgent-Browsers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTitle {
    pub name: String,
    pub view_id: Option<u64>,
}

/// Rohfenster aus der Enumeration (testbar ohne Win32).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowHint {
    pub hwnd: isize,
    pub pid: u32,
    pub title: String,
}

/// Ein dem TUI-Baum zugeordnetes Brain-Fenster, sortiert nach Name dann HWND.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedBrainWindow {
    pub hwnd: isize,
    pub pid: u32,
    pub name: String,
    pub view_id: Option<u64>,
}

/// Stabile Identitaet der aktuell entdeckten Fenster. Eine blosse Anzahl reicht
/// nicht: stirbt ein Worker und sein Ersatz oeffnet bei gleicher Gesamtzahl ein
/// neues Fenster, muss die Wall trotzdem neu angeordnet werden.
pub type WindowSignature = Vec<(u32, isize, Option<u64>)>;

/// Ein/Aus und zuletzt erfolgreich angeordnete Fenster.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WallState {
    pub on: bool,
    arranged_for: WindowSignature,
}

impl WallState {
    pub fn start_on() -> Self {
        Self {
            on: true,
            arranged_for: Vec::new(),
        }
    }

    /// `w`: Umschalten. `arranged_for` zurücksetzen, damit der nächste Tick
    /// den neuen Zustand wirklich anwendet.
    pub fn toggle(&mut self) {
        self.on = !self.on;
        self.arranged_for.clear();
    }

    pub fn needs_arrange(&self, discovered: &WindowSignature) -> bool {
        self.on && !discovered.is_empty() && discovered != &self.arranged_for
    }

    /// Neu legen, wenn die HWND-Liste sich aendert ODER eine Kachel als
    /// Symbol liegt. Minimize aendert die Signatur nicht — ohne das bleiben
    /// die Kacheln nach Win+D / Minimieren weg, waehrend die Wall
    /// „bereits angeordnet" glaubt.
    pub fn needs_relayout(&self, discovered: &WindowSignature, any_iconic: bool) -> bool {
        self.needs_arrange(discovered) || (self.on && !discovered.is_empty() && any_iconic)
    }

    pub fn mark_arranged(&mut self, discovered: WindowSignature) {
        self.arranged_for = discovered;
    }

    pub fn mark_parked(&mut self) {
        self.arranged_for.clear();
    }
}

/// Titel wie `webagent · claude (3)` oder `webagent-0`.
pub fn parse_window_title(title: &str) -> Option<ParsedTitle> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let rest = title
        .strip_prefix("webagent · ")
        .or_else(|| title.strip_prefix("webagent ·"))
        .or_else(|| title.strip_prefix("webagent-"));
    let rest = rest?;
    if let Some((name, id)) = rest.rsplit_once(" (") {
        let id = id.strip_suffix(')')?;
        let view_id = id.parse().ok();
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        return Some(ParsedTitle {
            name: name.to_string(),
            view_id,
        });
    }
    let name = rest.trim();
    if name.is_empty() {
        return None;
    }
    Some(ParsedTitle {
        name: name.to_string(),
        view_id: None,
    })
}

/// PIDs im Prozessbaum unter `root` (inklusive `root`).
///
/// `edges` sind `(pid, parent_pid)` — dieselbe Form wie ToolHelp.
pub fn descendant_pids(root: u32, edges: &[(u32, u32)]) -> HashSet<u32> {
    let mut children: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for &(pid, parent) in edges {
        if pid == 0 || pid == parent {
            continue;
        }
        children.entry(parent).or_default().push(pid);
    }
    let mut out = HashSet::new();
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        if !out.insert(pid) {
            continue;
        }
        if let Some(kids) = children.get(&pid) {
            stack.extend(kids.iter().copied());
        }
    }
    out
}

/// Fenster, deren PID im Besitzbaum liegt und deren Titel ein Brain ist.
pub fn select_owned_brains(
    windows: &[WindowHint],
    owner_pids: &HashSet<u32>,
) -> Vec<OwnedBrainWindow> {
    let mut out = Vec::new();
    for w in windows {
        if !owner_pids.contains(&w.pid) {
            continue;
        }
        let Some(parsed) = parse_window_title(&w.title) else {
            continue;
        };
        out.push(OwnedBrainWindow {
            hwnd: w.hwnd,
            pid: w.pid,
            name: parsed.name,
            view_id: parsed.view_id,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.hwnd.cmp(&b.hwnd)));
    out
}

/// Kacheln für `n` Fenster in `area`; überzählige bleiben ohne Rechteck (parken).
pub fn layout_for_count(area: Rect, n: usize) -> Vec<Option<Rect>> {
    let fitting = brain_grid::fitting_tile_count(area, n);
    let tiles = brain_grid::grid_layout(area, fitting);
    (0..n).map(|i| tiles.get(i).copied()).collect()
}

/// Anzahl sichtbarer Brain-Fenster der von diesem Prozess gestarteten Worker.
pub fn discover_owned_count() -> usize {
    discover_owned().len()
}

pub fn window_signature(windows: &[OwnedBrainWindow]) -> WindowSignature {
    windows
        .iter()
        .map(|window| (window.pid, window.hwnd, window.view_id))
        .collect()
}

/// `true`, wenn mindestens eine entdeckte Kachel als Symbol liegt.
pub fn any_iconic(windows: &[OwnedBrainWindow]) -> bool {
    #[cfg(all(windows, feature = "webview"))]
    {
        windows.iter().any(|window| hwnd_is_iconic(window.hwnd))
    }
    #[cfg(not(all(windows, feature = "webview")))]
    {
        let _ = windows;
        false
    }
}

pub fn discover_owned() -> Vec<OwnedBrainWindow> {
    #[cfg(all(windows, feature = "webview"))]
    {
        let edges = process_edges();
        let owners = descendant_pids(std::process::id(), &edges);
        let hints = enumerate_windows();
        select_owned_brains(&hints, &owners)
    }
    #[cfg(not(all(windows, feature = "webview")))]
    {
        Vec::new()
    }
}

/// Wall an: Terminal unten andocken, Worker-Fenster kacheln.
pub fn apply_wall(on: bool) -> String {
    apply_wall_checked(on).unwrap_or_else(|error| error)
}

/// Wie [`apply_wall`], aber mit belastbarem Erfolgssignal fuer den
/// Auto-Wall-Zustandsautomaten. Ein Fehler darf nicht als angeordnet quittiert
/// werden, sonst gibt es bei unveraenderter Fensterliste keinen Retry.
pub fn apply_wall_checked(on: bool) -> Result<String, String> {
    #[cfg(not(all(windows, feature = "webview")))]
    {
        let _ = on;
        return Err("Kacheln: ohne webview-Feature nicht verfuegbar".to_string());
    }
    #[cfg(all(windows, feature = "webview"))]
    {
        let windows = discover_owned();
        let open = windows.len();
        if !on {
            let parked = park_windows(&windows);
            let restore = brain_grid::restore_terminal();
            return match (parked, restore) {
                (Ok(_), Ok(())) => Ok(format!(
                    "Kacheln aus — {open} Fenster wieder geparkt, Terminal zurueck"
                )),
                (Ok(_), Err(e)) => Err(format!(
                    "Kacheln aus — {open} Fenster geparkt, Terminal-Reset fehlgeschlagen: {e}"
                )),
                (Err(e), _) => Err(format!("Kacheln aus fehlgeschlagen: {e}")),
            };
        }
        if open == 0 {
            return Err("Kacheln: kein Brain-Fenster offen".to_string());
        }
        if let Err(e) = brain_grid::dock_terminal_bottom() {
            return Err(format!(
                "Kacheln: Terminal unten andocken fehlgeschlagen: {e}"
            ));
        }
        let Some(area) = brain_grid::wall_area() else {
            return Err("Kacheln: Bildschirmflaeche nicht ermittelbar".to_string());
        };
        match tile_windows(&windows, area) {
            Ok(tiled) if tiled < open => Ok(format!(
                "Kacheln an — {tiled} von {open} Fenstern gekachelt, {} zu klein und geparkt \
                 · Alt+1…9 Fokus, Esc zurueck",
                open - tiled
            )),
            Ok(tiled) => Ok(format!(
                "Kacheln an — {tiled} Fenster gekachelt · Alt+1…9 Fokus, Esc zurueck"
            )),
            Err(e) => Err(format!("Kacheln fehlgeschlagen: {e}")),
        }
    }
}

/// Parkt die Kacheln, ohne das Terminal zu verschieben.
///
/// Fuer TUI-Minimize: `apply_wall_checked(false)` wuerde
/// [`brain_grid::restore_terminal`] aufrufen und das minimierte Terminal
/// wieder aufklappen.
pub fn park_owned() -> Result<usize, String> {
    #[cfg(not(all(windows, feature = "webview")))]
    {
        return Err("Kacheln: ohne webview-Feature nicht verfuegbar".to_string());
    }
    #[cfg(all(windows, feature = "webview"))]
    {
        park_windows(&discover_owned())
    }
}

/// Stellt beim Verlassen der TUI auch auf Fehler-/Unwind-Pfaden den Desktop
/// wieder her. Der explizite Cleanup darf vorher laufen; ein zweiter Aufruf ist
/// durch `restore_terminal` und eine leere Discovery harmlos.
pub struct WallCleanupGuard;

impl Drop for WallCleanupGuard {
    fn drop(&mut self) {
        let _ = apply_wall_checked(false);
        let _ = brain_grid::restore_terminal();
    }
}

pub fn focus_tile(index: usize) -> String {
    #[cfg(not(all(windows, feature = "webview")))]
    {
        let _ = index;
        return "Fokus: ohne webview-Feature nicht verfuegbar".to_string();
    }
    #[cfg(all(windows, feature = "webview"))]
    {
        let windows = discover_owned();
        let Some(win) = windows.get(index) else {
            return format!("Fokus fehlgeschlagen: keine Kachel {}", index + 1);
        };
        match set_tile_focus(win.hwnd, true) {
            Ok(()) => format!("Fokus auf {} — Esc zurueck ins Terminal", win.name),
            Err(e) => format!("Fokus fehlgeschlagen: {e}"),
        }
    }
}

pub fn release_focus() -> String {
    #[cfg(not(all(windows, feature = "webview")))]
    {
        return "Fokus: ohne webview-Feature nicht verfuegbar".to_string();
    }
    #[cfg(all(windows, feature = "webview"))]
    {
        let windows = discover_owned();
        if windows.is_empty() {
            return "Fokus: kein Brain-Fenster offen".to_string();
        }
        let mut n = 0usize;
        for win in &windows {
            if let Err(e) = set_tile_focus(win.hwnd, false) {
                return format!("Fokusrueckgabe fehlgeschlagen: {e}");
            }
            n += 1;
        }
        format!("Fokus zurueck im Terminal — {n} Kacheln wieder passiv")
    }
}

#[cfg(all(windows, feature = "webview"))]
fn process_edges() -> Vec<(u32, u32)> {
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut edges = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return edges;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                edges.push((entry.th32ProcessID, entry.th32ParentProcessID));
                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = windows::Win32::Foundation::CloseHandle(snapshot);
    }
    edges
}

#[cfg(all(windows, feature = "webview"))]
fn enumerate_windows() -> Vec<WindowHint> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    };

    struct Acc(Vec<WindowHint>);

    unsafe extern "system" fn callback(window: HWND, param: LPARAM) -> BOOL {
        let acc = &mut *(param.0 as *mut Acc);
        let mut pid = 0u32;
        GetWindowThreadProcessId(window, Some(&mut pid));
        let len = GetWindowTextLengthW(window);
        if len <= 0 {
            return BOOL(1);
        }
        let mut buf = vec![0u16; len as usize + 1];
        let written = GetWindowTextW(window, &mut buf);
        if written <= 0 {
            return BOOL(1);
        }
        buf.truncate(written as usize);
        let title = String::from_utf16_lossy(&buf);
        acc.0.push(WindowHint {
            hwnd: window.0 as isize,
            pid,
            title,
        });
        BOOL(1)
    }

    let mut acc = Acc(Vec::new());
    unsafe {
        let _ = EnumWindows(Some(callback), LPARAM(&mut acc as *mut Acc as isize));
    }
    acc.0
}

#[cfg(all(windows, feature = "webview"))]
fn tile_windows(windows: &[OwnedBrainWindow], area: Rect) -> Result<usize, String> {
    let layouts = layout_for_count(area, windows.len());
    let mut tiled = 0usize;
    for (win, bounds) in windows.iter().zip(layouts.iter()) {
        match bounds {
            Some(rect) => {
                place_window(win.hwnd, *rect, true)?;
                tiled += 1;
            }
            None => place_parked(win.hwnd)?,
        }
    }
    Ok(tiled)
}

#[cfg(all(windows, feature = "webview"))]
fn park_windows(windows: &[OwnedBrainWindow]) -> Result<usize, String> {
    for win in windows {
        place_parked(win.hwnd)?;
    }
    Ok(windows.len())
}

#[cfg(all(windows, feature = "webview"))]
fn hwnd_is_iconic(hwnd: isize) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::IsIconic;
    unsafe { IsIconic(HWND(hwnd as *mut core::ffi::c_void)).as_bool() }
}

#[cfg(all(windows, feature = "webview"))]
fn place_window(hwnd: isize, rect: Rect, on_screen: bool) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, ShowWindow, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE,
        SWP_NOZORDER, SW_RESTORE, SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        // Kacheln bleiben nicht aktivierbar — Enter gehört dem Terminal.
        set_ex_style(hwnd, WS_EX_NOACTIVATE, true);
        set_ex_style(hwnd, WS_EX_TOOLWINDOW, !on_screen);
        // Sichtbar: vor andere Apps, ohne Fokus zu stehlen. HWND_TOP allein
        // verliert gegen ein aktives Vollbild; SWP_NOZORDER ignoriert die
        // Einfuegeposition ganz (17.08.2026: Raster bei y=0, OCR las die
        // Session darueber). Parken nimmt TOPMOST wieder weg.
        let (insert, flags) = if on_screen {
            (HWND_TOPMOST, SWP_NOACTIVATE)
        } else {
            (HWND_NOTOPMOST, SWP_NOACTIVATE | SWP_NOZORDER)
        };
        SetWindowPos(
            hwnd,
            insert,
            rect.x,
            rect.y,
            rect.width as i32,
            rect.height as i32,
            flags,
        )
        .map_err(|_| "SetWindowPos fehlgeschlagen".to_string())?;
    }
    Ok(())
}

#[cfg(all(windows, feature = "webview"))]
fn place_parked(hwnd: isize) -> Result<(), String> {
    place_window(hwnd, Rect::new(PARK_X, PARK_Y, 1280, 900), false)
}

#[cfg(all(windows, feature = "webview"))]
fn set_ex_style(
    hwnd: windows::Win32::Foundation::HWND,
    bit: windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE,
    on: bool,
) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE,
    };
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let mask = bit.0 as isize;
        let updated = if on { current | mask } else { current & !mask };
        if updated != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        }
    }
}

#[cfg(all(windows, feature = "webview"))]
fn set_tile_focus(hwnd: isize, focus: bool) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_SHOW, WS_EX_NOACTIVATE,
    };
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    if focus {
        set_ex_style(hwnd, WS_EX_NOACTIVATE, false);
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    } else {
        set_ex_style(hwnd, WS_EX_NOACTIVATE, true);
        if let Some(term) = brain_grid::terminal_window_hwnd() {
            unsafe {
                let _ = SetForegroundWindow(HWND(term as *mut core::ffi::c_void));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titel_webagent_punkt_name_id() {
        let p = parse_window_title("webagent · claude (3)").expect("titel");
        assert_eq!(p.name, "claude");
        assert_eq!(p.view_id, Some(3));
    }

    #[test]
    fn titel_webagent_bindestrich() {
        let p = parse_window_title("webagent-kimi").expect("titel");
        assert_eq!(p.name, "kimi");
        assert_eq!(p.view_id, None);
    }

    #[test]
    fn fremde_titel_werden_verworfen() {
        assert!(parse_window_title("Windows Terminal").is_none());
        assert!(parse_window_title("").is_none());
        assert!(parse_window_title("chrome").is_none());
    }

    #[test]
    fn descendant_pids_folgt_nur_dem_eigenen_baum() {
        // tui=10 -> worker=20,21; fremder=99 unter 1
        let edges = [(20, 10), (21, 10), (30, 20), (99, 1), (10, 1)];
        let owned = descendant_pids(10, &edges);
        assert!(owned.contains(&10));
        assert!(owned.contains(&20));
        assert!(owned.contains(&21));
        assert!(owned.contains(&30));
        assert!(!owned.contains(&99));
    }

    #[test]
    fn select_owned_filtert_pid_und_titel_und_sortiert() {
        let owners: HashSet<u32> = [10, 20, 21].into_iter().collect();
        let windows = vec![
            WindowHint {
                hwnd: 2,
                pid: 21,
                title: "webagent · zai (0)".into(),
            },
            WindowHint {
                hwnd: 1,
                pid: 20,
                title: "webagent · claude (1)".into(),
            },
            WindowHint {
                hwnd: 9,
                pid: 99,
                title: "webagent · fremd (0)".into(),
            },
            WindowHint {
                hwnd: 3,
                pid: 20,
                title: "Notepad".into(),
            },
        ];
        let got = select_owned_brains(&windows, &owners);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].name, "claude");
        assert_eq!(got[1].name, "zai");
    }

    #[test]
    fn layout_folgt_der_anzahl_und_parkt_den_rest() {
        let area = Rect::new(0, 0, 700, 500);
        // Zwei Mindestkacheln 320x240 passen, drei nicht in 700x500 mit Lücken.
        let layout = layout_for_count(area, 3);
        assert_eq!(layout.len(), 3);
        let placed = layout.iter().filter(|t| t.is_some()).count();
        let parked = layout.iter().filter(|t| t.is_none()).count();
        assert!(placed >= 1);
        assert_eq!(placed + parked, 3);
        if let (Some(a), Some(b)) = (layout[0], layout.get(1).and_then(|x| *x)) {
            assert!(!a.overlaps(&b));
        }
    }

    #[test]
    fn wall_state_startet_an_und_w_toggelt() {
        let mut s = WallState::start_on();
        let two = vec![(10, 1, Some(0)), (20, 2, Some(0))];
        let three = vec![(10, 1, Some(0)), (20, 2, Some(0)), (30, 3, Some(0))];
        assert!(s.on);
        assert!(!s.needs_arrange(&Vec::new()));
        assert!(s.needs_arrange(&two));
        s.mark_arranged(two.clone());
        assert!(!s.needs_arrange(&two));
        assert!(s.needs_arrange(&three));
        s.toggle();
        assert!(!s.on);
        assert!(s.arranged_for.is_empty());
        s.toggle();
        assert!(s.on);
        assert!(s.needs_arrange(&three));
    }

    #[test]
    fn minimierte_kachel_erzwingt_relayout_bei_gleicher_signatur() {
        let mut s = WallState::start_on();
        let two = vec![(10, 1, Some(0)), (20, 2, Some(0))];
        s.mark_arranged(two.clone());
        assert!(
            !s.needs_arrange(&two),
            "HWND-Liste unveraendert — arrange allein greift nicht"
        );
        assert!(
            s.needs_relayout(&two, true),
            "IsIconic muss neu legen, sonst bleiben Kacheln nach Minimize weg"
        );
        assert!(!s.needs_relayout(&two, false));
        s.on = false;
        assert!(
            !s.needs_relayout(&two, true),
            "Wall aus: nicht still wieder aufklappen"
        );
    }

    #[test]
    fn worker_ersatz_mit_gleicher_anzahl_wird_neu_angeordnet() {
        let mut state = WallState::start_on();
        let vorher = vec![(10, 100, Some(0)), (20, 200, Some(0))];
        state.mark_arranged(vorher);
        let ersatz = vec![(10, 100, Some(0)), (30, 300, Some(0))];
        assert!(state.needs_arrange(&ersatz));
    }
}
