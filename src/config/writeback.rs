use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::paths::{profiles_dir, shared_profile_dir};
use super::profiles::copy_dir_sparse;
use super::selectors::encapsulated_profile_dir;

/// Laufzeit-Kopie des Master-Profils, einmal pro Prozess (`OnceLock`).
///
/// Das Master (`profiles/shared`) ist das **read-only Hauptprofil** mit allen
/// eingeloggten Brains. Der Betrieb öffnet es NIE direkt: Beim ersten Zugriff
/// wird eine sparsame Kopie der Login-Artefakte (Cookies, Local State,
/// Preferences, … — siehe [`SPARSE_COPY_WHITELIST`]) nach
/// `profiles/encapsulated/pool_<stamp>` gezogen und ausschließlich dort
/// gearbeitet. Caches erzeugt WebView2 in der Kopie frisch.
///
/// Damit überlebt ein Neu-Login dauerhaft: Selbst wenn eine Laufzeit-Kopie
/// aufgeräumt wird (siehe [`sweep_stale_runtime_profiles`]), klont der nächste
/// Start erneut aus dem unangetasteten Master.
static RUNTIME_POOL_PROFILE: OnceLock<PathBuf> = OnceLock::new();

pub fn runtime_pool_profile_dir() -> PathBuf {
    if let Some(p) = RUNTIME_POOL_PROFILE.get() {
        return p.clone();
    }
    let master = shared_profile_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let dst = encapsulated_profile_dir("pool", &stamp);
    let copied = copy_dir_sparse(&master, &dst).is_ok();
    if copied && has_login_artifacts(&dst) {
        crate::bench_events::eprint_line(&format!(
            "[master-profile] Laufzeit-Kopie des Hauptprofils → {:?}",
            dst
        ));
    } else {
        crate::bench_events::eprint_line(&format!(
            "[master-profile] WARN: Laufzeit-Kopie von {:?} ohne Login-Artefakte \
             (Master offen/gesperrt? Kopie nach {:?})",
            master, dst
        ));
    }
    // Master gegen die kanonischen Profile pruefen: kennt das Master einen
    // Brain nicht, den profiles/<brain> eingeloggt traegt, wird der Pool fuer
    // genau diesen Brain "Login nötig" melden - obwohl die Session gueltig ist
    // (gemessen 08.08.2026: kimi und chatgpt). Laut warnen statt still sperren.
    let missing = master_missing_sessions_from_canonical();
    if !missing.is_empty() {
        crate::bench_events::eprint_line(&format!(
            "[master-profile] WARN: Master kennt {} nicht, obwohl die kanonischen \
             Profile eingeloggt sind. Der Pool wuerde Login nötig melden trotz \
             gueltiger Session. Abhilfe: login-all (spiegelt die Sitzungen ins Master).",
            missing.join(", ")
        ));
    }
    let _ = RUNTIME_POOL_PROFILE.set(dst.clone());
    dst
}

/// Spielt die aufgefrischte Sitzung aus der Laufzeit-Kopie ins Master zurueck.
///
/// # Warum es das braucht
///
/// Das Master ist read-only und der Betrieb laeuft auf einem Klon. Der Browser
/// erneuert Sitzungen aber IN DIE KOPIE — und ohne Rueckweg blieb das Master
/// stehen. Gemessen am 05.08.2026: letzte Schreiboperation im Master
/// 03.08. 18:07:44, in der Laufzeit-Kopie laufend.
///
/// Das ist nicht bloss veraltet, es ist toedlich: die Anbieter rotieren
/// Refresh-Tokens. Sobald der Browser eine Sitzung erneuert, entwertet der
/// Anbieter den alten Token serverseitig. Die Kopie haelt den neuen, das Master
/// den bereits entwerteten. Nach ein bis zwei Runden ist das Master garantiert
/// abgemeldet — und eine frische Anmeldung haelt nur bis zur naechsten
/// Rotation. Genau so wurden 6 von 8 Brains abgemeldet.
///
/// # Die Schutzbedingung
///
/// Zurueckgeschrieben wird NUR, wenn die Kopie selbst Login-Artefakte hat. Ein
/// Lauf, der frueh scheitert oder mit leerem Profil startet, darf das Master
/// nicht ueberschreiben — sonst zerstoert genau dieser Rueckweg die
/// Anmeldung, die er schuetzen soll.
pub fn write_back_session_to_master() -> Result<(), String> {
    let Some(runtime) = RUNTIME_POOL_PROFILE.get() else {
        // Nie geklont, also nichts zurueckzuschreiben.
        return Ok(());
    };
    if !runtime.is_dir() {
        return Ok(());
    }
    write_back_dir_to_master(runtime)
}

