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

/// Auflösung eines gemessenen Annahmelimits.
///
/// `config` kennt nur den Wert, nicht die Messdatei: `Option<usize>` wird hier
/// direkt übernommen, die Auflösung aus einer Brain-ID implementiert
/// `brain_limits` (das ohnehin die Messdatei besitzt). Damit zeigt keine Kante
/// mehr von `config` nach `brain_limits`, während bestehende Aufrufer mit einer
/// Brain-ID unverändert kompilieren.
pub trait AcceptedChars {
    fn accepted_chars(&self) -> Option<usize>;
}

impl AcceptedChars for Option<usize> {
    fn accepted_chars(&self) -> Option<usize> {
        *self
    }
}

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
pub fn max_observation_chars_for<L: AcceptedChars>(limit: L) -> usize {
    if let Ok(v) = env::var("WEBAGENT_MAX_OBSERVATION_CHARS") {
        if let Some(n) = v.trim().parse::<usize>().ok().filter(|n| *n >= 1_000) {
            return n;
        }
    }
    match limit.accepted_chars() {
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

/// Vom `/v1/models`-Katalog beworbene Grenzen in Tokens.
///
/// Die alten Werte (128.000 Kontext / 16.384 Output) waren konservative
/// Schaetzungen. Beide Werte bleiben per Env ueberschreibbar. Dies ist die
/// einzige Quelle: Katalog, Pi-Extension und Doku leiten sich daraus ab.
pub const DEFAULT_MODEL_CONTEXT_WINDOW: u64 = 1_000_000;
pub const DEFAULT_MODEL_MAX_TOKENS: u64 = 65_536;

/// Gemessene Composer-Grenze von Gemini in Zeichen (T-943, T-1014).
///
/// Erfolgreiche Live-Turns lagen am 2026-11-26 bei maximal 26.523 Zeichen;
/// ab rund 32.000 Zeichen kuerzt die Oberflaeche still oder nimmt die Eingabe
/// gar nicht erst an (2026-11-26: 38.561/43.609 Zeichen, `pasted_chars=0`,
/// `sends=0`, 69-356 s bis zum Abbruch). 24.000 lsst Puffer gegen
/// Normalisierung und Selektor-Drift. Kein geratenes Modell-Kontextfenster:
/// dies ist die Eingabegrenze der WEBOBERFLAECHE, nicht des Modells.
pub const DEFAULT_GEMINI_PROMPT_CHARS: usize = 24_000;

/// Beworbene Gemini-Grenzen. Der Katalog meldet die tatsaechlich nutzbare
/// Kapazitaet, damit Pi vor dem Senden kompaktiert statt einen Prompt zu
/// erzeugen, den der Composer nicht annimmt.
pub const DEFAULT_GEMINI_CONTEXT_WINDOW: u64 = 12_000;
pub const DEFAULT_GEMINI_MAX_TOKENS: u64 = 4_000;

fn one_limit(raw: Option<&str>, default: u64) -> u64 {
    match raw.map(str::trim) {
        Some(s) if !s.is_empty() => match s.parse::<u64>() {
            Ok(0) | Err(_) => default,
            Ok(n) => n,
        },
        _ => default,
    }
}

/// Reine Auflösung der beworbenen Modellgrenzen (ohne Env-Zugriff, für Tests).
/// `None`/leer/`0`/ungültig → Default; positive Werte werden übernommen.
pub fn resolve_model_limits(raw_context: Option<&str>, raw_output: Option<&str>) -> (u64, u64) {
    (
        one_limit(raw_context, DEFAULT_MODEL_CONTEXT_WINDOW),
        one_limit(raw_output, DEFAULT_MODEL_MAX_TOKENS),
    )
}

/// Reine Auflösung der Gemini-Grenzen (ohne Env-Zugriff, für Tests).
pub fn resolve_gemini_limits(raw_context: Option<&str>, raw_output: Option<&str>) -> (u64, u64) {
    (
        one_limit(raw_context, DEFAULT_GEMINI_CONTEXT_WINDOW),
        one_limit(raw_output, DEFAULT_GEMINI_MAX_TOKENS),
    )
}

/// Reine Auflösung der Promt-Obergrenze (ohne Env-Zugriff, für Tests).
/// `None`/leer/`0`/ungültig → Default.
pub fn resolve_prompt_chars(raw: Option<&str>) -> usize {
    match raw.and_then(|s| s.trim().parse::<usize>().ok()) {
        Some(n) if n > 0 => n,
        _ => DEFAULT_GEMINI_PROMPT_CHARS,
    }
}

/// Promt-Obergrenze je Brain, BEVOR ein Browser geoeffnet wird.
///
/// Nur Gemini ist gemessen begrenzt; andere Brains behalten ihr Verhalten
/// (live bis 300k Zeichen belegt). Null bedeutet "keine Grenze" und wird
/// nie als solche fehlinterpretiert: ein ungueltiger Env-Wert faellt auf den
/// Messwert zurueck.
pub fn max_prompt_chars_for(brain_id: &str) -> usize {
    if brain_id == "gemini" {
        resolve_prompt_chars(env::var("WEBAGENT_GEMINI_PROMPT_CHARS").ok().as_deref())
    } else {
        usize::MAX
    }
}

fn env_model_limits() -> (u64, u64) {
    resolve_model_limits(
        env::var("WEBAGENT_MODEL_CONTEXT_WINDOW").ok().as_deref(),
        env::var("WEBAGENT_MODEL_MAX_TOKENS").ok().as_deref(),
    )
}

/// Kontextfenster, das der Katalog und die Pi-Konfiguration bewerben.
pub fn model_context_window() -> u64 {
    env_model_limits().0
}

/// Output-Limit, das der Katalog und die Pi-Konfiguration bewerben.
pub fn model_max_tokens() -> u64 {
    env_model_limits().1
}

/// Beworbene Modellgrenzen je Brain. Gemini weicht bewusst ab: das Modell
/// koennte 1M Tokens, der gemessene Web-Composer aber nur ~24k Zeichen.
/// Wer die Env-Variablen setzt, kann die gemessenen Werteueberschreiben.
pub fn model_limits_for(brain_id: &str) -> (u64, u64) {
    if brain_id == "gemini" {
        resolve_gemini_limits(
            env::var("WEBAGENT_GEMINI_CONTEXT_WINDOW").ok().as_deref(),
            env::var("WEBAGENT_GEMINI_MAX_TOKENS").ok().as_deref(),
        )
    } else {
        env_model_limits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_wird_genutzt_und_fallback_bleibt() {
        assert_eq!(max_observation_chars_for(Some(100_000)), 50_000);
        assert_eq!(
            max_observation_chars_for(None::<usize>),
            DEFAULT_MAX_OBSERVATION_CHARS
        );
    }

    #[test]
    fn modellgrenzen_defaults_sind_deutlich_hoever_als_128k() {
        let (context, output) = resolve_model_limits(None, None);
        assert_eq!(context, 1_000_000);
        assert_eq!(output, 65_536);
        assert!(context > 128_000);
        assert!(output > 16_384);
    }

    #[test]
    fn modellgrenzen_env_override_und_fallback() {
        assert_eq!(
            resolve_model_limits(Some("200000"), Some("32000")),
            (200_000, 32_000)
        );
        assert_eq!(
            resolve_model_limits(Some("0"), Some("nonsense")),
            (DEFAULT_MODEL_CONTEXT_WINDOW, DEFAULT_MODEL_MAX_TOKENS)
        );
    }

    #[test]
    fn gemini_limits_sind_kleiner_als_die_modellgrenzen() {
        let (context, output) = resolve_gemini_limits(None, None);
        assert_eq!(context, DEFAULT_GEMINI_CONTEXT_WINDOW);
        assert_eq!(output, DEFAULT_GEMINI_MAX_TOKENS);
        assert!(context < DEFAULT_MODEL_CONTEXT_WINDOW);
        assert_eq!(
            resolve_gemini_limits(Some("0"), Some("777")),
            (DEFAULT_GEMINI_CONTEXT_WINDOW, 777)
        );
    }

    #[test]
    fn prompt_cap_nur_fuer_gemini_und_mit_fallback() {
        assert_eq!(max_prompt_chars_for("gemini"), DEFAULT_GEMINI_PROMPT_CHARS);
        assert_eq!(max_prompt_chars_for("chatgpt"), usize::MAX);
        assert_eq!(resolve_prompt_chars(None), 24_000);
        assert_eq!(resolve_prompt_chars(Some(" 1000 ")), 1_000);
        assert_eq!(resolve_prompt_chars(Some("0")), DEFAULT_GEMINI_PROMPT_CHARS);
        assert_eq!(
            resolve_prompt_chars(Some("nonsense")),
            DEFAULT_GEMINI_PROMPT_CHARS
        );
    }
}
