//! CLI-Oberflaeche: Befehlsstruktur und Argumente.
//!
//! Bewusst getrennt von `main.rs`: dort standen 2577 Zeilen Code gegen 18
//! Zeilen Test, und ueber 580 davon waren reine clap-Deklarationen. Wer den
//! Ablauf verstehen will, musste sie erst ueberspringen — und ein Brain, das
//! im Benchmark an `main.rs` arbeitet, liest sie jedes Mal mit.

use clap::{Args, Parser, Subcommand};
#[derive(Parser)]
#[command(name = "webagent")]
#[command(version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("WEBAGENT_GIT_HASH"), ")"))]
#[command(about = "Gehirnunabhängiger lokaler Agent (Rust-Port)", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Autonomen Run starten
    Run {
        /// Brain-Backend (z.B. chatgpt, claude, deepseek)
        #[arg(long, default_value = "chatgpt")]
        brain: String,

        /// Benutzeraufgabe
        #[arg(long)]
        task: String,

        /// Run-ID fortsetzen
        #[arg(long)]
        resume: Option<String>,

        /// Headless-Browser (Standard: sichtbar)
        #[arg(long)]
        headless: bool,

        /// Maximale Anzahl an Zyklen
        #[arg(long, default_value = "100")]
        max_cycles: u32,

        /// Aufgabe ohne alte Run-Episoden und Wiki-Kontext starten
        #[arg(long)]
        no_memory: bool,
    },

    /// Sichtbaren Browser oeffnen und auf manuellen Login warten (keine Zugangsdaten-Eingabe)
    Login {
        /// Brain-Backend (z.B. chatgpt, claude, deepseek)
        #[arg(long)]
        brain: String,

        /// Maximale Wartezeit auf den Login in Sekunden
        #[arg(long, default_value = "300")]
        timeout: u64,

        /// Fenster offen halten, auch wenn der Login-Check schon "eingeloggt" meldet.
        /// Noetig, wo die Erkennung zu optimistisch ist (kimi, mistral: Composer ist
        /// auch anonym sichtbar) oder wo nur ein Dialog zu bestaetigen ist (mistral-AGB).
        #[arg(long)]
        force: bool,

        /// Login-Kette selbst durchklicken (Anmelden → ggf. Google-SSO → warten),
        /// statt nur auf manuelle Eingabe zu warten. Passwort/2FA bleibt Sache
        /// des Menschen; das Fenster bleibt offen.
        #[arg(long)]
        auto: bool,
    },

    /// Alle Brains nacheinander einloggen (canonical profiles/<brain>).
    /// Parallel nur opt-in und gedeckelt (siehe --parallel).
    LoginAll {
        /// Maximale Wartezeit pro Brain in Sekunden
        #[arg(long, default_value = "300")]
        timeout: u64,

        /// Auch bei positivem Login-Check erneut oeffnen
        #[arg(long)]
        force: bool,

        /// Parallelitaet (0 = sequenziell/Default; max 3 experimentell)
        #[arg(long, default_value = "0")]
        parallel: usize,
    },

    /// Live-Diagnose: echten Browser oeffnen und Login/Composer/Selektoren pruefen
    Diagnose {
        /// Brain-Backend (z.B. chatgpt, claude, deepseek)
        #[arg(long)]
        brain: String,

        /// Headless statt sichtbar (Standard: sichtbar)
        #[arg(long)]
        headless: bool,
    },

    /// Interaktive REPL: mehrere Aufgaben nacheinander gegen dasselbe Brain
    Repl {
        /// Brain-Backend (z.B. chatgpt, claude, deepseek)
        #[arg(long, default_value = "chatgpt")]
        brain: String,

        /// Headless-Browser (Standard: sichtbar)
        #[arg(long)]
        headless: bool,
    },

    /// Pro-Brain Diagnose: Selektoren, Profil-Lock, letzte Antwort, Recovery
    Doctor {
        /// Nur diese Gehirne prüfen (leer = alle)
        #[arg(long)]
        brain: Vec<String>,

        /// Maschinenlesbares JSON
        #[arg(long)]
        json: bool,
    },

    /// Capability-Verifikation: echte Live-Laeufe belegen die Faehigkeiten
    /// (ProofKind-gesteuert, schreibt Belege nach proofs.jsonl)
    Verify {
        /// Nur diese Gehirne pruefen (leer = alle)
        #[arg(long)]
        brain: Vec<String>,

        /// Nur diese Faehigkeiten pruefen (wiederholbar); leer = alle
        #[arg(long)]
        cap: Vec<String>,

        /// Headless statt sichtbar (Standard: sichtbar)
        #[arg(long)]
        headless: bool,
    },

    /// Zaehl-Spiel: die Brains zaehlen abwechselnd der Reihe nach bis `count`
    /// (Ball wandert im Kreis). Performance- und Zuverlaessigkeitsmass: eine
    /// Sitzung pro Brain, Traces werden als JSON gespeichert und ausgewertet.
    Count {
        /// Nur diese Brains (wiederholbar); leer = alle
        #[arg(long)]
        brain: Vec<String>,

        /// Zielzahl (Standard: 100)
        #[arg(long, default_value_t = 100)]
        count: u32,

        /// Headless statt sichtbar (Standard: sichtbar)
        #[arg(long)]
        headless: bool,
    },
    Watchdog {
        /// Bridge-Lock-Root (bot2bot Verzeichnis)
        #[arg(long)]
        bot2bot_root: Option<String>,

        /// Profil-Verzeichnis
        #[arg(long)]
        profile_dir: Option<String>,

        /// Runs-Verzeichnis (Fallback wenn kein RunStore)
        #[arg(long)]
        runs_dir: Option<String>,

        /// Reparieren (Standard: Dry-Run)
        #[arg(long)]
        repair: bool,

        /// Maschinenlesbares JSON
        #[arg(long)]
        json: bool,
    },

    /// Fehlerursachen vergangener Laeufe klassifizieren: trennt Harness-Fehler
    /// von echter Unfaehigkeit der Brains
    #[command(name = "runs-report")]
    RunsReport {
        /// Wie viele der juengsten Laeufe betrachtet werden
        #[arg(long, default_value = "20")]
        limit: usize,
    },

    /// Swarm entwirft ein TUI-Design, stimmt im Ausscheidungsverfahren ab
    /// (kick vote), der Gewinner wird von einem Brain umgesetzt
    #[command(name = "design-vote")]
    DesignVote {
        /// Komma-getrennte Brain-IDs (leer = alle verfuegbaren)
        #[arg(long, default_value = "")]
        brains: String,

        /// Worum es geht (Thema des Designs)
        #[arg(long, default_value = "das Worker-Pool-Dashboard der webagent-TUI")]
        topic: String,

        /// Optionaler Kontext (aktuelles Layout, Randbedingungen)
        #[arg(long, default_value = "")]
        context: String,

        /// Brain, das den Gewinner umsetzt (leer = nur abstimmen, nicht bauen)
        #[arg(long, default_value = "")]
        implement_brain: String,

        /// Headless-Browser
        #[arg(long)]
        headless: bool,
    },

    /// Pre-flight: Profile, Selektoren, Flags (ohne Browser)
    BrainsHealth {
        /// Leeres Shared-Profil akzeptieren (Exit 0)
        #[arg(long)]
        allow_empty_profile: bool,
    },

    /// Offline-Canary: prueft je Brain nur, ob Spec und Selektor-Datei da sind
    /// (kein Browser, kein Login, kein Netz — dafuer `diagnose`)
    Canary,

    /// Einen Bereich der Oberflaeche oeffnen (z.B. projects_button) und den
    /// Wechsel ueber die URL belegen
    Section {
        /// Brain-ID
        #[arg(long)]
        brain: String,
        /// Selektor-Schluessel des Bereichs
        #[arg(long)]
        key: String,
        /// Sichtbar statt headless
        #[arg(long)]
        visible: bool,
    },

    /// Segmentleiste umschalten (alle Stellungen sichtbar, z.B. deepseeks
    /// Instant/Expert/Vision)
    Mode {
        /// Brain-ID
        #[arg(long)]
        brain: String,
        /// Zu waehlende Stellung (Teilstring)
        #[arg(long)]
        set: String,
        /// Selektor-Schluessel der Stellungen
        #[arg(long, default_value = "mode_option")]
        options: String,
        /// Sichtbar statt headless
        #[arg(long)]
        visible: bool,
    },

    /// Beliebiges Aufklappmenue lesen oder waehlen (z.B. Denkstufe)
    Menu {
        /// Brain-ID
        #[arg(long)]
        brain: String,
        /// Selektor-Schluessel des Menuebuttons, z.B. reasoning_effort_menu
        #[arg(long)]
        key: String,
        /// Selektor-Schluessel der Eintraege (Default: model_option)
        #[arg(long, default_value = "model_option")]
        options: String,
        /// Zu waehlender Eintrag (Teilstring); ohne Angabe wird nur gelistet
        #[arg(long)]
        set: Option<String>,
        /// Sichtbar statt headless
        #[arg(long)]
        visible: bool,
    },

    /// Eine Option umschalten (z.B. reasoning_toggle, web_search_toggle) und
    /// belegen, dass sich der Zustand wirklich geaendert hat
    Toggle {
        /// Brain-ID
        #[arg(long)]
        brain: String,
        /// Selektor-Schluessel der Option
        #[arg(long)]
        option: String,
        /// Sichtbar statt headless
        #[arg(long)]
        visible: bool,
    },

    /// Bilderwand: alle Brains nebeneinander in einem Fenster, wie mehrere
    /// TV-Kanaele gleichzeitig
    Wall {
        /// Sekunden zwischen zwei Aufnahmerunden
        #[arg(long, default_value_t = 30)]
        interval: u64,
        /// Nur eine Runde aufnehmen und beenden
        #[arg(long)]
        once: bool,
        /// Nur diese Brains (mehrfach angebbar); ohne Angabe alle
        #[arg(long)]
        brain: Vec<String>,
    },

    /// Modelle eines Brains auflisten oder umschalten (mit Nachpruefung, dass
    /// der Wechsel wirklich griff)
    Model {
        /// Brain-ID
        #[arg(long)]
        brain: String,
        /// Zu waehlendes Modell (Teilstring genuegt); ohne Angabe wird nur gelistet
        #[arg(long)]
        set: Option<String>,
        /// Sichtbar statt headless
        #[arg(long)]
        visible: bool,
    },

    /// Nimmt die Oberflaeche eines Brains als PNG auf (Vorlage fuer die
    /// Faehigkeits-Vermessung: was im DOM keinen Namen hat, ist im Bild sichtbar)
    Shot {
        /// Brain-ID; ohne Angabe werden alle aufgenommen
        #[arg(long)]
        brain: Option<String>,
        /// Zielverzeichnis (Default: <stable_root>/data/shots)
        #[arg(long)]
        out: Option<String>,
        /// Vor der Aufnahme diesen Selektor-Schluessel anklicken, z.B.
        /// `model_menu` — ein geschlossenes Menue zeigt seine Eintraege nicht
        #[arg(long)]
        open: Option<String>,
        /// Sichtbar statt headless
        #[arg(long)]
        visible: bool,
    },

    /// Vermisst die Oberflaeche eines Brains im echten DOM und traegt die
    /// gefundenen Optionen als `ui_options` ein (der Nenner des Levels)
    Survey {
        /// Brain-ID; ohne Angabe werden alle vermessen
        #[arg(long)]
        brain: Option<String>,
        /// Ergebnis in die Nutzer-Selektordatei schreiben statt nur anzeigen
        #[arg(long)]
        write: bool,
        /// Vor der Vermessung diesen Selektor-Schluessel anklicken (z.B. model_menu)
        #[arg(long)]
        open: Option<String>,

        /// Rohe Beschriftungen aller gefundenen Bedienelemente ausgeben
        #[arg(long)]
        dump: bool,

        /// Sichtbar statt headless
        #[arg(long)]
        visible: bool,
    },

    /// Oberflaechen-Analyse wie die Link-Analyse in JDownloader: oeffnet eine
    /// Chat-URL, sammelt die Bedienelemente ein und deutet sie als
    /// Selektoren-Vorschlaege. Mit `--write` wird das Brain automatisch
    /// eingebunden: Selektoren-Datei schreiben + als Custom-Brain registrieren.
    Probe {
        /// URL des zu analysierenden Chats (neuer Brain) oder ohne URL ueber
        /// `--brain` ein bestehendes Brain nachvermessend pruefen
        #[arg(long)]
        url: Option<String>,
        /// Brain-ID für die Selektoren-Datei (Default: aus URL abgeleitet)
        #[arg(long)]
        brain_id: Option<String>,
        /// Bestehendes Brain nachvermessend pruefen (Featureliste auffuellen)
        #[arg(long)]
        brain: Option<String>,
        /// Ergebnis schreiben: `selectors/<id>.json` + Custom-Brain registrieren
        #[arg(long)]
        write: bool,
        /// Vorschlaege live nachpruefen (Zustandsbeleg, kann Zeit kosten)
        #[arg(long)]
        verify: bool,
        /// Nach dem ersten Scan diesen Vorschlag anklicken und erneut scannen
        /// (z.B. `model_menu`, damit die Menue-Eintraege sichtbar werden)
        #[arg(long)]
        open: Option<String>,
        /// Rohe DOM-Kandidaten ausgeben (fuer die Analyse von Fehlfunden)
        #[arg(long)]
        dump: bool,
        /// Waehrend einer laufenden Generierung scannen: sendet eine Probe und
        /// scannt erst, wenn die Antwort laeuft. Noetig fuer `stop_button` —
        /// den gibt es im Ruhezustand nicht. Schreibt eine echte Nachricht.
        #[arg(long)]
        generating: bool,
        /// Stop-Knopf ueber sein VERSCHWINDEN finden: scannt waehrend der
        /// Generierung und danach und meldet, was nur waehrenddessen da war.
        /// Fuer Oberflaechen ohne Label/Text/id an den Bedienelementen
        /// (deepseek). Schreibt eine echte Nachricht und wartet die ganze
        /// Antwort ab.
        #[arg(long)]
        stop_diff: bool,
        /// Sichtbar statt headless (beim ersten Mal fuer den Login noetig)
        #[arg(long)]
        visible: bool,
    },
    Quests {
        /// Maschinenlesbar statt Konsolenansicht
        #[arg(long)]
        json: bool,
    },

    /// Misst je Brain die zulaessige Eingabelaenge und merkt sie sich dauerhaft
    MeasureLimits {
        /// Brains als CSV (leer = alle verfuegbaren)
        #[arg(long, default_value = "")]
        brains: String,
        #[arg(long)]
        headless: bool,
        /// Auch bereits gemessene Brains erneut messen
        #[arg(long)]
        force: bool,
        /// Erste Probengroesse in Zeichen
        #[arg(long, default_value_t = 100_000)]
        start: usize,
        /// Obergrenze: wird sie angenommen, gilt der Wert als untere Schranke
        #[arg(long, default_value_t = 2_000_000)]
        ceiling: usize,
        /// Genauigkeit der Intervallschachtelung in Zeichen
        #[arg(long, default_value_t = 10_000)]
        tolerance: usize,
    },

    /// Single send+wait turn (bot2bot bridge debugging)
    Relay {
        #[arg(long)]
        brain: String,
        #[arg(long, default_value = "")]
        message: String,
        /// Nachricht aus einer Datei lesen statt aus dem Argument.
        ///
        /// Windows kappt die Kommandozeile bei rund 32.000 Zeichen. Ohne diese
        /// Option laesst sich gar nicht messen, wie lange Eingaben eine
        /// Oberflaeche annimmt — der Start scheitert vorher mit
        /// "Der Dateiname oder die Erweiterung ist zu lang".
        #[arg(long)]
        message_file: Option<String>,
        #[arg(long)]
        headless: bool,
        #[arg(long, default_value = "0")]
        timeout: f64,
        /// Maschinenlesbare JSON-Ausgabe (brain/ok/answer/latency_ms/reason)
        #[arg(long)]
        json: bool,
        /// Zielmodell in derselben Sitzung umschalten, bevor die Frage rausgeht.
        ///
        /// Ohne diese Option gilt das Standardmodell des Brains. Mit dieser
        /// Option bleibt der Wechsel UND die Frage in einer Sitzung — ein
        /// Wechsel in einem separaten Browserlauf wuerde aufs Standardmodell
        /// zurueckfallen und die Frage liefe mit dem falschen Modell.
        #[arg(long)]
        model: Option<String>,
    },

    /// Multi-Brain-Swarm (Relay je Brain + Synthese). Default menschenlesbar; `--json` fuer CLI-Anbindung.
    Swarm {
        /// Aufgabe / Prompt an alle Brains
        #[arg(long)]
        message: String,
        /// Headless-Browser (Standard: sichtbar)
        #[arg(long)]
        headless: bool,
        /// Timeout pro Brain in Sekunden (0 = Default)
        #[arg(long, default_value = "0")]
        timeout: f64,
        /// Komma-getrennte Brain-IDs (leer = alle verfuegbaren)
        #[arg(long, default_value = "")]
        brains: String,
        /// Maschinenlesbares JSON (pro Brain + synthesis)
        #[arg(long)]
        json: bool,
    },

    /// Autonomer bot2bot-Worker: Inbox pollen, Task via Controller abarbeiten,
    /// Ergebnis zurueck an Absender (grok-Aequivalent). Jeder Prozess nutzt ein
    /// eigenes isoliertes Profil (Q5-copy) -> N Worker laufen parallel.
    #[command(name = "bot2bot-worker")]
    Bot2BotWorker {
        /// Brain-Backend (z.B. deepseek)
        #[arg(long)]
        brain: String,
        /// Ein Durchlauf statt Endlos-Loop
        #[arg(long)]
        once: bool,
        /// Poll-Intervall in Sekunden
        #[arg(long, default_value = "30")]
        poll_secs: u64,
        /// Maximale Controller-Zyklen
        #[arg(long, default_value = "100")]
        max_cycles: u32,
        /// Headless-Browser
        #[arg(long)]
        headless: bool,
    },

    /// Worker-Pool-Manager (Teil 1): haelt N aktive bot2bot-Worker (je ein
    /// eigener Kindprozess) am Leben, Failover bei Crash (Brain -> unavailable,
    /// naechster Reserve-Brain promoviert). Status pro Brain in pool_state.json
    /// (available/active/unavailable, extern re-flaggbar).
    #[command(name = "workers")]
    Workers {
        /// Anzahl gleichzeitig aktiver Worker (Default 2 -> 6 Reserve bei 8 Brains)
        #[arg(long, default_value = "2")]
        active: usize,

        /// Komma-getrennte Brain-IDs (leer = alle verfuegbaren mit Profil)
        #[arg(long, default_value = "")]
        brains: String,

        /// Poll-Intervall der Supervisor-Schleife in Sekunden
        #[arg(long, default_value = "10")]
        poll_secs: u64,

        /// Headless-Browser fuer die Worker-Kindprozesse
        #[arg(long)]
        headless: bool,
    },

    /// Pool/Wand/Bench-TUI. Ohne Subcommand startet `webagent` die Session-
    /// Ansicht (`--view session`); `webagent tui` bleibt diese Pool-Wand.
    #[command(name = "tui")]
    Tui {
        /// Zielanzahl gleichzeitig aktiver Worker (Default 2)
        #[arg(long, default_value = "2")]
        active: usize,

        /// Komma-getrennte Brain-IDs (leer = alle verfuegbaren mit Profil)
        #[arg(long, default_value = "")]
        brains: String,

        /// Poll-Intervall der Supervisor-Schleife in Sekunden
        #[arg(long, default_value = "5")]
        poll_secs: u64,

        /// Headless-Browser fuer die Worker-Kindprozesse
        #[arg(long)]
        headless: bool,

        /// Startet beim Öffnen der TUI sofort diesen Benchmark-Argumentstring.
        ///
        /// Der Wert beginnt selbst mit `--`, deshalb MUSS die Gleichheitszeichen-
        /// Form benutzt werden — mit Leerzeichen haelt clap den Wert fuer ein
        /// weiteres Argument und bricht ab:
        /// `--benchmark="--rounds 1 --suggestions 3 --harvest"`
        #[arg(long)]
        benchmark: Option<String>,

        /// Ansicht, mit der die TUI startet: `session`, `workers`, `bench`,
        /// `capabilities` oder `config`.
        ///
        /// Ohne Angabe entscheidet der Kontext (mit `--benchmark` startet sie
        /// im Ereignisstrom, sonst im Worker-Dashboard). Explizit gesetzt,
        /// laesst sich jede Ansicht ohne Tastendruck oeffnen — noetig fuer
        /// automatisierte Abnahme per Screenshot.
        ///
        /// Die Liste hier muss zu `tui_state::parse_view` passen. Sie lief
        /// schon einmal auseinander: parse_view kannte vier Ansichten, clap
        /// liess zwei zu — die beiden anderen waren ueber die Kommandozeile
        /// unerreichbar, ohne dass irgendetwas gemeckert haette.
        #[arg(long, value_parser = ["session", "workers", "bench", "capabilities", "config"])]
        view: Option<String>,

        /// ratatui-TUI erzwingen, auch wenn stdout kein Terminal ist
        /// (z.B. fuer Launch aus opencode oder anderen Kontexten)
        #[arg(long)]
        force_tui: bool,
    },

    /// First-run setup: Brain-Auswahl und optional Login-Hinweise
    Oobe {
        #[arg(long)]
        brains: String,
        #[arg(long)]
        skip_login: bool,
        #[arg(long)]
        yes: bool,
    },

    /// Autoresearch: messbare Metrik autonom verbessern (Modify→Verify→Keep/Discard,
    /// Git als Sicherheitsnetz, eigener Branch autoresearch/<timestamp>)
    Autoresearch(AutoresearchArgs),

    /// Swarm-Selbstbewertung: der Brain-Pool sammelt/konsolidiert/abstimmt die
    /// wichtigsten nächsten Verbesserungen (Prioritätsfindung, kein Modify-Loop).
    #[command(name = "autoresearch-self")]
    AutoresearchSelf {
        /// Vorschläge je Brain (Phase 1)
        #[arg(long, default_value = "10")]
        suggestions: usize,

        /// Größe der gerankten Top-Liste (Phase 4)
        #[arg(long, default_value = "10")]
        top: usize,

        /// Headless-Browser (Standard: sichtbar)
        #[arg(long)]
        headless: bool,

        /// Projektfakten aus dieser Datei statt aus README/PROGRESS/src
        #[arg(long)]
        facts: Option<String>,
    },

    /// Code-Benchmark: vote-driven objektiver Code-Kompetenz-Score. Der Schwarm
    /// stimmt über den nächsten Verbesserungsschritt ab; jedes Brain baut den
    /// Sieger sequenziell, gemessen wird hart (Compiler + Tests, kein Selbst-Report).
    Benchmark {
        /// Brains als CSV (Standard: alle registrierten)
        #[arg(long)]
        brains: Option<String>,

        /// Anzahl Abstimm-/Bau-Runden
        #[arg(long, default_value = "1")]
        rounds: usize,

        /// Vorschläge je Brain in der Sammelphase
        #[arg(long, default_value = "10")]
        suggestions: usize,

        /// Harte Obergrenze der Repair-Iterationen je Brain. Frueher gestoppt
        /// wird ueber --stall-limit, sobald kein Fortschritt mehr kommt
        #[arg(long, default_value = "20")]
        max_iterations: u32,

        /// Reines Messgeraet: bestandenen Brain-Code verwerfen statt ihn
        /// einzuspielen und zu committen (Standard ist ernten)
        #[arg(long)]
        no_harvest: bool,

        /// Ausklappen: jeden Schritt (Shell-Kommandos, Datei-Aktionen,
        /// Brain-Antworten) als eigene Zeile zeigen, nicht nur den aktuellen
        /// in der mitlaufenden Zeile
        #[arg(long)]
        verbose: bool,

        /// Wie viele Brains beim Sammeln und Abstimmen gleichzeitig befragt
        /// werden (Bauen bleibt sequenziell)
        #[arg(long, default_value = "4")]
        parallel: usize,

        /// Nach wie vielen Iterationen ohne Fortschritt ein Brain aufgibt und
        /// die Aufgabe an das naechste weitergereicht wird
        #[arg(long, default_value = "3")]
        stall_limit: u32,

        /// Wie oft eine Aufgabe hoechstens weitergereicht wird
        #[arg(long, default_value = "2")]
        max_handoffs: usize,

        /// Lint-Tor fuer die Ernte: geernteter Code muss auch hier gruen sein
        /// (leer = kein Lint-Gate)
        #[arg(long, default_value = "cargo clippy --all-targets -- -D warnings")]
        lint_eval: String,

        /// Eval-Kommando „baut es?"
        #[arg(long, default_value = "cargo build --lib")]
        build_eval: String,

        /// Eval-Kommando „Tests grün?"
        #[arg(long, default_value = "cargo test --lib")]
        test_eval: String,

        /// Arbeitsverzeichnis (Standard: Repo-Root)
        #[arg(long)]
        workdir: Option<String>,

        /// Headless-Browser (Standard: sichtbar)
        #[arg(long)]
        headless: bool,

        /// Endlos-Schleife: nach der letzten Runde sofort neu starten
        #[arg(long)]
        loop_forever: bool,
    },

    /// Read-only gate for autonomous maintenance
    MaintenanceCheck {
        /// Maschinenlesbares JSON
        #[arg(long)]
        json: bool,

        /// Zusätzlich vollständige Test-Suite ausführen
        #[arg(long)]
        pytest: bool,

        /// Maximale Testlaufzeit in Sekunden
        #[arg(long, default_value = "600")]
        pytest_timeout: f64,
    },

    /// Spielt die neueste Laufzeit-Kopie (profiles/encapsulated/pool_*) ins
    /// Master-Profil zurueck. Manueller Rettungsweg, wenn der TUI-Exit den
    /// Rueckweg nicht mehr selbst schafft (alter Binärstand) oder eine Kopie
    /// uebrig geblieben ist.
    #[command(name = "sync-master")]
    SyncMaster,

    /// Versionierten Zielvertrag lokal verwalten.
    Goal {
        #[command(subcommand)]
        command: GoalCommands,
    },

    /// Aktive Arbeitsscheiben des lokalen Zielvertrags verwalten.
    Plan {
        #[command(subcommand)]
        command: PlanCommands,
    },

    /// Lokale OpenAI-/Anthropic-kompatible Provider-Bridge fuer Pi starten.
    Api {
        #[command(subcommand)]
        command: ApiCommands,
    },

    /// Lokale Registry und Free-only-Entscheidungen fÃ¼r optionale Cloud-Textchats.
    Cloud {
        #[command(subcommand)]
        command: CloudCommands,
    },
}

