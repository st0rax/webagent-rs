//! Sichere Registry- und Routing-Grundlage für optionale kostenlose Cloud-Textchats.
//!
//! Dieses Modul führt keine Inferenz aus. Es verwaltet nur transparente
//! Metadaten, Suchergebnisse und Free-only-Entscheidungen. Dadurch kann die
//! Oberfläche Quellen erklären, ohne kostenpflichtige Provider, Browser-
//! Automation oder ungeprüfte Gradio-Spaces automatisch zu verwenden.

use serde::{Deserialize, Serialize};

/// Version des serialisierbaren Registry-Schemas.
pub const REGISTRY_SCHEMA_VERSION: u32 = 1;

/// Nutzersicht auf den gewünschten Modelltyp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProfile {
    Auto,
    Fast,
    German,
    ReasoningCode,
    Creative,
    Privacy,
    Custom,
}

impl ModelProfile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Fast => "Schnell",
            Self::German => "Deutsch",
            Self::ReasoningCode => "Reasoning/Code",
            Self::Creative => "Kreativ",
            Self::Privacy => "Datensparsam",
            Self::Custom => "Custom",
        }
    }
}

/// Kosten- und Ausführungsstatus eines Eintrags.
///
/// `VerifiedFree` ist absichtlich streng: erst ein wiederholter technischer
/// Nachweis kann einen Eintrag in den automatischen Free-only-Router bringen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    VerifiedFree,
    ExplicitCredits,
    ManualOnly,
    Unavailable,
}

impl AccessMode {
    pub fn auto_routable_in_free_only(self) -> bool {
        matches!(self, Self::VerifiedFree)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::VerifiedFree => "verifiziert kostenlos",
            Self::ExplicitCredits => "nur nach ausdrücklicher Kredit-/Token-Freigabe",
            Self::ManualOnly => "nur manuell öffnen",
            Self::Unavailable => "derzeit nicht verfügbar",
        }
    }
}

/// Normalisierte Metadaten eines Modells, Providers oder öffentlichen Spaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudModel {
    pub model_id: String,
    pub display_name: String,
    pub provider: String,
    pub source_url: String,
    pub profiles: Vec<ModelProfile>,
    pub languages: Vec<String>,
    pub tags: Vec<String>,
    pub access: AccessMode,
    pub adapter_compatible: bool,
    pub last_verified_at: Option<String>,
}

impl CloudModel {
    pub fn manual_only(&self, free_only: bool) -> bool {
        !self.adapter_compatible || (free_only && !self.access.auto_routable_in_free_only())
    }
}

/// Erklärbares Suchergebnis für UI und Audit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub model: CloudModel,
    pub score: u32,
    pub reasons: Vec<String>,
    pub manual_only: bool,
}

/// Ein Inhalt wird nur bei `Auto` an einen Adapter übergeben.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub enum RouteDecision {
    Auto { model_id: String },
    ManualOnly { model_id: String, reason: String },
    Unavailable { model_id: String, reason: String },
}
/// Stable identifier for the in-process reference adapter. It is deliberately
/// not part of the public cloud registry and cannot be mistaken for a provider.
pub const DETERMINISTIC_MOCK_MODEL_ID: &str = "webagent/local-mock-stream";

/// A normalized input to a text-stream adapter. The model is explicit so every
/// adapter must honor the central routing decision before emitting data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextStreamRequest {
    pub model: CloudModel,
    pub prompt: String,
    pub free_only: bool,
}

/// Provider-neutral events for the first textchat contract slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "event")]
pub enum TextStreamEvent {
    Started { model_id: String },
    Token { text: String },
    Metadata { metadata: HubModelMetadata },
    Completed { model_id: String },
}

/// Explicit adapter failure. A denied route never produces partial events.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "error")]
pub enum TextStreamError {
    RouteDenied {
        decision: RouteDecision,
    },
    InvalidRequest {
        reason: String,
    },
    AdapterFailure {
        adapter: String,
        reason: String,
    },
    StaleMetadata {
        model_id: String,
        last_verified_at: i64,
        now_unix_seconds: i64,
        max_age_seconds: i64,
    },
}

