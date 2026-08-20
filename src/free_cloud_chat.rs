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

/// Versionierte Startregistry. Sie enthält absichtlich nur konservative
/// Einträge; Inference-Provider werden nicht als gratis beworben.
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
        "{} {} {} {} {}",
        model.model_id,
        model.display_name,
        model.provider,
        model.tags.join(" "),
        format!("{} {}", model.languages.join(" "), model.source_url)
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
}
