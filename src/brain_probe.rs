//! brain_probe — Oberflaechen-Analyse wie die Link-Analyse in JDownloader.
//!
//! Statt fuer jedes Brain die Selektoren von Hand zu pflegen, bekommt webagent
//! eine Chat-URL und klopft die Seite selbst ab: welche Bedienelemente gibt es,
//! welcher Faehigkeit aus [`crate::capability::CATALOG`] gehoert jedes davon,
//! und welcher Selektor trifft es stabil?
//!
//! # Warum das die drei offenen Faehigkeiten loest
//!
//! `deep_research` (kimi) fehlt der Selektor, `mode_switch` (deepseek) fehlt ein
//! nachweisbarer Zustandsmarker. Beides sind Suchaufgaben an der lebenden
//! Oberflaeche — genau das, was hier automatisiert wird, statt es pro Brain
//! einmal von Hand zu machen und beim naechsten UI-Umbau wieder zu verlieren.
//!
//! # Aufbau
//!
//! Das Sammeln braucht einen Browser, das Bewerten nicht. Deshalb dieselbe
//! Trennung wie in [`crate::design_vote`]: [`PROBE_SCRIPT`] holt rohe
//! Kandidaten aus dem DOM, [`classify`] ist reine Rechnung darauf und damit
//! ohne Browser testbar.

use serde::Deserialize;

use crate::page_driver::{PageDriver, Result};

/// Ein im DOM gefundenes Bedienelement, noch ohne Deutung.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Candidate {
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub aria_label: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub test_id: String,
    #[serde(default)]
    pub id: String,
    /// Klassennamen — fuer Discovery von Bedienelementen ohne Text/role (z.B.
    /// kimi's Modell-Picker, ein `div[class*=...]` ohne aria-label).
    #[serde(default)]
    pub class: String,
    /// `title`-Attribut (Tooltip), oft die einzige Beschriftung eines Icon-Buttons.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub contenteditable: bool,
    /// Deaktiviert (`disabled`- oder `aria-disabled`-Attribut). Ein
    /// deaktivierter Knopf ist ein Hinweis, kein Abfall: deepseeks Send-Button
    /// traegt bei leerem Composer `ds-button--disabled` — der Zustand wird
    /// mitgeschrieben, das Element nicht verschwiegen.
    #[serde(default)]
    pub disabled: bool,
    /// Sichtbar und bedienbar — unsichtbare Treffer sind Rauschen.
    #[serde(default)]
    pub visible: bool,
    /// Lage und Größe im Viewport. Nicht als Selektor gedacht, sondern als
    /// **Identität** für Vergleiche zwischen zwei Abzügen: Oberflächen wie
    /// deepseek rendern Dutzende `div[role=button]` mit identischer Klasse und
    /// ohne jedes Label — da ist die Position das Einzige, was sie
    /// unterscheidbar macht. Genau das braucht die Suche nach dem Stop-Knopf,
    /// der sich nur dadurch auszeichnet, dass er während der Generierung
    /// erscheint und danach verschwindet.
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub w: i32,
    #[serde(default)]
    pub h: i32,
}

impl Candidate {
    /// Alle Beschriftungsquellen zusammen, kleingeschrieben.
    ///
    /// Bewusst zusammengefasst: mal steht die Bedeutung im `aria-label`, mal im
    /// sichtbaren Text, mal nur im `data-testid`. Wer nur eine Quelle prueft,
    /// findet je nach Brain die Haelfte nicht.
    fn haystack(&self) -> String {
        // Trennzeichen vereinheitlichen: Maschinen-Kennungen schreiben
        // `new-chat-button` oder `send_message`, Menschen „new chat". Ohne
        // diese Umschrift greift kein einziges mehrwortiges Muster auf einem
        // `data-testid` — chatgpts `data-testid='new-chat-button'` ist genau
        // dieser Fall und war beim ersten Wurf ein stiller Fehlschlag.
        format!(
            "{} {} {} {} {} {} {}",
            self.aria_label,
            self.text,
            self.test_id,
            self.id,
            self.placeholder,
            self.class,
            self.title
        )
        .to_lowercase()
        .replace(['-', '_', '.'], " ")
    }
}

/// Vorschlag: dieser Selektor bedient diese Faehigkeit.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// Schluessel aus [`crate::capability::CATALOG`], z.B. `deep_research`.
    pub capability_key: &'static str,
    /// Schluessel in der Selektor-JSON, z.B. `deep_research_toggle`.
    pub selector_key: &'static str,
    /// Vorgeschlagener Selektor.
    pub selector: String,
    /// 0..100. Ein `data-testid` ist verlaesslicher als ein Textfund.
    pub confidence: u8,
    /// War das Element zum Scan-Zeitpunkt deaktiviert? Ein deaktivierter Knopf
    /// ist kein Nicht-Fund — nur nicht bedienbar, bis der Zustand kippt. Der
    /// Zustand gehoert in den Vorschlag, sonst faellt der Send-Button eines
    /// leeren Composer (deepseek: `ds-button--disabled`) aus jeder Auswertung.
    pub disabled: bool,
    /// Woran es erkannt wurde — fuer die Nachpruefung durch einen Menschen.
    pub evidence: String,
}

/// Ein Suchmuster fuer eine Faehigkeit.
struct Rule {
    capability_key: &'static str,
    selector_key: &'static str,
    /// Treffer in irgendeiner Beschriftungsquelle. **Mehrsprachig**: die
    /// Oberflaechen laufen hier auf Deutsch. Ein rein englisches Muster greift
    /// nie — claudes Stopp-Knopf heisst „Antwort stoppen", nicht „stop".
    needles: &'static [&'static str],
    /// Diese Woerter schliessen einen Treffer aus (Falschfreunde).
    excludes: &'static [&'static str],
    /// Nur Kandidaten mit einer dieser `role`-Werte sind Kandidaten; `None`
    /// = jede Rolle erlaubt. Fuer Menue-Eintraege (`model_option`) zwingend,
    /// sonst matcht z.B. „Projekte" auf die Modell-Option, weil der Teilstring
    /// „pro" im aria-label steckt.
    roles: Option<&'static [&'static str]>,
}

macro_rules! rule {
    ($cap:expr, $key:expr, $needles:expr, $excludes:expr $(,)?) => {
        Rule {
            capability_key: $cap,
            selector_key: $key,
            needles: $needles,
            excludes: $excludes,
            roles: None,
        }
    };
    ($cap:expr, $key:expr, $needles:expr, $excludes:expr, $roles:expr $(,)?) => {
        Rule {
            capability_key: $cap,
            selector_key: $key,
            needles: $needles,
            excludes: $excludes,
            roles: Some($roles),
        }
    };
}