impl std::fmt::Display for TextStreamError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RouteDenied { decision } => write!(formatter, "route denied: {decision:?}"),
            Self::InvalidRequest { reason } => {
                write!(formatter, "invalid text stream request: {reason}")
            }
            Self::AdapterFailure { adapter, reason } => {
                write!(formatter, "{adapter} adapter failed: {reason}")
            }
            Self::StaleMetadata {
                model_id,
                last_verified_at,
                now_unix_seconds,
                max_age_seconds,
            } => write!(
                formatter,
                "metadata for {model_id} is stale: verified at {last_verified_at}, now {now_unix_seconds}, maximum age {max_age_seconds} seconds"
            ),
        }
    }
}

impl std::error::Error for TextStreamError {}

/// Contract that every future external adapter must implement before use.
pub trait TextStreamAdapter {
    fn stream(&self, request: &TextStreamRequest) -> Result<Vec<TextStreamEvent>, TextStreamError>;
}

/// Deterministic local reference adapter for contract, CLI and regression tests.
/// It cannot access a network, browser, credential, or third-party model.
#[derive(Debug, Default, Clone, Copy)]
pub struct DeterministicMockAdapter;

/// Returns the only model accepted by the local reference adapter.
pub fn deterministic_mock_model() -> CloudModel {
    CloudModel {
        model_id: DETERMINISTIC_MOCK_MODEL_ID.to_string(),
        display_name: "WebAgent local deterministic mock".to_string(),
        provider: "WebAgent local test adapter".to_string(),
        source_url: "local://webagent/mock-stream".to_string(),
        profiles: vec![ModelProfile::Auto, ModelProfile::Custom],
        languages: vec!["de".to_string(), "en".to_string()],
        tags: vec![
            "local".to_string(),
            "deterministic".to_string(),
            "mock".to_string(),
        ],
        access: AccessMode::VerifiedFree,
        adapter_compatible: true,
        last_verified_at: Some("local-contract".to_string()),
    }
}

impl TextStreamAdapter for DeterministicMockAdapter {
    fn stream(&self, request: &TextStreamRequest) -> Result<Vec<TextStreamEvent>, TextStreamError> {
        if request.prompt.trim().is_empty() {
            return Err(TextStreamError::InvalidRequest {
                reason: "prompt must not be empty".to_string(),
            });
        }
        let decision = decide_route(&request.model, request.free_only);
        if !matches!(decision, RouteDecision::Auto { .. }) {
            return Err(TextStreamError::RouteDenied { decision });
        }
        Ok(vec![
            TextStreamEvent::Started {
                model_id: request.model.model_id.clone(),
            },
            TextStreamEvent::Token {
                text: format!("Local mock reply: {}", request.prompt.trim()),
            },
            TextStreamEvent::Completed {
                model_id: request.model.model_id.clone(),
            },
        ])
    }
}

/// Convenience entry point for the CLI demo and integration tests.
pub fn stream_deterministic_mock(prompt: &str) -> Result<Vec<TextStreamEvent>, TextStreamError> {
    let model = deterministic_mock_model();
    DeterministicMockAdapter.stream(&TextStreamRequest {
        model,
        prompt: prompt.to_string(),
        free_only: true,
    })
}

/// Versionierte Startregistry. Sie enthält absichtlich nur konservative
/// Einträge; Inference-Provider werden nicht als gratis beworben.
/// Schema for the local-only Hub metadata transport contract.
pub const HUB_METADATA_ADAPTER_SCHEMA_VERSION: u32 = 1;
/// Stable identity of the adapter capability, separate from Hub model IDs.
pub const HUB_METADATA_ADAPTER_MODEL_ID: &str = "huggingface/hub-metadata";
/// A conservative freshness window used by the metadata adapter.
pub const DEFAULT_HUB_METADATA_MAX_AGE_SECONDS: i64 = 86_400;

/// The metadata transport is public by default. A later credential-capable
/// transport must receive an explicit opt-in and still owns no token here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HubMetadataAccess {
    #[default]
    PublicOnly,
    UserCredentialOptIn,
}

