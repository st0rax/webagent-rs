//! Konfiguration: Pfade, Brain-Definitionen, Umgebungsvariablen.
//!
//! Portiert aus ../src/webagent/config.py

mod brains;
mod clone;
mod limits;
mod paths;
mod profiles;
mod selectors;
mod writeback;

pub use brains::{
    block_cooldown_secs, bot2bot_root, brains, consensus_workspace, custom_brains_path,
    ensure_data_dirs, load_custom_brains, parse_custom_brains, persist_browser_tabs,
    reference_profile_dir, reference_profile_dir_in, register_custom_brain, resolve_selectors_path,
    retry_unavailable_secs, sanitize_brain_id, selectors_dir, shared_debug_port,
    stale_heartbeat_secs, swarm_profile_dir, swarm_profile_dir_in, use_shared_browser,
    use_sparse_profile_copy, user_selectors_dir, BRAIN_TABLE, SPARSE_COPY_WHITELIST,
};
pub use clone::{CloneEntry, DryRunReport, ProfileClonePlan, ProfileClonePlanner};
pub use limits::{
    max_observation_chars, max_observation_chars_for, max_run_wall_secs, resolve_max_run_wall_secs,
    DEFAULT_MAX_OBSERVATION_CHARS, LOOP_GUARD_ABORT_COUNT, LOOP_GUARD_WARN_COUNT,
    MAX_RUN_WALL_SECONDS,
};
pub use paths::{
    data_dir, memory_dir, profiles_dir, root_dir, runs_dir, shared_profile_dir, src_dir,
    webagent_root_stable,
};
pub use profiles::{
    cleanup_swarm_profiles, cleanup_swarm_profiles_in, copy_dir_all, copy_dir_sparse,
    copy_dir_without_caches, ensure_stable_layout, prepare_swarm_profile, prepare_swarm_profile_in,
    sweep_stale_runtime_profiles, sweep_stale_runtime_profiles_in,
};
pub use selectors::{
    available_brain_ids, debug_port, embedded_selector, encapsulated_profile_dir, load_selectors,
    shipped_selector_table, shipped_selectors, user_selectors,
};
#[cfg(feature = "webview")]
pub(crate) use writeback::prepare_shared_profile_for_clone;
pub use writeback::{
    runtime_pool_profile_dir, seal_master_profile, unseal_master_profile, write_back_dir_to_master,
    write_back_session_to_master,
};

pub(crate) use selectors::fnv1a;

#[cfg(test)]
mod tests {
    use super::profiles::restore_sparse_backup;
    use super::selectors::{merge_selectors, EMBEDDED_SELECTORS};
    use super::writeback::{
        bytes_contain, cookies_db_bytes, cookies_db_path, has_login_artifacts,
        master_missing_sessions, reserve_unique_backup_dir, runtime_lost_sessions,
        write_back_dir_to_master_at, write_back_is_safe,
    };
    use super::*;
    use std::env;