const RULES: &[Rule] = &[
    rule!(
        "chat",
        "send_button",
        &[
            "send",
            "senden",
            "absenden",
            "abschicken",
            "nachricht senden",
            "send prompt",
            "send message",
        ],
        &["sprach", "voice", "audio"],
    ),
    rule!(
        "stop_generation",
        "stop_button",
        &[
            "stop",
            "stopp",
            "stoppen",
            "abbrechen",
            "beenden",
            "antwort stoppen",
            "generierung beenden",
            "stop streaming",
        ],
        &["sprach", "voice"],
    ),
    rule!(
        "new_chat",
        "new_chat_button",
        &[
            "new chat",
            "neuer chat",
            "neue unterhaltung",
            "neuer thread",
            "new conversation",
            "neues gespraech",
            "neues gespräch",
        ],
        &["temporary", "temporaer", "temporär"],
    ),
    rule!(
        "temporary_chat",
        "temporary_chat_button",
        &[
            "temporary chat",
            "temporaerer chat",
            "temporärer chat",
            "temporary",
            "temporaer",
            "temporär",
            "incognito",
            "inkognito",
        ],
        &[],
    ),
    rule!(
        "deep_research",
        "deep_research_toggle",
        &[
            "deep research",
            "deepresearch",
            "tiefe recherche",
            "tiefenrecherche",
            "ausfuehrliche recherche",
            "ausführliche recherche",
        ],
        &[],
    ),
    rule!(
        "web_search",
        "web_search_toggle",
        &[
            "web search",
            "websuche",
            "web-suche",
            "im internet suchen",
            "search the web",
            "internetsuche",
        ],
        &["deep"],
    ),
    rule!(
        "reasoning_toggle",
        "reasoning_toggle",
        &["reason", "denken", "nachdenken", "think", "thinking"],
        &["stufe", "effort", "tiefe"],
    ),
    rule!(
        "regenerate",
        "regenerate_button",
        &[
            "regenerate",
            "neu erzeugen",
            "neu generieren",
            "erneut versuchen",
            "try again",
            "wiederholen",
        ],
        &[],
    ),
    // VOR der allgemeinen Login-Regel: „Anmelden mit Google" enthaelt
    // „anmelden" und wurde deshalb als gewoehnlicher Login-Knopf eingeordnet
    // (von der Gegenprobe gegen selectors/chatgpt.json gefunden). Der
    // Unterschied ist keine Feinheit — wer den SSO-Knopf fuer den Login-Knopf
    // haelt, landet in einem fremden Anmeldedialog statt im Formular.
    rule!(
        "chat",
        "google_sso_button",
        &[
            "mit google",
            "with google",
            "continue with google",
            "weiter mit google",
            "google fortfahren",
            "google-konto",
        ],
        &[],
    ),
    rule!(
        "chat",
        "login_button",
        &[
            "anmelden",
            "log in",
            "sign in",
            "signin",
            "einloggen",
            "login",
            "se connecter",
            "sich anmelden",
        ],
        // Anbieter-Anmeldungen sind eigene Knoepfe, keine Login-Knoepfe.
        &[
            "ab",
            "aus",
            "logout",
            "abmelden",
            "google",
            "apple",
            "microsoft"
        ],
    ),
    rule!(
        "model_switch",
        "model_menu",
        &[
            "modell",
            "model selector",
            "choose model",
            "switch model",
            "modell wählen",
            "change model",
            "model",
        ],
        // „item"/„entry" bezeichnet einen EINTRAG in der Liste, nicht den
        // Knopf, der sie oeffnet. Von der Gegenprobe gegen selectors/zai.json
        // gefunden: `[data-testid='model-item']` wurde als model_menu
        // eingeordnet. Wer den Eintrag fuer den Oeffner haelt, klickt beim
        // Modellwechsel ins Leere, solange das Menue zu ist.
        //
        // Bewusst NUR die zusammengesetzten Muster: das Einzelwort „item"
        // kollidiert mit Tailwinds `items-center`, das im Klassen-Attribut
        // praktisch jedes Buttons steckt — am 2026-08-12 am live Perplexity-
        // DOM gemessen, wo der echte Modell-Knopf deshalb nie als model_menu
        // klassifiziert wurde.
        &[
            "option",
            "model item",
            "model entry",
            "einstellung",
            "vereinbarung",
            "dienste",
            "richtlinie",
            "bedingungen",
            "nutzung",
        ],
    ),
    rule!(
        "model_switch",
        "model_option",
        &[
            "gpt",
            "o3",
            "o4",
            "claude",
            "sonnet",
            "opus",
            "haiku",
            "deepseek",
            "reasoner",
            "kimi",
            "moonshot",
            "mistral",
            "mixtral",
            "gemini",
            "flash",
            "qwen",
            "max",
            "plus",
            "turbo",
            "glm",
            "llama",
            "perplexity",
            "sonar",
        ],
        &[
            "modell wählen",
            "model selector",
            "switch model",
            "change model"
        ],
        &["option", "menuitem", "radio"],
    ),
    rule!(
        "projects",
        "projects_button",
        &[
            "projekte",
            "projects",
            "arbeitsbereiche",
            "workspaces",
            "projektübersicht",
            "projektuebersicht",
        ],
        &[],
    ),
    rule!(
        "file_attach",
        "attach_button",
        &[
            "datei",
            "dateien",
            "anhängen",
            "anhaengen",
            "hinzufügen",
            "hinzufuegen",
            "attach",
            "upload",
            "hochladen",
        ],
        &["tool"],
    ),
    rule!(
        "voice_input",
        "voice_input_button",
        &[
            "mikrofon",
            "spracheingabe",
            "diktiermodus",
            "voice input",
            "diktieren",
        ],
        &[],
    ),
    rule!(
        "voice_mode",
        "voice_mode_button",
        &[
            "sprachmodus",
            "voice mode",
            "sprachdialog",
            "sprachmodus verwenden",
        ],
        &[],
    ),
    rule!(
        // Kein Capability-Key im Katalog: `consent_reject_button` ist ein
        // reiner Dialog-Selektor fuer `dismiss_consent`, kein Level-Faehigkeit.
        // Der capability_key bleibt dennoch ein gueltiges Katalog-Mitglied,
        // damit die Deutung typgleich bleibt.
        "chat",
        "consent_reject_button",
        &[
            "nur notwendige",
            "ablehnen",
            "reject all",
            "tout refuser",
            "nur notwendige cookies",
            "nicht akzeptieren",
        ],
        &["alle", "zulassen", "akzeptieren", "accept"],
    ),
];

/// JS, das die Bedienelemente der Seite einsammelt.
///
/// Sammelt bewusst grob und filtert erst in Rust: was ein Knopf bedeutet,
/// entscheidet sich an Beschriftungen in mehreren Sprachen, und diese Logik
/// gehoert an eine testbare Stelle — nicht in einen String, den kein Test je
/// ausfuehrt.
pub const PROBE_SCRIPT: &str = r#"
(() => {
  const out = [];
  const sel = [
    'button', 'a[href]', '[role=button]', '[role=menuitem]', '[role=option]',
    '[role=switch]', '[role=radio]', '[role=tab]', 'textarea',
    'input[type=text]', '[contenteditable=true]'
  ].join(', ');
  for (const el of document.querySelectorAll(sel)) {
    const r = el.getBoundingClientRect();
    out.push({
      tag: el.tagName.toLowerCase(),
      role: el.getAttribute('role') || '',
      aria_label: el.getAttribute('aria-label') || '',
      text: (el.innerText || el.textContent || '').trim().slice(0, 80),
      placeholder: el.getAttribute('placeholder') || '',
      test_id: el.getAttribute('data-testid') || '',
      id: el.id || '',
      title: el.getAttribute('title') || '',
      class: (typeof el.className === 'string' ? el.className : '').trim().slice(0, 400),
      contenteditable: el.getAttribute('contenteditable') === 'true',
      disabled: el.disabled === true || el.getAttribute('aria-disabled') === 'true',
      visible: r.width > 0 && r.height > 0,
      x: Math.round(r.left),
      y: Math.round(r.top),
      w: Math.round(r.width),
      h: Math.round(r.height)
    });
  }
  return out;
})()
"#;

/// Sammelt TEXTCONTAINER statt Bedienelemente.
///
/// [`PROBE_SCRIPT`] sieht nur Interaktives — `button`, `a[href]`, `role=*`,
/// Eingabefelder. Ein Antwortbereich ist nichts davon, deshalb kann `probe`
/// einen `assistant_message`-Selektor prinzipiell nicht finden, fuer kein
/// Brain. Genau daran scheiterte die Vermessung von perplexity am 2026-08-23:
/// Das Brain antwortete sichtbar, der Scan meldete den Container nie, und der
/// Relay-Lauf wartete bis zum Timeout auf Text, den er nicht sehen konnte.
///
/// Die Heuristik ist bewusst grob: Blattnahe Elemente mit spuerbar Text, ohne
/// interaktive Rolle. Was davon der Antwortcontainer ist, entscheidet ein
/// Mensch beim Lesen der Ausgabe — das Skript liefert die Kandidatenliste,
/// nicht das Urteil.
pub const TEXT_PROBE_SCRIPT: &str = r#"
(() => {
  const out = [];
  const skip = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEXTAREA', 'BUTTON', 'A', 'INPUT']);
  for (const el of document.querySelectorAll('div, p, article, section, main, li, span')) {
    if (skip.has(el.tagName)) { continue; }
    // Nur echte Eingabe- und Knopfflaechen ausschliessen. Ein Antwortbereich
    // liegt bei manchen Oberflaechen INNERHALB eines klickbaren Containers
    // (Perplexity macht Antwortkarten anfassbar) — wer auf `a[href]` oder
    // `[role=button]` filtert, wirft genau den gesuchten Text weg.
    if (el.closest('button, [contenteditable=true]')) { continue; }
    const text = (el.innerText || '').trim();
    // Schwelle bewusst niedrig: Eine Antwort auf eine Testfrage ist KURZ
    // ("BEREIT"). Mit 40 Zeichen Mindestlaenge warf der erste Wurf genau den
    // gesuchten Container weg und zeigte nur die langen eigenen Prompts —
    // eine Suche, die ihr Ziel per Konstruktion nicht finden konnte.
    if (text.length < 3) { continue; }
    const r = el.getBoundingClientRect();
    if (r.width < 20 || r.height < 8) { continue; }
    let eigen = 0;
    for (const kind of el.childNodes) {
      if (kind.nodeType === 3) { eigen += (kind.textContent || '').trim().length; }
    }
    // Elternkette mitgeben: Bei zeilenweise gerenderten Antworten traegt das
    // einzelne Element (Perplexity: `p.my-2` je Zeile) den Text, der SELEKTOR
    // muss aber den umschliessenden Container treffen. Ohne die Kette sieht
    // man die Blaetter und nie den Ast.
    const eltern = [];
    let auf = el.parentElement;
    for (let i = 0; i < 3 && auf; i++) {
      const k = (typeof auf.className === 'string' ? auf.className : '').trim().split(/\s+/)[0] || '';
      eltern.push(auf.tagName.toLowerCase() + (auf.id ? '#' + auf.id : (k ? '.' + k : '')));
      auf = auf.parentElement;
    }
    out.push({
      tag: el.tagName.toLowerCase(),
      id: el.id || '',
      test_id: el.getAttribute('data-testid') || '',
      role: el.getAttribute('role') || '',
      parents: eltern.join(' < '),
      class: (typeof el.className === 'string' ? el.className : '').trim().slice(0, 200),
      len: text.length,
      own_text: eigen,
      kids: el.children.length,
      text: text.slice(0, 120),
      y: Math.round(r.top),
      h: Math.round(r.height)
    });
  }
  // Zwei Sichten, weil eine nicht reicht: Nach Textmenge sortiert dominieren
  // die eigenen Prompts (sie sind laenger als jede Antwort), nach Position
  // sortiert steht die JUENGSTE Antwort unten. Beides zusammen zeigt den
  // Antwortcontainer auch dann, wenn er kurz ist.
  const nachText = [...out].sort((a, b) => b.own_text - a.own_text);
  const nachLage = [...out].sort((a, b) => b.y - a.y);
  const gesehen = new Set();
  const misch = [];
  for (const el of [...nachLage.slice(0, 14), ...nachText.slice(0, 14)]) {
    const key = el.tag + '|' + el.class + '|' + el.y + '|' + el.len;
    if (gesehen.has(key)) { continue; }
    gesehen.add(key);
    misch.push(el);
  }
  return misch;
})()
"#;

/// Ein Textcontainer-Kandidat aus [`TEXT_PROBE_SCRIPT`].
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TextCandidate {
    pub tag: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub test_id: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub class: String,
    /// Bis zu drei Ebenen Elternkette, aeusserste zuletzt.
    #[serde(default)]
    pub parents: String,
    #[serde(default)]
    pub len: usize,
    #[serde(default)]
    pub own_text: usize,
    #[serde(default)]
    pub kids: usize,
    #[serde(default)]
    pub text: String,
}

