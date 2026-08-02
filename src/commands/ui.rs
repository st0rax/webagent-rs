//! Befehle, die eine Brain-Oberflaeche bedienen oder vermessen: Bilderwand,
//! Bereiche, Segmentleisten, Menues, Schalter, Modelle, Aufnahmen, Questlog.
pub fn cmd_wall(interval: u64, once: bool, only: &[String]) -> i32 {
    use webagent::browser::WebBrainBackend;

    let dir = webagent::config::data_dir().join("shots");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[wall] Zielverzeichnis nicht anlegbar: {e}");
        return 2;
    }
    let brains: Vec<String> = if only.is_empty() {
        webagent::config::available_brain_ids()
    } else {
        only.to_vec()
    };
    if brains.is_empty() {
        eprintln!("[wall] keine Brains");
        return 2;
    }

    let mut round: u64 = 0;
    loop {
        round += 1;
        let mut ok = 0usize;
        for id in &brains {
            match WebBrainBackend::from_config(id).and_then(|mut b| b.live_screenshot(true)) {
                Ok(png) => match std::fs::write(dir.join(format!("{id}.png")), &png) {
                    Ok(()) => ok += 1,
                    Err(e) => eprintln!("[wall] {id}: schreiben fehlgeschlagen: {e}"),
                },
                // Ein Brain, das gerade nicht will, darf die Wand nicht
                // aufhalten — die alte Kachel bleibt dann einfach stehen.
                Err(e) => eprintln!("[wall] {id}: {e}"),
            }
        }
        match webagent::welcome::write_wall_html(&dir, &brains, interval, round) {
            Ok(path) => {
                println!(
                    "[wall] Runde {round}: {ok}/{} aufgenommen -> {}",
                    brains.len(),
                    path.display()
                );
                if round == 1 {
                    println!("[wall] im Browser oeffnen: {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("[wall] wall.html nicht schreibbar: {e}");
                return 1;
            }
        }
        if once {
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_secs(interval.max(5)));
    }
}

pub fn cmd_section(brain: &str, key: &str, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[section] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[section] {brain}: {e}");
        return 2;
    }
    let code = match backend.open_section(key) {
        Ok((before, after)) => {
            println!("[section] {brain}/{key}: ''{before}'' -> ''{after}''");
            0
        }
        Err(e) => {
            eprintln!("[section] {brain}/{key}: {e}");
            1
        }
    };
    let _ = backend.close_ui();
    code
}

pub fn cmd_mode(brain: &str, set: &str, options: &str, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[mode] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[mode] {brain}: {e}");
        return 2;
    }
    let code = match backend.select_segment(options, set) {
        Ok(state) => {
            println!("[mode] {brain}: ''{set}'' aktiv ({state})");
            0
        }
        Err(e) => {
            eprintln!("[mode] {brain}: {e}");
            1
        }
    };
    let _ = backend.close_ui();
    code
}

pub fn cmd_menu(brain: &str, key: &str, options: &str, set: Option<&str>, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[menu] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[menu] {brain}: {e}");
        return 2;
    }
    let code = match set {
        None => {
            println!(
                "[menu] {brain}/{key}: aktuell ''{}''",
                backend.menu_label(key)
            );
            match backend.list_menu(key, options) {
                Ok(list) if !list.is_empty() => {
                    for m in &list {
                        println!("    {m}");
                    }
                    0
                }
                Ok(_) => {
                    eprintln!("[menu] {brain}/{key}: Menue lieferte keine Eintraege");
                    1
                }
                Err(e) => {
                    eprintln!("[menu] {brain}/{key}: {e}");
                    1
                }
            }
        }
        Some(want) if want.contains('>') => {
            // Pfad durch Untermenues: "Aufwand > Hoch". Claude legt die
            // Denkstufe eine Ebene tiefer als das Modell.
            let path: Vec<&str> = want
                .split('>')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            match backend.select_in_menu_path(key, options, &path) {
                Ok(now) => {
                    println!("[menu] {brain}/{key}: ''{now}''");
                    0
                }
                Err(e) => {
                    eprintln!("[menu] {brain}/{key}: {e}");
                    1
                }
            }
        }
        Some(want) => match backend.select_in_menu(key, options, want) {
            Ok(now) => {
                println!("[menu] {brain}/{key}: ''{now}''");
                0
            }
            Err(e) => {
                eprintln!("[menu] {brain}/{key}: {e}");
                1
            }
        },
    };
    let _ = backend.close_ui();
    code
}

