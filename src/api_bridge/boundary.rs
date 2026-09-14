//! Auth- und Fehlervertrag der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Dieses Modul kapselt Tokenpruefung und provider-spezifische Fehlerkoerper.
//! Es lockert keine Sicherheitssemantik und erfindet keine neuen Auth-Arten
//! oder Fehlermeldungstexte. T-913 verdrahtet `mod boundary`.
//!
//! Invarianten:
//! - Gueltig: `Authorization: Bearer <token>` oder `x-api-key: <token>`.
//! - Vergleich ist timing-sicher und laengenempfindlich.
//! - OpenAI-Fehler: `{"error":{"message","type","param","code"}}`.
//! - Anthropic-Fehler: `{"type":"error","error":{"type","message"}}`.
//! - 401-Text bleibt: `Ungueltiger oder fehlender API-Token.`

use super::{ApiFlavor, BridgeConfig, HttpResponse};
use serde_json::json;
use std::collections::BTreeMap;

/// Bearer- oder x-api-key-Pruefung gegen den konfigurierten Token.
pub fn authorize(
    headers: &BTreeMap<String, String>,
    config: &BridgeConfig,
    flavor: ApiFlavor,
) -> Result<(), HttpResponse> {
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "));
    let provided = bearer.or_else(|| headers.get("x-api-key").map(String::as_str));
    if provided.is_some_and(|token| constant_time_equal(token, &config.api_key)) {
        return Ok(());
    }
    Err(api_error(
        flavor,
        401,
        "Ungueltiger oder fehlender API-Token.",
    ))
}

/// Timing-sicherer Vergleich gleicher Laenge.
pub fn constant_time_equal(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.as_bytes().iter().zip(right.as_bytes()) {
        difference |= a ^ b;
    }
    difference == 0
}

/// Provider-spezifischer Fehlerkoerper.
pub fn api_error(flavor: ApiFlavor, status: u16, message: &str) -> HttpResponse {
    api_error_with(flavor, status, message, None, None)
}

pub fn api_error_code(status: u16, message: &str, param: &str, code: &str) -> HttpResponse {
    api_error_with(ApiFlavor::OpenAi, status, message, Some(param), Some(code))
}

/// Ein unbekannter Modellname ist in beiden APIs ein 404 mit `model_not_found`,
/// kein 400. Clients unterscheiden daran einen Tippfehler im Modellnamen von
/// einer strukturell kaputten Anfrage und koennen den Katalog neu laden,
/// statt die Anfrage als unrettbar zu verwerfen.
pub fn model_not_found(flavor: ApiFlavor, message: &str) -> HttpResponse {
    api_error_with(flavor, 404, message, Some("model"), Some("model_not_found"))
}

pub fn api_error_with(
    flavor: ApiFlavor,
    status: u16,
    message: &str,
    param: Option<&str>,
    code: Option<&str>,
) -> HttpResponse {
    let body = match flavor {
        ApiFlavor::OpenAi => json!({
            "error": {
                "message": message,
                "type": "invalid_request_error",
                "param": param,
                "code": code
            }
        }),
        ApiFlavor::Anthropic => json!({
            "type": "error",
            "error": {"type": "invalid_request_error", "message": message}
        }),
    };
    HttpResponse::json(status, body)
}

#[cfg(test)]
mod tests {
    use super::{api_error, authorize, constant_time_equal, ApiFlavor, BridgeConfig};
    use std::collections::BTreeMap;

    fn test_config() -> BridgeConfig {
        BridgeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            brain: "chatgpt".to_string(),
            timeout_secs: None,
            headless: true,
            api_key: "test-secret".to_string(),
            fake_reply: None,
        }
    }

    #[test]
    fn timing_safe_comparison_requires_equal_content() {
        assert!(constant_time_equal("same", "same"));
        assert!(!constant_time_equal("same", "diff"));
        assert!(!constant_time_equal("short", "longer"));
    }

    #[test]
    fn api_error_uses_provider_specific_shapes() {
        let openai = String::from_utf8(api_error(ApiFlavor::OpenAi, 401, "x").body).unwrap();
        let anthropic = String::from_utf8(api_error(ApiFlavor::Anthropic, 401, "x").body).unwrap();
        assert!(openai.contains("\"error\":{"));
        assert!(anthropic.contains("\"type\":\"error\""));
    }

    #[test]
    fn accepts_bearer_and_x_api_key_header() {
        let config = test_config();
        let x_api = BTreeMap::from([("x-api-key".to_string(), "test-secret".to_string())]);
        let bearer = BTreeMap::from([(
            "authorization".to_string(),
            "Bearer test-secret".to_string(),
        )]);
        let wrong = BTreeMap::from([("x-api-key".to_string(), "nope".to_string())]);
        assert!(authorize(&x_api, &config, ApiFlavor::OpenAi).is_ok());
        assert!(authorize(&bearer, &config, ApiFlavor::Anthropic).is_ok());
        assert!(authorize(&wrong, &config, ApiFlavor::OpenAi).is_err());
        assert!(authorize(&BTreeMap::new(), &config, ApiFlavor::OpenAi).is_err());
    }
}