/// Input for a versioned Hub metadata transport. It carries an access policy,
/// never a credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubMetadataRequest {
    pub schema_version: u32,
    pub model_id: String,
    pub access: HubMetadataAccess,
}

/// Normalized Hub metadata returned by an injected transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HubModelMetadata {
    pub schema_version: u32,
    pub model_id: String,
    pub revision: Option<String>,
    pub pipeline_tag: Option<String>,
    pub tags: Vec<String>,
    /// Unix seconds at which this exact metadata record was last verified.
    pub last_verified_at: i64,
}

/// Failure returned by a transport seam. It intentionally does not expose a
/// network implementation in this local-only slice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "error")]
pub enum HubMetadataTransportError {
    Unavailable { reason: String },
    Malformed { reason: String },
}

impl std::fmt::Display for HubMetadataTransportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable { reason } => write!(formatter, "metadata unavailable: {reason}"),
            Self::Malformed { reason } => write!(formatter, "malformed metadata: {reason}"),
        }
    }
}

impl std::error::Error for HubMetadataTransportError {}

/// Transport seam for future Hub metadata retrieval. The current slice provides
/// only the in-memory fixture below; it has no HTTP, browser, or credential use.
pub trait HubMetadataTransport {
    fn fetch(
        &self,
        request: &HubMetadataRequest,
    ) -> Result<HubModelMetadata, HubMetadataTransportError>;
}

/// In-memory fixture transport for deterministic local tests and CLI evidence.
#[derive(Debug, Clone)]
pub struct StaticHubMetadataTransport {
    metadata: HubModelMetadata,
}

impl StaticHubMetadataTransport {
    pub fn new(metadata: HubModelMetadata) -> Self {
        Self { metadata }
    }
}

impl HubMetadataTransport for StaticHubMetadataTransport {
    fn fetch(
        &self,
        request: &HubMetadataRequest,
    ) -> Result<HubModelMetadata, HubMetadataTransportError> {
        if request.schema_version != HUB_METADATA_ADAPTER_SCHEMA_VERSION {
            return Err(HubMetadataTransportError::Malformed {
                reason: format!(
                    "unsupported request schema version {}",
                    request.schema_version
                ),
            });
        }
        Ok(self.metadata.clone())
    }
}

/// Versioned Hub metadata adapter. Routing, freshness and schema checks happen
/// before any text event is emitted. The transport is injected for testability.
#[derive(Debug, Clone)]
pub struct HubMetadataAdapter<T> {
    transport: T,
    now_unix_seconds: i64,
    max_age_seconds: i64,
    access: HubMetadataAccess,
}

impl<T> HubMetadataAdapter<T> {
    pub fn new(transport: T, now_unix_seconds: i64, max_age_seconds: i64) -> Self {
        Self {
            transport,
            now_unix_seconds,
            max_age_seconds,
            access: HubMetadataAccess::PublicOnly,
        }
    }

    /// This opt-in selects a future transport policy but cannot inject or store
    /// credentials in the adapter contract itself.
    pub fn with_user_credential_opt_in(mut self) -> Self {
        self.access = HubMetadataAccess::UserCredentialOptIn;
        self
    }
}