pub fn cmd_toggle(brain: &str, option: &str, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[toggle] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[toggle] {brain}: {e}");
        return 2;
    }
    // Der Faehigkeitsname aus `capability.rs` muss hier ankommen, sonst ist der
    // Antrieb zwar gebaut, aber von aussen nur ueber den internen Selektornamen
    // erreichbar — und damit praktisch unbenutzt.
    let code = match if option == "temporary_chat" {
        backend.toggle_temporary_chat()
    } else {
        backend.toggle_option(option)
    } {
        Ok((before, after)) => {
            println!("[toggle] {brain}/{option}: '{before}' -> '{after}'");
            0
        }
        Err(e) => {
            eprintln!("[toggle] {brain}/{option}: {e}");
            1
        }
    };
    let _ = backend.close_ui();
    code
}

pub fn cmd_model(brain: &str, set: Option<&str>, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[model] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[model] {brain}: {e}");
        return 2;
    }
    let code = match set {
        None => {
            println!("[model] {brain}: aktuell '{}'", backend.current_model());
            match backend.list_models() {
                Ok(list) if !list.is_empty() => {
                    for m in &list {
                        println!("    {m}");
                    }
                    0
                }
                Ok(_) => {
                    // Leeres Menue ist ein Messfehler, kein Ergebnis: entweder
                    // greift model_option nicht oder das Menue ging nicht auf.
                    eprintln!("[model] {brain}: Menue lieferte keine Eintraege");
                    1
                }
                Err(e) => {
                    eprintln!("[model] {brain}: {e}");
                    1
                }
            }
        }
        Some(want) => match backend.switch_model(want) {
            Ok(now) => {
                println!("[model] {brain}: umgestellt auf '{now}'");
                0
            }
            Err(e) => {
                eprintln!("[model] {brain}: {e}");
                1
            }
        },
    };
    let _ = backend.close_ui();
    code
}

pub fn cmd_shot(brain: Option<&str>, out: Option<&str>, open: Option<&str>, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let dir = match out {
        Some(o) => std::path::PathBuf::from(o),
        None => webagent::config::data_dir().join("shots"),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[shot] Zielverzeichnis nicht anlegbar: {e}");
        return 2;
    }
    let targets: Vec<String> = match brain {
        Some(b) => vec![b.to_string()],
        None => webagent::config::available_brain_ids(),
    };
    let mut failures = 0;
    for id in &targets {
        let mut backend = match WebBrainBackend::from_config(id) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[shot] {id}: {e}");
                failures += 1;
                continue;
            }
        };
        eprintln!("[shot] {id}: nehme Oberflaeche auf (headless={headless})…");
        match backend.live_screenshot_with(headless, open) {
            Ok(png) => {
                let path = dir.join(format!("{id}.png"));
                match std::fs::write(&path, &png) {
                    Ok(()) => {
                        println!("  {:<10} {} KB -> {}", id, png.len() / 1024, path.display())
                    }
                    Err(e) => {
                        eprintln!("[shot] {id}: Schreiben fehlgeschlagen: {e}");
                        failures += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("[shot] {id}: Fehler: {e}");
                failures += 1;
            }
        }
    }
    if failures > 0 {
        eprintln!("[shot] {failures}/{} fehlgeschlagen", targets.len());
        1
    } else {
        0
    }
}

