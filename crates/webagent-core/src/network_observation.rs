//! T-940: ausschließlich **lesende** Netzwerkbeobachtung für genau einen
//! Provider-Turn.
//!
//! Der CDP-Kanal wird nur mit `Network.enable` und den drei Sensor-Events
//! `requestWillBeSent`, `responseReceived`, `loadingFinished` belegt. Es gibt
//! **kein** `Network.setRequestInterception`, kein `Fetch.enable`, keine
//! Mutation von Anfragen oder Antworten und keine Port-/WebSocket-Verbindung.
//!
//! # Schutzregeln
//!
//! - URLs werden auf `scheme://host` reduziert ([`redact_origin`]); Query,
//!   Fragment und Userinfo — also genau die Stellen, in denen Tokens stehen
//!   könnten — werden **nie** gespeichert.
//! - Es werden nur Zähler, Status, Ressourcentyp und Zeitstempel gesammelt.
//! - Die Beobachtung ist opt-in (`WEBAGENT_NETWORK_OBSERVE`); der Regelbetrieb
//!   registriert keine CDP-Receiver.
//! - Jede Liste ist auf [`MAX_EVENTS`] Einträge begrenzt; verworfene Ereignisse
//!   werden gezählt, damit ein Beleg nicht stillschweigend unvollständig ist.
//!
//! Die Klassifikation in [`NetworkLog::verdict`] ist reine Logik und wird ohne
//! Browser unit-getestet. Der WebView2-spezifische Teil liegt in
//! `webview_runtime.rs`.

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Obergrenze je Ereignisliste.
pub const MAX_EVENTS: usize = 512;

/// Umweltvariable, die die CDP-Beobachtung für diesen Prozess aktiviert.
pub const OBSERVE_ENV: &str = "WEBAGENT_NETWORK_OBSERVE";

/// Opt-in-Prüfung für den WebView-Thread.
pub fn observation_enabled() -> bool {
    std::env::var_os(OBSERVE_ENV).is_some()
}

/// Nur XHR/Fetch sind Kandidaten für den eigentlichen Provider-Aufruf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedResource {
    Xhr,
    Fetch,
    Document,
    Image,
    Other(String),
}

impl ObservedResource {
    /// CDP liefert den Ressourcentyp in Großbuchstaben (`XHR`, `Fetch`, …).
    pub fn from_cdp(value: &str) -> Self {
        match value {
            "XHR" => Self::Xhr,
            "Fetch" => Self::Fetch,
            "Document" => Self::Document,
            "Image" => Self::Image,
            other => Self::Other(other.to_string()),
        }
    }