    /// Der Rueckweg darf eine gute Anmeldung niemals durch eine leere Kopie
    /// ersetzen.
    ///
    /// Ohne diese Schranke wuerde ausgerechnet die Reparatur den Schaden
    /// anrichten: ein Lauf, der frueh scheitert oder mit leerem Profil startet,
    /// wuerde das Master ueberschreiben und alle acht Brains auf einen Schlag
    /// abmelden.
    #[test]
    fn rueckweg_lehnt_eine_kopie_ohne_login_artefakte_ab() {
        let base = std::env::temp_dir().join(format!("webagent_wb_{}", std::process::id()));
        let leer = base.join("leer");
        let voll = base.join("voll");
        std::fs::create_dir_all(leer.join("EBWebView/Default")).unwrap();
        std::fs::create_dir_all(voll.join("EBWebView/Default/Network")).unwrap();
        // Nur Krimskrams, keine Anmeldung.
        std::fs::write(leer.join("EBWebView/Default/History"), b"x").unwrap();
        // Eine echte Anmeldung liegt unter Default/Network/Cookies.
        std::fs::write(voll.join("EBWebView/Default/Network/Cookies"), b"x").unwrap();

        assert!(
            !has_login_artifacts(&leer),
            "eine Kopie ohne Cookies/Local State/Login Data ist ausgeloggt"
        );
        assert!(
            has_login_artifacts(&voll),
            "Cookies liegen bei WebView2 unter Default/Network — rekursiv suchen"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `assistant_message` darf NIE den laufenden Streaming-Container treffen.
    ///
    /// Real 2026-07-26: claudes Liste enthielt `div[data-is-streaming='true']`.
    /// Der Scraper las damit die Denk-Anzeige ("Crystallizing" samt
    /// Private-Use-Glyph) als fertige Antwort. Auswertung ueber 176 Laeufe:
    /// mit Denk-Glyph im Transkript 67% protocol_error, ohne 3% — Faktor 22,
    /// und 73 der 75 Protokollfehler hatten den Glyph. Schlimmer als die
    /// verfaelschte Messung: der Harness feuerte daraufhin identische
    /// Reparatur-Prompts, bis die Gegenseite das Gespraech beendete.
    #[test]
    fn assistant_message_trifft_nie_den_streaming_container() {
        for (brain, json_text) in EMBEDDED_SELECTORS {
            let v: serde_json::Value =
                serde_json::from_str(json_text).unwrap_or_else(|e| panic!("{brain}: {e}"));
            let Some(list) = v.get("assistant_message").and_then(|x| x.as_array()) else {
                continue;
            };
            for sel in list {
                let sel = sel.as_str().unwrap_or_default();
                assert!(
                    !sel.contains("data-is-streaming"),
                    "{brain}: assistant_message enthaelt den Streaming-Container `{sel}`                      — der Scraper liest damit die Denk-Anzeige als Antwort"
                );
            }
        }
    }

    #[test]
    fn test_root_dir_exists() {
        let root = root_dir();
        assert!(root.exists(), "Root-Verzeichnis sollte existieren");
        assert!(root.is_dir(), "Root sollte ein Verzeichnis sein");
    }

    #[test]
    fn test_embedded_selectors_cover_all_brains_and_parse() {
        // Beweist Portabilitaet: jede heruntergeladene exe hat die Selektoren
        // fuer alle BRAIN_TABLE-Brains eingebettet und sie sind gueltiges JSON.
        for (id, _url) in BRAIN_TABLE {
            let embedded = embedded_selector(id)
                .unwrap_or_else(|| panic!("kein eingebetteter Selektor fuer Brain '{id}'"));
            let parsed: serde_json::Value = serde_json::from_str(embedded)
                .unwrap_or_else(|e| panic!("eingebetteter Selektor '{id}' ist kein JSON: {e}"));
            assert!(
                parsed.is_object(),
                "eingebetteter Selektor '{id}' sollte ein JSON-Objekt sein"
            );
        }
    }

    #[test]
    fn test_embedded_selector_unknown_brain_is_none() {
        assert!(embedded_selector("does-not-exist").is_none());
    }

    #[test]
    fn test_brains_config() {
        let brains = brains();
        assert!(
            !brains.is_empty(),
            "Mindestens ein Brain sollte konfiguriert sein"
        );

        // ChatGPT sollte vorhanden sein
        assert!(brains.contains_key("chatgpt"));
        let chatgpt = &brains["chatgpt"];
        assert!(chatgpt.contains_key("url"));
        assert!(chatgpt.contains_key("selectors"));
        assert!(chatgpt.contains_key("profile_dir"));
    }

    #[test]
    fn test_available_brain_ids() {
        let ids = available_brain_ids();
        assert!(!ids.is_empty());
        assert!(ids.contains(&"chatgpt".to_string()));

        // Sollte sortiert sein
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_debug_port_deterministic_and_in_range() {
        let p1 = debug_port("chatgpt");
        assert_eq!(p1, debug_port("chatgpt"), "deterministisch");
        assert!((9222..9622).contains(&p1), "in Range: {p1}");
        // Die 8 konfigurierten Brains sollten großteils verschiedene Ports haben.
        let ports: std::collections::HashSet<u16> =
            BRAIN_TABLE.iter().map(|(id, _)| debug_port(id)).collect();
        assert!(ports.len() >= 6, "zu viele Port-Kollisionen: {ports:?}");
    }

    #[test]
    fn test_parity_constants() {
        assert_eq!(DEFAULT_MAX_OBSERVATION_CHARS, 12_000);
        // Ohne Env-Ueberschreibung gilt der Standard.
        assert!(max_observation_chars() >= 1_000);
        assert_eq!(LOOP_GUARD_WARN_COUNT, 3);
        assert_eq!(LOOP_GUARD_ABORT_COUNT, 8);
    }

    #[test]
    fn test_resolve_max_run_wall_secs_parsing() {
        // Default-Fälle: None, leer, nur Whitespace, "0", Müll → Default.
        assert_eq!(resolve_max_run_wall_secs(None), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("   ")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("0")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("abc")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("-5")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("12x")), MAX_RUN_WALL_SECONDS);
        // Gültige positive Werte (auch mit umgebendem Whitespace) → übernommen.
        assert_eq!(resolve_max_run_wall_secs(Some("900")), 900);
        assert_eq!(resolve_max_run_wall_secs(Some("  900  ")), 900);
        assert_eq!(resolve_max_run_wall_secs(Some("1")), 1);
        assert_eq!(MAX_RUN_WALL_SECONDS, 600);
    }

    /// Der Fall vom 07.08.2026, als Zahlenpaar.
    ///
    /// Das Hauptprofil trug nach dem Login 108 KB Cookies, die Laufzeit-Kopie
    /// brachte 40 KB zurueck — und danach meldete der halbe Lauf „Login
    /// noetig". Dieser Test ist der Waechter dagegen.
    #[test]
    fn aermere_kopie_darf_das_hauptprofil_nicht_ueberschreiben() {
        assert!(!write_back_is_safe(40 * 1024, 108 * 1024));
    }

    #[test]
    fn leere_kopie_wird_immer_abgelehnt() {
        assert!(!write_back_is_safe(0, 108 * 1024));
        assert!(
            !write_back_is_safe(0, 0),
            "nichts zu schreiben ist kein Fortschritt"
        );
    }

    #[test]
    fn leeres_ziel_nimmt_jede_quelle() {
        // Ein frisch angelegtes Master hat nichts zu verlieren.
        assert!(write_back_is_safe(1024, 0));
    }

    #[test]
    fn normales_atmen_der_datenbank_loest_keinen_fehlalarm_aus() {
        // SQLite schrumpft auch beim Aufraeumen. Ein Schutz, der bei jeder
        // Schwankung anschlaegt, wird abgeschaltet — und schuetzt dann nie.
        let target = 100 * 1024;
        assert!(write_back_is_safe(95 * 1024, target));
        assert!(write_back_is_safe(70 * 1024, target));
        // Genau auf der Schwelle noch erlaubt, knapp darunter nicht mehr.
        assert!(write_back_is_safe(60 * 1024, target));
        assert!(!write_back_is_safe(59 * 1024, target));
    }

    #[test]
    fn gewachsene_kopie_ist_selbstverstaendlich_erlaubt() {
        // Der Normalfall: der Browser hat Sitzungen erneuert, die Kopie ist
        // reicher als das Master. Genau dafuer gibt es das Rueckschreiben.
        assert!(write_back_is_safe(200 * 1024, 100 * 1024));
    }

    #[test]
    fn bytes_contain_findet_und_vermisst() {
        assert!(bytes_contain(b"a b kimi-auth c", "kimi-auth"));
        assert!(bytes_contain(
            b"prefix __Secure-next-auth.session-token.0",
            "__Secure-next-auth.session-token"
        ));
        assert!(!bytes_contain(b"a b c", "kimi-auth"));
        assert!(bytes_contain(b"", ""));
        assert!(!bytes_contain(b"", "kimi-auth"));
    }

    /// Der Fall vom 08.08.2026: das Master trug kimi-auth, die Laufzeit-Kopie
    /// hatte es verloren. Das Rueckschreiben haette die gueltige Anmeldung
    /// vernichtet - das Gewichts-Mass sah es nicht, der pro-Brain-Nachweis muss
    /// es sehen.
    #[test]
    fn kopie_die_eine_sitzung_verloren_hat_darf_master_nicht_ueberschreiben() {
        let master = b"kimi-auth mistral ory_session";
        let runtime = b"mistral ory_session";
        assert_eq!(runtime_lost_sessions(master, runtime), vec!["kimi"]);
    }

    #[test]
    fn kopie_mit_rotierter_sitzung_ist_kein_verlust() {
        // Sitzung erneuert: der Cookie-Name bleibt, nur der Wert ist neu.
        let master = b"kimi-auth";
        let runtime = b"kimi-auth";
        assert!(runtime_lost_sessions(master, runtime).is_empty());
    }

    #[test]
    fn kanonisch_eingeloggt_aber_master_nicht_wird_gemeldet() {
        let canonical = b"kimi-auth";
        let master = b"no kimi here";
        assert_eq!(master_missing_sessions(canonical, master), vec!["kimi"]);
        assert!(master_missing_sessions(b"kimi-auth", b"kimi-auth").is_empty());
    }

    #[test]
    fn fehlende_cookies_db_zaehlt_als_leer() {
        assert!(runtime_lost_sessions(&[], b"kimi-auth").is_empty());
        assert!(master_missing_sessions(b"kimi-auth", &[]).contains(&"kimi"));
    }

    #[test]
    fn cookies_db_wird_verschachtelt_gefunden() {
        let tmp = std::env::temp_dir().join(format!("wa_cookies_db_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let nested = tmp.join("EBWebView").join("Default").join("Network");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("Cookies"), b"kimi-auth").unwrap();
        // Nur die exakte Datei zaehlt, nicht Journal oder Backups.
        std::fs::write(nested.join("Cookies-journal"), b"x").unwrap();
        std::fs::write(nested.join("Cookies.bak"), b"kimi-auth").unwrap();
        assert_eq!(cookies_db_path(&tmp), Some(nested.join("Cookies")));
        assert!(bytes_contain(&cookies_db_bytes(&tmp), "kimi-auth"));
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn writeback_rollback_restores_master_after_strict_copy_failure() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_writeback_rollback_{stamp}"));
        let master = base.join("master");
        let runtime = base.join("runtime");
        let backup = base.join("backup");
        fs::create_dir_all(master.join("Cookies")).unwrap();
        fs::write(master.join("Local State"), b"master-state").unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(runtime.join("Cookies"), b"kimi-auth runtime").unwrap();
        fs::write(runtime.join("Local State"), b"runtime-state").unwrap();

        let error = write_back_dir_to_master_at(&runtime, &master, &backup).unwrap_err();
        assert!(error.contains("wiederhergestellt"), "{error}");
        assert!(
            master.join("Cookies").is_dir(),
            "Kollision bleibt Verzeichnis"
        );
        assert_eq!(
            fs::read(master.join("Local State")).unwrap(),
            b"master-state"
        );
        assert!(
            backup.is_dir(),
            "erfolgreiches Backup bleibt als Evidenz erhalten"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_backup_failure_blocks_master_update() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_writeback_backup_{stamp}"));
        let master = base.join("master");
        let runtime = base.join("runtime");
        let backup = base.join("backup-is-file");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(master.join("Cookies"), b"kimi-auth master").unwrap();
        fs::write(runtime.join("Cookies"), b"kimi-auth runtime").unwrap();
        fs::write(&backup, b"block backup directory").unwrap();

        let error = write_back_dir_to_master_at(&runtime, &master, &backup).unwrap_err();
        assert!(error.contains("Sicherung"), "{error}");
        assert_eq!(
            fs::read(master.join("Cookies")).unwrap(),
            b"kimi-auth master"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_strict_happy_path_updates_master_after_backup() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_writeback_happy_{stamp}"));
        let master = base.join("master");
        let runtime = base.join("runtime");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(master.join("Cookies"), b"kimi-auth master").unwrap();
        fs::write(master.join("Local State"), b"master-state").unwrap();
        fs::write(runtime.join("Cookies"), b"kimi-auth refreshed").unwrap();
        fs::write(runtime.join("Local State"), b"runtime-state").unwrap();

        write_back_dir_to_master_at(&runtime, &master, &backup).unwrap();
        assert_eq!(
            fs::read(master.join("Cookies")).unwrap(),
            b"kimi-auth refreshed"
        );
        assert_eq!(
            fs::read(master.join("Local State")).unwrap(),
            b"runtime-state"
        );
        assert!(
            backup.is_dir(),
            "vorheriger Master-Zustand bleibt gesichert"
        );
        let _ = fs::remove_dir_all(&base);
    }
    #[test]
    fn writeback_invalid_runtime_leaves_master_and_backup_untouched() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_writeback_prevalidate_{stamp}"));
        let runtime = base.join("runtime");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&runtime).unwrap();
        fs::create_dir_all(&master).unwrap();
        fs::write(master.join("Cookies"), b"master-login-state").unwrap();

        let error = write_back_dir_to_master_at(&runtime, &master, &backup).unwrap_err();
        assert!(error.contains("keine Login-Artefakte"), "{error}");
        assert_eq!(
            fs::read(master.join("Cookies")).unwrap(),
            b"master-login-state"
        );
        assert!(
            !backup.exists(),
            "ungÃ¼ltige Laufzeitquelle darf keinen Backup-Pfad anlegen"
        );
        let _ = fs::remove_dir_all(&base);
    }
    #[test]
    fn writeback_zero_weight_master_still_receives_backup_snapshot() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_zero_weight_backup_{stamp}"));
        let runtime = base.join("runtime");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&runtime).unwrap();
        fs::create_dir_all(&master).unwrap();
        fs::write(master.join("Local State"), b"").unwrap();
        fs::write(runtime.join("Cookies"), b"runtime-login").unwrap();
        fs::write(runtime.join("Local State"), b"runtime-state").unwrap();

        write_back_dir_to_master_at(&runtime, &master, &backup).unwrap();
        assert!(
            backup.join("Local State").exists(),
            "zero-weight master requires a snapshot"
        );
        assert_eq!(fs::read(master.join("Cookies")).unwrap(), b"runtime-login");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_os_file_lock_blocks_master_mutation() {
        use fs2::FileExt;
        use std::fs::{self, OpenOptions};
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_os_lock_{stamp}"));
        let runtime = base.join("runtime");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&runtime).unwrap();
        fs::create_dir_all(&master).unwrap();
        fs::write(runtime.join("Cookies"), b"runtime-login").unwrap();
        fs::write(runtime.join("Local State"), b"runtime-state").unwrap();
        fs::write(master.join("Cookies"), b"master-login").unwrap();
        let lock = master.with_file_name("shared.session-writeback.lock");
        let holder = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock)
            .unwrap();
        holder.lock_exclusive().unwrap();

        let error = write_back_dir_to_master_at(&runtime, &master, &backup).unwrap_err();
        assert!(error.contains("gesperrt"), "{error}");
        assert_eq!(fs::read(master.join("Cookies")).unwrap(), b"master-login");
        assert!(
            !backup.exists(),
            "held OS lock must block before backup mutation"
        );
        holder.unlock().unwrap();
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_pending_journal_recovers_before_new_validation() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_journal_recovery_{stamp}"));
        let runtime = base.join("runtime");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&runtime).unwrap();
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(master.join("Cookies"), b"corrupted").unwrap();
        fs::write(backup.join("Cookies"), b"last-good").unwrap();
        fs::write(
            master.with_file_name("shared.session-writeback.journal.pending"),
            format!("{}\n", backup.display()),
        )
        .unwrap();

        let error =
            write_back_dir_to_master_at(&runtime, &master, &base.join("new-backup")).unwrap_err();
        assert!(error.contains("keine Login-Artefakte"), "{error}");
        assert_eq!(fs::read(master.join("Cookies")).unwrap(), b"last-good");
        assert!(!master
            .with_file_name("shared.session-writeback.journal")
            .exists());
        let _ = fs::remove_dir_all(&base);
    }
    #[test]
    fn runtime_clone_preparation_recovers_pending_master_before_read() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_preclone_recovery_{stamp}"));
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(master.join("Cookies"), b"partial-master").unwrap();
        fs::write(backup.join("Cookies"), b"last-good").unwrap();
        fs::write(
            master.with_file_name("shared.session-writeback.journal.pending"),
            format!("{}\n", backup.display()),
        )
        .unwrap();