/// Spielt eine Laufzeit-Kopie ins Master-Profil zurueck — der gemeinsame Kern
/// von [`write_back_session_to_master`] und dem CLI-Rettungsweg
/// `webagent sync-master`.
///
/// Das Master ist read-only und der Betrieb laeuft auf einem Klon. Der Browser
/// erneuert Sitzungen aber IN DIE KOPIE — und ohne Rueckweg blieb das Master
/// stehen. Gemessen am 05.08.2026: letzte Schreiboperation im Master
/// 03.08. 18:07:44, in der Laufzeit-Kopie laufend.
///
/// # Die Schutzbedingung
///
/// Zurueckgeschrieben wird NUR, wenn die Kopie selbst Login-Artefakte hat. Ein
/// Lauf, der frueh scheitert oder mit leerem Profil startet, darf das Master
/// nicht ueberschreiben — sonst zerstoert genau dieser Rueckweg die
/// Anmeldung, die er schuetzen soll.
pub fn write_back_dir_to_master(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("{:?} ist kein Verzeichnis", dir));
    }
    if !has_login_artifacts(dir) {
        return Err(format!(
            "Laufzeit-Kopie {:?} hat keine Login-Artefakte — nicht zurueckgeschrieben, \
             sonst wuerde das Master ueberschrieben",
            dir
        ));
    }
    let master = shared_profile_dir();

    // Zweite, schaerfere Bedingung: die Kopie darf nicht AERMER sein als das
    // Master. Die blosse Existenz von Login-Dateien genuegt nicht — eine Kopie
    // mit einer einzigen Sitzung besteht diese Pruefung und wuerde ein Master
    // mit acht Sitzungen ueberschreiben. Gemessen am 07.08.2026: `Cookies` fiel
    // von 108 KB auf 40 KB, danach meldete der halbe Lauf „Login noetig".
    let source_weight = login_artifact_weight(dir);
    let target_weight = login_artifact_weight(&master);
    if !write_back_is_safe(source_weight, target_weight) {
        let msg = format!(
            "[master-profile] Rueckschreiben ABGELEHNT: Laufzeit-Kopie traegt {} KB \
             Sitzungsdaten, das Hauptprofil {} KB. Ueberschreiben wuerde Anmeldungen \
             vernichten. Kopie bleibt unter {:?}",
            source_weight / 1024,
            target_weight / 1024,
            dir
        );
        crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
        return Err(msg);
    }

    // Dritte, pro-Brain Bedingung: eine Kopie, die eine Sitzung verloren hat,
    // darf das Master mit dieser Sitzung nicht ueberschreiben. Gemessen am
    // 08.08.2026: kimi und chatgpt waren im Master angemeldet (kanonische
    // Profile trugen kimi-auth bzw. session-token), die Laufzeit-Kopie hatte
    // die Cookies verloren - und der Pool meldete "Login nötig" trotz
    // gueltiger Session. Das Gewichts-Mass sieht den Verlust eines einzelnen
    // Brains nicht.
    let lost = runtime_lost_sessions(&cookies_db_bytes(&master), &cookies_db_bytes(dir));
    if !lost.is_empty() {
        let msg = format!(
            "[master-profile] Rueckschreiben ABGELEHNT: Laufzeit-Kopie hat die Sitzung \
             verloren, die das Hauptprofil noch traegt: {}. Ueberschreiben wuerde \
             diese Anmeldung vernichten. Kopie bleibt unter {:?}",
            lost.join(", "),
            dir
        );
        crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
        return Err(msg);
    }

    // Vor dem Ueberschreiben sichern. Auch eine bestandene Pruefung kann
    // danebenliegen (Groesse ist ein Mass, kein Beweis) — und eine verlorene
    // Anmeldung kostet den Menschen davor echte Zeit. Die Sicherung ist die
    // Versicherung gegen genau diesen Irrtum.
    let backup = master.with_file_name(format!(
        "shared.session-bak-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    if let Err(e) = copy_dir_sparse(&master, &backup) {
        crate::bench_events::emit(
            crate::bench_events::Level::Warn,
            None,
            &format!("[master-profile] Sicherung vor dem Rueckschreiben fehlgeschlagen: {e}"),
        );
    }

    unseal_master_profile();
    let result = copy_dir_sparse(&dir.to_path_buf(), &master).map_err(|e| e.to_string());
    seal_master_profile();
    match &result {
        Ok(()) => crate::bench_events::emit(
            crate::bench_events::Level::Info,
            None,
            "[master-profile] aufgefrischte Sitzung ins Hauptprofil zurueckgeschrieben",
        ),
        Err(e) => crate::bench_events::emit(
            crate::bench_events::Level::Warn,
            None,
            &format!("[master-profile] Rueckschreiben fehlgeschlagen: {e}"),
        ),
    }
    result
}