pub fn cmd_survey(
    brain: Option<&str>,
    write: bool,
    headless: bool,
    dump: bool,
    open: Option<&str>,
) -> i32 {
    use webagent::browser::WebBrainBackend;

    let targets: Vec<String> = match brain {
        Some(b) => vec![b.to_string()],
        None => webagent::config::available_brain_ids(),
    };
    let mut failures = 0;
    for id in &targets {
        let mut backend = match WebBrainBackend::from_config(id) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[survey] {id}: {e}");
                failures += 1;
                continue;
            }
        };
        eprintln!("[survey] {id}: oeffne Oberflaeche (headless={headless})…");
        let report = match backend.live_survey_with(headless, open) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[survey] {id}: Fehler: {e}");
                failures += 1;
                continue;
            }
        };
        let buttons = report
            .get("buttons")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();
        let has_composer = report
            .get("counts")
            .and_then(|c| c.get("composer"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            > 0;
        let options = webagent::capability::detect_ui_options(&buttons, has_composer);

        if dump {
            // Rohe Beschriftungen zeigen, damit die Stichwortlisten an echten
            // Texten wachsen statt an geratenen.
            let mut seen: Vec<String> = Vec::new();
            for b in &buttons {
                let label = ["al", "ti", "dt", "tp"]
                    .iter()
                    .filter_map(|k| b.get(*k).and_then(|v| v.as_str()))
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" | ");
                if !label.is_empty() && !seen.contains(&label) {
                    seen.push(label);
                }
            }
            println!("  --- {id}: ROH (erste 5) ---");
            for b in buttons.iter().take(40) {
                println!("      {b}");
            }
            println!(
                "  --- {id}: counts = {} ---",
                report
                    .get("counts")
                    .map(|c| c.to_string())
                    .unwrap_or_default()
            );
            println!("  --- {id}: {} beschriftete Elemente ---", seen.len());
            for l in seen.iter().take(60) {
                println!("      {l}");
            }
        }

        if options.is_empty() {
            // Kein Fund ist ein Messfehler, kein Ergebnis — sonst schriebe man
            // eine leere Wahrheit fest und das Brain stuende dauerhaft auf [n/0].
            eprintln!(
                "[survey] {id}: nichts erkannt ({} Buttons, composer={}) — nicht geschrieben",
                buttons.len(),
                has_composer
            );
            eprintln!(
                "[survey] {id}: url={} title={:?}",
                report
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<keine>"),
                report.get("title").and_then(|v| v.as_str()).unwrap_or("")
            );
            failures += 1;
            continue;
        }

        println!(
            "  {:<10} {} Optionen aus {} Buttons: {}",
            id,
            options.len(),
            buttons.len(),
            options.join(", ")
        );

        if write {
            match write_ui_options(id, &options) {
                Ok(p) => println!("             geschrieben nach {}", p.display()),
                Err(e) => {
                    eprintln!("[survey] {id}: Schreiben fehlgeschlagen: {e}");
                    failures += 1;
                }
            }
        }
    }
    if failures > 0 {
        eprintln!("[survey] {failures}/{} fehlgeschlagen", targets.len());
        1
    } else {
        0
    }
}

/// Leitfaehigkeit: ableiten einer Brain-ID aus einer URL (host bis zum ersten
/// `.com/.ai/.dev/...`). Rein regelbasiert, keine Raten.
fn brain_id_from_url(url: &str) -> String {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(url)
        .split(':')
        .next()
        .unwrap_or(url);
    let domain = host.split('.').collect::<Vec<_>>();
    // `www.perplexity.ai` → perplexity, nicht www; `chat.deepseek.com` → chat.
    let first = match domain.as_slice() {
        ["www", rest, ..] => rest,
        [first, ..] => first,
        [] => host,
    };
    webagent::config::sanitize_brain_id(first)
}

/// Baut die Selektor-JSON eines Brains aus den Proposals zusammen.
fn selectors_from_proposals(proposals: &[webagent::brain_probe::Proposal]) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for p in proposals {
        obj.entry(p.selector_key.to_string())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()))
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::String(p.selector.clone()));
    }
    serde_json::Value::Object(obj)
}