/// Fuehrt [`TEXT_PROBE_SCRIPT`] aus und liefert die Kandidaten.
pub fn collect_text(driver: &mut dyn PageDriver) -> Result<Vec<TextCandidate>> {
    let raw = driver.evaluate(TEXT_PROBE_SCRIPT)?;
    Ok(serde_json::from_value(raw).unwrap_or_default())
}

/// Schlaegt fuer einen Textcontainer den stabilsten Selektor vor — dieselbe
/// Rangfolge wie [`selector_for`]: `data-testid`, `id`, dann eine einzelne
/// Klasse. Klassenketten aus Utility-Frameworks (Tailwind) taugen nicht, also
/// wird die erste Klasse genommen, die nicht wie eine Utility aussieht.
pub fn text_selector_for(candidate: &TextCandidate) -> Option<String> {
    if !candidate.test_id.is_empty() {
        return Some(format!("[data-testid='{}']", candidate.test_id));
    }
    if !candidate.id.is_empty() {
        // Perplexity nummeriert Antworten durch (`markdown-content-0`), eine
        // exakte id taugt dann nur fuer die erste. Der Praefix bleibt stabil.
        if let Some(stamm) = candidate.id.rsplit_once('-').map(|(kopf, _)| kopf) {
            if stamm.len() >= 4
                && candidate
                    .id
                    .rsplit_once('-')
                    .is_some_and(|(_, z)| z.chars().all(|c| c.is_ascii_digit()))
            {
                return Some(format!("[id^='{stamm}-']"));
            }
        }
        return Some(format!("#{}", candidate.id));
    }
    candidate
        .class
        .split_whitespace()
        .find(|c| c.len() >= 4 && !c.contains('[') && !c.contains(':') && !c.contains('/'))
        .map(|c| format!("{}.{}", candidate.tag, c))
}

/// Baut den stabilsten Selektor fuer ein Element.
///
/// Reihenfolge nach Haltbarkeit: `data-testid` ueberlebt Umbauten am ehesten,
/// eine `id` meist auch; `aria-label` ist sprachabhaengig und faellt beim
/// naechsten Sprachwechsel um; reiner Text ist der naechste Ausweg. Erst
/// danach `title` (Tooltip, eine absichtliche Beschriftung) und `class`
/// (Deko, wechselt bei jedem Theme-Redesign) — die zwei tragen nur
/// Icon-only-Oberflaechen, auf denen alle anderen Quellen leer sind
/// (deepseek: `div[role=button]` ohne jede Beschriftung).
fn selector_for(candidate: &Candidate) -> Option<(String, u8, String)> {
    if !candidate.test_id.is_empty() {
        return Some((
            format!("[data-testid='{}']", candidate.test_id),
            95,
            format!("data-testid={}", candidate.test_id),
        ));
    }
    if !candidate.id.is_empty() {
        return Some((
            format!("#{}", candidate.id),
            85,
            format!("id={}", candidate.id),
        ));
    }
    if !candidate.aria_label.is_empty() {
        // `i` = ohne Ruecksicht auf Gross-/Kleinschreibung; die Oberflaechen
        // sind darin nicht konsistent.
        return Some((
            format!(
                "{}[aria-label*='{}' i]",
                candidate.tag, candidate.aria_label
            ),
            70,
            format!("aria-label={}", candidate.aria_label),
        ));
    }
    if !candidate.placeholder.is_empty() {
        // Ein Platzhalter ist eine bewusste Design-Beschriftung: stabiler als
        // sichtbarer Text, aber nicht so stabil wie aria-label. deepseeks
        // Composer traegt placeholder='Message DeepSeek', waehrend seine
        // Klassen Hash-Fragmente sind, die beim naechsten Deploy verfallen.
        return Some((
            format!(
                "{}[placeholder*='{}' i]",
                candidate.tag, candidate.placeholder
            ),
            60,
            format!("placeholder={}", candidate.placeholder),
        ));
    }
    if !candidate.role.is_empty() && !candidate.text.is_empty() {
        // role-qualifizierter Text: `[role='radio']:has-text('Instant')` trifft
        // genau ein Element, wo `div:has-text('Instant')` auch den Container
        // treffen koennte. Genau der Fall bei deepseeks Segmenten
        // (Instant/Expert/Vision), die role=radio und Klartext tragen und nur
        // ueber verfallende Hash-Klassen erreichbar wuerden — der Selektor ist
        // sichtbar, er wird nur nicht gebildet.
        return Some((
            format!("[role='{}']:has-text('{}')", candidate.role, candidate.text),
            55,
            format!("role={} text={}", candidate.role, candidate.text),
        ));
    }
    if !candidate.text.is_empty() {
        return Some((
            format!("{}:has-text('{}')", candidate.tag, candidate.text),
            50,
            format!("text={}", candidate.text),
        ));
    }
    if !candidate.title.is_empty() {
        return Some((
            format!("[title*='{}' i]", candidate.title),
            45,
            format!("title={}", candidate.title),
        ));
    }
    if !candidate.class.is_empty() {
        // `i` wie oben; der Teilstring-Match greift auch, wenn die Klasse
        // mehrere Namen traegt (`className` ist eine Leerzeichen-Liste).
        return Some((
            format!("[class*='{}' i]", candidate.class),
            35,
            format!("class={}", candidate.class),
        ));
    }
    None
}

/// Findet den Composer: das Eingabefeld fuer die Nachricht.
fn classify_composer(candidates: &[Candidate]) -> Option<Proposal> {
    let field = candidates.iter().find(|c| {
        c.visible && (c.contenteditable || c.tag == "textarea") && !c.haystack().contains("such")
    })?;
    let (selector, confidence, evidence) = selector_for(field).or_else(|| {
        // Ein Composer ohne jede Beschriftung ist trotzdem einer.
        Some((
            if field.contenteditable {
                "[contenteditable='true']".to_string()
            } else {
                "textarea".to_string()
            },
            40,
            "Formfeld ohne Kennzeichnung".to_string(),
        ))
    })?;
    Some(Proposal {
        capability_key: "chat",
        selector_key: "composer",
        selector,
        confidence,
        disabled: false,
        evidence,
    })
}

/// Deutet die eingesammelten Kandidaten.
///
/// Reine Rechnung — kein Browser, kein Netz. Genau deshalb pruefbar.
pub fn classify(candidates: &[Candidate]) -> Vec<Proposal> {
    let mut proposals: Vec<Proposal> = Vec::new();
    if let Some(composer) = classify_composer(candidates) {
        proposals.push(composer);
    }

    for rule in RULES {
        // Bester Treffer je Regel: hoechste Selektor-Guete gewinnt, damit ein
        // beschriftungsloser Zufallstreffer keinen data-testid verdraengt.
        let mut best: Option<Proposal> = None;
        for candidate in candidates.iter().filter(|c| c.visible) {
            if let Some(roles) = rule.roles {
                if !roles.contains(&candidate.role.as_str()) {
                    continue;
                }
            }
            let haystack = candidate.haystack();
            if haystack.trim().is_empty() {
                continue;
            }
            if rule.excludes.iter().any(|bad| haystack.contains(bad)) {
                continue;
            }
            if !rule.needles.iter().any(|n| haystack.contains(n)) {
                continue;
            }
            let Some((selector, confidence, evidence)) = selector_for(candidate) else {
                continue;
            };
            let proposal = Proposal {
                capability_key: rule.capability_key,
                selector_key: rule.selector_key,
                selector,
                confidence,
                disabled: candidate.disabled,
                evidence,
            };
            if best
                .as_ref()
                .is_none_or(|b| b.confidence < proposal.confidence)
            {
                best = Some(proposal);
            }
        }
        if let Some(found) = best {
            proposals.push(found);
        }
    }
    proposals
}

/// Faehrt die Analyse gegen eine offene Seite.
///
/// Der Browser-Teil ist absichtlich duenn: einsammeln und deuten lassen. So
/// bleibt die Logik im testbaren Teil.
///
/// Kein Timeout-Parameter an dieser Stelle: weder [`collect`] noch
/// ``PageDriver::evaluate`` nehmen eine `Duration` entgegen, also gibt es
/// hier nichts, das gegen `Duration::ZERO` zu validieren oder auf 30 Sekunden
/// zu begrenzen waere. Sobald ein Timeout-Parameter an dieser Stelle
/// entsteht, gehoert genau diese Pruefung hierher.
pub fn probe(driver: &mut dyn PageDriver) -> Result<Vec<Proposal>> {
    Ok(classify(&collect(driver)?))
}

/// Sammelt nur ein, ohne zu deuten.
///
/// Getrennt von [`probe`], weil der Befehl auch die Rohzahl der gefundenen
/// Bedienelemente braucht: „0 Vorschlaege" heisst etwas voellig anderes, je
/// nachdem ob die Seite 0 oder 200 Elemente hatte. Ohne diese Zahl kann man
/// einen Messfehler nicht von einem Ergebnis unterscheiden.
pub fn collect(driver: &mut dyn PageDriver) -> Result<Vec<Candidate>> {
    let raw = driver.evaluate(PROBE_SCRIPT)?;
    Ok(serde_json::from_value(raw).unwrap_or_default())
}

/// Identitaet eines Kandidaten fuer den Zwei-Abzug-Vergleich.
///
/// Bei Icon-only-Oberflaechen (deepseek: Dutzende `div[role=button]` mit
/// identischer Klasse, ohne Label, Text, id oder title) ist die Position im
/// Viewport das Einzige, was zwei sonst identische Elemente unterscheidet —
/// deshalb Lage UND Groesse, nicht etwa die Klasse.
fn stop_diff_key(c: &Candidate) -> (String, i32, i32, i32, i32) {
    (c.tag.clone(), c.x, c.y, c.w, c.h)
}