impl<T: HubMetadataTransport> HubMetadataAdapter<T> {
    /// Fetches and validates a metadata snapshot without emitting stream events.
    pub fn fetch_metadata(
        &self,
        request: &TextStreamRequest,
    ) -> Result<HubModelMetadata, TextStreamError> {
        let decision = decide_route(&request.model, request.free_only);
        if !matches!(decision, RouteDecision::Auto { .. }) {
            return Err(TextStreamError::RouteDenied { decision });
        }
        if request.model.model_id != HUB_METADATA_ADAPTER_MODEL_ID {
            return Err(TextStreamError::InvalidRequest {
                reason: format!(
                    "Hub metadata adapter requires model {}",
                    HUB_METADATA_ADAPTER_MODEL_ID
                ),
            });
        }
        let model_id = request.prompt.trim();
        if model_id.is_empty() {
            return Err(TextStreamError::InvalidRequest {
                reason: "Hub model id must not be empty".to_string(),
            });
        }
        if self.max_age_seconds <= 0 {
            return Err(TextStreamError::InvalidRequest {
                reason: "metadata max age must be positive".to_string(),
            });
        }

        let metadata = self
            .transport
            .fetch(&HubMetadataRequest {
                schema_version: HUB_METADATA_ADAPTER_SCHEMA_VERSION,
                model_id: model_id.to_string(),
                access: self.access,
            })
            .map_err(|error| TextStreamError::AdapterFailure {
                adapter: HUB_METADATA_ADAPTER_MODEL_ID.to_string(),
                reason: error.to_string(),
            })?;

        if metadata.schema_version != HUB_METADATA_ADAPTER_SCHEMA_VERSION {
            return Err(TextStreamError::AdapterFailure {
                adapter: HUB_METADATA_ADAPTER_MODEL_ID.to_string(),
                reason: format!(
                    "unsupported metadata schema version {}",
                    metadata.schema_version
                ),
            });
        }
        if metadata.model_id != model_id {
            return Err(TextStreamError::AdapterFailure {
                adapter: HUB_METADATA_ADAPTER_MODEL_ID.to_string(),
                reason: format!(
                    "metadata model id {} does not match requested model {model_id}",
                    metadata.model_id
                ),
            });
        }
        if metadata.last_verified_at > self.now_unix_seconds {
            return Err(TextStreamError::AdapterFailure {
                adapter: HUB_METADATA_ADAPTER_MODEL_ID.to_string(),
                reason: "metadata verification timestamp is in the future".to_string(),
            });
        }
        if self
            .now_unix_seconds
            .saturating_sub(metadata.last_verified_at)
            > self.max_age_seconds
        {
            return Err(TextStreamError::StaleMetadata {
                model_id: metadata.model_id.clone(),
                last_verified_at: metadata.last_verified_at,
                now_unix_seconds: self.now_unix_seconds,
                max_age_seconds: self.max_age_seconds,
            });
        }
        Ok(metadata)
    }
}

impl<T: HubMetadataTransport> TextStreamAdapter for HubMetadataAdapter<T> {
    fn stream(&self, request: &TextStreamRequest) -> Result<Vec<TextStreamEvent>, TextStreamError> {
        let metadata = self.fetch_metadata(request)?;
        Ok(vec![
            TextStreamEvent::Started {
                model_id: request.model.model_id.clone(),
            },
            TextStreamEvent::Metadata { metadata },
            TextStreamEvent::Completed {
                model_id: request.model.model_id.clone(),
            },
        ])
    }
}

/// Conservative capability record for the local metadata adapter contract.
pub fn hub_metadata_adapter_model() -> CloudModel {
    CloudModel {
        model_id: HUB_METADATA_ADAPTER_MODEL_ID.to_string(),
        display_name: "Hugging Face Hub metadata adapter".to_string(),
        provider: "WebAgent local metadata contract".to_string(),
        source_url: "local://webagent/hub-metadata".to_string(),
        profiles: vec![ModelProfile::Auto, ModelProfile::Custom],
        languages: vec!["metadata".to_string()],
        tags: vec![
            "local".to_string(),
            "hub".to_string(),
            "metadata".to_string(),
            "fixture".to_string(),
        ],
        access: AccessMode::VerifiedFree,
        adapter_compatible: true,
        last_verified_at: Some("local-contract".to_string()),
    }
}

