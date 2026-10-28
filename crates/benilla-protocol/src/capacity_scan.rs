//! A source scan of `src/messages/`: every capacity hint sized from the wire is bounded, so a
//! lying count skips a packet instead of aborting on an unsatisfiable allocation.

/// Every `with_capacity` under `messages/` whose argument is a bare variable goes through
/// [`crate::wire::capacity_hint`], carries a `.min(`, or is listed in [`EXEMPT`].
#[test]
fn bare_variable_capacity_hints_under_messages_are_capped() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/messages");
    let mut offenders = Vec::new();
    for file in rust_files(&root) {
        let text = std::fs::read_to_string(&file).expect("readable source");
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        for (line_no, arg) in capacity_arguments(&text) {
            if uncapped_wire_count(&arg) && !EXEMPT.contains(&(rel.as_str(), arg.as_str())) {
                offenders.push(format!("{rel}:{line_no}: `with_capacity({arg})`"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these capacity hints are raw wire counts — an unsatisfiable allocation aborts the \
         process instead of skipping the packet. Bound each with \
         `crate::wire::capacity_hint(count, CAP)` (CAP = the protocol's own bound, cited at \
         the site, or a generous sane one), or add it to `EXEMPT` in `capacity_scan.rs` with the reason \
         it is not a wire count:\n  {}",
        offenders.join("\n  ")
    );
}

/// Bare-variable arguments that are not wire counts, as `(path under src/messages/, argument)`.
const EXEMPT: &[(&str, &str)] = &[];

#[test]
fn the_scan_flags_a_bare_wire_count_and_passes_a_bounded_one() {
    for arg in [
        "count as usize",
        "count",
        "(count as usize)",
        "usize::from(count)",
        "option_count as usize",
        "n as usize",
    ] {
        assert!(uncapped_wire_count(arg), "should flag `{arg}`");
    }
    for arg in [
        "12",
        "name.len() + 1",
        "NPC_TEXT_BLOCKS",
        "QUEST_OBJECTIVES_COUNT as usize",
        "capacity_hint(count, 64)",
        "(count as usize).min(64)",
        "count.min(0xFFFF) as usize",
        "8 + 8 * entries.len()",
        "addons.iter().map(|a| a.name.len() + 10).sum()",
    ] {
        assert!(!uncapped_wire_count(arg), "should pass `{arg}`");
    }
    let src = "let a = Vec::with_capacity(count as usize);\nlet b = Vec::with_capacity((n as usize).min(4));\n";
    assert_eq!(
        capacity_arguments(src),
        vec![
            (1, "count as usize".to_string()),
            (2, "(n as usize).min(4)".to_string())
        ]
    );
}

/// A lowercase identifier, however cast or wrapped, with no `.min(` or `capacity_hint(`.
fn uncapped_wire_count(arg: &str) -> bool {
    if arg.contains(".min(") || arg.contains("capacity_hint(") {
        return false;
    }
    let mut inner = arg.trim();
    loop {
        let t = inner.trim();
        if let Some(rest) = t
            .strip_prefix("usize::from(")
            .and_then(|r| r.strip_suffix(')'))
        {
            inner = rest;
        } else if let Some(rest) = t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
            inner = rest;
        } else if let Some(rest) = t.strip_suffix("as usize") {
            inner = rest;
        } else {
            inner = t;
            break;
        }
    }
    let is_ident = !inner.is_empty()
        && inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !inner.starts_with(|c: char| c.is_ascii_digit());
    // A `const` is SCREAMING_CASE; a wire count is a local.
    is_ident && inner.chars().any(|c| c.is_ascii_lowercase())
}

/// Every `with_capacity(` call's balanced argument text in `text`, with its 1-based line.
fn capacity_arguments(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let needle = "with_capacity(";
    let mut from = 0;
    while let Some(at) = text[from..].find(needle) {
        let open = from + at + needle.len();
        let mut depth = 1usize;
        let mut end = open;
        for (i, c) in text[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let line = text[..open].matches('\n').count() + 1;
        out.push((line, text[open..end].trim().to_string()));
        from = end.max(open);
    }
    out
}

fn rust_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