        let lock = super::writeback::prepare_master_for_runtime_clone(&master).unwrap();
        assert_eq!(fs::read(master.join("Cookies")).unwrap(), b"last-good");
        assert!(!master
            .with_file_name("shared.session-writeback.journal.pending")
            .exists());
        drop(lock);
        let _ = fs::remove_dir_all(&base);
    }
    #[test]
    fn writeback_committed_journal_preserves_verified_master() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_journal_commit_{stamp}"));
        let runtime = base.join("runtime");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&runtime).unwrap();
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(master.join("Cookies"), b"verified-new").unwrap();
        fs::write(backup.join("Cookies"), b"old-backup").unwrap();
        fs::write(
            master.with_file_name("shared.session-writeback.journal.committed"),
            format!("{}\n", backup.display()),
        )
        .unwrap();

        let error =
            write_back_dir_to_master_at(&runtime, &master, &base.join("new-backup")).unwrap_err();
        assert!(error.contains("keine Login-Artefakte"), "{error}");
        assert_eq!(fs::read(master.join("Cookies")).unwrap(), b"verified-new");
        assert!(!master
            .with_file_name("shared.session-writeback.journal.committed")
            .exists());
        let _ = fs::remove_dir_all(&base);
    }
    #[test]
    fn writeback_backup_reservations_are_unique() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_backup_reserve_{stamp}"));
        let master = base.join("shared");
        fs::create_dir_all(&master).unwrap();
        let first = reserve_unique_backup_dir(&master).unwrap();
        let second = reserve_unique_backup_dir(&master).unwrap();
        assert_ne!(first, second, "zwei Backups duerfen keinen Pfad teilen");
        assert!(
            first.is_dir() && second.is_dir(),
            "beide Pfade sind reserviert"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_restore_prunes_runtime_only_sparse_artifacts() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_restore_prune_{stamp}"));
        let backup = base.join("backup");
        let master = base.join("master");
        fs::create_dir_all(&backup).unwrap();
        fs::create_dir_all(&master).unwrap();
        fs::write(backup.join("Cookies"), b"kimi-auth backup").unwrap();
        fs::write(backup.join("Local State"), b"backup-state").unwrap();
        fs::write(master.join("Cookies"), b"kimi-auth runtime").unwrap();
        fs::write(master.join("Local State"), b"runtime-state").unwrap();
        fs::write(master.join("Preferences"), b"runtime-only").unwrap();

        restore_sparse_backup(&backup, &master).unwrap();
        assert_eq!(
            fs::read(master.join("Cookies")).unwrap(),
            b"kimi-auth backup"
        );
        assert_eq!(
            fs::read(master.join("Local State")).unwrap(),
            b"backup-state"
        );
        assert!(
            !master.join("Preferences").exists(),
            "runtime-only sparse artifact entfernt"
        );
        let _ = fs::remove_dir_all(&base);
    }
    #[test]
    fn test_persist_browser_tabs_defaults() {
        let shared_key = "WEBAGENT_USE_SHARED_BROWSER";
        let tabs_key = "WEBAGENT_PERSIST_TABS";
        let prev_shared = env::var(shared_key).ok();
        let prev_tabs = env::var(tabs_key).ok();
        env::set_var(shared_key, "1");
        env::remove_var(tabs_key);
        assert!(persist_browser_tabs());
        env::set_var(tabs_key, "0");
        assert!(!persist_browser_tabs());
        match prev_tabs {
            Some(v) => env::set_var(tabs_key, v),
            None => env::remove_var(tabs_key),
        }
        match prev_shared {
            Some(v) => env::set_var(shared_key, v),
            None => env::remove_var(shared_key),
        }
    }

    #[test]
    fn test_use_shared_browser_env_names() {
        let key = "WEBAGENT_USE_SHARED_BROWSER";
        let prev = env::var(key).ok();
        env::set_var(key, "1");
        assert!(use_shared_browser());
        env::set_var(key, "0");
        assert!(!use_shared_browser());
        match prev {
            Some(v) => env::set_var(key, v),
            None => env::remove_var(key),
        }
    }

    #[test]
    fn test_ensure_data_dirs() {
        // Sollte nicht fehlschlagen (erstellt Verzeichnisse oder sie existieren bereits)
        assert!(ensure_data_dirs().is_ok());
        assert!(data_dir().exists());
        assert!(runs_dir().exists());
    }

    #[test]
    fn test_prepare_swarm_profile_fallback_and_cleanup() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_prep_{}", stamp));
        let _ = fs::create_dir_all(&base);
        let run_id = format!("testswarm_{}", stamp);
        let brain = "chatgpt";
        let default = base.join(brain);
        let marker_src = default.join("_grok_swarm_marker.txt");
        let _ = fs::create_dir_all(&default);
        fs::write(&marker_src, b"swarm-src").expect("write marker");
        let _ = fs::write(default.join("SingletonLock"), b"pid");
        let _ = fs::write(default.join("lockfile"), b"x");

        let dst = prepare_swarm_profile_in(&base, &run_id, brain, false);
        assert!(dst.is_dir(), "swarm profile dir");
        assert!(
            dst.join("_grok_swarm_marker.txt").is_file(),
            "marker copied from profiles/<brain>"
        );
        assert!(
            !dst.join("SingletonLock").exists(),
            "lock files must be skipped"
        );
        assert!(!dst.join("lockfile").exists());

        cleanup_swarm_profiles_in(&base, &run_id).expect("cleanup");
        assert!(!dst.exists(), "cleaned after run");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_sweep_stale_runtime_profiles_spares_fresh_and_logins() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_sweep_{}", stamp));
        let swarm_orphan = base.join("swarm").join("deadrun_chatgpt");
        let enc_orphan = base.join("encapsulated").join("chatgpt_deadstamp");
        // Kanonische Profile — hier liegen die Logins, die darf der Sweep nie anfassen.
        let login_shared = base.join("shared");
        let login_brain = base.join("chatgpt");
        for d in [&swarm_orphan, &enc_orphan, &login_shared, &login_brain] {
            fs::create_dir_all(d).expect("mkdir");
            fs::write(d.join("marker"), b"x").expect("write");
        }

        // max_age = 0 -> jedes Wegwerf-Profil gilt als alt. Beide Wurzeln werden
        // erfasst, die Login-Profile aber nicht.
        assert_eq!(sweep_stale_runtime_profiles_in(&base, 0), 2);
        assert!(!swarm_orphan.exists(), "swarm-Waise entfernt");
        assert!(!enc_orphan.exists(), "encapsulated-Waise entfernt");
        assert!(login_shared.is_dir(), "shared-Login unangetastet");
        assert!(login_brain.is_dir(), "Brain-Login unangetastet");

        // Frisch + realistische Grenze -> ein laufender Run bleibt stehen.
        fs::create_dir_all(&swarm_orphan).expect("recreate");
        assert_eq!(sweep_stale_runtime_profiles_in(&base, 12 * 60 * 60), 0);
        assert!(swarm_orphan.is_dir(), "laufender Run darf nicht weg");

        // Fehlende Wurzeln sind kein Fehler.
        let _ = fs::remove_dir_all(&base);
        assert_eq!(sweep_stale_runtime_profiles_in(&base, 0), 0);
    }

    #[test]
    fn volle_profilkopie_laesst_caches_weg_aber_keine_logins() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_cachecopy_{stamp}"));
        let src = base.join("src");
        let dst = base.join("dst");

        // Anmeldebezogenes ...
        let net = src.join("EBWebView").join("Default").join("Network");
        fs::create_dir_all(&net).expect("mkdir");
        fs::write(net.join("Cookies"), b"keks").expect("write");
        let ls = src.join("EBWebView").join("Default").join("Local Storage");
        fs::create_dir_all(&ls).expect("mkdir");
        fs::write(ls.join("leveldb.log"), b"token").expect("write");

        // ... und reiner Cache, der 88 % des Volumens ausmacht.
        let cc = src.join("EBWebView").join("Default").join("Code Cache");
        fs::create_dir_all(&cc).expect("mkdir");
        fs::write(cc.join("gross.bin"), vec![0u8; 4096]).expect("write");
        let crx = src.join("EBWebView").join("component_crx_cache");
        fs::create_dir_all(&crx).expect("mkdir");
        fs::write(crx.join("x.crx"), b"crx").expect("write");

        copy_dir_without_caches(&src, &dst).expect("copy");

        let d = dst.join("EBWebView").join("Default");
        assert!(
            d.join("Network").join("Cookies").is_file(),
            "Cookies fehlen"
        );
        assert!(
            d.join("Local Storage").join("leveldb.log").is_file(),
            "Local Storage fehlt"
        );
        assert!(!d.join("Code Cache").exists(), "Code Cache mitkopiert");
        assert!(
            !dst.join("EBWebView").join("component_crx_cache").exists(),
            "crx-Cache mitkopiert"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn tests_never_write_into_the_production_data_dir() {
        // Real beobachtet: im Score-Log standen Eintraege mit brain_id "a" und
        // "b" neben den echten Brains — aus Testlaeufen, die in dieselbe Datei
        // schrieben wie der Betrieb. Das verfaelscht das Leaderboard.
        let d = data_dir();
        assert!(
            d.starts_with(std::env::temp_dir()),
            "unter cargo test muss data_dir im Temp liegen, ist aber {d:?}"
        );
        assert!(
            !d.starts_with(webagent_root_stable()),
            "darf nicht auf den Produktivort zeigen"
        );
    }

    #[test]
    fn test_sanitize_brain_id_blocks_path_escape() {
        assert_eq!(sanitize_brain_id("  MyBrain  "), "mybrain");
        assert_eq!(sanitize_brain_id("chat.z.ai"), "chat-z-ai");
        // Ein Eintrag in custom_brains.json darf nicht aus dem Datenverzeichnis ausbrechen.
        assert_eq!(sanitize_brain_id("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_brain_id("a/b\\c"), "a-b-c");
        assert!(!sanitize_brain_id("../x").contains(".."));
        assert_eq!(sanitize_brain_id("---"), "");
        assert_eq!(sanitize_brain_id(""), "");
    }

    #[test]
    fn test_load_custom_brains_skips_junk_and_builtin_shadowing() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("webagent_custom_{}", stamp));
        fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("custom_brains.json");

        // Kaputtes JSON darf den Agenten nicht lahmlegen -> leere Liste, kein Panic.
        fs::write(&path, b"{ not json").expect("write");
        assert!(parse_custom_brains(&fs::read_to_string(&path).unwrap()).is_empty());

        let raw = r#"[
            {"id": "Grok", "url": "https://grok.com/"},
            {"id": "chatgpt", "url": "https://evil.example/"},
            {"id": "grok", "url": "https://dup.example/"},
            {"id": "", "url": "https://nada/"},
            {"id": "leer-url", "url": "  "}
        ]"#;
        let got = parse_custom_brains(raw);
        assert_eq!(
            got,
            vec![("grok".to_string(), "https://grok.com/".to_string())],
            "eingebautes chatgpt nicht ueberschreibbar, Dubletten und Luecken raus"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_selectors_prefers_user_copy() {
        // Ohne Nutzer-Datei muss der mitgelieferte Pfad herauskommen.
        let p = resolve_selectors_path("chatgpt");
        assert!(p.to_string_lossy().ends_with("chatgpt.json"));
    }

    #[test]
    fn nutzer_overlay_ersetzt_nur_die_eigenen_schluessel() {
        // Der Kern des Overlays: der Mensch repariert `composer`, und alles
        // andere aus der Auslieferung bleibt sichtbar. Vorher ersetzte die
        // Nutzer-Datei die mitgelieferte komplett — ein Messschnappschuss
        // konnte damit gepflegte Selektoren dauerhaft verdecken.
        let mut base = serde_json::json!({
            "composer": ["#alt"],
            "send_button": ["#send"],
            "ui_options": ["chat", "new_chat"],
        });
        merge_selectors(&mut base, serde_json::json!({ "composer": ["#neu"] }));
        assert_eq!(base["composer"][0], "#neu", "Reparatur gewinnt");
        assert_eq!(base["send_button"][0], "#send", "ungenannt = unangetastet");
        assert_eq!(base["ui_options"][0], "chat");

        // Genannte Schluessel gewinnen ganz, nicht listenweise vereinigt: wer
        // einen gebrochenen Selektor ersetzt, will ihn los sein.
        merge_selectors(
            &mut base,
            serde_json::json!({ "send_button": ["#nur-der"] }),
        );
        assert_eq!(base["send_button"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn load_selectors_liefert_den_lieferstand_ohne_nutzer_datei() {
        // Unter `cargo test` zeigt user_selectors_dir() ins Temp — dieser Test
        // sieht also garantiert nur die ausgelieferten Daten, egal auf welcher
        // Maschine er laeuft.
        let sel = load_selectors("kimi").expect("kimi ist mitgeliefert");
        let opts = crate::capability::available_options(&sel).expect("ui_options gepflegt");
        assert!(opts.contains(&"chat".to_string()), "kimi kann chatten");
    }

    #[test]
    fn load_selectors_faellt_auf_die_generische_maske_zurueck() {
        // Ein registrierter, aber unvermessener Brain (keine mitgelieferte
        // Datei; unter cargo test auch keine Nutzer-Datei) bekommt die Maske —
        // statt des frueheren `NotFound`-Fehlers.
        let sel = load_selectors("perplexity").expect("Maske greift statt NotFound");
        for key in [
            "composer",
            "send_button",
            "stop_button",
            "new_chat_button",
            "login_button",
            "assistant_message",
        ] {
            let has = sel
                .get(key)
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            assert!(has, "Maske muss {key} liefern");
        }
        // Bewusst OHNE ui_options: die Maske belegt nichts von selbst, nur ein
        // bestandener Live-Lauf zaehlt ein Level (capability-proof).
        assert!(sel.get("ui_options").is_none());
    }

    #[test]
    fn brain_datei_gewinnt_die_maske_pro_schluessel() {
        // Der Brain-Selektor ueberschreibt die Maske je Oberschluessel komplett:
        // kimi's Composer-Anker (lexical editor) schlaegt den generischen.
        let sel = load_selectors("kimi").expect("kimi ist mitgeliefert");
        let composer = sel
            .get("composer")
            .and_then(|v| v.as_array())
            .expect("composer-Liste");
        assert_eq!(
            composer[0], "div[data-lexical-editor=\"true\"]",
            "kimi gewinnt ueber die Maske"
        );
    }

    #[test]
    fn generische_maske_traegt_jeden_brain_mit_den_kern_keys() {
        // Multiplikator-Anspruch aus dem Masken-Plan: JEDES registrierte Brain —
        // mit eigener Datei oder nur Maske — hat sofort alle 8 Kern-Keys. Die
        // Maske ist die unterste Stufe; eine Brain-Datei darf pro Key drueberlegen,
        // aber nichts darf fehlen. Ein Kern-Key als leere Liste (oder ganz weg)
        // bricht diesen Test.
        let ids: Vec<String> = {
            let mut v: Vec<String> = brains().keys().cloned().collect();
            v.sort();
            v
        };
        assert!(!ids.is_empty(), "keine Brains registriert");
        for id in &ids {
            let sel = load_selectors(id)
                .unwrap_or_else(|e| panic!("{id}: load_selectors fehlgeschlagen: {e}"));
            for key in [
                "composer",
                "send_button",
                "stop_button",
                "new_chat_button",
                "login_button",
                "login_indicator",
                "google_sso_button",
                "assistant_message",
            ] {
                let has = sel
                    .get(key)
                    .and_then(|v| v.as_array())
                    .map(|a| !a.is_empty())
                    .unwrap_or(false);
                assert!(has, "{id}: Kern-Key {key} fehlt — Maske traegt nicht");
            }
        }
    }

    #[test]
    fn test_swarm_and_reference_paths() {
        let r = reference_profile_dir("claude");
        assert!(r.ends_with(std::path::Path::new("reference").join("claude")));
        let s = swarm_profile_dir("run1", "claude");
        let lossy = s.to_string_lossy();
        assert!(lossy.contains("swarm"));
        assert!(lossy.contains("run1_claude"));
    }

    #[test]
    fn test_copy_dir_sparse_finds_nested_webview2_artifacts() {
        // Regression fuer den Fund 2026-07-21: WebView2 legt alles unter
        // EBWebView/Default/ ab (Cookies sogar unter Default/Network/Cookies).
        // Die frueher nur oberflaechliche Suche traf die Whitelist NIE — die
        // Swarm-Kopien waren leer und die Brains wirkten ausgeloggt.
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let src = root_dir().join(format!("data/test_sparse_nested_src_{}", stamp));
        let dst = root_dir().join(format!("data/test_sparse_nested_dst_{}", stamp));
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);

        let web = src.join("EBWebView");
        let default = web.join("Default");
        fs::create_dir_all(default.join("Network")).unwrap();
        fs::create_dir_all(default.join("Local Storage")).unwrap();
        fs::create_dir_all(web.join("Crashpad")).unwrap();
        fs::create_dir_all(default.join("Cache")).unwrap();
        // Auth-relevant, verschachtelt:
        fs::write(default.join("Network").join("Cookies"), b"jar").unwrap();
        fs::write(web.join("Local State"), b"key").unwrap();
        fs::write(default.join("Preferences"), b"prefs").unwrap();
        fs::write(default.join("Local Storage").join("leveldb"), b"ls").unwrap();
        // Ballast, der NICHT mitkommen soll:
        fs::write(web.join("Crashpad").join("dump"), b"x").unwrap();
        fs::write(default.join("Cache").join("blob"), b"x").unwrap();
        fs::write(default.join("History"), b"h").unwrap();

        copy_dir_sparse(&src.to_path_buf(), &dst.to_path_buf()).unwrap();

        let d_web = dst.join("EBWebView");
        let d_def = d_web.join("Default");
        assert!(
            d_def.join("Network").join("Cookies").is_file(),
            "Cookie-Jar (Default/Network/Cookies) muss mitkommen"
        );
        assert!(
            d_web.join("Local State").is_file(),
            "Local State (Entschluesselungs-Key) muss mitkommen"
        );
        assert!(d_def.join("Preferences").is_file(), "Preferences kopiert");
        assert!(
            d_def.join("Local Storage").join("leveldb").is_file(),
            "Local Storage kopiert"
        );
        assert!(!d_def.join("History").exists(), "History bleibt weg");
        assert!(
            !d_web.join("Crashpad").exists(),
            "Crashpad wird uebersprungen"
        );
        assert!(!d_def.join("Cache").exists(), "Cache wird uebersprungen");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn test_copy_dir_sparse_keeps_only_whitelist() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let src = root_dir().join(format!("data/test_sparse_src_{}", stamp));
        let dst = root_dir().join(format!("data/test_sparse_dst_{}", stamp));
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
        fs::create_dir_all(&src).unwrap();

        // Whitelist-Dateien/Ordner
        fs::write(src.join("Cookies"), b"cookies").unwrap();
        fs::write(src.join("Login Data"), b"login").unwrap();
        fs::write(src.join("Preferences"), b"prefs").unwrap();
        fs::create_dir_all(src.join("Local Storage")).unwrap();
        fs::write(src.join("Local Storage").join("x"), b"ls").unwrap();
        // Nicht-Whitelist
        fs::write(src.join("History"), b"history").unwrap();
        fs::write(src.join("Bookmarks"), b"bm").unwrap();
        // Lock-File
        fs::write(src.join("SingletonLock"), b"pid").unwrap();

        copy_dir_sparse(&src.to_path_buf(), &dst.to_path_buf()).unwrap();

        assert!(dst.join("Cookies").is_file(), "Cookies (whitelist) kopiert");
        assert!(
            dst.join("Login Data").is_file(),
            "Login Data (whitelist) kopiert"
        );
        assert!(
            dst.join("Preferences").is_file(),
            "Preferences (whitelist) kopiert"
        );
        assert!(
            dst.join("Local Storage").join("x").is_file(),
            "Local Storage (whitelist) kopiert"
        );

        assert!(
            !dst.join("History").exists(),
            "History (nicht whitelist) nicht kopiert"
        );
        assert!(
            !dst.join("Bookmarks").exists(),
            "Bookmarks (nicht whitelist) nicht kopiert"
        );
        assert!(
            !dst.join("SingletonLock").exists(),
            "Lock-File nicht kopiert"
        );

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn test_prepare_swarm_profile_respects_sparse_env() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_sparse_{}", stamp));
        let _ = fs::create_dir_all(&base);
        let run_id = format!("testsparse_{}", stamp);
        let brain = "chatgpt";
        let reference = reference_profile_dir_in(&base, brain);
        let _ = fs::create_dir_all(&reference);
        fs::write(reference.join("Cookies"), b"c").unwrap();
        fs::write(reference.join("History"), b"h").unwrap();
        fs::write(reference.join("SingletonLock"), b"pid").unwrap();

        // explizit sparse (kein globales Env -> nebenlaeufig sicher)
        let dst = prepare_swarm_profile_in(&base, &run_id, brain, true);
        assert!(dst.join("Cookies").is_file(), "sparse: Cookies kopiert");
        assert!(
            !dst.join("History").exists(),
            "sparse: History nicht kopiert"
        );
        assert!(
            !dst.join("SingletonLock").exists(),
            "sparse: Lock nicht kopiert"
        );

        cleanup_swarm_profiles_in(&base, &run_id).unwrap();
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_clone_planner_dry_run_classification() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_clone_{}", stamp));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        // (A) Read-only -> link
        fs::write(base.join("resources.pak"), b"pak").unwrap();
        fs::write(base.join("chrome_100_percent.pak"), b"pak").unwrap();
        fs::write(base.join("icudtl.dat"), b"dat").unwrap();
        fs::write(base.join("snapshot_blob.bin"), b"bin").unwrap();
        fs::write(base.join("v8_context_snapshot.bin"), b"bin").unwrap();
        fs::create_dir_all(base.join("Extensions")).unwrap();
        fs::write(base.join("Extensions").join("ext.pak"), b"e").unwrap();
        fs::create_dir_all(base.join("pnacl")).unwrap();
        fs::create_dir_all(base.join("Subresource Filter")).unwrap();
        fs::create_dir_all(base.join("WidevineCdm")).unwrap();
        fs::create_dir_all(base.join("MEIPreload")).unwrap();

        // (B)-minimal -> copy
        fs::write(base.join("Cookies"), b"c").unwrap();
        fs::write(base.join("Login Data"), b"l").unwrap();
        fs::write(base.join("Web Data"), b"w").unwrap();
        fs::write(base.join("Local State"), b"s").unwrap();
        fs::write(base.join("Preferences"), b"p").unwrap();
        fs::create_dir_all(base.join("IndexedDB")).unwrap();
        fs::create_dir_all(base.join("Local Storage")).unwrap();

        // Rest (B) + Lockfiles -> skipped
        fs::write(base.join("Cookies-journal"), b"cj").unwrap();
        fs::write(base.join("Login Data-journal"), b"lj").unwrap();
        fs::write(base.join("Web Data-journal"), b"wj").unwrap();
        fs::write(base.join("Secure Preferences"), b"sp").unwrap();
        fs::create_dir_all(base.join("Service Worker")).unwrap();
        fs::create_dir_all(base.join("Cache")).unwrap();
        fs::create_dir_all(base.join("Code Cache")).unwrap();
        fs::create_dir_all(base.join("Session Storage")).unwrap();
        fs::create_dir_all(base.join("Network")).unwrap();
        fs::write(base.join("History"), b"h").unwrap();
        fs::write(base.join("SingletonLock"), b"pid").unwrap();
        fs::write(base.join("lockfile"), b"x").unwrap();

        let report = ProfileClonePlanner::dry_run(&base);
        let link_names: std::collections::HashSet<String> =
            report.links.iter().map(|e| e.name.clone()).collect();
        let copy_names: std::collections::HashSet<String> =
            report.copies.iter().map(|e| e.name.clone()).collect();
        let skip_names: std::collections::HashSet<String> =
            report.skipped.iter().map(|e| e.name.clone()).collect();

        // (A) -> links
        for a in [
            "resources.pak",
            "chrome_100_percent.pak",
            "icudtl.dat",
            "snapshot_blob.bin",
            "v8_context_snapshot.bin",
            "Extensions",
            "pnacl",
            "Subresource Filter",
            "WidevineCdm",
            "MEIPreload",
        ] {
            assert!(link_names.contains(a), "(A) '{a}' sollte link sein");
        }
        // (B)-minimal -> copies
        for b in [
            "Cookies",
            "Login Data",
            "Web Data",
            "Local State",
            "Preferences",
            "IndexedDB",
            "Local Storage",
        ] {
            assert!(copy_names.contains(b), "(B)-minimal '{b}' sollte copy sein");
        }
        // Rest (B) + Unbekanntes -> skipped
        for s in [
            "Cookies-journal",
            "Login Data-journal",
            "Web Data-journal",
            "Secure Preferences",
            "Service Worker",
            "Cache",
            "Code Cache",
            "Session Storage",
            "Network",
            "History",
        ] {
            assert!(skip_names.contains(s), "Rest( B) '{s}' sollte skipped sein");
        }
        // Lockfiles aus beiden (links UND copies) ausgelassen
        assert!(
            !link_names.contains("SingletonLock"),
            "Lockfile darf nicht gelinkt werden"
        );
        assert!(
            !copy_names.contains("SingletonLock"),
            "Lockfile darf nicht kopiert werden"
        );
        assert!(skip_names.contains("SingletonLock"), "Lockfile skipped");
        assert!(skip_names.contains("lockfile"), "lockfile skipped");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_clone_planner_materialize_links_and_omits_locks() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_mat_{}", stamp));
        let dst = std::env::temp_dir().join(format!("webagent_mat_dst_{}", stamp));
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dst);
        fs::create_dir_all(&base).unwrap();

        // (A) Datei + (A) Verzeichnis
        fs::write(base.join("resources.pak"), b"PAK-A").unwrap();
        fs::create_dir_all(base.join("Extensions")).unwrap();
        fs::write(base.join("Extensions").join("ext.pak"), b"PAK-B").unwrap();
        // (B)-minimal Datei + Verzeichnis
        fs::write(base.join("Cookies"), b"CK").unwrap();
        fs::create_dir_all(base.join("Local Storage")).unwrap();
        fs::write(base.join("Local Storage").join("ls.txt"), b"LS").unwrap();
        // Lockfile + Rest
        fs::write(base.join("SingletonLock"), b"pid").unwrap();
        fs::write(base.join("Cookies-journal"), b"cj").unwrap();
        fs::write(base.join("History"), b"h").unwrap();

        let plan = ProfileClonePlanner::plan_canonical(&base, &dst, "run1");
        ProfileClonePlanner::materialize(&plan).expect("materialize");

        // (A) verlinkt/kopiert, (B)-minimal kopiert
        assert!(dst.join("resources.pak").is_file(), "(A) Datei vorhanden");
        assert!(
            dst.join("Extensions").join("ext.pak").is_file(),
            "(A) Verzeichnis rekursiv verarbeitet"
        );
        assert!(dst.join("Cookies").is_file(), "(B)-minimal Datei kopiert");
        assert!(
            dst.join("Local Storage").join("ls.txt").is_file(),
            "(B)-minimal Verzeichnis kopiert"
        );
        // Lockfiles + Rest weggelassen
        assert!(
            !dst.join("SingletonLock").exists(),
            "Lockfile nicht im Klon"
        );
        assert!(
            !dst.join("Cookies-journal").exists(),
            "Journal nicht im Klon"
        );
        assert!(!dst.join("History").exists(), "Rest( B) nicht im Klon");

        // (A) wird auf same-volume ueber Hardlink geteilt: Mutation der Basis
        // spiegelt sich im Klon (gleiche Inode).
        assert!(plan.same_volume, "same-volume erkannt");
        fs::write(base.join("resources.pak"), b"PAK-A-MUT").unwrap();
        let linked = fs::read_to_string(dst.join("resources.pak")).unwrap();
        assert_eq!(linked, "PAK-A-MUT", "(A) ist Hardlink (geteilt)");
        fs::write(base.join("Extensions").join("ext.pak"), b"PAK-B-MUT").unwrap();
        let linked2 = fs::read_to_string(dst.join("Extensions").join("ext.pak")).unwrap();
        assert_eq!(linked2, "PAK-B-MUT", "(A) Verzeichnis-Datei ist Hardlink");

        // (B)-minimal ist eine echte Kopie: Mutation der Basis aendert den Klon NICHT.
        fs::write(base.join("Cookies"), b"CK-MUT").unwrap();
        let copied = fs::read_to_string(dst.join("Cookies")).unwrap();
        assert_eq!(copied, "CK", "(B)-minimal ist Kopie (nicht geteilt)");

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn test_clone_planner_cross_drive_copies() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_xd_{}", stamp));
        let dst = std::env::temp_dir().join(format!("webagent_xd_dst_{}", stamp));
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dst);
        fs::create_dir_all(&base).unwrap();

        fs::write(base.join("resources.pak"), b"PAK-A").unwrap();
        fs::write(base.join("Cookies"), b"CK").unwrap();
        fs::write(base.join("SingletonLock"), b"pid").unwrap();

        // Klassifikation uebernehmen, aber Volume-Gleichheit erzwingen=false
        // (simuliert cross-drive: alles wird kopiert, nichts gelinkt).
        let mut plan = ProfileClonePlanner::plan_canonical(&base, &dst, "run1");
        plan.same_volume = false;
        ProfileClonePlanner::materialize(&plan).expect("materialize");

        assert!(
            dst.join("resources.pak").is_file(),
            "(A) kopiert cross-drive"
        );
        assert!(dst.join("Cookies").is_file(), "(B)-minimal kopiert");
        assert!(!dst.join("SingletonLock").exists(), "Lock weggelassen");

        // Copy, kein Hardlink: Mutation der Basis aendert den Klon nicht.
        fs::write(base.join("resources.pak"), b"PAK-A-MUT").unwrap();
        let content = fs::read_to_string(dst.join("resources.pak")).unwrap();
        assert_eq!(
            content, "PAK-A",
            "cross-drive: (A) ist Kopie, keine geteilte Inode"
        );

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn test_encapsulated_profile_dir_path() {
        let p = encapsulated_profile_dir("chatgpt", "run42");
        assert!(p.to_string_lossy().contains("encapsulated"));
        assert!(p.to_string_lossy().contains("chatgpt_run42"));
    }

    /// Regression 2026-08-07: Das versiegelte Master ist read-only, und
    /// `fs::copy` uebernimmt das Attribut in die Laufzeit-Kopie. WebView2 konnte
    /// dann im Klon nie Cookies/Local State persistieren (Cookie-DB stand
    /// dauerhaft auf der alten mtime). Die Kopie muss beschreibbar sein — das
    /// Siegel bleibt am Master.
    #[cfg(windows)]
    #[test]
    fn klon_aus_versiegeltem_master_ist_beschreibbar() {
        let base = std::env::temp_dir().join(format!("webagent_ro_{}", std::process::id()));
        let src = base.join("master");
        let dst = base.join("klon");
        std::fs::create_dir_all(src.join("EBWebView/Default/Network")).unwrap();
        std::fs::write(src.join("EBWebView/Default/Network/Cookies"), b"x").unwrap();
        std::fs::write(src.join("EBWebView/Default/Local State"), b"y").unwrap();
        for f in [
            "EBWebView/Default/Network/Cookies",
            "EBWebView/Default/Local State",
        ] {
            let mut perm = std::fs::metadata(src.join(f)).unwrap().permissions();
            perm.set_readonly(true);
            std::fs::set_permissions(src.join(f), perm).unwrap();
            assert!(
                std::fs::metadata(src.join(f))
                    .unwrap()
                    .permissions()
                    .readonly(),
                "Vorbedingung: Master-Datei ist versiegelt"
            );
        }

        copy_dir_sparse(&src, &dst).unwrap();

        for f in [
            "EBWebView/Default/Network/Cookies",
            "EBWebView/Default/Local State",
        ] {
            let dst_file = dst.join(f);
            assert!(
                dst_file.exists(),
                "Klon muss die Datei enthalten (rekursive Kopie)"
            );
            assert!(
                !std::fs::metadata(&dst_file)
                    .unwrap()
                    .permissions()
                    .readonly(),
                "Klon darf nicht read-only sein: {f}"
            );
        }

        let _ = std::fs::remove_dir_all(&base);
    }
}