/// Der Ertrag der Stop-Discovery: die Elemente, die nur **waehrend** der
/// Generierung da waren oder sich an gleicher Stelle verwandelt haben.
///
/// Der Stop-Knopf traegt bei manchen Oberflaechen kein unterscheidendes
/// Attribut und ist statisch nicht findbar — wohl aber an seiner *zeitlichen*
/// Signatur: er ist da, solange die Antwort laeuft, und verschwindet danach.
/// ``WebBrainBackend::probe_stop_by_disappearance`` macht zwei Abzuege und
/// reicht sie hierher; unter den Rueckgaeben muss der Stop-Knopf sein.
///
/// Zwei Muster:
/// - **Verschwinden**: ein Element, das waehrend der Generierung sichtbar war
///   und danach an gleicher Stelle (Lage + Groesse) nicht mehr existiert.
/// - **Verwandlung**: viele Oberflaechen benutzen EIN Element, das zwischen
///   „senden" und „stoppen" wechselt — dann bleibt die Stelle, aber Klasse,
///   `aria-label` oder `title` aendern sich.
pub fn stop_diff_candidates(during: &[Candidate], after: &[Candidate]) -> Vec<Candidate> {
    use std::collections::HashSet;

    let after_keys: HashSet<_> = after
        .iter()
        .filter(|c| c.visible)
        .map(stop_diff_key)
        .collect();
    let mut candidates: Vec<_> = during
        .iter()
        .filter(|c| c.visible && !after_keys.contains(&stop_diff_key(c)))
        .cloned()
        .collect();

    for d in during.iter().filter(|c| c.visible) {
        if let Some(a) = after.iter().find(|a| a.visible && a.x == d.x && a.y == d.y) {
            if a.class != d.class || a.aria_label != d.aria_label || a.title != d.title {
                candidates.push(d.clone());
            }
        }
    }
    candidates
}

/// Verlangt die Seite eine Anmeldung?
///
/// Wichtig fuer die Ehrlichkeit des Befehls: eine Anmeldemaske hat auch Knoepfe
/// und Textfelder, also liefert die Analyse dort *irgendwelche* Vorschlaege.
/// Ohne diese Pruefung meldet `probe` einen Erfolg, wo es in Wahrheit die
/// falsche Seite vermessen hat. Es wird nur erkannt und gemeldet — nie
/// angemeldet.
pub fn looks_like_login(candidates: &[Candidate], url: &str) -> bool {
    const IN_URL: &[&str] = &["/login", "/signin", "/sign-in", "/auth", "accounts.google"];
    if IN_URL.iter().any(|n| url.to_lowercase().contains(n)) {
        return true;
    }
    // Ein Passwortfeld ist der eindeutigste Marker; das PROBE_SCRIPT sammelt
    // aber nur `input[type=text]`. Also ueber die Beschriftungen gehen — und
    // zwar streng: ein blosser „Anmelden"-Knopf steht auch neben einem voll
    // bedienbaren Gast-Chat (perplexity). Erst wenn KEIN Eingabefeld fuer eine
    // Nachricht da ist, ist die Anmeldung wirklich der einzige Weg weiter.
    const LOGIN_WORDS: &[&str] = &[
        "log in",
        "login",
        "sign in",
        "anmelden",
        "einloggen",
        "passwort",
        "password",
    ];
    let has_login = candidates
        .iter()
        .filter(|c| c.visible)
        .any(|c| LOGIN_WORDS.iter().any(|w| c.haystack().contains(w)));
    has_login && classify_composer(candidates).is_none()
}

/// Ergebnis der Nachpruefung eines [`Proposal`]s an der lebenden Oberflaeche.
///
/// Ein Vorschlag ist ein Fund, keine Faehigkeit. Erst wenn ein Klick
/// nachweislich einen lesbaren Zustand geaendert hat, gilt sie als fahrbar —
/// „kein Beleg, kein Level".
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub capability_key: &'static str,
    pub selector_key: &'static str,
    pub selector: String,
    /// Zustand vor dem Klick (Ausgabe der gemeinsamen Zustandsauslese).
    pub before: String,
    /// Zustand nach dem Klick.
    pub after: String,
    /// Nur `true`, wenn sich der Zustand messbar geaendert hat.
    pub proven: bool,
    /// Wurde der Ausgangszustand wiederhergestellt? `None`, wenn gar nichts zu
    /// widerrufen war (kein Wechsel).
    pub restored: Option<bool>,
    /// Klartext fuer Menschen — auch und gerade im Nicht-belegt-Fall.
    pub note: String,
}

/// **Ein** Weg in den Store: die Messung der Oberflaeche, quellenunabhaengig.
///
/// `Verdict` ist die Messung, `capability_proof::ProofOutcome` das Urteil —
/// der Uebergang passiert in `capability_proof::record_measurement`, nicht hier.
/// Diese Konvertierung sitzt bewusst in `brain_probe` und nicht in
/// `capability_proof`: das rechnende Modul bleibt frei von Browser-Typen (§6 des
/// Capability-Proof-Plans).
///
/// `winning_selector` ist der tatsaechlich geklickte Selektor — der im Betrieb
/// aufgeloeste Gewinner der Fallback-Kette, nicht der erste Eintrag.
impl From<&Verdict> for crate::capability_proof::Measurement {
    fn from(v: &Verdict) -> Self {
        Self {
            capability_key: v.capability_key.to_string(),
            before: v.before.clone(),
            after: v.after.clone(),
            proven: v.proven,
            restored: v.restored,
            note: v.note.clone(),
            winning_selector: Some(v.selector.clone()),
        }
    }
}

/// Baut aus einer belegten Faehigkeit und dem aufgeloesten Gewinner der
/// Fallback-Kette einen [`Proposal`] fuer die Nachpruefung an der lebenden
/// Oberflaeche (`operations::verify_surface`). Der Gewinner ist der Selektor,
/// den `resolve_fallback` wirklich getroffen hat — nicht der erste
/// Katalogeintrag.
pub fn proposal_from(cap: &crate::capability::Capability, winner: &str) -> Proposal {
    Proposal {
        capability_key: cap.key,
        selector_key: cap.needs[0],
        selector: winner.to_string(),
        confidence: 100,
        disabled: false,
        evidence: format!("aufgeloester Fallback: {winner}"),
    }
}

/// Klickt einen Vorschlag an und prueft, ob sich ein lesbarer Zustand aendert.
///
/// Wichtig: **kein** Zustandswechsel ist ein gueltiges Ergebnis, kein Fehler.
/// Genau dieser Fall ist bei deepseeks `mode_switch` real aufgetreten — die
/// Segmente tragen keinerlei Auswahlmarkierung —, und er ist der Grund, warum
/// die Faehigkeit dort `driveable:false` steht. Ein `Err` gibt es nur, wenn die
/// Seite selbst nicht antwortet.
///
/// Nebenwirkungsarm: hat der Klick gewirkt, wird zurueckgeklickt und vermerkt,
/// ob der Rueckweg gelang. Ein Test, der die Oberflaeche anders hinterlaesst,
/// als er sie vorfand, ist eine Nebenwirkung — keine Messung.
pub fn verify(driver: &mut dyn PageDriver, proposal: &Proposal) -> Result<Verdict> {
    // Dieselben JS-Bausteine wie der Umschaltpfad des Backends
    // (`browser::ui::toggle_state_expr` / `click_toggle_expr`); nur die Quelle
    // der Selektorliste ist eine andere. Eine zweite handgepflegte Kopie waere
    // der doppelte Auslesepfad, an dem hier schon Fixes vorbeigelaufen sind.
    let selectors = vec![proposal.selector.clone()];
    let state_expr = crate::browser::js::toggle_state_expr_for(&selectors);
    let click_expr = crate::browser::js::click_toggle_expr_for(&selectors);

    let before = driver.eval_string(&state_expr)?;
    let clicked = driver.evaluate(&click_expr)?.as_bool().unwrap_or(false);
    if !clicked {
        return Ok(Verdict {
            capability_key: proposal.capability_key,
            selector_key: proposal.selector_key,
            selector: proposal.selector.clone(),
            before: before.clone(),
            after: before,
            proven: false,
            restored: None,
            note: format!("Selektor '{}' war nicht anklickbar", proposal.selector),
        });
    }
    let mut after = driver.eval_string(&state_expr)?;

    // Zweiter Anlauf: `element.click()` feuert nur ein synthetisches
    // `click`-Ereignis. Oberflaechen, die auf pointerdown/mousedown lauschen,
    // ignorieren es — qwens Denkstufen-Menue und Perplexitys Modellmenue
    // (ui.rs `open_menu` kennt genau diesen Fall und klickt dann real). Der
    // echte Mausklick an den Koordinaten geht den vollstaendigen Ereignisweg.
    let mut real_path = false;
    if before == after {
        if let Some((x, y)) = click_point_of(driver, &selectors) {
            if driver.click_at(x, y).is_ok() {
                std::thread::sleep(std::time::Duration::from_millis(700));
                after = driver.eval_string(&state_expr)?;
                real_path = true;
            }
        }
    }

    if before == after {
        return Ok(Verdict {
            capability_key: proposal.capability_key,
            selector_key: proposal.selector_key,
            selector: proposal.selector.clone(),
            before,
            after,
            proven: false,
            restored: None,
            note: "Klick kam an, Zustand unveraendert — kein Beleg, kein Level".into(),
        });
    }

    // Zustand hat sich geaendert: zurueckschalten und den Rueckweg belegen.
    // Der Rueckweg nimmt dieselbe Strasse, die geoeffnet hat.
    let restored = if real_path {
        match click_point_of(driver, &selectors) {
            Some((x, y)) => {
                driver.click_at(x, y).is_ok()
                    && driver
                        .eval_string(&state_expr)
                        .map(|s| s == before)
                        .unwrap_or(false)
            }
            None => false,
        }
    } else {
        driver.evaluate(&click_expr)?.as_bool().unwrap_or(false)
            && driver.eval_string(&state_expr)? == before
    };
    let note = if restored {
        "Zustandswechsel belegt, Ausgangszustand wiederhergestellt".to_string()
    } else {
        format!(
            "Zustandswechsel belegt, aber Rueckweg misslungen — Oberflaeche steht jetzt auf '{after}' statt '{before}'"
        )
    };
    let note = if real_path {
        format!("{note} (via echtem Mausklick)")
    } else {
        note
    };
    Ok(Verdict {
        capability_key: proposal.capability_key,
        selector_key: proposal.selector_key,
        selector: proposal.selector.clone(),
        before,
        after,
        proven: true,
        restored: Some(restored),
        note,
    })
}