/// Argumente des `autoresearch`-Subcommands (Spec: docs/AUTORESEARCH_PLAN.md §6).

#[derive(Subcommand, Debug, Clone)]
pub enum CloudCommands {
    /// Listet die versionierte lokale Cloud-Registry ohne Netz- oder Providerzugriff.
    List,
    /// Durchsucht die lokale Registry und erklÃ¤rt jede Trefferentscheidung.
    Search {
        #[arg(long, default_value = "custom")]
        profile: String,
        #[arg(long, default_value = "")]
        query: String,
        /// Erlaubt Modelle mit explizit bestÃ¤tigten Credits; Standard bleibt Free-only.
        #[arg(long)]
        allow_credits: bool,
    },
    /// Zeigt fÃ¼r einen Registry-Eintrag die konkrete Routerentscheidung.
    Decide {
        #[arg(long)]
        model_id: String,
        /// Erlaubt Modelle mit explizit bestÃ¤tigten Credits; Standard bleibt Free-only.
        #[arg(long)]
        allow_credits: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum ApiCommands {
    /// Startet einen token-geschuetzten Loopback-Dienst.
    Serve {
        /// Lokale Bind-Adresse; nur Loopback-Adressen werden akzeptiert.
        #[arg(long, default_value = "127.0.0.1")]
        bind: String,
        /// TCP-Port des lokalen Dienstes.
        #[arg(long, default_value_t = 8787)]
        port: u16,
        /// Browser-Brain, das die Provider-Anfragen bearbeitet.
        #[arg(long, default_value = "chatgpt")]
        brain: String,
        /// Maximale Agentenzyklen pro API-Anfrage.
        #[arg(long, default_value_t = 100)]
        max_cycles: u32,
        /// Browser ohne sichtbares Fenster ausfuehren.
        #[arg(long)]
        headless: bool,
        /// Name der Umgebungsvariable mit dem lokalen Bearer-Token.
        #[arg(long, default_value = "WEBAGENT_API_KEY")]
        api_key_env: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum GoalCommands {
    Create {
        #[arg(long)]
        objective: String,
        #[arg(long = "acceptance", required = true)]
        acceptance: Vec<String>,
        #[arg(long = "scope")]
        scope: Vec<String>,
    },
    Get {
        #[arg(long)]
        json: bool,
    },
    Complete {
        #[arg(long = "evidence", required = true)]
        evidence: Vec<String>,
        #[arg(long)]
        reviewer: String,
        #[arg(long)]
        verdict: String,
    },
    Abandon {
        #[arg(long)]
        reason: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum PlanCommands {
    Create {
        #[arg(long)]
        title: String,
        #[arg(long = "item", required = true)]
        item: Vec<String>,
    },
    Get {
        #[arg(long)]
        json: bool,
    },
    Done {
        #[arg(long)]
        id: u32,
    },
}

#[derive(Args)]
pub struct AutoresearchArgs {
    /// Brain-Backend (z.B. chatgpt, claude, deepseek)
    #[arg(long)]
    pub brain: String,

    /// Messbares Ziel in Textform (fließt in den Modify-Prompt ein)
    #[arg(long)]
    pub goal: String,

    /// Eval-Befehl — Vertrag: exit 0 + letzte stdout-Zeile ist eine Zahl
    #[arg(long)]
    pub eval: String,

    /// Richtung der Verbesserung: higher|lower
    #[arg(long, default_value = "higher")]
    pub direction: String,

    /// Maximale Anzahl Iterationen
    #[arg(long, default_value = "10")]
    pub max_iterations: usize,

    /// Abbruch nach N Iterationen ohne Verbesserung in Folge
    #[arg(long, default_value = "3")]
    pub no_improve_abort: usize,

    /// Headless-Browser (Standard: sichtbar)
    #[arg(long)]
    pub headless: bool,

    /// Git-Repo-Root (Default: Repo-Root des aktuellen Verzeichnisses)
    #[arg(long)]
    pub workdir: Option<String>,

    /// Timeout des Eval-Befehls in Sekunden
    #[arg(long, default_value = "300")]
    pub eval_timeout: u64,
}