/// Gesamtgroesse aller Login-Dateien in `dir` — ein grobes Mass dafuer, wie
/// viele Sitzungen ein Profil traegt.
///
/// Kein exakter Zaehler (dafuer muesste man die SQLite-Datenbank oeffnen), aber
/// ein belastbares Warnsignal: acht angemeldete Dienste hinterlassen deutlich
/// mehr als zwei. Gemessen am 07.08.2026 fiel `Cookies` im Hauptprofil von
/// 108 KB auf 40 KB, nachdem eine aermere Laufzeit-Kopie darueber geschrieben
/// wurde.
fn login_artifact_weight(dir: &Path) -> u64 {
    const NEEDED: &[&str] = &["Cookies", "Local State", "Login Data"];
    let mut sum = 0u64;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ty) = e.file_type() else { continue };
            if ty.is_dir() {
                stack.push(e.path());
            } else {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if NEEDED.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    sum += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
    }
    sum
}

/// Ab welchem Anteil des Ziels eine Quelle noch als „nicht aermer" durchgeht.
///
/// Nicht 100 %: eine SQLite-Datenbank schrumpft auch beim Aufraeumen, und ein
/// Schutz, der bei jeder Schwankung anschlaegt, wird abgeschaltet. 60 % laesst
/// normales Atmen zu und faengt den Fall vom 07.08.2026 (Abfall auf 37 %).
const WRITE_BACK_MIN_RATIO: f64 = 0.6;

/// Darf `source` das `target` ueberschreiben, ohne Sitzungen zu vernichten?
///
/// Reine Funktion auf den beiden Gewichten, damit die Entscheidung ohne
/// Dateisystem pruefbar ist.
pub(crate) fn write_back_is_safe(source: u64, target: u64) -> bool {
    if source == 0 {
        return false; // nichts drin — niemals
    }
    if target == 0 {
        return true; // Ziel leer, jede Quelle ist eine Verbesserung
    }
    source as f64 >= target as f64 * WRITE_BACK_MIN_RATIO
}

/// Liegt in `dir` (rekursiv) mindestens eine der Login-Dateien, deren Fehlen
/// eine ausgeloggte Kopie bedeutet?
pub(crate) fn has_login_artifacts(dir: &Path) -> bool {
    const NEEDED: &[&str] = &["Cookies", "Local State", "Login Data"];
    let mut found = false;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ty) = e.file_type() else {
                continue;
            };
            if ty.is_dir() {
                stack.push(e.path());
            } else {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if NEEDED.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    found = true;
                    break;
                }
            }
        }
        if found {
            break;
        }
    }
    found
}