    pub fn is_api_candidate(&self) -> bool {
        matches!(self, Self::Xhr | Self::Fetch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedRequest {
    pub request_id: String,
    pub method: String,
    /// Redaktiert: `scheme://host` ohne Query, Fragment und Userinfo.
    pub origin: String,
    pub resource: ObservedResource,
    /// Millisekunden seit [`reset`].
    pub at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedResponse {
    pub request_id: String,
    /// Redaktiert: `scheme://host`.
    pub origin: String,
    pub status: u16,
    pub mime_type: String,
    pub at_ms: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedFinish {
    pub request_id: String,
    pub at_ms: u128,
    pub encoded_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkLog {
    pub requests: Vec<ObservedRequest>,
    pub responses: Vec<ObservedResponse>,
    pub finished: Vec<ObservedFinish>,
    /// Anzahl Ereignisse, die wegen [`MAX_EVENTS`] verworfen wurden.
    pub dropped_events: u64,
    /// Gesamtzähler, auch für verworfene Ereignisse.
    pub total_requests: u64,
    pub total_responses: u64,
    pub total_finished: u64,
}

/// Teamfreundliche Entscheidung: **Füllfehler** gegen **Antwortfehler**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnNetworkVerdict {
    /// Kein API-Request abgegangen: der Fehler lag vor dem Absenden
    /// (Composer nie gefüllt / Sendeknopf nicht ausgelöst).
    NoProviderRequest { total_requests: u64, reason: String },
    /// Request ging raus, aber es kam keine passende Antwort zurück.
    RequestWithoutResponse {
        origin: String,
        method: String,
        at_ms: u128,
        reason: String,
    },
    /// Antwort wurde empfangen — mit Status, gemessener Dauer und der
    /// nachvollziehbaren Begründung, welcher Request gewählt wurde.
    ProviderResponse {
        request_id: String,
        origin: String,
        status: u16,
        mime_type: String,
        duration_ms: u128,
        finished: bool,
        selection_reason: String,
    },
}

impl NetworkLog {
    /// Alle beobachteten XHR/Fetch-Requests in Ereignisreihenfolge.
    pub fn api_candidates(&self) -> Vec<&ObservedRequest> {
        self.requests
            .iter()
            .filter(|request| request.resource.is_api_candidate())
            .collect()
    }

    /// Klassifiziert den Turn.
    ///
    /// Die Auswahl des Provider-Calls ist bewusst **nicht** „der jüngste
    /// Request": ein echter DeepSeek-Livebeleg zeigte, dass danach ein
    /// 4-ms-Static-Fetch von `fe-static.deepseek.com` kam. Stattdessen wird
    /// gewichtet: SSE-MIME (`text/event-stream`) ist das stärkste Signal für
    /// den Antwortstrom, danach POST, die Origin der Dokumentseite und 2xx.
    /// Bei Gleichstand gewinnt die längere Dauer. Die Begründung steht im
    /// Urteil, damit der Beleg die Wahl nachvollziehen kann.
    pub fn verdict(&self) -> TurnNetworkVerdict {
        let candidates = self.api_candidates();
        if candidates.is_empty() {
            return TurnNetworkVerdict::NoProviderRequest {
                total_requests: self.total_requests,
                reason: format!(
                    "Kein XHR/Fetch-Request beobachtet ({} Request(s) insgesamt). \
                     Der Fehler lag vor dem Absenden: Composer-Füll- oder Sendeweg.",
                    self.total_requests
                ),
            };
        }

        let site_origin = self
            .requests
            .iter()
            .find(|request| request.resource == ObservedResource::Document)
            .map(|request| request.origin.clone());

        let mut best: Option<(i32, u128, &ObservedRequest, &ObservedResponse, String)> = None;
        for request in &candidates {
            let Some(response) = self
                .responses
                .iter()
                .find(|response| response.request_id == request.request_id)
            else {
                continue;
            };
            let duration_ms = self
                .finished
                .iter()
                .rev()
                .find(|finish| finish.request_id == request.request_id)
                .map(|finish| finish.at_ms.saturating_sub(request.at_ms))
                .unwrap_or_else(|| response.at_ms.saturating_sub(request.at_ms));

            let mut score = 0_i32;
            let mut reasons = Vec::new();
            let mime = response.mime_type.to_ascii_lowercase();
            if mime.contains("text/event-stream") {
                score += 8;
                reasons.push("SSE-Antwort (text/event-stream)");
            }
            if request.method.eq_ignore_ascii_case("POST") {
                score += 3;
                reasons.push("POST");
            }
            if site_origin.as_deref() == Some(request.origin.as_str()) {
                score += 2;
                reasons.push("Origin der Dokumentseite");
            }
            if (200..300).contains(&response.status) {
                score += 1;
                reasons.push("2xx");
            }

            let better = match &best {
                None => true,
                Some((best_score, best_duration, ..)) => {
                    (score, duration_ms) > (*best_score, *best_duration)
                }
            };
            if better {
                best = Some((score, duration_ms, request, response, reasons.join(", ")));
            }
        }

        if let Some((_score, duration_ms, request, response, reason)) = best {
            let finished = self
                .finished
                .iter()
                .any(|finish| finish.request_id == request.request_id);
            return TurnNetworkVerdict::ProviderResponse {
                request_id: request.request_id.clone(),
                origin: request.origin.clone(),
                status: response.status,
                mime_type: response.mime_type.clone(),
                duration_ms,
                finished,
                selection_reason: if reason.is_empty() {
                    "kein positiver Signalwert; einziger Kandidat mit Antwort".to_string()
                } else {
                    reason
                },
            };
        }

        let request = candidates.last().expect("candidates ist nicht leer");
        TurnNetworkVerdict::RequestWithoutResponse {
            origin: request.origin.clone(),
            method: request.method.clone(),
            at_ms: request.at_ms,
            reason: format!(
                "{} API-Request(s) abgegangen, aber keine passende Network-Antwort.",
                candidates.len()
            ),
        }
    }
}

fn log() -> &'static Mutex<NetworkLog> {
    static LOG: OnceLock<Mutex<NetworkLog>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(NetworkLog::default()))
}

fn clock() -> &'static Mutex<Instant> {
    static CLOCK: OnceLock<Mutex<Instant>> = OnceLock::new();
    CLOCK.get_or_init(|| Mutex::new(Instant::now()))
}

/// Setzt Log und Uhr zurück — genau einmal direkt vor dem zu belegenden Turn.
pub fn reset() {
    *log().lock().expect("Network-Log vergiftet") = NetworkLog::default();
    *clock().lock().expect("Network-Uhr vergiftet") = Instant::now();
}

/// Millisekunden seit dem letzten [`reset`].
pub fn now_ms() -> u128 {
    clock()
        .lock()
        .expect("Network-Uhr vergiftet")
        .elapsed()
        .as_millis()
}

/// Kopie des aktuellen Logs für den Beleg.
pub fn snapshot() -> NetworkLog {
    log().lock().expect("Network-Log vergiftet").clone()
}

/// Reduziert eine URL auf `scheme://host`. Query, Fragment, Port und Userinfo
/// werden entfernt — dort könnten Tokens oder Zugangsdaten stehen.
pub fn redact_origin(raw: &str) -> String {
    let Some(split) = raw.find("://") else {
        return "<ohne-schema>".to_string();
    };
    let scheme = &raw[..split];
    let rest = &raw[split + 3..];
    let host_part = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host_part.rsplit('@').next().unwrap_or(host_part);
    if host.is_empty() {
        "<ohne-host>".to_string()
    } else {
        format!("{scheme}://{host}")
    }
}

fn push_bounded<T>(items: &mut Vec<T>, item: T, dropped: &mut u64) {
    if items.len() >= MAX_EVENTS {
        items.remove(0);
        *dropped += 1;
    }
    items.push(item);
}

/// Wird vom CDP-Handler `Network.requestWillBeSent` aufgerufen.
pub fn record_request(
    request_id: impl Into<String>,
    method: impl Into<String>,
    raw_url: &str,
    resource: &str,
    at_ms: u128,
) {
    let state = &mut *log().lock().expect("Network-Log vergiftet");
    state.total_requests += 1;
    let NetworkLog {
        requests,
        dropped_events,
        ..
    } = state;
    push_bounded(
        requests,
        ObservedRequest {
            request_id: request_id.into(),
            method: method.into(),
            origin: redact_origin(raw_url),
            resource: ObservedResource::from_cdp(resource),
            at_ms,
        },
        dropped_events,
    );
}

/// Wird vom CDP-Handler `Network.responseReceived` aufgerufen.
pub fn record_response(
    request_id: impl Into<String>,
    raw_url: &str,
    status: u16,
    mime_type: impl Into<String>,
    at_ms: u128,
) {
    let state = &mut *log().lock().expect("Network-Log vergiftet");
    state.total_responses += 1;
    let NetworkLog {
        responses,
        dropped_events,
        ..
    } = state;
    push_bounded(
        responses,
        ObservedResponse {
            request_id: request_id.into(),
            origin: redact_origin(raw_url),
            status,
            mime_type: mime_type.into(),
            at_ms,
        },
        dropped_events,
    );
}

/// Wird vom CDP-Handler `Network.loadingFinished` aufgerufen.
pub fn record_finish(request_id: impl Into<String>, at_ms: u128, encoded_bytes: u64) {
    let state = &mut *log().lock().expect("Network-Log vergiftet");
    state.total_finished += 1;
    let NetworkLog {
        finished,
        dropped_events,
        ..
    } = state;
    push_bounded(
        finished,
        ObservedFinish {
            request_id: request_id.into(),
            at_ms,
            encoded_bytes,
        },
        dropped_events,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(id: &str, resource: &str, at_ms: u128) -> ObservedRequest {
        ObservedRequest {
            request_id: id.to_string(),
            method: "POST".to_string(),
            origin: "https://api.example.test".to_string(),
            resource: ObservedResource::from_cdp(resource),
            at_ms,
        }
    }

    fn resp(id: &str, status: u16, at_ms: u128) -> ObservedResponse {
        ObservedResponse {
            request_id: id.to_string(),
            origin: "https://api.example.test".to_string(),
            status,
            mime_type: "application/json".to_string(),
            at_ms,
        }
    }

    #[test]
    fn t940_redacts_tokens_userinfo_fragment_and_port() {
        let url = "https://user:secret@api.example.test:8443/v1/chat?api_key=TOPSECRET#frag";
        assert_eq!(redact_origin(url), "https://api.example.test:8443");
        assert_eq!(
            redact_origin("wss://relay.example.test/socket?token=abc"),
            "wss://relay.example.test"
        );
        assert_eq!(redact_origin("kein-schema"), "<ohne-schema>");
    }

    #[test]
    fn t940_no_api_request_means_fill_error() {
        let log = NetworkLog {
            requests: vec![req("doc", "Document", 10)],
            total_requests: 1,
            ..NetworkLog::default()
        };
        match log.verdict() {
            TurnNetworkVerdict::NoProviderRequest {
                total_requests,
                reason,
            } => {
                assert_eq!(total_requests, 1);
                assert!(reason.contains("vor dem Absenden"));
            }
            other => panic!("erwartete NoProviderRequest, erhalten {other:?}"),
        }
    }

    #[test]
    fn t940_request_without_response_means_response_error() {
        let log = NetworkLog {
            requests: vec![req("chat", "Fetch", 25)],
            total_requests: 1,
            ..NetworkLog::default()
        };
        match log.verdict() {
            TurnNetworkVerdict::RequestWithoutResponse {
                origin,
                method,
                at_ms,
                ..
            } => {
                assert_eq!(origin, "https://api.example.test");
                assert_eq!(method, "POST");
                assert_eq!(at_ms, 25);
            }
            other => panic!("erwartete RequestWithoutResponse, erhalten {other:?}"),
        }
    }

    #[test]
    fn t940_response_reports_status_and_finish_duration() {
        let log = NetworkLog {
            requests: vec![req("chat", "XHR", 100)],
            responses: vec![resp("chat", 200, 900)],
            finished: vec![ObservedFinish {
                request_id: "chat".to_string(),
                at_ms: 1_450,
                encoded_bytes: 2_048,
            }],
            total_requests: 1,
            total_responses: 1,
            total_finished: 1,
            ..NetworkLog::default()
        };
        assert_eq!(
            log.verdict(),
            TurnNetworkVerdict::ProviderResponse {
                request_id: "chat".to_string(),
                origin: "https://api.example.test".to_string(),
                status: 200,
                mime_type: "application/json".to_string(),
                duration_ms: 1_350,
                finished: true,
                selection_reason: "POST, 2xx".to_string(),
            }
        );
    }

    #[test]
    fn t940_prefers_sse_provider_call_over_later_static_fetch() {
        // Realer DeepSeek-Livefall: der SSE-Call (`chat.deepseek.com`, 1676 ms)
        // startet früh; danach lädt die SPA ein statisches Bundle 4 ms lang.
        // Der jüngste Request darf nicht als Provider-Antwort gelten.
        let document = ObservedRequest {
            request_id: "doc".to_string(),
            method: "GET".to_string(),
            origin: "https://chat.deepseek.com".to_string(),
            resource: ObservedResource::Document,
            at_ms: 0,
        };
        let chat_request = ObservedRequest {
            request_id: "5568.202".to_string(),
            method: "POST".to_string(),
            origin: "https://chat.deepseek.com".to_string(),
            resource: ObservedResource::Fetch,
            at_ms: 3_709,
        };
        let static_request = ObservedRequest {
            request_id: "5568.300".to_string(),
            method: "GET".to_string(),
            origin: "https://fe-static.deepseek.com".to_string(),
            resource: ObservedResource::Fetch,
            at_ms: 9_000,
        };
        let mut chat_response = resp("5568.202", 200, 4_048);
        chat_response.origin = "https://chat.deepseek.com".to_string();
        chat_response.mime_type = "text/event-stream".to_string();
        let log = NetworkLog {
            requests: vec![document, chat_request, static_request],
            responses: vec![
                chat_response,
                ObservedResponse {
                    request_id: "5568.300".to_string(),
                    origin: "https://fe-static.deepseek.com".to_string(),
                    status: 200,
                    mime_type: "text/javascript".to_string(),
                    at_ms: 9_004,
                },
            ],
            finished: vec![ObservedFinish {
                request_id: "5568.202".to_string(),
                at_ms: 5_385,
                encoded_bytes: 12_345,
            }],
            total_requests: 3,
            total_responses: 2,
            total_finished: 1,
            ..NetworkLog::default()
        };
        match log.verdict() {
            TurnNetworkVerdict::ProviderResponse {
                request_id,
                origin,
                status,
                mime_type,
                duration_ms,
                finished,
                selection_reason,
            } => {
                assert_eq!(request_id, "5568.202");
                assert_eq!(origin, "https://chat.deepseek.com");
                assert_eq!(status, 200);
                assert_eq!(mime_type, "text/event-stream");
                assert_eq!(duration_ms, 1_676);
                assert!(finished);
                assert!(selection_reason.contains("SSE-Antwort"));
                assert!(selection_reason.contains("Origin der Dokumentseite"));
            }
            other => panic!("erwartete ProviderResponse, erhalten {other:?}"),
        }
    }

    #[test]
    fn t940_bounded_log_counts_dropped_events() {
        let mut state = log().lock().unwrap();
        *state = NetworkLog::default();
        drop(state);
        reset();
        for index in 0..(MAX_EVENTS as u64 + 8) {
            record_request(
                format!("r{index}"),
                "POST",
                "https://api.example.test",
                "XHR",
                index as u128,
            );
        }
        let state = snapshot();
        assert_eq!(state.requests.len(), MAX_EVENTS);
        assert_eq!(state.dropped_events, 8);
        assert_eq!(state.total_requests, MAX_EVENTS as u64 + 8);
    }
}