/// `webagent probe`: Oberflaechen-Analyse wie die Link-Analyse in JDownloader.
///
/// Erkennt Bedienelemente einer Chat-Oberflaeche und macht daraus Selektoren —
/// fuer einen neuen Brain (`--url`) oder als Nachvermessung eines bestehenden
/// (`--brain`). Mit `--write` wird das Brain automatisch eingebunden.
pub fn cmd_probe(
    url: Option<&str>,
    brain_id: Option<&str>,
    brain: Option<&str>,
    write: bool,
    verify: bool,
    headless: bool,
) -> i32 {
    use webagent::brain_probe::{Proposal, Verdict};
    use webagent::browser::WebBrainBackend;

    let (id, url, is_new) = match (brain, url) {
        (Some(b), None) => (b.to_string(), None, false),
        (None, Some(u)) => {
            let id = brain_id
                .map(|s| webagent::config::sanitize_brain_id(s))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| brain_id_from_url(u));
            (id, Some(u.to_string()), true)
        }
        (Some(_), Some(_)) => {
            eprintln!("[probe] --brain und --url schliessen sich aus");
            return 2;
        }
        (None, None) => {
            eprintln!("[probe] bitte --url <chat-url> (neuer Brain) oder --brain <id> angeben");
            return 2;
        }
    };

    // Für einen bestehenden Brain brauchen wir einen brauchbaren Ausgangs-
    // Selektorsatz, sonst ist die Analyse gegen einen leeren Katalog sinnlos.
    let mut backend = match (is_new, url.clone()) {
        (true, Some(u)) => match WebBrainBackend::from_url(&id, &u) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[probe] {id}: {e}");
                return 1;
            }
        },
        (true, None) => {
            eprintln!("[probe] {id}: keine URL fuer neues Brain");
            return 2;
        }
        (false, _) => match WebBrainBackend::from_config(&id) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[probe] {id}: {e}");
                return 1;
            }
        },
    };

    eprintln!("[probe] {id}: oeffne Oberflaeche (headless={headless})…");
    let proposals = match backend.probe_surface(headless) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[probe] {id}: Fehler: {e}");
            return 1;
        }
    };

    if proposals.is_empty() {
        eprintln!("[probe] {id}: keine Bedienelemente gefunden — ist ein Login noetig?");
        eprintln!("[probe] Tipp: mit --visible laeuft der Browser sichtbar, dann einloggen.");
        return 1;
    }

    // Nachvermessung eines bestehenden Brains: nur Funde zeigen, die es noch
    // nicht gibt — die schon vorhandenen Selektoren sind die Wahrheit.
    let known: Vec<String> = if is_new {
        Vec::new()
    } else {
        webagent::config::load_selectors(&id)
            .ok()
            .and_then(|s| s.as_object().map(|o| o.keys().cloned().collect()))
            .unwrap_or_default()
    };

    println!("  --- {id}: Oberflaechen-Analyse ({}) ---", proposals.len());
    for p in &proposals {
        let is_new_find = !known.contains(&p.selector_key.to_string());
        let marker = if is_new_find { "+" } else { "=" };
        println!(
            "   {marker} {:<22} {:>3}%  {:<14} {}",
            p.selector_key,
            p.confidence,
            p.evidence,
            p.selector
        );
    }

    // Verifikation: nur die Vorschlaege, die einen messbaren Zustandswechsel
    // versprechen (Toggles/Schalter). Composer/Senden wuerden Nebenwirkungen
    // erzeugen — die bleiben ein Fund, kein Beleg.
    let mut verdicts: Vec<Verdict> = Vec::new();
    if verify {
        println!();
        for p in proposals
            .iter()
            .filter(|p| p.selector_key.ends_with("toggle") || p.selector_key.contains("switch"))
        {
            println!("   pruefe {} ({})…", p.selector_key, p.selector);
            match backend.verify_surface(headless, p) {
                Ok(v) => {
                    let proven = v.proven;
                    let note = v.note.clone();
                    verdicts.push(v);
                    println!(
                        "   {:<22} {}",
                        p.selector_key,
                        if proven { "PASS" } else { "FAIL" }
                    );
                    println!("        {note}");
                }
                Err(e) => eprintln!("   {:<22} FEHLER: {e}", p.selector_key),
            }
        }
    }

    if !write {
        eprintln!();
        eprintln!("[probe] kein Schreiben (--write fehlt). Funde oben = Vorschlaege.");
        return 0;
    }

    // Schreiben: die Vorschlaege als Selektoren-Datei ablegen und das Brain
    // (falls neu) in custom_brains.json registrieren.
    let all: Vec<Proposal> = proposals;
    let fresh = selectors_from_proposals(&all);
    let dir = webagent::config::user_selectors_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[probe] {id}: Selektoren-Verzeichnis nicht anlegbar: {e}");
        return 1;
    }
    let path = dir.join(format!("{id}.json"));

    // Bestehende Datei mergen statt ueberschreiben: der Prober ist ein
    // Ergaenzungs-Tool, keine Neuschreibung.
    let merged = if let Ok(existing) = webagent::config::load_selectors(&id) {
        let mut obj = existing;
        if let Some(map) = obj.as_object_mut() {
            if let Some(fresh_map) = fresh.as_object() {
                for (k, v) in fresh_map {
                    map.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }
        obj
    } else {
        fresh
    };

    let body = match serde_json::to_string_pretty(&merged) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[probe] {id}: JSON-Fehler: {e}");
            return 1;
        }
    };
    if let Err(e) = std::fs::write(&path, body) {
        eprintln!("[probe] {id}: schreiben fehlgeschlagen: {e}");
        return 1;
    }
    println!("             Selektoren geschrieben nach {}", path.display());

    if is_new {
        match webagent::config::register_custom_brain(&id, &url.unwrap_or_default()) {
            Ok(true) => println!("             Brain '{id}' registriert (custom_brains.json)"),
            Ok(false) => eprintln!("[probe] {id}: ID bereits vergeben — nicht ueberschrieben"),
            Err(e) => {
                eprintln!("[probe] {id}: Registrierung fehlgeschlagen: {e}");
                return 1;
            }
        }
    } else {
        println!("             Brain '{id}' (bestehend) um Selektoren ergaenzt");
    }

    if !verdicts.is_empty() {
        let passed = verdicts.iter().filter(|v| v.proven).count();
        println!(
            "             Verifikation: {passed}/{} belegt",
            verdicts.len()
        );
    }
    0
}