/// Deterministic local Hub metadata fixture. It is not a claim about a live Hub
/// record and does not contact a provider.
pub fn stream_hub_metadata_fixture(
    model_id: &str,
) -> Result<Vec<TextStreamEvent>, TextStreamError> {
    const FIXTURE_TIMESTAMP: i64 = 1_725_000_000;
    let metadata = HubModelMetadata {
        schema_version: HUB_METADATA_ADAPTER_SCHEMA_VERSION,
        model_id: model_id.trim().to_string(),
        revision: Some("local-fixture".to_string()),
        pipeline_tag: Some("text-generation".to_string()),
        tags: vec!["fixture".to_string(), "metadata-only".to_string()],
        last_verified_at: FIXTURE_TIMESTAMP,
    };
    HubMetadataAdapter::new(
        StaticHubMetadataTransport::new(metadata),
        FIXTURE_TIMESTAMP,
        DEFAULT_HUB_METADATA_MAX_AGE_SECONDS,
    )
    .stream(&TextStreamRequest {
        model: hub_metadata_adapter_model(),
        prompt: model_id.to_string(),
        free_only: true,
    })
}
pub fn default_registry() -> Vec<CloudModel> {
    vec![
        CloudModel {
            model_id: "huggingchat/catalog".to_string(),
            display_name: "HuggingChat-Modellkatalog".to_string(),
            provider: "Hugging Face".to_string(),
            source_url: "https://huggingface.co/chat".to_string(),
            profiles: vec![
                ModelProfile::Fast,
                ModelProfile::German,
                ModelProfile::ReasoningCode,
                ModelProfile::Creative,
            ],
            languages: vec![
                "de".to_string(),
                "en".to_string(),
                "multilingual".to_string(),
            ],
            tags: vec![
                "huggingchat".to_string(),
                "catalog".to_string(),
                "open-models".to_string(),
            ],
            access: AccessMode::ManualOnly,
            adapter_compatible: false,
            last_verified_at: None,
        },
        CloudModel {
            model_id: "huggingface/inference-providers".to_string(),
            display_name: "Hugging Face Inference Providers".to_string(),
            provider: "Hugging Face".to_string(),
            source_url: "https://huggingface.co/docs/inference-providers/en/pricing".to_string(),
            profiles: vec![
                ModelProfile::Fast,
                ModelProfile::German,
                ModelProfile::ReasoningCode,
                ModelProfile::Creative,
            ],
            languages: vec!["multilingual".to_string()],
            tags: vec![
                "api".to_string(),
                "chat-completions".to_string(),
                "credits".to_string(),
            ],
            access: AccessMode::ExplicitCredits,
            adapter_compatible: true,
            last_verified_at: None,
        },
        CloudModel {
            model_id: "huggingface/spaces".to_string(),
            display_name: "Öffentliche Hugging-Face-Gradio-Spaces".to_string(),
            provider: "Hugging Face Spaces".to_string(),
            source_url: "https://huggingface.co/spaces".to_string(),
            profiles: vec![ModelProfile::Custom, ModelProfile::Creative],
            languages: vec!["unknown".to_string()],
            tags: vec![
                "gradio".to_string(),
                "space".to_string(),
                "manual-review".to_string(),
            ],
            access: AccessMode::ManualOnly,
            adapter_compatible: false,
            last_verified_at: None,
        },
    ]
}

/// Sucht ausschließlich in lokalen, normalisierten Metadaten.
pub fn search_registry(
    registry: &[CloudModel],
    profile: ModelProfile,
    query: &str,
    free_only: bool,
) -> Vec<SearchResult> {
    let terms = normalized_terms(query);
    let mut results = registry
        .iter()
        .filter(|model| profile_matches(model, profile))
        .map(|model| score_model(model, &terms, free_only))
        .filter(|result| {
            terms.is_empty()
                || result
                    .reasons
                    .iter()
                    .any(|reason| reason.starts_with("Treffer für Suchbegriff"))
        })
        .collect::<Vec<_>>();

    results.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.model.display_name.cmp(&right.model.display_name))
    });
    results
}

/// Erzwingt die Free-only-Grenze an einer zentralen Stelle.
pub fn decide_route(model: &CloudModel, free_only: bool) -> RouteDecision {
    if model.access == AccessMode::Unavailable {
        return RouteDecision::Unavailable {
            model_id: model.model_id.clone(),
            reason: "Quelle ist derzeit als nicht verfügbar markiert.".to_string(),
        };
    }

    if free_only && !model.access.auto_routable_in_free_only() {
        return RouteDecision::ManualOnly {
            model_id: model.model_id.clone(),
            reason: format!(
                "Free-only-Modus erlaubt keine automatische Ausführung: {}.",
                model.access.label()
            ),
        };
    }

    if !model.adapter_compatible {
        return RouteDecision::ManualOnly {
            model_id: model.model_id.clone(),
            reason: "Quelle besitzt keinen explizit freigegebenen Adapter.".to_string(),
        };
    }

    RouteDecision::Auto {
        model_id: model.model_id.clone(),
    }
}