/// Sitzungs-Nachweis je Brain: ein markanter Cookie-Name, der NUR in einem
/// eingeloggten Profil auftaucht. Gemessen am 08.08.2026 an den echten
/// Cookie-Datenbanken: kimi traegt `kimi-auth`, chatgpt
/// `__Secure-next-auth.session-token` (Präfix, Suffix .0/.1), mistral
/// `ory_session` (variables Suffix). Das Master trug diese Cookies am 07.08.
/// noch; nach einem Rueckschreiben aus einer Laufzeit-Kopie, die die Brains
/// als ausgeloggt erlebt hatte, fehlten kimi und chatgpt — und der Pool
/// meldete "Login nötig", obwohl `profiles/<brain>` die gueltige Sitzung trug.
///
/// Bewusst keine Vollstaendigkeit: ein Brain ohne bekannten, eindeutigen
/// Sitzungs-Cookie faellt einfach unter den Gewichts-Schutz.
const SESSION_PROOF_COOKIES: &[(&str, &[&str])] = &[
    ("kimi", &["kimi-auth"]),
    ("chatgpt", &["__Secure-next-auth.session-token"]),
    ("mistral", &["ory_session"]),
];

/// Enthaelt der Byte-Haufen `hay` den ASCII-Text `needle`?
///
/// Chromium speichert Cookie-Namen als Klartext-Spalten in der SQLite-Datei;
/// ein Rohbyte-Scan genuegt als Sitzungs-Nachweis, ohne SQLite zu parsen oder
/// verschluesselte Werte zu lesen.
pub(crate) fn bytes_contain(hay: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > hay.len() {
        return n.is_empty();
    }
    hay.windows(n.len()).any(|w| w == n)
}

/// Pfad zur Cookies-Datenbank unter `dir` (rekursiv, genau der Name "Cookies").
pub(crate) fn cookies_db_path(dir: &Path) -> Option<PathBuf> {
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ty) = e.file_type() else {
                continue;
            };
            let p = e.path();
            if ty.is_dir() {
                stack.push(p);
            } else if e.file_name().to_string_lossy().eq_ignore_ascii_case("Cookies") {
                return Some(p);
            }
        }
    }
    None
}

/// Rohbytes der Cookies-Datenbank unter `dir` (leer, wenn keine da ist).
pub(crate) fn cookies_db_bytes(dir: &Path) -> Vec<u8> {
    cookies_db_path(dir)
        .and_then(|p| std::fs::read(p).ok())
        .unwrap_or_default()
}

/// Welche Brains hat die Laufzeit-Kopie ausgeloggt, die das Master noch kennt?
///
/// Kern der Schutzwache beim Rueckschreiben: nur der Fall "Nachweis im Master
/// da, in der Kopie weg" vernichtet eine gueltige Anmeldung. Das Gewichts-Mass
/// aus ed802aa sieht genau das nicht — am 08.08.2026 fehlten einem ~90-KB-
/// Cookie-Vorrat ein paar hundert Bytes (kimi-auth) und die Schranke schwieg.
/// Reine Funktion auf den Rohbytes, damit ohne Dateisystem pruefbar.
pub(crate) fn runtime_lost_sessions(master_cookies: &[u8], runtime_cookies: &[u8]) -> Vec<&'static str> {
    SESSION_PROOF_COOKIES
        .iter()
        .filter_map(|(brain, needles)| {
            let master_has = needles.iter().any(|n| bytes_contain(master_cookies, n));
            let runtime_has = needles.iter().any(|n| bytes_contain(runtime_cookies, n));
            (master_has && !runtime_has).then_some(*brain)
        })
        .collect()
}

