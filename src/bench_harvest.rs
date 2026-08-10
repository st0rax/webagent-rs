//! bench_harvest — was von einem Lauf als Code im Repo bleibt.
//!
//! Zweiter Schnitt des Modul-Splits (nach `bench_scoring`). Die Ernte ist ein
//! eigener Gegenstand: sie entscheidet, WELCHER bestandene Beitrag uebernommen
//! wird und ob er ueberhaupt uebernommen werden darf.
//!
//! Der Kern ist die Zurueckweisung. Ein Beitrag, der nur sich selbst benutzt,
//! ist toter Code mit gruenem Anstrich — und genau das ist real passiert
//! (608d599: eine tote `Observer`-Struktur, aufgerufen ausschliesslich in fuenf
//! mitgelieferten Tests, galt damit als „benutzt"). Deshalb zaehlt ein Aufruf
//! nur ausserhalb eines Testrumpfs.
//!
//! `benchmark.rs` re-exportiert, damit kein Aufrufer angefasst wird.

use crate::benchmark::HarvestCandidate;
/// Wählt den zu erntenden Kandidaten: wenige Iterationen schlagen viele, bei
/// Gleichstand entscheidet die kürzere Laufzeit.
///
/// „Beim ersten Versuch grün" ist das stärkere Signal als „nach neun Korrekturen
/// grün" — beide bestehen, aber nur eines davon ist verlässliche Arbeit.
pub fn pick_harvest(candidates: &[HarvestCandidate]) -> Option<&HarvestCandidate> {
    candidates
        .iter()
        .filter(|c| !c.patch.trim().is_empty())
        .filter(|c| harvest_rejection(&c.patch).is_none())
        .min_by_key(|c| (c.iterations, c.latency_ms))
}

/// Prüft einen Kandidaten-Diff auf die zwei Wege, das Erntekriterium zu
/// erfüllen, ohne etwas zu verbessern. Gibt den Grund zurück, wenn verworfen.
///
/// Hintergrund, real am 2026-07-29: der Benchmark erntete drei Beiträge, die
/// alle „kompiliert + Tests grün" erfüllten und trotzdem nichts brachten —
/// jeweils eine neue öffentliche Funktion samt eigener Tests, die nirgends
/// aufgerufen wird (`normalize_research_suggestion`, `is_command_allowed`,
/// `map_protocol_error_code`). Einer der Beiträge löschte dafür einen
/// bestehenden Test.
///
/// Das ist kein Fehlverhalten der Brains, sondern die logische Antwort auf ein
/// Kriterium, das Ballast nicht von Verbesserung unterscheidet. Also muss das
/// Kriterium schärfer werden, nicht der Prompt frommer.
pub fn harvest_rejection(patch: &str) -> Option<String> {
    let removed = count_marker(patch, '-', "#[test]");
    let added = count_marker(patch, '+', "#[test]");
    if removed > added {
        return Some(format!(
            "entfernt {} Test(s) mehr als er hinzufügt — weniger Tests machen den Build grün",
            removed - added
        ));
    }
    let new_fns = added_public_fns(patch);
    if !new_fns.is_empty() && !patch_uses_any(patch, &new_fns) {
        return Some(format!(
            "fügt {} an: nirgends aufgerufen (toter Code)",
            new_fns.join(", ")
        ));
    }
    None
}

/// Zählt Diff-Zeilen mit gegebenem Vorzeichen, die `needle` enthalten.
fn count_marker(patch: &str, sign: char, needle: &str) -> usize {
    patch
        .lines()
        .filter(|l| l.starts_with(sign) && !l.starts_with("+++") && !l.starts_with("---"))
        .filter(|l| l.contains(needle))
        .count()
}

/// Namen der im Diff neu hinzugefügten öffentlichen Funktionen.
pub fn added_public_fns(patch: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in patch.lines() {
        if !line.starts_with('+') || line.starts_with("+++") {
            continue;
        }
        let t = line.trim_start_matches('+').trim();
        let Some(rest) = t.strip_prefix("pub fn ") else {
            continue;
        };
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// True, wenn mindestens einer der Namen im Diff auch AUFGERUFEN wird — also
/// in einer hinzugefügten Zeile vorkommt, die weder die Definition noch eine
/// Testzeile ist.
///
/// Bewusst grob: der Diff allein verrät nicht, ob der Aufruf im Testmodul
/// steht. Deshalb zählt eine Zeile nur, wenn sie den Namen mit `(` enthält und
/// nicht selbst die `pub fn`-Zeile ist — und Zeilen innerhalb erkennbarer
/// Testblöcke (`assert`, `#[test]`) werden ausgeschlossen. Lieber einmal zu
/// streng: ein faelschlich verworfener Beitrag kostet eine Runde, ein
/// faelschlich geernteter bleibt für immer im Repo.
fn patch_uses_any(patch: &str, names: &[String]) -> bool {
    // Klammertiefe ab dem `#[test]`-Marker mitzaehlen, um Testrumpfe zu
    // ueberspringen.
    //
    // Der erste Entwurf schloss nur Zeilen mit `assert` oder `#[test]` aus —
    // und fiel sofort herein: ein geernteter Beitrag (608d599) legte eine tote
    // `Observer`-Struktur an und rief sie in fuenf mitgelieferten Tests auf.
    // Damit galt sie als "benutzt". Ein Brain muss also nur seine eigene tote
    // Funktion selbst testen, und die Pruefung ist zufrieden.
    //
    // Ein Aufruf zaehlt nur noch, wenn er AUSSERHALB eines Testrumpfs steht —
    // also echten Produktivcode erreicht.
    let mut in_test = false;
    let mut depth: i32 = 0;
    for line in patch.lines() {
        if !line.starts_with('+') || line.starts_with("+++") {
            continue;
        }
        let t = line.trim_start_matches('+').trim();
        if t.starts_with("#[test]") || t.starts_with("#[cfg(test)]") {
            in_test = true;
            depth = 0;
            continue;
        }
        if in_test {
            depth += t.matches('{').count() as i32;
            depth -= t.matches('}').count() as i32;
            if depth <= 0 && t.contains('}') {
                in_test = false;
            }
            continue;
        }
        if t.starts_with("pub fn ") || t.starts_with("fn ") {
            continue;
        }
        if names.iter().any(|n| t.contains(&format!("{n}("))) {
            return true;
        }
    }
    false
}