fn normalized_terms(query: &str) -> Vec<String> {
    query
        .split(|character: char| !character.is_alphanumeric() && character != '-')
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(|term| term.to_lowercase())
        .collect()
}

fn profile_matches(model: &CloudModel, profile: ModelProfile) -> bool {
    matches!(profile, ModelProfile::Auto | ModelProfile::Custom)
        || model.profiles.contains(&profile)
}

fn score_model(model: &CloudModel, terms: &[String], free_only: bool) -> SearchResult {
    let mut score = 0_u32;
    let mut reasons = Vec::new();
    let haystack = format!(
        "{} {} {} {} {} {}",
        model.model_id,
        model.display_name,
        model.provider,
        model.tags.join(" "),
        model.languages.join(" "),
        model.source_url,
    )
    .to_lowercase();

    for term in terms {
        if haystack.contains(term) {
            score = score.saturating_add(20);
            reasons.push(format!("Treffer für Suchbegriff '{term}'"));
        }
    }

    if terms.is_empty() {
        score = 1;
        reasons.push("kein Suchbegriff: profilbasierte Liste".to_string());
    }

    if model
        .languages
        .iter()
        .any(|language| language == "de" || language == "multilingual")
    {
        score = score.saturating_add(10);
        reasons.push("deutsche oder mehrsprachige Metadaten".to_string());
    }

    if model.adapter_compatible {
        score = score.saturating_add(10);
        reasons.push("dokumentierter Adapterpfad".to_string());
    }

    if model.access.auto_routable_in_free_only() {
        score = score.saturating_add(10);
        reasons.push("kostenfreier Status ist verifiziert".to_string());
    } else if free_only {
        reasons.push(format!(
            "nicht automatisch geroutet: {}",
            model.access.label()
        ));
    }

    SearchResult {
        model: model.clone(),
        score,
        reasons,
        manual_only: model.manual_only(free_only),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_only_never_auto_routes_credit_limited_provider() {
        let provider = default_registry()
            .into_iter()
            .find(|model| model.model_id == "huggingface/inference-providers")
            .unwrap();

        assert!(matches!(
            decide_route(&provider, true),
            RouteDecision::ManualOnly { .. }
        ));
    }

    #[test]
    fn custom_search_returns_explanations_and_manual_marker() {
        let results = search_registry(
            &default_registry(),
            ModelProfile::Custom,
            "huggingface api",
            true,
        );
        let provider = results
            .iter()
            .find(|result| result.model.model_id == "huggingface/inference-providers")
            .unwrap();

        assert!(provider.manual_only);
        assert!(provider
            .reasons
            .iter()
            .any(|reason| reason.contains("Suchbegriff 'api'")));
        assert!(provider
            .reasons
            .iter()
            .any(|reason| reason.contains("nicht automatisch geroutet")));
    }

    #[test]
    fn verified_free_adapter_is_the_only_free_only_auto_route() {
        let model = CloudModel {
            model_id: "verified/test".to_string(),
            display_name: "Test".to_string(),
            provider: "Test".to_string(),
            source_url: "https://example.invalid".to_string(),
            profiles: vec![ModelProfile::Fast],
            languages: vec!["de".to_string()],
            tags: Vec::new(),
            access: AccessMode::VerifiedFree,
            adapter_compatible: true,
            last_verified_at: Some("2026-08-20T00:00:00Z".to_string()),
        };

        assert!(matches!(
            decide_route(&model, true),
            RouteDecision::Auto { .. }
        ));
    }

    #[test]
    fn safety_adjacent_search_term_is_only_metadata() {
        let results = search_registry(
            &default_registry(),
            ModelProfile::Custom,
            "uncensored",
            true,
        );
        assert!(results.is_empty());
    }
    #[test]
    fn unicode_query_terms_are_preserved() {
        assert_eq!(
            normalized_terms("\u{00dc}bersicht f\u{00fc}r \u{00e4}"),
            vec!["\u{00fc}bersicht", "f\u{00fc}r", "\u{00e4}"]
        );
    }

    #[test]
    fn additional_real_term_matches_keep_a_higher_score() {
        let model = CloudModel {
            model_id: "test/model".to_string(),
            display_name: "Testmodell".to_string(),
            provider: "Test".to_string(),
            source_url: "https://example.invalid/alpha".to_string(),
            profiles: vec![ModelProfile::Custom],
            languages: vec!["de".to_string()],
            tags: vec!["alpha beta gamma".to_string()],
            access: AccessMode::ManualOnly,
            adapter_compatible: false,
            last_verified_at: None,
        };
        let two = score_model(&model, &["alpha".to_string(), "beta".to_string()], true).score;
        let three = score_model(
            &model,
            &["alpha".to_string(), "beta".to_string(), "gamma".to_string()],
            true,
        )
        .score;

        assert!(three > two);
    }
    #[test]
    fn deterministic_mock_streams_only_after_the_free_only_route() {
        let events = stream_deterministic_mock("Hallo Vertrag").unwrap();
        assert_eq!(events.len(), 3);
        assert!(
            matches!(events.first(), Some(TextStreamEvent::Started { model_id }) if model_id == DETERMINISTIC_MOCK_MODEL_ID)
        );
        assert!(
            matches!(events.get(1), Some(TextStreamEvent::Token { text }) if text == "Local mock reply: Hallo Vertrag")
        );
        assert!(
            matches!(events.last(), Some(TextStreamEvent::Completed { model_id }) if model_id == DETERMINISTIC_MOCK_MODEL_ID)
        );
    }

    #[test]
    fn hub_metadata_adapter_streams_a_fresh_mocked_snapshot() {
        let timestamp = 1_725_000_000;
        let model_id = "HuggingFaceH4/zephyr-7b-beta";
        let metadata = HubModelMetadata {
            schema_version: HUB_METADATA_ADAPTER_SCHEMA_VERSION,
            model_id: model_id.to_string(),
            revision: Some("mock-revision".to_string()),
            pipeline_tag: Some("text-generation".to_string()),
            tags: vec!["mocked".to_string()],
            last_verified_at: timestamp,
        };
        let events = HubMetadataAdapter::new(
            StaticHubMetadataTransport::new(metadata.clone()),
            timestamp + 10,
            60,
        )
        .stream(&TextStreamRequest {
            model: hub_metadata_adapter_model(),
            prompt: model_id.to_string(),
            free_only: true,
        })
        .unwrap();

        assert!(
            matches!(events.first(), Some(TextStreamEvent::Started { model_id }) if model_id == HUB_METADATA_ADAPTER_MODEL_ID)
        );
        let TextStreamEvent::Metadata { metadata: returned } = &events[1] else {
            panic!("second event must be Hub metadata");
        };
        assert_eq!(returned, &metadata);
        assert!(
            matches!(events.last(), Some(TextStreamEvent::Completed { model_id }) if model_id == HUB_METADATA_ADAPTER_MODEL_ID)
        );
    }

    #[test]
    fn hub_metadata_adapter_denies_a_credit_limited_route_before_transport() {
        struct PanickingTransport;
        impl HubMetadataTransport for PanickingTransport {
            fn fetch(
                &self,
                _request: &HubMetadataRequest,
            ) -> Result<HubModelMetadata, HubMetadataTransportError> {
                panic!("transport must not run after a denied route");
            }
        }

        let timestamp = 1_725_000_000;
        let mut adapter_model = hub_metadata_adapter_model();
        adapter_model.access = AccessMode::ExplicitCredits;
        let error = HubMetadataAdapter::new(PanickingTransport, timestamp, 60)
            .stream(&TextStreamRequest {
                model: adapter_model,
                prompt: "any/model".to_string(),
                free_only: true,
            })
            .unwrap_err();
        assert!(matches!(
            error,
            TextStreamError::RouteDenied {
                decision: RouteDecision::ManualOnly { .. }
            }
        ));
    }

    #[test]
    fn hub_metadata_adapter_rejects_a_stale_mocked_snapshot_without_events() {
        let timestamp = 1_725_000_000;
        let error = HubMetadataAdapter::new(
            StaticHubMetadataTransport::new(HubModelMetadata {
                schema_version: HUB_METADATA_ADAPTER_SCHEMA_VERSION,
                model_id: "stale/model".to_string(),
                revision: None,
                pipeline_tag: None,
                tags: Vec::new(),
                last_verified_at: timestamp,
            }),
            timestamp + 61,
            60,
        )
        .stream(&TextStreamRequest {
            model: hub_metadata_adapter_model(),
            prompt: "stale/model".to_string(),
            free_only: true,
        })
        .unwrap_err();

        assert!(matches!(error, TextStreamError::StaleMetadata { .. }));
    }

    #[test]
    fn hub_metadata_adapter_handles_extreme_old_timestamps_without_overflow() {
        let error = HubMetadataAdapter::new(
            StaticHubMetadataTransport::new(HubModelMetadata {
                schema_version: HUB_METADATA_ADAPTER_SCHEMA_VERSION,
                model_id: "extreme/model".to_string(),
                revision: None,
                pipeline_tag: None,
                tags: Vec::new(),
                last_verified_at: i64::MIN,
            }),
            i64::MAX,
            60,
        )
        .stream(&TextStreamRequest {
            model: hub_metadata_adapter_model(),
            prompt: "extreme/model".to_string(),
            free_only: true,
        })
        .unwrap_err();
        assert!(matches!(error, TextStreamError::StaleMetadata { .. }));
    }
    #[test]
    fn hub_metadata_adapter_rejects_a_future_verification_timestamp() {
        let error = HubMetadataAdapter::new(
            StaticHubMetadataTransport::new(HubModelMetadata {
                schema_version: HUB_METADATA_ADAPTER_SCHEMA_VERSION,
                model_id: "future/model".to_string(),
                revision: None,
                pipeline_tag: None,
                tags: Vec::new(),
                last_verified_at: i64::MAX,
            }),
            0,
            60,
        )
        .stream(&TextStreamRequest {
            model: hub_metadata_adapter_model(),
            prompt: "future/model".to_string(),
            free_only: true,
        })
        .unwrap_err();
        assert!(matches!(error, TextStreamError::AdapterFailure { .. }));
    }
    #[test]
    fn hub_metadata_adapter_requires_explicit_credential_opt_in() {
        let adapter = HubMetadataAdapter::new(
            StaticHubMetadataTransport::new(HubModelMetadata {
                schema_version: HUB_METADATA_ADAPTER_SCHEMA_VERSION,
                model_id: "any/model".to_string(),
                revision: None,
                pipeline_tag: None,
                tags: Vec::new(),
                last_verified_at: 1_725_000_000,
            }),
            1_725_000_000,
            60,
        );
        assert_eq!(adapter.access, HubMetadataAccess::PublicOnly);
        assert_eq!(
            adapter.with_user_credential_opt_in().access,
            HubMetadataAccess::UserCredentialOptIn
        );
    }
    #[test]
    fn mock_adapter_denies_a_credit_limited_route_before_emitting_events() {
        let mut model = deterministic_mock_model();
        model.access = AccessMode::ExplicitCredits;
        let error = DeterministicMockAdapter
            .stream(&TextStreamRequest {
                model,
                prompt: "must stay local".to_string(),
                free_only: true,
            })
            .unwrap_err();
        assert!(matches!(
            error,
            TextStreamError::RouteDenied {
                decision: RouteDecision::ManualOnly { .. }
            }
        ));
    }

    #[test]
    fn mock_adapter_rejects_an_empty_prompt_without_events() {
        let error = stream_deterministic_mock("   ").unwrap_err();
        assert!(matches!(error, TextStreamError::InvalidRequest { .. }));
    }
}
