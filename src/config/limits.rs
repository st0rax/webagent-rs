use std::env;

/// Voreingestellte maximale Observation-Länge in Zeichen.
///
/// Bleibt bei 12.000. Ich hatte den Wert am 30.07.2026 auf 40.000 angehoben,
/// weil Brains sich durch Dateien *lesen* statt sie zu bearbeiten — und das
/// wieder zurückgenommen: der Wert war geraten, nicht gemessen, und er behebt
/// die Ursache nicht.
///
/// `src/protocol.rs` hat 62.276 Zeichen, aber nur 34 Signaturen. Ein Brain, das
/// eine Funktion ergänzen soll, braucht die Fundstelle und genug Umgebung für
/// einen eindeutigen Anker — ein paar Dutzend Zeilen, nicht 1567. Ein höheres
/// Limit macht dieselbe Verschwendung nur teurer: dieselbe Datei, jetzt in
/// einem Rutsch, und das in jedem Turn des Verlaufs erneut.
///
/// Der Hebel ist stattdessen die Gliederung im Aufgabentext (siehe
/// `benchmark::file_outline`): Signaturen mit Zeilennummern, rund 2.000 statt
/// 62.000 Zeichen, danach liest das Brain gezielt.
pub const DEFAULT_MAX_OBSERVATION_CHARS: usize = 12_000;

/// Observation-Kappung für ein bestimmtes Brain — aus der Messung abgeleitet.
///
/// Am 30.07.2026 mit `webagent measure-limits` gemessen: chatgpt, deepseek,
/// kimi und zai nahmen jeweils **100.000 Zeichen** an, alle beim ersten
/// Versuch. Das war die oberste Sprosse der damaligen Probenleiter, also eine
/// untere Schranke — nach oben blieb es offen. Genau daran ist die alte Messung
/// missverstanden worden: `rejected_chars: null` heisst „nie abgelehnt", nicht
/// „hier ist Schluss". Seit 02.08.2026 sucht `measure-limits` deshalb nach oben
/// weiter und schachtelt bei der ersten Ablehnung ein; ein Eintrag ohne
/// `rejected_chars` ist weiterhin ausdruecklich nur eine untere Schranke.
/// Der frueher genutzte Wert von 12.000 war rund achtmal zu vorsichtig; er
/// stammte aus der Python-Portierung und war nie nachgeprüft worden.
///
/// Genutzt wird die **Hälfte** des gemessenen Werts: die Messung gilt für eine
/// ganze Nachricht, unsere besteht aber aus Aufgabentext, Verlauf UND
/// Observation. Die Hälfte lässt Luft für den Rest, damit kein Turn an einer
/// Ablehnung verlorengeht.
///
/// Ohne Messwert bleibt der konservative Standard — bewusst klein, bis gemessen
/// wurde, statt zu raten.
pub fn max_observation_chars_for(brain_id: &str) -> usize {
    if let Ok(v) = env::var("WEBAGENT_MAX_OBSERVATION_CHARS") {
        if let Some(n) = v.trim().parse::<usize>().ok().filter(|n| *n >= 1_000) {
            return n;
        }
    }
    match crate::brain_limits::accepted_chars(brain_id) {
        Some(gemessen) => (gemessen / 2).max(DEFAULT_MAX_OBSERVATION_CHARS),
        None => DEFAULT_MAX_OBSERVATION_CHARS,
    }
}

/// Maximale Observation-Länge ohne Brain-Bezug (Fallback).
pub fn max_observation_chars() -> usize {
    env::var("WEBAGENT_MAX_OBSERVATION_CHARS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n >= 1_000)
        .unwrap_or(DEFAULT_MAX_OBSERVATION_CHARS)
}
/// Loop-Guard Warn-/Abort-Schwellen — Python `LOOP_GUARD_*`.
pub const LOOP_GUARD_WARN_COUNT: usize = 3;
pub const LOOP_GUARD_ABORT_COUNT: usize = 8;

/// Gesamte Wall-Clock-Obergrenze (Sekunden) für einen einzelnen Run. Fängt
/// hängende Läufe ab, die weder max_cycles noch der Loop-Guard je erreichen,
/// weil sie in der Warte-/Sendephase eines Brains klemmen (real beobachtet:
/// kimi hing 30+ min). Default; via WEBAGENT_MAX_RUN_SECONDS überschreibbar.
pub const MAX_RUN_WALL_SECONDS: u64 = 600;

/// Aufgelöste Wall-Clock-Deadline (Sekunden) eines Runs: WEBAGENT_MAX_RUN_SECONDS
/// falls gesetzt und sinnvoll, sonst MAX_RUN_WALL_SECONDS. Leer/„0"/ungültig →
/// Default.
pub fn max_run_wall_secs() -> u64 {
    resolve_max_run_wall_secs(env::var("WEBAGENT_MAX_RUN_SECONDS").ok().as_deref())
}

/// Reine Auflösung der Wall-Clock-Deadline (ohne Env-Zugriff, für Tests).
/// `None`/leer/„0"/nicht-parsebar → `MAX_RUN_WALL_SECONDS`; eine positive Zahl
/// wird übernommen.
pub fn resolve_max_run_wall_secs(raw: Option<&str>) -> u64 {
    match raw.map(str::trim) {
        Some(s) if !s.is_empty() => match s.parse::<u64>() {
            Ok(0) | Err(_) => MAX_RUN_WALL_SECONDS,
            Ok(n) => n,
        },
        _ => MAX_RUN_WALL_SECONDS,
    }
}