pub fn cmd_quests(json: bool) -> i32 {
    let levels = webagent::capability::levels_all();
    if levels.is_empty() {
        println!("[quests] keine Brains registriert");
        return 2;
    }

    if json {
        let payload: Vec<_> = levels
            .iter()
            .map(|l| {
                serde_json::json!({
                    "brain_id": l.brain_id,
                    "level": l.level(),
                    // null = unvermessen; bewusst kein geratener Zahlenwert.
                    "max_level": l.max_level(),
                    "surveyed": l.surveyed,
                    "rank": l.rank(),
                    "have": l.have,
                    "quests": l.quests.iter().map(|q| serde_json::json!({
                        "key": q.key,
                        "label": q.label,
                        "blocker": q.blocker.as_str(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("[quests] JSON-Fehler: {e}");
                return 1;
            }
        }
        return 0;
    }

    println!();
    println!("      GREETINGS PROFESSOR.");
    println!("      SHALL WE PLAY A GAME?");
    println!();

    let total: usize = levels.iter().map(|l| l.level()).sum();
    let total_max: usize = levels.iter().filter_map(|l| l.max_level()).sum();

    let unreachable: usize = levels.iter().map(|l| l.out_of_reach.len()).sum();
    println!("  ── POKIDEX ────────────────────────────────────────────");
    for l in &levels {
        println!(
            "   {:<12} {:<8} {}  {:<14} {}",
            l.brain_id,
            l.label()
                .rsplit_once('[')
                .map(|(_, r)| format!("[{r}"))
                .unwrap_or_default(),
            level_bar(l.level(), l.max_level().unwrap_or(0), 8),
            l.rank(),
            if l.maxed() { "ausgereizt" } else { "" }
        );
    }
    println!();
    let unsurveyed = levels.iter().filter(|l| !l.surveyed).count();
    if unsurveyed > 0 {
        println!(
            "  GESAMT {total}/?  —  {unsurveyed} von {} Eintraegen noch nicht vermessen.",
            levels.len()
        );
        println!("  Ohne Zaehlung der Oberflaeche ist kein Maximum bekannt (nicht geraten).");
    } else {
        println!(
            "  GESAMT {total}/{total_max}  {}",
            level_bar(total, total_max, 20)
        );
    }
    if unreachable > 0 {
        // Sichtbar halten statt verschweigen: sonst sieht es aus, als gaebe es
        // diese Optionen nicht — dabei gibt es sie, nur nicht fuer uns.
        println!(
            "  ({unreachable} angebotene Optionen sind fuer diesen Agenten prinzipiell\n   nicht nachweisbar fahrbar und stehen deshalb nicht im Nenner.)"
        );
        println!();
    }

    let log = webagent::capability::quest_log();
    if log.is_empty() {
        println!("  KEINE OFFENEN QUESTS. A STRANGE GAME.");
        return 0;
    }

    println!("  ── OFFENE QUESTS ──────────────────────────────────────");
    println!("  (nach Reichweite: oben bringt eine Umsetzung die meisten Level)");
    println!();
    for (key, quests) in &log {
        let label = webagent::capability::capability(key)
            .map(|c| c.label)
            .unwrap_or(key.as_str());
        println!("   {label}  (+{} Level)", quests.len());
        for q in quests {
            println!("      {:<10} {}", q.brain_id, q.blocker.as_str());
        }
        println!();
    }
    println!("  THE ONLY WINNING MOVE IS TO IMPLEMENT.");
    0
}

/// Balken der Breite `width` fuer `have/max`. Bei `max == 0` bewusst leer:
/// ein Brain ohne bekanntes Angebot soll nicht wie ein volles aussehen.
pub fn level_bar(have: usize, max: usize, width: usize) -> String {
    if max == 0 {
        return "·".repeat(width);
    }
    let filled = (have * width).div_ceil(max).min(width);
    format!("{}{}", "▓".repeat(filled), "░".repeat(width - filled))
}

/// Schreibt die gefundenen `ui_options` in die Nutzer-Selektordatei.
/// Bewusst die Nutzer-Kopie unter `<stable_root>/selectors`: der Quellbaum ist
/// bei einer deployten exe evtl. gar nicht da, und mitgelieferte Dateien
/// sollen von der Automatik nicht ueberschrieben werden.
pub fn write_ui_options(brain: &str, options: &[String]) -> Result<std::path::PathBuf, String> {
    let mut sel = webagent::config::load_selectors(brain)
        .map_err(|e| format!("Selektoren nicht lesbar: {e}"))?;
    let obj = sel
        .as_object_mut()
        .ok_or_else(|| "Selektordatei ist kein JSON-Objekt".to_string())?;
    obj.insert(
        "ui_options".to_string(),
        serde_json::Value::Array(
            options
                .iter()
                .map(|o| serde_json::Value::String(o.clone()))
                .collect(),
        ),
    );
    let dir = webagent::config::user_selectors_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    let path = dir.join(format!("{brain}.json"));
    let body = serde_json::to_string_pretty(&sel).map_err(|e| format!("{e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("{e}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brain_id_wird_aus_url_abgeleitet() {
        assert_eq!(brain_id_from_url("https://www.perplexity.ai/"), "perplexity");
        assert_eq!(brain_id_from_url("https://chat.deepseek.com/"), "chat");
        assert_eq!(brain_id_from_url("https://gemini.google.com/app"), "gemini");
        assert_eq!(brain_id_from_url("https://chat.qwen.ai/"), "chat");
        // Kaputte URLs werden nicht schlimmer: kein Panic, ein brauchbarer Fallback.
        assert_eq!(brain_id_from_url(""), "");
    }

    #[test]
    fn brain_id_aus_url_wird_sanitisiert() {
        assert_eq!(brain_id_from_url("https://www.mistral.ai/chat"), "mistral");
        assert_eq!(brain_id_from_url("https://chat.deepseek.com/"), "chat");
    }

    #[test]
    fn selectors_from_proposals_sammelt_schluessel() {
        let p = |key: &'static str, sel: &'static str| webagent::brain_probe::Proposal {
            capability_key: "chat",
            selector_key: key,
            selector: sel.to_string(),
            confidence: 90,
            evidence: "test".into(),
        };
        let json = selectors_from_proposals(&[p("composer", "[contenteditable]"), p("send_button", "button.send")]);
        let obj = json.as_object().unwrap();
        assert_eq!(obj["composer"][0], "[contenteditable]");
        assert_eq!(obj["send_button"][0], "button.send");
    }
}