/// Welche Brains traegt das kanonische `canonical_cookies`-Profil eingeloggt,
/// das Master aber nicht?
///
/// Gemessen 08.08.2026: kimi und chatgpt fehlten im Master, obwohl
/// `profiles/kimi` und `profiles/chatgpt` gueltige Sitzungen trugen. Der Pool
/// klont ausschliesslich aus dem Master — daher "Login nötig" trotz gueltiger
/// Session. Reine Funktion auf den Rohbytes.
// Reiner Kern des Vergleichs. Die Produktivfunktion
// `master_missing_sessions_from_canonical` liest pro Brain ein eigenes
// kanonisches Profil und kann diesen Kern deshalb nicht direkt nutzen;
// hier bleibt er als pruefbare Fassung der Regel.
#[cfg(test)]
pub(crate) fn master_missing_sessions(canonical_cookies: &[u8], master_cookies: &[u8]) -> Vec<&'static str> {
    SESSION_PROOF_COOKIES
        .iter()
        .filter_map(|(brain, needles)| {
            let canonical_has = needles.iter().any(|n| bytes_contain(canonical_cookies, n));
            let master_has = needles.iter().any(|n| bytes_contain(master_cookies, n));
            (canonical_has && !master_has).then_some(*brain)
        })
        .collect()
}

/// Kanonische Profile gegen das Master vergleichen: welche Brains kennt das
/// Master nicht, obwohl `profiles/<brain>` eingeloggt ist? Laute Warnung beim
/// Pool-Klon, damit die Luecke sichtbar wird, statt still in "Login nötig" zu
/// muenden.
fn master_missing_sessions_from_canonical() -> Vec<&'static str> {
    let master_cookies = cookies_db_bytes(&shared_profile_dir());
    SESSION_PROOF_COOKIES
        .iter()
        .filter_map(|(brain, needles)| {
            // Nur den eigenen Sitzungs-Nachweis pruefen: ein kanonisches Profil
            // kann auch fremde Cookies tragen (Sitzung aus einer gemeinsamen
            // Login-Runde), das darf das Nachbar-Brain nicht als fehlend melden.
            let canonical = cookies_db_bytes(&profiles_dir().join(brain));
            let canonical_has = needles.iter().any(|n| bytes_contain(&canonical, n));
            let master_has = needles.iter().any(|n| bytes_contain(&master_cookies, n));
            (canonical_has && !master_has).then_some(*brain)
        })
        .collect()
}

/// Versiegelt das Master-Profil: alle Dateien unter `profiles/shared` werden
/// read-only. NUR der Login-Mechanismus (unseal → schreiben → seal) macht das
/// Profil jemals wieder beschreibbar; der Betrieb nutzt ausschließlich die
/// Laufzeit-Kopie ([`runtime_pool_profile_dir`]).
pub fn seal_master_profile() {
    set_master_readonly(true);
}

/// Macht das Master-Profil wieder beschreibbar — NUR vom Login-Mechanismus
/// aufgerufen, bevor ein Browser das Master öffnet.
pub fn unseal_master_profile() {
    set_master_readonly(false);
}

fn set_master_readonly(readonly: bool) {
    let root = shared_profile_dir();
    if !root.is_dir() {
        return;
    }
    let mut files = Vec::new();
    collect_files(&root, &mut files);
    let mut ok = 0usize;
    let mut failed = 0usize;
    for f in &files {
        if let Ok(md) = std::fs::metadata(f) {
            let mut perms = md.permissions();
            perms.set_readonly(readonly);
            if std::fs::set_permissions(f, perms).is_ok() {
                ok += 1;
            } else {
                failed += 1;
            }
        }
    }
    crate::bench_events::eprint_line(&format!(
        "[master-profile] Hauptprofil {}: {ok} Dateien ({failed} Fehler)",
        if readonly {
            "versiegelt (read-only)"
        } else {
            "entsiegelt (beschreibbar)"
        }
    ));
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(ty) = e.file_type() else {
            continue;
        };
        if ty.is_dir() {
            collect_files(&e.path(), out);
        } else {
            out.push(e.path());
        }
    }
}

