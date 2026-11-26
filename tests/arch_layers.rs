//! Architecture ratchet for the top-level module graph.
//!
//! The graph is derived from production code only: every `crate::<module>` path
//! found in `src/<module>.rs` and `src/<module>/**/*.rs` is one edge. Comments,
//! doc comments, string literals and `#[cfg(test)]` items are removed first, so
//! documentation links and test-only imports cannot create fake edges.
//!
//! Two invariants are enforced:
//! 1. Every strongly connected component with more than one module (or a
//!    self-loop) must be present in `docs/arch/layer_exceptions.txt`.
//! 2. An edge from a lower-ranked layer to a higher-ranked layer must be listed
//!    as an exception. Every listed exception must still exist, so the list can
//!    only shrink when someone removes an edge or breaks a cycle.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Layers {
    rank: BTreeMap<String, u32>,
    #[allow(dead_code)]
    groups: BTreeMap<String, Vec<String>>,
}

#[derive(Default)]
struct Exceptions {
    cycles: BTreeSet<Vec<String>>,
    edges: BTreeSet<(String, String)>,
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn top_level_modules(root: &Path) -> BTreeSet<String> {
    let source = fs::read_to_string(root.join("src/lib.rs")).expect("src/lib.rs must be readable");
    let mut modules = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim();
        let rest = line
            .strip_prefix("pub mod ")
            .or_else(|| line.strip_prefix("mod "));
        if let Some(rest) = rest {
            let name = rest.trim().trim_end_matches(';').trim();
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                modules.insert(name.to_string());
            }
        }
    }
    modules
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).expect("module directory must be readable");
    for entry in entries {
        let path = entry.expect("directory entry must be readable").path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn files_for_module(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let single = root.join("src").join(format!("{name}.rs"));
    if single.is_file() {
        files.push(single);
    }
    let folder = root.join("src").join(name);
    if folder.is_dir() {
        collect_rs_files(&folder, &mut files);
    }
    files.sort();
    files
}

/// Remove comments, doc comments, string literals, byte strings and char
/// literals. Braces stay intact so `#[cfg(test)]` can be stripped afterwards.
fn sanitize_code(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            let mut depth = 1usize;
            i += 2;
            while i < chars.len() && depth > 0 {
                if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            out.push(' ');
            continue;
        }

        // Raw strings: r"...", r#"..."#, br#"..."#.
        let raw_start = if chars[i] == 'r' {
            Some(i + 1)
        } else if chars[i] == 'b' && i + 1 < chars.len() && chars[i + 1] == 'r' {
            Some(i + 2)
        } else {
            None
        };
        if let Some(mut j) = raw_start {
            let mut hashes = 0usize;
            while j < chars.len() && chars[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < chars.len() && chars[j] == '"' {
                let mut end = j + 1;
                while end < chars.len() {
                    if chars[end] == '"' {
                        let mut k = end + 1;
                        let mut seen = 0usize;
                        while k < chars.len() && chars[k] == '#' && seen < hashes {
                            seen += 1;
                            k += 1;
                        }
                        if seen == hashes {
                            end = k;
                            break;
                        }
                    }
                    end += 1;
                }
                i = end.min(chars.len());
                out.push(' ');
                continue;
            }
        }

        if chars[i] == '"' {
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                } else if chars[i] == '"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            out.push(' ');
            continue;
        }

        // A char literal is at most four chars with an escape; a lifetime is not.
        if chars[i] == '\'' && i + 2 < chars.len() {
            if chars[i + 1] == '\\' {
                if i + 3 < chars.len() && chars[i + 3] == '\'' {
                    i += 4;
                    out.push(' ');
                    continue;
                }
            } else if chars[i + 2] == '\'' {
                i += 3;
                out.push(' ');
                continue;
            }
        }

        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Drop items annotated with `#[cfg(test)]`, including whole `mod tests` blocks.
fn strip_cfg_test(source: &str) -> String {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if !(trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[cfg(all(test")) {
            out.push_str(lines[i]);
            i += 1;
            continue;
        }
        i += 1;
        while i < lines.len() && lines[i].trim().starts_with("#[") {
            i += 1;
        }
        if i >= lines.len() {
            break;
        }
        let mut j = i;
        while j < lines.len() && !lines[j].contains('{') && !lines[j].contains(';') {
            j += 1;
        }
        if j >= lines.len() {
            break;
        }
        let semicolon = lines[j].find(';');
        let brace = lines[j].find('{');
        if semicolon.is_some() && (brace.is_none() || semicolon < brace) {
            i = j + 1;
            continue;
        }
        let mut depth = 0i32;
        let mut started = false;
        while j < lines.len() {
            for ch in lines[j].chars() {
                if ch == '{' {
                    depth += 1;
                    started = true;
                } else if ch == '}' {
                    depth -= 1;
                }
            }
            j += 1;
            if started && depth <= 0 {
                break;
            }
        }
        i = j;
    }
    out
}

fn module_edges(root: &Path, modules: &BTreeSet<String>) -> BTreeMap<String, BTreeSet<String>> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for name in modules {
        for file in files_for_module(root, name) {
            let source = fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
            let code = strip_cfg_test(&sanitize_code(&source));
            let bytes = code.as_bytes();
            let needle = b"crate::";
            let mut i = 0;
            while i + needle.len() <= bytes.len() {
                if &bytes[i..i + needle.len()] == needle {
                    let start = i + needle.len();
                    let mut end = start;
                    while end < bytes.len()
                        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                    {
                        end += 1;
                    }
                    if end > start {
                        let target = &code[start..end];
                        if modules.contains(target) && target != name {
                            edges
                                .entry(name.clone())
                                .or_default()
                                .insert(target.to_string());
                        }
                    }
                    i = end.max(i + needle.len());
                } else {
                    i += 1;
                }
            }
        }
    }
    edges
}

#[allow(clippy::too_many_arguments)]
fn tarjan_visit(
    node: &str,
    edges: &BTreeMap<String, BTreeSet<String>>,
    indices: &mut BTreeMap<String, usize>,
    low: &mut BTreeMap<String, usize>,
    stack: &mut Vec<String>,
    on_stack: &mut BTreeSet<String>,
    components: &mut Vec<Vec<String>>,
    counter: &mut usize,
) {
    indices.insert(node.to_string(), *counter);
    low.insert(node.to_string(), *counter);
    *counter += 1;
    stack.push(node.to_string());
    on_stack.insert(node.to_string());

    if let Some(targets) = edges.get(node) {
        for target in targets {
            if !indices.contains_key(target) {
                tarjan_visit(
                    target, edges, indices, low, stack, on_stack, components, counter,
                );
                let child_low = low[target];
                if child_low < low[node] {
                    low.insert(node.to_string(), child_low);
                }
            } else if on_stack.contains(target) && indices[target] < low[node] {
                low.insert(node.to_string(), indices[target]);
            }
        }
    }

    if low[node] == indices[node] {
        let mut component = Vec::new();
        loop {
            let member = stack.pop().expect("tarjan stack must not be empty");
            on_stack.remove(&member);
            let is_root = member == node;
            component.push(member);
            if is_root {
                break;
            }
        }
        component.sort();
        components.push(component);
    }
}

fn find_cycles(
    modules: &BTreeSet<String>,
    edges: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<Vec<String>> {
    let mut indices = BTreeMap::new();
    let mut low = BTreeMap::new();
    let mut stack = Vec::new();
    let mut on_stack = BTreeSet::new();
    let mut components = Vec::new();
    let mut counter = 0;
    for name in modules {
        if !indices.contains_key(name) {
            tarjan_visit(
                name,
                edges,
                &mut indices,
                &mut low,
                &mut stack,
                &mut on_stack,
                &mut components,
                &mut counter,
            );
        }
    }
    components.sort();
    let mut cycles = BTreeSet::new();
    for component in components {
        let self_loop = component.len() == 1
            && component.first().is_some_and(|name| {
                edges
                    .get(name)
                    .is_some_and(|targets| targets.contains(name))
            });
        if component.len() > 1 || self_loop {
            cycles.insert(component);
        }
    }
    cycles
}

fn parse_layers(text: &str) -> Layers {
    let mut current = String::new();
    let mut collecting = false;
    let mut ranks: BTreeMap<String, u32> = BTreeMap::new();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line.trim_start_matches('[').trim_end_matches(']').trim();
            current = name.strip_prefix("layer.").unwrap_or(name).to_string();
            collecting = false;
            continue;
        }
        if collecting {
            if line == "]" {
                collecting = false;
            } else {
                let name = line.trim_end_matches(',').trim().trim_matches('"');
                groups
                    .entry(current.clone())
                    .or_default()
                    .push(name.to_string());
            }
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "rank" => {
                    ranks.insert(
                        current.clone(),
                        value
                            .trim()
                            .parse::<u32>()
                            .expect("layer rank must be a number"),
                    );
                }
                "modules" => {
                    let value = value.trim();
                    if value == "[" {
                        collecting = true;
                    } else {
                        let inner = value.trim_start_matches('[').trim_end_matches(']');
                        for name in inner.split(',') {
                            let name = name.trim().trim_matches('"');
                            if !name.is_empty() {
                                groups
                                    .entry(current.clone())
                                    .or_default()
                                    .push(name.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut rank = BTreeMap::new();
    for (group, names) in &groups {
        let group_rank = ranks[group];
        for name in names {
            assert!(
                rank.insert(name.clone(), group_rank).is_none(),
                "module assigned to two layers: {name}"
            );
        }
    }
    Layers { rank, groups }
}

fn parse_exceptions(text: &str) -> Exceptions {
    let mut exceptions = Exceptions::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("cycle:") {
            let mut names: Vec<String> = rest
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_string)
                .collect();
            names.sort();
            names.dedup();
            assert!(
                exceptions.cycles.insert(names.clone()),
                "duplicate cycle: {names:?}"
            );
        } else if let Some(rest) = line.strip_prefix("edge:") {
            let (from, to) = rest.split_once("->").expect("edge needs lower->higher");
            assert!(
                exceptions
                    .edges
                    .insert((from.trim().to_string(), to.trim().to_string())),
                "duplicate edge: {rest}"
            );
        } else {
            panic!("unknown exception line: {line}");
        }
    }
    exceptions
}

/// All ratchet checks in one place, so the negative tests can prove that a new
/// cycle, a new upward edge and a stale exception are each rejected.
fn check_ratchet(
    modules: &BTreeSet<String>,
    graph: &BTreeMap<String, BTreeSet<String>>,
    layers: &Layers,
    exceptions: &Exceptions,
) -> (Vec<String>, usize) {
    let mut problems = Vec::new();
    if layers.rank.len() != modules.len() {
        problems.push(format!(
            "layer config covers {} modules, src/lib.rs declares {}",
            layers.rank.len(),
            modules.len()
        ));
    }
    for name in modules {
        if !layers.rank.contains_key(name) {
            problems.push(format!("module without layer: {name}"));
        }
    }

    let cycles = find_cycles(modules, graph);
    for cycle in &cycles {
        if !exceptions.cycles.contains(cycle) {
            problems.push(format!(
                "new cycle outside the ratchet: {}",
                cycle.join(", ")
            ));
        }
    }
    for exception in &exceptions.cycles {
        if !cycles.contains(exception) {
            problems.push(format!(
                "stale cycle exception (remove the line): {}",
                exception.join(", ")
            ));
        }
    }

    let mut upward = 0usize;
    for (from, targets) in graph {
        for to in targets {
            let Some(from_rank) = layers.rank.get(from) else {
                continue;
            };
            let Some(to_rank) = layers.rank.get(to) else {
                continue;
            };
            if from_rank >= to_rank {
                continue;
            }
            let inside_excepted_cycle = exceptions
                .cycles
                .iter()
                .any(|cycle| cycle.contains(from) && cycle.contains(to));
            if inside_excepted_cycle {
                continue;
            }
            upward += 1;
            if !exceptions.edges.contains(&(from.clone(), to.clone())) {
                problems.push(format!(
                    "new upward edge {from} (rank {from_rank}) -> {to} (rank {to_rank})"
                ));
            }
        }
    }
    for (from, to) in &exceptions.edges {
        if !graph.get(from).is_some_and(|targets| targets.contains(to)) {
            problems.push(format!("stale edge exception {from} -> {to}"));
            continue;
        }
        if let (Some(from_rank), Some(to_rank)) = (layers.rank.get(from), layers.rank.get(to)) {
            if from_rank >= to_rank {
                problems.push(format!(
                    "edge exception is no longer upward: {from} -> {to}"
                ));
            }
        }
    }
    (problems, upward)
}

#[test]
fn top_level_layers_and_cycles_are_a_ratchet() {
    let root = manifest_dir();
    let modules = top_level_modules(&root);
    assert!(modules.len() >= 80, "module list unexpectedly short");

    let graph = module_edges(&root, &modules);
    let layers = parse_layers(
        &fs::read_to_string(root.join("docs/arch/layers.toml"))
            .expect("docs/arch/layers.toml must be readable"),
    );
    let exceptions = parse_exceptions(
        &fs::read_to_string(root.join("docs/arch/layer_exceptions.txt"))
            .expect("docs/arch/layer_exceptions.txt must be readable"),
    );

    let (problems, upward) = check_ratchet(&modules, &graph, &layers, &exceptions);
    assert!(
        problems.is_empty(),
        "architecture ratchet failed:\n{}",
        problems.join("\n")
    );

    let edge_count: usize = graph.values().map(BTreeSet::len).sum();
    let cycles = find_cycles(&modules, &graph);
    eprintln!(
        "architecture guard: modules={}, edges={edge_count}, cycles={}, upward_edges={upward}",
        modules.len(),
        cycles.len()
    );
}

fn names(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|name| (*name).to_string()).collect()
}

fn synthetic_layers(ranks: &[(&str, u32)]) -> Layers {
    Layers {
        rank: ranks
            .iter()
            .map(|(name, rank)| ((*name).to_string(), *rank))
            .collect(),
        groups: BTreeMap::new(),
    }
}

#[test]
fn ratchet_rejects_a_new_cycle() {
    let modules = names(&["a", "b"]);
    let mut graph = BTreeMap::new();
    graph.insert("a".to_string(), BTreeSet::from(["b".to_string()]));
    graph.insert("b".to_string(), BTreeSet::from(["a".to_string()]));
    let layers = synthetic_layers(&[("a", 0), ("b", 0)]);
    let (problems, _) = check_ratchet(&modules, &graph, &layers, &Exceptions::default());
    assert!(
        problems.iter().any(|p| p.contains("new cycle")),
        "a new cycle must be rejected: {problems:?}"
    );
}

#[test]
fn ratchet_rejects_a_new_upward_edge() {
    let modules = names(&["a", "b"]);
    let mut graph = BTreeMap::new();
    graph.insert("a".to_string(), BTreeSet::from(["b".to_string()]));
    let layers = synthetic_layers(&[("a", 0), ("b", 1)]);
    let (problems, _) = check_ratchet(&modules, &graph, &layers, &Exceptions::default());
    assert!(
        problems.iter().any(|p| p.contains("new upward edge")),
        "a new upward edge must be rejected: {problems:?}"
    );
}

#[test]
fn ratchet_rejects_a_stale_exception() {
    let modules = names(&["a", "b"]);
    let graph = BTreeMap::new();
    let layers = synthetic_layers(&[("a", 0), ("b", 1)]);
    let mut exceptions = Exceptions::default();
    exceptions.edges.insert(("a".to_string(), "b".to_string()));
    let (problems, _) = check_ratchet(&modules, &graph, &layers, &exceptions);
    assert!(
        problems.iter().any(|p| p.contains("stale edge exception")),
        "a stale exception must be removed: {problems:?}"
    );
}