/// Mittelpunkt des klickbaren Vorfahren im Viewport — die Koordinaten fuer
/// den echten Mausklick, den [`verify`] als zweiten Anlauf faehrt, wenn der
/// synthetische Klick keinen Zustandswechsel bringt.
pub(crate) fn click_point_of(
    driver: &mut dyn PageDriver,
    selectors: &[String],
) -> Option<(f64, f64)> {
    let expr = crate::browser::js::js_scan(
        &crate::browser::js::js_selectors(selectors),
        "var el=Q(S[i]);if(el){var t=el.closest('button,[role=button],[role=switch],[role=checkbox],[class*=button],[class*=btn]')||el;var r=t.getBoundingClientRect();if(r.width>0&&r.height>0)return {x:r.left+r.width/2,y:r.top+r.height/2};}",
        "null",
    );
    let v = driver.evaluate(&expr).ok()?;
    Some((v.get("x")?.as_f64()?, v.get("y")?.as_f64()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(aria: &str) -> Candidate {
        Candidate {
            tag: "button".into(),
            aria_label: aria.into(),
            visible: true,
            ..Default::default()
        }
    }

    #[test]
    fn deutsche_beschriftungen_werden_gefunden() {
        // Der Kern: die Oberflaechen laufen hier auf Deutsch. Ein rein
        // englisches Muster findet claudes Stopp-Knopf nie.
        let found = classify(&[button("Antwort stoppen")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].capability_key, "stop_generation");
        assert_eq!(found[0].selector_key, "stop_button");
    }

    #[test]
    fn englische_beschriftungen_ebenso() {
        let found = classify(&[button("Stop streaming")]);
        assert_eq!(found[0].capability_key, "stop_generation");
    }

    #[test]
    fn testid_schlaegt_aria_label() {
        // Zwei Kandidaten fuer dieselbe Faehigkeit: der stabilere gewinnt.
        let candidates = vec![
            button("Nachricht senden"),
            Candidate {
                tag: "button".into(),
                test_id: "send-button".into(),
                text: "Senden".into(),
                visible: true,
                ..Default::default()
            },
        ];
        let found = classify(&candidates);
        let send = found
            .iter()
            .find(|p| p.selector_key == "send_button")
            .unwrap();
        assert_eq!(send.selector, "[data-testid='send-button']");
        assert!(send.confidence >= 95);
    }

    #[test]
    fn unsichtbare_elemente_zaehlen_nicht() {
        let hidden = Candidate {
            visible: false,
            ..button("Antwort stoppen")
        };
        assert!(classify(&[hidden]).is_empty());
    }

    #[test]
    fn falschfreunde_werden_ausgeschlossen() {
        // „Sprachnachricht senden" ist der Sprachdialog, nicht der Absendeknopf.
        assert!(classify(&[button("Sprachnachricht senden")])
            .iter()
            .all(|p| p.selector_key != "send_button"));
        // Ein temporaerer Chat ist kein neuer Chat.
        let found = classify(&[button("Temporärer Chat")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].capability_key, "temporary_chat");
    }

    #[test]
    fn findet_die_bisher_fehlenden_faehigkeiten() {
        // deep_research fehlte kimi als Selektor, temporary_chat qwen als Code.
        // Genau die soll die Analyse selbst finden.
        let found = classify(&[
            button("Deep Research"),
            button("Tiefe Recherche"),
            button("Temporärer Chat"),
        ]);
        let keys: Vec<&str> = found.iter().map(|p| p.capability_key).collect();
        assert!(keys.contains(&"deep_research"), "{keys:?}");
        assert!(keys.contains(&"temporary_chat"), "{keys:?}");
    }

    #[test]
    fn tailwind_items_center_verdraengt_model_menu_nicht() {
        // Real aus dem Perplexity-DOM geerntet (headless, 2026-08-12): der
        // Modell-Knopf traegt die volle Tailwind-Klasse mit `items-center`.
        // Das alte Exclude „item" schlug dort zu — der echte Oeffner wurde
        // nie als model_menu klassifiziert und das Modell-Menue blieb fuer
        // die Analyse unsichtbar.
        let candidates = vec![Candidate {
            tag: "button".into(),
            aria_label: "Modell".into(),
            text: "Modell".into(),
            class: "reset interactable select-none [-webkit-user-drag:none] outline-none font-medium transition-[background-color,border-color,transform,color,opacity] duration-300 ease-out font-sans text-center items-center justify-center leading-loose whitespace-nowrap disabled:cursor-default disabled:opacity-50 data-[state=open]:text-primary data-[state=open]:bg-quiet h-8 text-sm cursor-pointer origin-center activ".into(),
            visible: true,
            ..Default::default()
        }];
        let found = classify(&candidates);
        let menu = found
            .iter()
            .find(|p| p.selector_key == "model_menu")
            .expect("Modell-Knopf muss model_menu bleiben, {found:?}");
        assert!(menu.selector.contains("Modell"), "{}", menu.selector);
    }

    #[test]
    fn model_item_bleibt_vom_oeffner_ausgeschlossen() {
        // zai-Gegenprobe: `[data-testid='model-item']` ist ein EINTRAG in der
        // Liste, kein Oeffner — darf auch mit dem geschaerften Exclude nicht
        // als model_menu durchrutschen.
        let candidates = vec![Candidate {
            tag: "button".into(),
            test_id: "model-item".into(),
            visible: true,
            ..Default::default()
        }];
        let found = classify(&candidates);
        assert!(
            found.iter().all(|p| p.selector_key != "model_menu"),
            "{found:?}"
        );
    }

    #[test]
    fn perplexity_deutsche_oberflaeche_wird_gedeutet() {
        // Real aus dem Perplexity-DOM geerntet (headless, 2026-07): deutsche
        // Beschriftungen, die die englischen Muster alle verfehlt haetten.
        let candidates = vec![
            Candidate {
                tag: "button".into(),
                aria_label: "Modell".into(),
                text: "Modell".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "a".into(),
                aria_label: "Projekte".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "button".into(),
                text: "Anmelden".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "button".into(),
                aria_label: "Inkognito verwenden: Anonyme Sitzungen erstellen".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "button".into(),
                aria_label: "Diktiermodus".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "button".into(),
                text: "Nur notwendige".into(),
                visible: true,
                ..Default::default()
            },
        ];
        let found = classify(&candidates);
        let keys: Vec<(String, String)> = found
            .iter()
            .map(|p| (p.capability_key.to_string(), p.selector_key.to_string()))
            .collect();
        assert!(
            keys.contains(&("model_switch".into(), "model_menu".into())),
            "{keys:?}"
        );
        assert!(
            keys.contains(&("projects".into(), "projects_button".into())),
            "{keys:?}"
        );
        assert!(
            keys.contains(&("chat".into(), "login_button".into())),
            "{keys:?}"
        );
        assert!(
            keys.contains(&("temporary_chat".into(), "temporary_chat_button".into())),
            "{keys:?}"
        );
        assert!(
            keys.contains(&("voice_input".into(), "voice_input_button".into())),
            "{keys:?}"
        );
        assert!(
            keys.contains(&("chat".into(), "consent_reject_button".into())),
            "{keys:?}"
        );
    }

    #[test]
    fn alle_zulassen_ist_kein_ablehnen() {
        // „Alle zulassen" (Perplexity-Cookie-Banner) ist Zustimmen — der
        // Ablehnen-Selektor muss es uebersehen.
        let found = classify(&[button("Alle zulassen")]);
        assert!(
            found
                .iter()
                .all(|p| p.selector_key != "consent_reject_button"),
            "{found:?}"
        );
    }

    #[test]
    fn composer_wird_erkannt_auch_ohne_beschriftung() {
        let candidates = vec![Candidate {
            tag: "div".into(),
            contenteditable: true,
            visible: true,
            ..Default::default()
        }];
        let found = classify(&candidates);
        let composer = found.iter().find(|p| p.selector_key == "composer").unwrap();
        assert_eq!(composer.selector, "[contenteditable='true']");
    }

    #[test]
    fn suchfeld_wird_nicht_als_composer_verwechselt() {
        let candidates = vec![Candidate {
            tag: "textarea".into(),
            placeholder: "Suchen".into(),
            visible: true,
            ..Default::default()
        }];
        assert!(classify(&candidates)
            .iter()
            .all(|p| p.selector_key != "composer"));
    }

    #[test]
    fn probe_script_hat_keine_zeilenuebergreifenden_stringliterale() {
        // Diese Klasse Fehler faellt sonst durch JEDES Sieb: kein Unit-Test
        // fuehrt das JS aus, also meldet die Suite gruen, waehrend das Skript
        // im Browser an einem Syntaxfehler stirbt. Genau so ist es hier beim
        // ersten Wurf passiert — ein ueber zwei Zeilen gebrochenes
        // '...'-Literal. Der Test ist billig und faengt die Wiederholung.
        for (number, line) in PROBE_SCRIPT.lines().enumerate() {
            let code = line.split("//").next().unwrap_or(line);
            assert_eq!(
                code.matches('\'').count() % 2,
                0,
                "Zeile {}: unpaarige Anfuehrungszeichen — {line}",
                number + 1
            );
        }
    }

    #[test]
    fn text_probe_script_hat_keine_zeilenuebergreifenden_stringliterale() {
        // Gleiche Falle wie beim PROBE_SCRIPT: kein Unit-Test fuehrt das JS
        // aus, ein gebrochenes Literal stirbt erst im Browser.
        for (number, line) in TEXT_PROBE_SCRIPT.lines().enumerate() {
            let code = line.split("//").next().unwrap_or(line);
            assert_eq!(
                code.matches('\'').count() % 2,
                0,
                "Zeile {}: unpaarige Anfuehrungszeichen — {line}",
                number + 1
            );
        }
    }

    fn text_kandidat(id: &str, test_id: &str, class: &str) -> TextCandidate {
        TextCandidate {
            tag: "div".into(),
            id: id.into(),
            test_id: test_id.into(),
            role: String::new(),
            class: class.into(),
            parents: String::new(),
            len: 200,
            own_text: 180,
            kids: 0,
            text: "Antwort".into(),
        }
    }

    /// Rangfolge wie bei Bedienelementen: `data-testid` vor `id` vor Klasse.
    #[test]
    fn text_selector_bevorzugt_das_haltbarste_merkmal() {
        assert_eq!(
            text_selector_for(&text_kandidat("x", "answer", "prose")),
            Some("[data-testid='answer']".to_string())
        );
        assert_eq!(
            text_selector_for(&text_kandidat("", "", "prose dark")),
            Some("div.prose".to_string())
        );
        assert_eq!(text_selector_for(&text_kandidat("", "", "")), None);
    }

    /// Durchnummerierte Antworten: die exakte `id` traefe nur die erste,
    /// deshalb der Praefix. Perplexity vergibt `markdown-content-0`, `-1`, …
    #[test]
    fn durchnummerierte_id_wird_zum_praefix_selektor() {
        assert_eq!(
            text_selector_for(&text_kandidat("markdown-content-0", "", "")),
            Some("[id^='markdown-content-']".to_string())
        );
        // Ohne Zahlenendung bleibt es bei der exakten id.
        assert_eq!(
            text_selector_for(&text_kandidat("haupttext", "", "")),
            Some("#haupttext".to_string())
        );
    }

    /// Utility-Klassen (Tailwind) taugen nicht als Anker — sie tragen
    /// Doppelpunkte, Klammern und Schraegstriche und wechseln mit jedem Theme.
    #[test]
    fn utility_klassen_werden_uebersprungen() {
        assert_eq!(
            text_selector_for(&text_kandidat(
                "",
                "",
                "md:flex hover:bg-soft w-1/2 antwort"
            )),
            Some("div.antwort".to_string())
        );
    }

    #[test]
    fn test_empty_webview_response_returns_error() {
        let mut driver =
            crate::mock_page::MockPageDriver::new(crate::mock_page::MockPageState::new());
        let result = probe(&mut driver);
        assert!(
            result.is_err(),
            "probe sollte bei leerer Webview-Antwort einen Fehler liefern, bekam aber: {result:?}"
        );
    }

    #[test]
    fn probe_faehrt_ueber_den_page_driver() {
        // Beweist die Verdrahtung ohne Browser: der Mock liefert die Rohdaten,
        // die Deutung passiert in Rust.
        let dom = serde_json::json!([
            {"tag": "button", "aria_label": "Antwort stoppen", "visible": true},
            {"tag": "div", "contenteditable": true, "visible": true}
        ]);
        let state = crate::mock_page::MockPageState::new().on_eval(PROBE_SCRIPT, dom);
        let mut driver = crate::mock_page::MockPageDriver::new(state);
        let found = probe(&mut driver).expect("probe");
        let keys: Vec<&str> = found.iter().map(|p| p.selector_key).collect();
        assert!(keys.contains(&"stop_button"), "{keys:?}");
        assert!(keys.contains(&"composer"), "{keys:?}");
    }

    // ── Gegenprobe gegen die gepflegten Selektordateien ──────────────────
    //
    // Der eigentliche Pruefstein: eine Regel, die zufaellig auf perplexity
    // passt und bei den sieben eingetragenen Brains danebengreift, ist keine
    // Regel. `selectors/*.json` ist die Referenz — dort steht, welche
    // Beschriftungen die Oberflaechen wirklich tragen.
    //
    // Grenze dieses Tests, ehrlich benannt: er misst das WORTSCHATZ-Problem
    // (findet die Regel den Knopf an seiner echten Beschriftung?), nicht das
    // lebende DOM. Ein Selektor ohne jede Beschriftung — `button[type=submit]`,
    // `a[href='/']`, `button:has(svg)` — ist hier grundsaetzlich nicht
    // entscheidbar und wird deshalb ausgeklammert statt als Treffer gezaehlt.

    /// Baut aus einem gepflegten Selektor den Kandidaten, den er meint.
    /// `None`, wenn der Selektor keinerlei Beschriftung traegt.
    fn candidate_from_selector(selector: &str) -> Option<Candidate> {
        let tag = selector
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>();
        let tag = if tag.is_empty() { "button".into() } else { tag };
        let mut c = Candidate {
            tag,
            visible: true,
            ..Default::default()
        };
        // Wert zwischen den Anfuehrungszeichen nach einem Attributnamen.
        let value_after = |needle: &str| -> Option<String> {
            let rest = selector.split_once(needle)?.1;
            let quote = rest.chars().find(|c| *c == '\'' || *c == '"')?;
            let rest = rest.split_once(quote)?.1;
            Some(rest.split(quote).next()?.to_string())
        };
        if let Some(v) = value_after("data-testid") {
            c.test_id = v;
        }
        if let Some(v) = value_after("aria-label") {
            c.aria_label = v;
        }
        if let Some(v) = value_after(":has-text(") {
            c.text = v;
        }
        if let Some(v) = selector.split_once("text=") {
            if !v.0.ends_with("has-") {
                c.text = v.1.to_string();
            }
        }
        if let Some(v) = selector.split_once('#') {
            c.id =
                v.1.chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
                    .collect();
        }
        if selector.contains("contenteditable") {
            c.contenteditable = true;
        }
        if selector.contains("textarea") {
            c.tag = "textarea".into();
        }
        let labelled = !c.test_id.is_empty()
            || !c.aria_label.is_empty()
            || !c.text.is_empty()
            || !c.id.is_empty();
        if labelled || c.contenteditable || c.tag == "textarea" {
            Some(c)
        } else {
            None
        }
    }

    fn selector_files() -> Vec<(String, serde_json::Value)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("selectors");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).expect("selectors/ muss existieren") {
            let path = entry.expect("Eintrag").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let body = std::fs::read_to_string(&path).expect("lesbar");
            out.push((name, serde_json::from_str(&body).expect("gueltiges JSON")));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    #[test]
    fn gegenprobe_trifft_die_gepflegten_selektoren() {
        // Fuer jeden Selektorschluessel, fuer den es eine Regel gibt: erkennt
        // die Analyse das gemeinte Element an seiner echten Beschriftung?
        let interesting = [
            "composer",
            "send_button",
            "stop_button",
            "new_chat_button",
            "reasoning_toggle",
            "web_search_toggle",
            "temporary_chat_button",
        ];
        let mut misses: Vec<String> = Vec::new();
        for (brain, sel) in selector_files() {
            for key in interesting {
                let Some(list) = sel.get(key).and_then(|v| v.as_array()) else {
                    continue;
                };
                let candidates: Vec<Candidate> = list
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(candidate_from_selector)
                    .collect();
                if candidates.is_empty() {
                    // Nur beschriftungslose Selektoren — nicht entscheidbar.
                    continue;
                }
                // Jeden Kandidaten EINZELN pruefen waere zu streng: die Listen
                // sind Fallback-Ketten, in denen absichtlich auch grobe
                // Eintraege stehen. Es genuegt, dass die Analyse den Knopf an
                // IRGENDEINER seiner gepflegten Beschriftungen erkennt.
                let found = candidates.iter().any(|c| {
                    classify(std::slice::from_ref(c))
                        .iter()
                        .any(|p| p.selector_key == key)
                });
                if !found {
                    misses.push(format!("{brain}/{key}"));
                }
            }
        }
        // Bekannte, benannte Luecken statt einer geschoenten Zahl. Wer eine
        // Regel schaerft, muss diese Liste kuerzen — nicht verlaengern.
        let known: &[&str] = &[
            // deepseek beschriftet den Websuche-Knopf nur mit „Search". Ein
            // Muster auf das blosse Wort wuerde bei allen anderen Brains das
            // Suchfeld der Seitenleiste treffen — ein falscher Vorschlag ist
            // schlimmer als ein fehlender, also bleibt die Luecke stehen.
            "deepseek/web_search_toggle",
        ];
        let unexpected: Vec<&String> = misses
            .iter()
            .filter(|m| !known.contains(&m.as_str()))
            .collect();
        assert!(
            unexpected.is_empty(),
            "Analyse verfehlt gepflegte Selektoren: {unexpected:?}"
        );
    }

    #[test]
    fn gegenprobe_schlaegt_nichts_falsches_vor() {
        // Die gefaehrlichere Haelfte: ein falscher Vorschlag verdrahtet
        // stillschweigend das falsche Element. Geprueft wird, dass ein
        // gepflegter Selektor NICHT einer fremden Faehigkeit zugeordnet wird.
        let mut wrong: Vec<String> = Vec::new();
        for (brain, sel) in selector_files() {
            let Some(obj) = sel.as_object() else { continue };
            for (key, list) in obj {
                let Some(list) = list.as_array() else {
                    continue;
                };
                for raw in list.iter().filter_map(|v| v.as_str()) {
                    let Some(c) = candidate_from_selector(raw) else {
                        continue;
                    };
                    for p in classify(std::slice::from_ref(&c)) {
                        // Composer-Vorschlag auf einem Textfeld ist richtig,
                        // egal unter welchem Schluessel es steht.
                        if p.selector_key == "composer" {
                            continue;
                        }
                        if p.selector_key != key.as_str() {
                            wrong.push(format!("{brain}/{key}: '{raw}' -> {}", p.selector_key));
                        }
                    }
                }
            }
        }
        // Erwartete, harmlose Ueberschneidungen: `login_indicator` und
        // `login_button` zeigen bewusst auf dieselben Elemente wie composer
        // bzw. der Anmeldeknopf — das sind Zeiger, keine eigenen Knoepfe.
        let benign = |s: &String| {
            s.contains("/login_indicator")
                || s.contains("/login_button")
                || s.contains("/reasoning_effort_menu")
                || s.contains("/model_menu")
                || s.contains("/mode_option")
        };
        let real: Vec<&String> = wrong.iter().filter(|s| !benign(s)).collect();
        assert!(real.is_empty(), "falsche Zuordnungen: {real:#?}");
    }

    fn proposal() -> Proposal {
        Proposal {
            capability_key: "reasoning_toggle",
            selector_key: "reasoning_toggle",
            selector: "[data-testid='think']".into(),
            confidence: 95,
            disabled: false,
            evidence: "data-testid=think".into(),
        }
    }

    #[test]
    fn proposal_from_nimmt_den_gewinner_nicht_den_ersten_eintrag() {
        let cap = crate::capability::CATALOG
            .iter()
            .find(|c| c.key == "reasoning_toggle")
            .expect("capability");
        let p = proposal_from(cap, "[data-testid='think']");
        assert_eq!(p.capability_key, "reasoning_toggle");
        assert_eq!(p.selector_key, "reasoning_toggle");
        assert_eq!(p.selector, "[data-testid='think']");
        assert_eq!(p.confidence, 100);
        assert!(p.evidence.contains("[data-testid='think']"));
    }

    /// Baut den Mock so, wie `verify` fragt: Zustandsauslese als Antwortfolge,
    /// Klick als Festwert.
    fn mock(states: Vec<&str>, click: bool) -> crate::mock_page::MockPageDriver {
        let sels = vec![proposal().selector];
        let state_expr = crate::browser::js::toggle_state_expr_for(&sels);
        let click_expr = crate::browser::js::click_toggle_expr_for(&sels);
        let state = crate::mock_page::MockPageState::new()
            .on_eval_seq(
                state_expr,
                states.into_iter().map(|s| serde_json::json!(s)).collect(),
            )
            .on_eval(click_expr, serde_json::json!(click));
        crate::mock_page::MockPageDriver::new(state)
    }

    #[test]
    fn verify_belegt_zustandswechsel_und_raeumt_auf() {
        // vorher / nachher / nach dem Rueckklick wieder wie vorher.
        let mut driver = mock(
            vec![
                "aria-pressed=false|",
                "aria-pressed=true|",
                "aria-pressed=false|",
            ],
            true,
        );
        let verdict = verify(&mut driver, &proposal()).expect("verify");
        assert!(verdict.proven);
        assert_eq!(verdict.restored, Some(true));
        assert_ne!(verdict.before, verdict.after);
    }

    #[test]
    fn verify_faehrt_echten_mausklick_wenn_synthetisch_ohne_wechsel() {
        // Perplexitys Modellmenue lauscht auf pointerdown: der synthetische
        // `element.click()` kommt an, aendert aber nichts. Erst der echte
        // Mausklick an den Koordinaten oeffnet es (data-state=open) — und
        // der Rueckweg nimmt dieselbe Strasse.
        let sels = vec![proposal().selector];
        let state_expr = crate::browser::js::toggle_state_expr_for(&sels);
        let click_expr = crate::browser::js::click_toggle_expr_for(&sels);
        let point_expr = crate::browser::js::js_scan(
            &crate::browser::js::js_selectors(&sels),
            "var el=Q(S[i]);if(el){var t=el.closest('button,[role=button],[role=switch],[role=checkbox],[class*=button],[class*=btn]')||el;var r=t.getBoundingClientRect();if(r.width>0&&r.height>0)return {x:r.left+r.width/2,y:r.top+r.height/2};}",
            "null",
        );
        let state = crate::mock_page::MockPageState::new()
            .on_eval_seq(
                state_expr,
                [
                    "aria-pressed=false|",
                    "aria-pressed=false|",
                    "aria-pressed=true|",
                    "aria-pressed=false|",
                ]
                .into_iter()
                .map(|s| serde_json::json!(s))
                .collect(),
            )
            .on_eval(click_expr, serde_json::json!(true))
            .on_eval(point_expr, serde_json::json!({"x": 100.0, "y": 50.0}));
        let mut driver = crate::mock_page::MockPageDriver::new(state);
        let verdict = verify(&mut driver, &proposal()).expect("verify");
        assert!(verdict.proven, "{verdict:?}");
        assert_eq!(verdict.restored, Some(true));
        assert!(
            verdict.note.contains("echtem Mausklick"),
            "{}",
            verdict.note
        );
    }

    #[test]
    fn verify_ohne_wechsel_ist_kein_fehler() {
        // deepseeks mode_switch: die Segmente tragen keine Auswahlmarkierung.
        // Der Klick kommt an, der Zustand bleibt — das ist ein sauberes
        // Ergebnis (proven=false), kein Err.
        let mut driver = mock(vec!["|seg"], true);
        let verdict = verify(&mut driver, &proposal()).expect("verify darf hier nicht scheitern");
        assert!(!verdict.proven);
        assert_eq!(verdict.restored, None);
        assert_eq!(verdict.before, verdict.after);
        assert!(verdict.note.contains("unveraendert"), "{}", verdict.note);
    }

    #[test]
    fn verify_meldet_misslungenen_rueckweg() {
        // Der dritte Wert bleibt beim geaenderten Zustand: der Rueckklick hat
        // nicht zurueckgeschaltet. Belegt ist die Faehigkeit trotzdem — aber
        // die Nebenwirkung muss sichtbar sein.
        let mut driver = mock(
            vec![
                "aria-pressed=false|",
                "aria-pressed=true|",
                "aria-pressed=true|",
            ],
            true,
        );
        let verdict = verify(&mut driver, &proposal()).expect("verify");
        assert!(verdict.proven);
        assert_eq!(verdict.restored, Some(false));
        assert!(
            verdict.note.contains("Rueckweg misslungen"),
            "{}",
            verdict.note
        );
    }

    #[test]
    fn verify_meldet_nicht_anklickbaren_selektor() {
        let mut driver = mock(vec!["aria-pressed=false|"], false);
        let verdict = verify(&mut driver, &proposal()).expect("verify");
        assert!(!verdict.proven);
        assert!(
            verdict.note.contains("nicht anklickbar"),
            "{}",
            verdict.note
        );
    }

    #[test]
    fn verify_nutzt_dieselbe_zustandsauslese_wie_der_toggle_pfad() {
        // Der Kern der Extraktion: haette verify eine eigene Kopie des JS,
        // wuerde dieser Vergleich auseinanderlaufen — und ein Fix am einen
        // Pfad am anderen vorbei.
        let sels = vec!["[data-testid='think']".to_string()];
        let expr = crate::browser::js::toggle_state_expr_for(&sels);
        assert!(expr.contains("closest("), "{expr}");
        assert!(expr.contains("data-"), "{expr}");
        assert!(crate::browser::js::click_toggle_expr_for(&sels).contains("t.click()"));
    }

    fn at(tag: &str, x: i32, y: i32, w: i32, h: i32) -> Candidate {
        Candidate {
            tag: tag.into(),
            x,
            y,
            w,
            h,
            visible: true,
            ..Default::default()
        }
    }

    /// Der Stop-Knopf erscheint waehrend der Generierung und verschwindet
    /// danach — das ist seine zeitliche Signatur auf Icon-only-Oberflaechen.
    #[test]
    fn stop_diff_findet_das_verschwundene_element() {
        let during = vec![at("div", 10, 20, 30, 40), at("button", 5, 5, 20, 20)];
        // Der Button bleibt, das div verschwindet (der Stop-Knopf).
        let after = vec![at("button", 5, 5, 20, 20)];
        let found = stop_diff_candidates(&during, &after);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].x, found[0].y), (10, 20));
    }

    /// Bleibt alles gleich, ist nichts ein Kandidat — sonst meldete der
    /// Vergleich bei jeder ruhigen Seite den halben DOM als „Stop".
    #[test]
    fn stop_diff_ignoriert_bleibende_elemente() {
        let both = vec![at("button", 5, 5, 20, 20)];
        let found = stop_diff_candidates(&both, &both);
        assert!(found.is_empty());
    }

    /// Gleiche Stelle, geaenderte Klasse/Beschriftung: manche Oberflaechen
    /// benutzen EIN Element, das zwischen „senden" und „stoppen" wechselt.
    #[test]
    fn stop_diff_erkennt_die_verwandlung() {
        let mut during = at("button", 5, 5, 20, 20);
        during.class = "btn-send".into();
        let mut after = at("button", 5, 5, 20, 20);
        after.class = "btn-stop".into();
        let found = stop_diff_candidates(&[during], &[after]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].class, "btn-send");
    }

    /// Unsichtbare Elemente sind Rauschen und niemals Kandidaten — der
    /// Zwei-Abzug soll den Stop-Knopf finden, nicht verstecktes DOM.
    #[test]
    fn stop_diff_ignoriert_unsichtbare() {
        let mut during = at("div", 10, 20, 30, 40);
        during.visible = false;
        let found = stop_diff_candidates(&[during], &[]);
        assert!(found.is_empty());
    }

    /// Ein reines Icon-`div` ohne jede Beschriftung (deepseek) wird ueber seine
    /// Klasse gefunden — der einzige Anker, der dort bleibt.
    #[test]
    fn selector_for_faellt_auf_class_zurueck() {
        let c = Candidate {
            tag: "div".into(),
            class: "d4910adc".into(),
            visible: true,
            ..Default::default()
        };
        let (sel, conf, ev) = selector_for(&c).expect("class muss einen Selektor liefern");
        assert_eq!(sel, "[class*='d4910adc' i]");
        assert_eq!(ev, "class=d4910adc");
        assert!(conf < 50, "class ist der unbeständigste Anker: {conf}");
    }

    /// Ein Tooltip ist eine absichtliche Beschriftung und schlaegt die blosse
    /// CSS-Klasse — beide aber erst, wenn text/id/aria-label leer sind.
    #[test]
    fn selector_for_nutzt_title_vor_class_aber_nach_aria_label() {
        let title_only = Candidate {
            tag: "button".into(),
            title: "Antwort stoppen".into(),
            visible: true,
            ..Default::default()
        };
        let (sel, conf, _) = selector_for(&title_only).unwrap();
        assert_eq!(sel, "[title*='Antwort stoppen' i]");
        assert!((45..50).contains(&conf), "{conf}");

        // Aria-label gewinnt: es ist stabiler als der Tooltip.
        let both = Candidate {
            tag: "button".into(),
            aria_label: "Stop".into(),
            title: "Antwort stoppen".into(),
            visible: true,
            ..Default::default()
        };
        let (sel, _, _) = selector_for(&both).unwrap();
        assert!(sel.contains("aria-label"), "{sel}");
    }

    /// Ein Platzhalter ist eine bewusste Beschriftung: er gewinnt gegen die
    /// blosse Klassenkette (deepseek: Composer `placeholder='Message DeepSeek'`
    /// gegen verfallende Hash-Klassen).
    #[test]
    fn selector_for_nutzt_placeholder_vor_klassen() {
        let c = Candidate {
            tag: "textarea".into(),
            placeholder: "Message DeepSeek".into(),
            class: "d96f2d2a _27c9245 ds-scroll-area".into(),
            visible: true,
            ..Default::default()
        };
        let (sel, conf, ev) = selector_for(&c).expect("placeholder muss einen Selektor liefern");
        assert_eq!(sel, "textarea[placeholder*='Message DeepSeek' i]");
        assert_eq!(ev, "placeholder=Message DeepSeek");
        assert!(
            (55..65).contains(&conf),
            "Placeholder ist ein starker Anker: {conf}"
        );
    }

    /// role-qualifizierter Text gewinnt gegen die Klassenkette (deepseek:
    /// Segmente mit role=radio und Klartext). Der Selektor ist sichtbar — er
    /// wurde nur nicht gebildet.
    #[test]
    fn selector_for_nutzt_role_text_vor_klassen() {
        let c = Candidate {
            tag: "div".into(),
            role: "radio".into(),
            text: "Instant".into(),
            class: "d4910adc".into(),
            visible: true,
            ..Default::default()
        };
        let (sel, conf, ev) = selector_for(&c).expect("role+text muss einen Selektor liefern");
        assert_eq!(sel, "[role='radio']:has-text('Instant')");
        assert_eq!(ev, "role=radio text=Instant");
        assert!(
            (50..60).contains(&conf),
            "role+text ist praeziser als nackter Text: {conf}"
        );
    }

    /// role ohne Text ist kein Selektor — ein blosser `[role=radio]` traefe
    /// alle Segmente auf einmal. Dann gilt weiterhin die Klassenkette.
    #[test]
    fn selector_for_role_ohne_text_faellt_auf_klassen() {
        let c = Candidate {
            tag: "div".into(),
            role: "radio".into(),
            class: "d4910adc".into(),
            visible: true,
            ..Default::default()
        };
        let (sel, _, ev) = selector_for(&c).expect("class muss tragen");
        assert_eq!(sel, "[class*='d4910adc' i]");
        assert_eq!(ev, "class=d4910adc");
    }

    /// Ein deaktivierter Knopf ist ein Hinweis, kein Abfall: der Vorschlag
    /// traegt den Zustand mit, statt das Element zu verschweigen (deepseek:
    /// Send-Button traegt bei leerem Composer `ds-button--disabled`).
    #[test]
    fn deaktivierter_knopf_wird_nicht_verworfen_und_traegt_den_zustand() {
        let mut c = button("Nachricht senden");
        c.disabled = true;
        let found = classify(&[c]);
        let send = found
            .iter()
            .find(|p| p.selector_key == "send_button")
            .expect("deaktivierter Send-Button bleibt ein Fund");
        assert!(send.disabled, "{send:?}");
    }

    /// Ein aktiver Knopf bleibt ein aktiver Vorschlag — die Unterscheidung
    /// ist der Sinn des Zustands.
    #[test]
    fn aktiver_knopf_traegt_disabled_false() {
        let found = classify(&[button("Nachricht senden")]);
        let send = found
            .iter()
            .find(|p| p.selector_key == "send_button")
            .expect("Send-Button");
        assert!(!send.disabled, "{send:?}");
    }

    // ── Sichere Serialisierung von Selektor-/Eingabewert-Inhalten in JS ───
    //
    // `selector_for` kann Beschriftungen (aria-label, Text, Klasse, ...)
    // woertlich in den Selektor uebernehmen. Enthaelt eine Beschriftung
    // Anfuehrungszeichen oder Backslashes, darf der daraus gebaute Selektor
    // beim Umbau in JS (`crate::browser::js::js_selectors`, das intern
    // `serde_json::to_string` nutzt) nicht aus seinem String-Literal
    // ausbrechen koennen. Die folgenden Tests belegen das doppelt: der
    // erzeugte Ausdruck enthaelt den GESAMTEN Selektor als EIN sicher
    // escaptes JSON-Literal, und die betroffenen Funktionen (`verify`,
    // `click_point_of`) laufen damit fehlerfrei durch statt an einer
    // gebrochenen JS-Syntax zu scheitern.

    #[test]
    fn verify_mit_javascript_sonderzeichen_im_selektor_bleibt_sicher() {
        let gefaehrlich = Proposal {
            selector: r#"button[aria-label*='x\'; alert(1); "y' i]"#.to_string(),
            ..proposal()
        };
        let sels = vec![gefaehrlich.selector.clone()];
        let state_expr = crate::browser::js::toggle_state_expr_for(&sels);
        let click_expr = crate::browser::js::click_toggle_expr_for(&sels);
        let expected_literal = serde_json::to_string(&gefaehrlich.selector).unwrap();
        assert!(state_expr.contains(&expected_literal), "{state_expr}");
        assert!(click_expr.contains(&expected_literal), "{click_expr}");

        let state = crate::mock_page::MockPageState::new()
            .on_eval(state_expr, serde_json::json!("|"))
            .on_eval(click_expr, serde_json::json!(false));
        let mut driver = crate::mock_page::MockPageDriver::new(state);
        let verdict = verify(&mut driver, &gefaehrlich)
            .expect("verify darf an Sonderzeichen im Selektor nicht scheitern");
        assert!(!verdict.proven);
        assert!(
            verdict.note.contains("nicht anklickbar"),
            "{}",
            verdict.note
        );
    }

    #[test]
    fn click_point_of_mit_javascript_sonderzeichen_im_selektor_bleibt_sicher() {
        let sels = vec![r#"button[aria-label*='x\'; alert(1); "y' i]"#.to_string()];
        let point_expr = crate::browser::js::js_scan(
            &crate::browser::js::js_selectors(&sels),
            "var el=Q(S[i]);if(el){var t=el.closest('button,[role=button],[role=switch],[role=checkbox],[class*=button],[class*=btn]')||el;var r=t.getBoundingClientRect();if(r.width>0&&r.height>0)return {x:r.left+r.width/2,y:r.top+r.height/2};}",
            "null",
        );
        let expected_literal = serde_json::to_string(&sels[0]).unwrap();
        assert!(point_expr.contains(&expected_literal), "{point_expr}");

        let state = crate::mock_page::MockPageState::new()
            .on_eval(point_expr, serde_json::json!({"x": 12.0, "y": 34.0}));
        let mut driver = crate::mock_page::MockPageDriver::new(state);
        let point = click_point_of(&mut driver, &sels);
        assert_eq!(point, Some((12.0, 34.0)));
    }

    #[test]
    fn probe_liefert_bei_sonderzeichen_im_aria_label_einen_sicher_serialisierbaren_selektor() {
        // Ende-zu-Ende: eine reale Beschriftung mit Anfuehrungszeichen darf
        // den von `probe` gelieferten Selektor nicht in einen Selektor
        // verwandeln, der beim Umbau in JS aus seinem String-Literal
        // ausbrechen koennte.
        let dom = serde_json::json!([
            {"tag": "button", "aria_label": "Roger's \"Chat\" stoppen", "visible": true}
        ]);
        let state = crate::mock_page::MockPageState::new().on_eval(PROBE_SCRIPT, dom);
        let mut driver = crate::mock_page::MockPageDriver::new(state);
        let found = probe(&mut driver).expect("probe");
        let stop = found
            .iter()
            .find(|p| p.selector_key == "stop_button")
            .expect("stop_button muss trotz Anfuehrungszeichen im Label gefunden werden");

        let sels = vec![stop.selector.clone()];
        let state_expr = crate::browser::js::toggle_state_expr_for(&sels);
        let expected_literal = serde_json::to_string(&stop.selector).unwrap();
        assert!(
            state_expr.contains(&expected_literal),
            "Selektor mit Anfuehrungszeichen muss als EIN sicheres JSON-Literal landen: {state_expr}"
        );
    }
}
