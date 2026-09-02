//! Minimaler HTTPS-Client (T-601): HTTP/1.1 über tokio-rustls + webpki-roots.
//!
//! Kein `reqwest`, kein Hyper, kein System-OpenSSL. Keys in `providers.json`
//! nur als Umgebungsvariablen-Namen.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Eine manuelle API-Quelle fuer einen Brain (oder `default` = Browser).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSource {
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub base_url_env: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
}

fn default_source() -> String {
    "default".to_string()
}

impl Default for ProviderSource {
    fn default() -> Self {
        Self {
            source: default_source(),
            base_url_env: None,
            api_key_env: None,
            model: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProvidersFile {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub brains: BTreeMap<String, ProviderSource>,
}

impl ProvidersFile {
    pub fn load_path(path: &Path) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("providers.json nicht lesbar: {e}"))?;
        let parsed: ProvidersFile =
            serde_json::from_str(&raw).map_err(|e| format!("providers.json ungueltig: {e}"))?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn load_default() -> Result<Self, String> {
        let path = crate::config::data_dir().join("providers.json");
        if !path.exists() {
            return Ok(Self {
                version: 1,
                brains: BTreeMap::new(),
            });
        }
        Self::load_path(&path)
    }

    fn validate(&self) -> Result<(), String> {
        for (brain, src) in &self.brains {
            if src.source.trim().is_empty() {
                return Err(format!("Brain {brain}: leere Quelle"));
            }
            if let Some(key) = &src.api_key_env {
                if key.contains("sk-") || key.len() > 80 {
                    return Err(format!(
                        "Brain {brain}: api_key_env sieht nach Klartext-Secret aus, nur Env-Name erlaubt"
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn source_of(&self, brain: &str) -> ProviderSource {
        self.brains.get(brain).cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpsResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// GET über TLS 1.2+ mit webpki-roots.
pub fn https_get(url: &str) -> Result<HttpsResponse, String> {
    https_request("GET", url, None, &[])
}

/// POST `application/json`.
pub fn https_post_json(url: &str, body: &[u8]) -> Result<HttpsResponse, String> {
    https_request(
        "POST",
        url,
        Some(body),
        &[("content-type", "application/json")],
    )
}

fn https_request(
    method: &str,
    url: &str,
    body: Option<&[u8]>,
    extra: &[(&str, &str)],
) -> Result<HttpsResponse, String> {
    let (host, port, path) = split_https_url(url)?;
    let server_name = rustls::pki_types::ServerName::try_from(host.clone())
        .map_err(|e| format!("TLS-Servername: {e}"))?;
    let config = client_config()?;
    let connector = tokio_rustls::TlsConnector::from(config);

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    rt.block_on(async {
        let stream = tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio::net::TcpStream::connect((host.as_str(), port)),
        )
        .await
        .map_err(|_| format!("Connect-Timeout {host}:{port}"))?
        .map_err(|e| format!("Connect {host}:{port}: {e}"))?;
        let mut tls = connector
            .connect(server_name, stream)
            .await
            .map_err(|e| format!("TLS-Handshake: {e}"))?;
        let mut req = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nUser-Agent: webagent/0.11\r\nConnection: close\r\nAccept: */*\r\n"
        );
        for (k, v) in extra {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        if let Some(b) = body {
            req.push_str(&format!("Content-Length: {}\r\n", b.len()));
        }
        req.push_str("\r\n");
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tls.write_all(req.as_bytes())
            .await
            .map_err(|e| format!("Request schreiben: {e}"))?;
        if let Some(b) = body {
            tls.write_all(b)
                .await
                .map_err(|e| format!("Body schreiben: {e}"))?;
        }
        tls.flush().await.map_err(|e| format!("Flush: {e}"))?;
        let mut buf = Vec::new();
        tokio::time::timeout(READ_TIMEOUT, tls.read_to_end(&mut buf))
            .await
            .map_err(|_| "Lese-Timeout".to_string())?
            .map_err(|e| format!("Lesen: {e}"))?;
        parse_http_response(&buf)
    })
}

fn client_config() -> Result<Arc<rustls::ClientConfig>, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let cfg = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

pub(crate) fn split_https_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "Nur https:// wird unterstuetzt.".to_string())?;
    let (hostport, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match hostport.split_once(':') {
        Some((h, p)) => (
            h.to_string(),
            p.parse::<u16>()
                .map_err(|_| format!("Ungueltiger Port: {p}"))?,
        ),
        None => (hostport.to_string(), 443),
    };
    if host.is_empty() {
        return Err("Host fehlt.".to_string());
    }
    Ok((host, port, path))
}

pub(crate) fn parse_http_response(raw: &[u8]) -> Result<HttpsResponse, String> {
    let text = std::str::from_utf8(raw).map_err(|_| "Antwort ist nicht UTF-8.".to_string())?;
    let (head, body) = text
        .split_once("\r\n\r\n")
        .or_else(|| text.split_once("\n\n"))
        .ok_or_else(|| "HTTP-Kopf fehlt.".to_string())?;
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| format!("Statuszeile unlesbar: {status_line}"))?;
    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    Ok(HttpsResponse {
        status,
        headers,
        body: body.as_bytes().to_vec(),
    })
}

/// Release-Artefakt muss unter 10 MB bleiben (Plan Phase 6).
pub fn release_binary_within_budget(path: &Path) -> Result<u64, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Binary nicht lesbar: {e}"))?;
    let n = meta.len();
    if n >= 10 * 1024 * 1024 {
        return Err(format!("Release-Binary {n} Bytes liegt nicht unter 10 MB"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn cargo_toml_hat_rustls_und_kein_reqwest() {
        let toml = include_str!("../Cargo.toml");
        assert!(toml.contains("tokio-rustls"));
        assert!(toml.contains("webpki-roots"));
        let dep_lines: Vec<&str> = toml
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .collect();
        let joined = dep_lines.join("\n");
        assert!(!joined.contains("reqwest"));
        assert!(!joined.contains("native-tls"));
        assert!(!joined.contains("openssl-sys"));
    }

    #[test]
    fn split_https_url_host_path_port() {
        let (h, p, path) = split_https_url("https://example.com/v1/models").unwrap();
        assert_eq!(h, "example.com");
        assert_eq!(p, 443);
        assert_eq!(path, "/v1/models");
        let (h, p, path) = split_https_url("https://127.0.0.1:8443").unwrap();
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 8443);
        assert_eq!(path, "/");
        assert!(split_https_url("http://x").is_err());
    }

    #[test]
    fn parse_http_response_status_und_body() {
        let raw = b"HTTP/1.1 204 No Content\r\nX-Test: a\r\n\r\n";
        let r = parse_http_response(raw).unwrap();
        assert_eq!(r.status, 204);
        assert_eq!(r.headers[0].0, "x-test");
        assert!(r.body.is_empty());
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nhi";
        let r = parse_http_response(raw).unwrap();
        assert_eq!(r.status, 200);
        assert_eq!(r.body, b"hi");
    }

    #[test]
    fn providers_json_nur_env_namen_keine_secrets() {
        let dir = std::env::temp_dir().join(format!("wa-prov-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("providers.json");
        let mut f = std::fs::File::create(&path).unwrap();
        write!(
            f,
            r#"{{"version":1,"brains":{{"claude":{{"source":"default"}},"chatgpt":{{"source":"openrouter","api_key_env":"OPENROUTER_API_KEY","base_url_env":"OPENROUTER_URL"}}}}}}"#
        )
        .unwrap();
        let file = ProvidersFile::load_path(&path).unwrap();
        assert_eq!(file.source_of("claude").source, "default");
        assert_eq!(
            file.source_of("chatgpt").api_key_env.as_deref(),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(file.source_of("missing").source, "default");

        let bad = dir.join("bad.json");
        let mut f = std::fs::File::create(&bad).unwrap();
        write!(
            f,
            r#"{{"brains":{{"x":{{"source":"api","api_key_env":"sk-this-looks-like-a-plaintext-secret-value-xxxxxxxx"}}}}}}"#
        )
        .unwrap();
        assert!(ProvidersFile::load_path(&bad).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn release_budget_prueft_vorhandenes_artefakt() {
        for name in ["webagent.exe", "webagent"] {
            let p = Path::new("target/release").join(name);
            if p.exists() {
                let n = release_binary_within_budget(&p).unwrap();
                assert!(n > 0);
            }
        }
    }
}
