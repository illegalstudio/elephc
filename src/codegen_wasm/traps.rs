//! Purpose:
//! Builds and validates the machine-readable inventory of Core WebAssembly
//! `unreachable` instructions emitted by the wasm32-wasi backend.
//!
//! Called from:
//! - `crate::codegen_wasm::generate()` after WAT rendering and before assembly
//!   or publication.
//!
//! Key details:
//! - Every emitted `unreachable` must carry an inline
//!   `elephc-trap:<class>:<site>` marker.
//! - PHP-visible trap sites are inventory entries but are rejected at the
//!   production boundary until their PHP behavior is implemented.

const MARKER: &str = "elephc-trap:";

/// Closed classification used by the generated `unreachable` inventory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapClass {
    /// The immediately preceding call is contractually non-returning.
    PostNoreturn,
    /// Capability validation or a closed runtime invariant proves the site dead.
    ProvenInvariant,
    /// The site follows the backend's deterministic out-of-memory path.
    DeterministicOom,
    /// The trap exists only in an import-free reactor excluded from the public target.
    NonPublic,
    /// Valid PHP can observe a raw Core trap and the behavior must be implemented.
    PhpVisible,
}

impl TrapClass {
    /// Parses one stable marker class without accepting aliases.
    fn parse(value: &str) -> Option<Self> {
        match value {
            "post-noreturn" => Some(Self::PostNoreturn),
            "proven-invariant" => Some(Self::ProvenInvariant),
            "deterministic-oom" => Some(Self::DeterministicOom),
            "non-public" => Some(Self::NonPublic),
            "php-visible" => Some(Self::PhpVisible),
            _ => None,
        }
    }
}

/// One classified Core trap in a rendered WAT module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnreachableSite {
    /// One-based WAT line number.
    pub(crate) line: usize,
    /// Enclosing WAT function identifier, or `<module>` outside a function.
    pub(crate) function: String,
    /// Closed semantic classification.
    pub(crate) class: TrapClass,
    /// Stable source-owned site identifier.
    pub(crate) site: String,
    /// WAT code on the trap line, excluding its comment.
    code: String,
    /// Immediately preceding non-empty WAT code line.
    predecessor: String,
}

/// Stateful lexical context for nested WAT block comments.
#[derive(Default)]
struct WatLexState {
    block_comment_depth: usize,
}

/// Code and line-comment text extracted from one WAT source line.
struct WatLine {
    code: String,
    code_with_strings: String,
    line_comment: String,
}

/// Counts standalone WAT identifier tokens without matching comments or names.
fn token_count(haystack: &str, needle: &str) -> usize {
    haystack
        .match_indices(needle)
        .filter(|(start, _)| {
            let before = haystack[..*start].chars().next_back();
            let after = haystack[*start + needle.len()..].chars().next();
            let is_boundary = |ch: Option<char>| {
                ch.map_or(true, |value| {
                    value.is_ascii_whitespace() || matches!(value, '(' | ')' | ';')
                })
            };
            is_boundary(before) && is_boundary(after)
        })
        .count()
}

/// Lexes one WAT line while removing strings and nested comments from code.
fn lex_wat_line(line: &str, state: &mut WatLexState) -> WatLine {
    let bytes = line.as_bytes();
    let mut code = String::with_capacity(bytes.len());
    let mut code_with_strings = String::with_capacity(bytes.len());
    let mut line_comment = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut offset = 0usize;
    while offset < bytes.len() {
        let value = bytes[offset];
        let next = bytes.get(offset + 1).copied();
        if state.block_comment_depth > 0 {
            code.push(' ');
            code_with_strings.push(' ');
            if value == b'(' && next == Some(b';') {
                state.block_comment_depth += 1;
                code.push(' ');
                code_with_strings.push(' ');
                offset += 2;
                continue;
            }
            if value == b';' && next == Some(b')') {
                state.block_comment_depth -= 1;
                code.push(' ');
                code_with_strings.push(' ');
                offset += 2;
                continue;
            }
            offset += 1;
            continue;
        }
        if in_string {
            code.push(' ');
            code_with_strings.push(value as char);
            if escaped {
                escaped = false;
            } else if value == b'\\' {
                escaped = true;
            } else if value == b'"' {
                in_string = false;
            }
            offset += 1;
            continue;
        }
        if value == b'"' {
            in_string = true;
            code.push(' ');
            code_with_strings.push('"');
            offset += 1;
            continue;
        }
        if value == b';' && next == Some(b';') {
            line_comment = line[offset + 2..].to_string();
            break;
        }
        if value == b'(' && next == Some(b';') {
            state.block_comment_depth = 1;
            code.push(' ');
            code.push(' ');
            code_with_strings.push(' ');
            code_with_strings.push(' ');
            offset += 2;
            continue;
        }
        code.push(value as char);
        code_with_strings.push(value as char);
        offset += 1;
    }
    WatLine {
        code,
        code_with_strings,
        line_comment,
    }
}

/// Extracts a WAT function identifier from a function-opening line.
fn function_identifier(code: &str) -> Option<String> {
    let tail = code.split_once("(func $")?.1;
    let identifier: String = tail
        .chars()
        .take_while(|value| !value.is_ascii_whitespace() && *value != ')')
        .collect();
    (!identifier.is_empty()).then(|| format!("${identifier}"))
}

/// Parses the required inline classification marker for one WAT line.
fn parse_marker(comment: &str, line: usize) -> Result<(TrapClass, String), String> {
    let markers: Vec<&str> = comment
        .split_ascii_whitespace()
        .filter(|token| token.starts_with(MARKER))
        .collect();
    if markers.is_empty() {
        return Err(
            format!(
                "unclassified WebAssembly `unreachable` at rendered WAT line {line}"
            ),
        );
    }
    if markers.len() != 1 {
        return Err(format!(
            "multiple WebAssembly trap markers share rendered WAT line {line}"
        ));
    }
    let marker = markers[0];
    let mut parts = marker[MARKER.len()..].splitn(2, ':');
    let class_name = parts.next().unwrap_or_default();
    let site = parts.next().unwrap_or_default();
    let class = TrapClass::parse(class_name).ok_or_else(|| {
        format!(
            "unknown WebAssembly trap class `{class_name}` at rendered WAT line {line}"
        )
    })?;
    if site.is_empty()
        || !site
            .chars()
            .all(|value| {
                value.is_ascii_lowercase()
                    || value.is_ascii_digit()
                    || "-._".contains(value)
            })
    {
        return Err(format!(
            "invalid WebAssembly trap site `{site}` at rendered WAT line {line}"
        ));
    }
    Ok((class, site.to_string()))
}

/// Inventories every emitted Core `unreachable` and rejects missing markers.
pub(crate) fn inventory_unreachable_sites(wat: &str) -> Result<Vec<UnreachableSite>, String> {
    let mut function = "<module>".to_string();
    let mut predecessor = String::new();
    let mut sites = Vec::new();
    let mut lex_state = WatLexState::default();
    for (offset, raw_line) in wat.lines().enumerate() {
        let line = offset + 1;
        let parsed = lex_wat_line(raw_line, &mut lex_state);
        if let Some(identifier) = function_identifier(&parsed.code) {
            function = identifier;
        }
        let count = token_count(&parsed.code, "unreachable");
        if count == 0 {
            if !parsed.code.trim().is_empty() {
                predecessor = parsed.code.trim().to_string();
            }
            continue;
        }
        if count != 1 {
            return Err(format!(
                "multiple WebAssembly `unreachable` instructions share rendered WAT line {line}"
            ));
        }
        let (class, site) = parse_marker(&parsed.line_comment, line)?;
        sites.push(UnreachableSite {
            line,
            function: function.clone(),
            class,
            site,
            code: parsed.code.trim().to_string(),
            predecessor: predecessor.clone(),
        });
        predecessor = parsed.code.trim().to_string();
    }
    Ok(sites)
}

/// Extracts one defined WAT function, excluding import type annotations with a
/// nested `(func $name ...)` clause.
fn function_definition_code(wat: &str, target: &str) -> Option<String> {
    let mut state = WatLexState::default();
    let mut body = String::new();
    let mut depth = 0i64;
    let mut collecting = false;
    for raw_line in wat.lines() {
        let parsed = lex_wat_line(raw_line, &mut state);
        let code = parsed.code;
        if !collecting {
            if function_identifier(&code).as_deref() != Some(target)
                || !code.trim_start().starts_with("(func $")
            {
                continue;
            }
            collecting = true;
        }
        body.push_str(&code);
        body.push('\n');
        depth += code.bytes().filter(|value| *value == b'(').count() as i64;
        depth -= code.bytes().filter(|value| *value == b')').count() as i64;
        if collecting && depth == 0 {
            return Some(body);
        }
    }
    None
}

/// Returns whether a WAT function body contains any control operator that can
/// bypass straight-line fallthrough to its final registered trap.
///
/// Core 3.0 adds tail calls and typed-reference branch variants to the original
/// `return`/`br*` family. Exception handlers can also transfer control to a
/// surrounding label, so failure helpers reject that entire surface rather than
/// maintaining a version-fragile list of individual spellings.
fn has_noreturn_escape_operator(body: &str) -> bool {
    body.split(|value: char| value.is_ascii_whitespace() || matches!(value, '(' | ')' | ';'))
        .any(|token| {
            token == "return"
                || token.starts_with("return_call")
                || token == "br"
                || token.starts_with("br_")
                || matches!(token, "throw" | "throw_ref" | "try_table")
        })
}

/// Proves a generated failure helper cannot bypass its final registered trap
/// through a Core control-flow escape.
///
/// The approved helpers are straight-line/conditional diagnostic writers. They
/// may use nested `if` expressions, but never a return/tail-call, structured
/// branch, or exception handler; without those escape operators, every normally
/// completing path falls through to the final non-returning call plus
/// `unreachable`.
fn helper_has_closed_noreturn_body(
    wat: &str,
    target: &str,
    proof_site: &UnreachableSite,
) -> bool {
    if proof_site.function != target || proof_site.code.trim() != "unreachable" {
        return false;
    }
    let Some(body) = function_definition_code(wat, target) else {
        return false;
    };
    if has_noreturn_escape_operator(&body) {
        return false;
    }
    let Some(without_function_close) = body.trim_end().strip_suffix(')') else {
        return false;
    };
    without_function_close.trim_end().ends_with("unreachable")
}

/// Counts Core `unreachable` operators in the assembled final artifact.
fn binary_unreachable_count(wat: &str) -> Result<usize, String> {
    let bytes = ::wat::parse_str(wat)
        .map_err(|error| format!("trap inventory WAT assembly failed: {error}"))?;
    let mut count = 0usize;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        let payload =
            payload.map_err(|error| format!("trap inventory binary parse failed: {error}"))?;
        let wasmparser::Payload::CodeSectionEntry(body) = payload else {
            continue;
        };
        let mut operators = body
            .get_operators_reader()
            .map_err(|error| format!("trap inventory operator parse failed: {error}"))?;
        while !operators.eof() {
            if matches!(
                operators
                    .read()
                    .map_err(|error| format!("trap inventory operator read failed: {error}"))?,
                wasmparser::Operator::Unreachable
            ) {
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Returns whether one code fragment is exactly one unfolded direct call.
fn is_unfolded_direct_call(code: &str, target: &str) -> bool {
    let mut tokens = code.split_ascii_whitespace();
    matches!(
        (tokens.next(), tokens.next(), tokens.next()),
        (Some("call"), Some(actual), None) if actual == target
    )
}

/// Returns whether a folded direct call is the final expression in `code`.
fn ends_with_folded_direct_call(code: &str, target: &str) -> bool {
    let marker = format!("(call {target}");
    let starts = code
        .match_indices(marker.as_str())
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    for start in starts.into_iter().rev() {
        let bytes = code.as_bytes();
        let target_end = start + marker.len();
        if !matches!(bytes.get(target_end), Some(b' ' | b'\t' | b'\r' | b'\n' | b')')) {
            continue;
        }
        let mut depth = 0usize;
        for offset in start..bytes.len() {
            match bytes[offset] {
                b'(' => depth += 1,
                b')' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return code[offset + 1..].trim().is_empty();
                    }
                }
                _ => {}
            }
        }
    }
    false
}

/// Returns whether `unreachable` is immediately dominated by one direct call.
fn immediately_preceded_by_call(site: &UnreachableSite, target: &str) -> bool {
    let trap_offset = site
        .code
        .match_indices("unreachable")
        .find(|(offset, _)| {
            let before = site.code[..*offset].chars().next_back();
            let after = site.code[*offset + "unreachable".len()..]
                .chars()
                .next();
            let boundary = |value: Option<char>| {
                value.map_or(true, |ch| {
                    ch.is_ascii_whitespace() || matches!(ch, '(' | ')' | ';')
                })
            };
            boundary(before) && boundary(after)
        })
        .map(|(offset, _)| offset);
    let same_line = trap_offset.is_some_and(|offset| {
        let prefix = site.code[..offset].trim_end();
        is_unfolded_direct_call(prefix, target) || ends_with_folded_direct_call(prefix, target)
    });
    same_line
        || is_unfolded_direct_call(&site.predecessor, target)
        || ends_with_folded_direct_call(&site.predecessor, target)
}

/// Returns whether the same final artifact proves one approved call target
/// cannot return to its caller.
fn target_has_registered_noreturn_proof(
    target: &str,
    wat: &str,
    sites: &[UnreachableSite],
    command_module: bool,
) -> bool {
    let has_site = |function: &str,
                    class: TrapClass,
                    site_name: &str,
                    predecessor: Option<&str>| {
        sites.iter().any(|site| {
            site.function == function
                && site.class == class
                && site.site == site_name
                && helper_has_closed_noreturn_body(wat, function, site)
                && predecessor
                    .is_none_or(|callee| immediately_preceded_by_call(site, callee))
        })
    };
    let proc_exit = command_module;
    let runtime_fail = proc_exit
        && has_site(
            "$__rt_fail",
            TrapClass::PostNoreturn,
            "runtime-fatal-exit",
            Some("$wasi_proc_exit"),
        );
    match target {
        "$wasi_proc_exit" => proc_exit,
        "$__rt_fail" => runtime_fail,
        "$__rt_oom" if command_module => {
            runtime_fail
                && has_site(
                    "$__rt_oom",
                    TrapClass::DeterministicOom,
                    "command-oom",
                    Some("$__rt_fail"),
                )
        }
        "$__rt_oom" => has_site(
            "$__rt_oom",
            TrapClass::NonPublic,
            "reactor-oom",
            None,
        ),
        "$__rt_fail_callable_dispatch" if command_module => {
            runtime_fail
                && has_site(
                    "$__rt_fail_callable_dispatch",
                    TrapClass::PostNoreturn,
                    "command-callable-failure",
                    Some("$__rt_fail"),
                )
        }
        "$__rt_fail_callable_dispatch" => has_site(
            "$__rt_fail_callable_dispatch",
            TrapClass::NonPublic,
            "reactor-callable-corruption",
            None,
        ),
        // `(string)` of an object without `__toString` is PHP's fatal, and the helper writes
        // the class name before exiting — so it is non-returning on the same proof as the
        // method-call fatal: its own body ends in `wasi_proc_exit`.
        "$__rt_fail_object_to_string" => {
            proc_exit
                && has_site(
                    "$__rt_fail_object_to_string",
                    TrapClass::PostNoreturn,
                    "object-to-string-fatal",
                    Some("$wasi_proc_exit"),
                )
        }
        "$__rt_fail_method_call_non_object" => {
            proc_exit
                && has_site(
                    "$__rt_fail_method_call_non_object",
                    TrapClass::PostNoreturn,
                    "method-type-fatal-exit",
                    Some("$wasi_proc_exit"),
                )
        }
        "$__rt_fail_undefined_method" => {
            proc_exit
                && has_site(
                    "$__rt_fail_undefined_method",
                    TrapClass::PostNoreturn,
                    "undefined-method-fatal-exit",
                    Some("$wasi_proc_exit"),
                )
        }
        _ => false,
    }
}

/// Returns whether one trap immediately follows a boundary proven by the same artifact.
fn has_noreturn_predecessor(
    site: &UnreachableSite,
    wat: &str,
    sites: &[UnreachableSite],
    command_module: bool,
) -> bool {
    [
        "$wasi_proc_exit",
        "$__rt_fail",
        "$__rt_oom",
        "$__rt_fail_callable_dispatch",
        "$__rt_fail_method_call_non_object",
        "$__rt_fail_undefined_method",
        "$__rt_fail_object_to_string",
    ]
    .iter()
    .any(|target| {
        immediately_preceded_by_call(site, target)
            && target_has_registered_noreturn_proof(target, wat, sites, command_module)
    })
}

/// Verifies the WAT location associated with one registered invariant proof.
fn validate_invariant_site(site: &UnreachableSite) -> Result<(), String> {
    let valid_location = match site.site.as_str() {
        // `try-table-tail` closes the exception-guarded region of a dispatch loop. Control
        // leaves that region only by branching — to the landing pad on a thrown exception, or
        // out of the loop on return — so falling off its end is unreachable for the same reason
        // `dispatch-loop-tail` is, and it lives in exactly the same functions.
        "dispatch-loop-tail" | "dispatch-state-range" | "eir-unreachable" | "try-table-tail" => {
            site.function.starts_with("$fn_") || site.function == "$_entry"
        }
        "hash-capacity-limit" => site.function == "$__rt_hash_zend_table_size",
        "hash-insert-room-contract" => site.function == "$__rt_hash_insert_owned",
        _ => false,
    };
    if !valid_location {
        return Err(format!(
            "trap `{}` in {}:{} has no matching registered invariant proof",
            site.site, site.function, site.line
        ));
    }
    Ok(())
}

/// Verifies an explicitly excluded trap belongs only to an import-free reactor.
fn validate_non_public_site(site: &UnreachableSite, command_module: bool) -> Result<(), String> {
    let valid_location = match site.site.as_str() {
        "reactor-arithmetic-failure"
        | "reactor-hash-append-occupied"
        | "reactor-mixed-heap-mismatch" => {
            !command_module && site.function.starts_with("$fn_")
        }
        "reactor-oom" => !command_module && site.function == "$__rt_oom",
        "reactor-callable-corruption" => {
            !command_module && site.function == "$__rt_fail_callable_dispatch"
        }
        // `(string)` of an object is PHP's fatal, and a reactor has no WASI to report it
        // through — so the answer is the same trap the other import-free boundaries take.
        "reactor-object-to-string" => {
            !command_module && site.function == "$__rt_mixed_cast_string"
        }
        _ => false,
    };
    if !valid_location {
        return Err(format!(
            "non-public trap `{}` in {}:{} is outside its registered reactor exclusion",
            site.site, site.function, site.line
        ));
    }
    Ok(())
}

/// Verifies that a classified site carries its class-specific structural proof.
fn validate_site_proof(
    site: &UnreachableSite,
    wat: &str,
    sites: &[UnreachableSite],
    command_module: bool,
) -> Result<(), String> {
    match site.class {
        TrapClass::PostNoreturn => {
            if has_noreturn_predecessor(site, wat, sites, command_module) {
                Ok(())
            } else {
                Err(format!(
                    "post-noreturn trap `{}` in {}:{} lacks an approved non-returning predecessor",
                    site.site, site.function, site.line
                ))
            }
        }
        TrapClass::DeterministicOom => {
            let proven = if site.site == "command-oom" && site.function == "$__rt_oom" {
                immediately_preceded_by_call(site, "$__rt_fail")
                    && target_has_registered_noreturn_proof(
                        "$__rt_fail",
                        wat,
                        sites,
                        command_module,
                    )
            } else {
                immediately_preceded_by_call(site, "$__rt_oom")
                    && target_has_registered_noreturn_proof(
                        "$__rt_oom",
                        wat,
                        sites,
                        command_module,
                    )
            };
            if proven {
                Ok(())
            } else {
                Err(format!(
                    "deterministic OOM trap `{}` in {}:{} lacks its immediate OOM failure boundary",
                    site.site, site.function, site.line
                ))
            }
        }
        TrapClass::ProvenInvariant => validate_invariant_site(site),
        TrapClass::NonPublic => validate_non_public_site(site, command_module),
        TrapClass::PhpVisible => Err(format!(
            "PHP-visible WebAssembly `unreachable` site requires implementation: {}:{}:{}",
            site.function, site.line, site.site
        )),
    }
}

/// Detects the generated WASI command import without matching PHP string data.
fn is_command_module(wat: &str) -> bool {
    let mut state = WatLexState::default();
    wat.lines().any(|line| {
        let parsed = lex_wat_line(line, &mut state);
        parsed
            .code_with_strings
            .trim_start()
            .starts_with("(import \"wasi_snapshot_preview1\" \"proc_exit\"")
    })
}

/// Enforces the complete inventory and blocks PHP-visible raw traps.
pub(crate) fn validate_unreachable_inventory(wat: &str) -> Result<(), String> {
    let sites = inventory_unreachable_sites(wat)?;
    let binary_count = binary_unreachable_count(wat)?;
    if binary_count != sites.len() {
        return Err(format!(
            "WebAssembly trap inventory mismatch: {binary_count} binary operators but {} classified WAT sites",
            sites.len()
        ));
    }
    let command_module = is_command_module(wat);
    for site in &sites {
        validate_site_proof(site, wat, &sites, command_module)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Regression tests for the generated Core-trap inventory.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Tests exercise classification syntax, comment handling, and the
    //!   production rejection of PHP-visible raw traps.

    use super::{
        has_noreturn_escape_operator, inventory_unreachable_sites,
        validate_unreachable_inventory, TrapClass,
    };

    /// Verifies every closed non-PHP-visible class is parsed into the inventory.
    #[test]
    fn inventories_the_closed_non_visible_classes() {
        let wat = r#"(module
  (func $f
    unreachable ;; elephc-trap:post-noreturn:runtime-fail
    unreachable ;; elephc-trap:proven-invariant:dispatch-tail
    unreachable ;; elephc-trap:deterministic-oom:heap-grow
    unreachable ;; elephc-trap:non-public:reactor-gap
  )
)"#;
        let sites = inventory_unreachable_sites(wat).expect("inventory should parse");
        assert_eq!(sites.len(), 4);
        assert_eq!(sites[0].function, "$f");
        assert_eq!(sites[0].class, TrapClass::PostNoreturn);
        assert_eq!(sites[1].class, TrapClass::ProvenInvariant);
        assert_eq!(sites[2].class, TrapClass::DeterministicOom);
        assert_eq!(sites[3].class, TrapClass::NonPublic);
    }

    /// Verifies an emitted Core trap without an inline marker is rejected.
    #[test]
    fn rejects_unclassified_unreachable() {
        let error = inventory_unreachable_sites("(module (func $f unreachable))")
            .expect_err("missing marker must fail");
        assert!(error.contains("unclassified WebAssembly"));
    }

    /// Verifies prose mentioning `unreachable` does not create an inventory entry.
    #[test]
    fn ignores_comment_only_mentions() {
        let sites = inventory_unreachable_sites(
            "(module\n  ;; unreachable is discussed here but not emitted\n)",
        )
        .expect("comment-only text should be ignored");
        assert!(sites.is_empty());
    }

    /// Verifies PHP data containing WAT-looking text and comment delimiters is
    /// never interpreted as code by the inventory lexer.
    #[test]
    fn ignores_instruction_text_inside_wat_strings() {
        let wat = r#"(module
  (memory 1)
  (data (i32.const 0) "call $__rt_fail unreachable ;; still data")
)"#;
        let sites = inventory_unreachable_sites(wat).expect("string data should be ignored");
        assert!(sites.is_empty());
        validate_unreachable_inventory(wat).expect("string data must not create a binary mismatch");
    }

    /// Verifies PHP-visible raw traps fail the production validation boundary.
    #[test]
    fn rejects_php_visible_sites_before_publication() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $f\n    unreachable ;; elephc-trap:php-visible:cast-gap\n  )\n)",
        )
        .expect_err("PHP-visible trap must fail");
        assert!(error.contains("$f:3:cast-gap"));
    }

    /// Verifies a post-noreturn label cannot bless a trap without an approved call.
    #[test]
    fn rejects_unproved_post_noreturn_classification() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $f\n    unreachable ;; elephc-trap:post-noreturn:fake-tail\n  )\n)",
        )
        .expect_err("post-noreturn requires a structural predecessor proof");
        assert!(error.contains("lacks an approved non-returning predecessor"));
    }

    /// Verifies a helper name that merely prefixes an approved boundary is not
    /// mistaken for the exact non-returning call target.
    #[test]
    fn rejects_noreturn_boundary_name_prefixes() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $__rt_fail_but_returns)\n  (func $f\n    (call $__rt_fail_but_returns)\n    unreachable ;; elephc-trap:post-noreturn:fake-prefix\n  )\n)",
        )
        .expect_err("lookalike helper must not prove non-returnability");
        assert!(error.contains("lacks an approved non-returning predecessor"));
    }

    /// Verifies an approved helper name is insufficient when the same final
    /// artifact provides a returning implementation instead of its registered
    /// non-returning proof site.
    #[test]
    fn rejects_returning_implementation_of_approved_noreturn_helper() {
        let error = validate_unreachable_inventory(
            r#"(module
  (import "wasi_snapshot_preview1" "proc_exit" (func $wasi_proc_exit (param i32)))
  (func $__rt_fail (param i32))
  (func $f
    (call $__rt_fail (i32.const 1))
    unreachable ;; elephc-trap:post-noreturn:fake-returning-helper
  )
)"#,
        )
        .expect_err("a returning helper implementation must not prove non-returnability");
        assert!(error.contains("lacks an approved non-returning predecessor"));
    }

    /// Verifies a helper cannot combine the registered final proof site with an
    /// earlier conditional return that bypasses the failure boundary.
    #[test]
    fn rejects_registered_helper_with_an_early_return_path() {
        let error = validate_unreachable_inventory(
            r#"(module
  (import "wasi_snapshot_preview1" "proc_exit" (func $wasi_proc_exit (param i32)))
  (func $__rt_fail (param $code i32)
    (if (i32.eqz (local.get $code)) (then return))
    (call $wasi_proc_exit (i32.const 255))
    unreachable ;; elephc-trap:post-noreturn:runtime-fatal-exit
  )
  (func $f
    (call $__rt_fail (i32.const 0))
    unreachable ;; elephc-trap:post-noreturn:caller-after-returning-helper
  )
)"#,
        )
        .expect_err("an early return must invalidate the helper's non-returning proof");
        assert!(error.contains("lacks an approved non-returning predecessor"));
    }

    /// Verifies a Core tail call cannot return from the registered helper while
    /// leaving its final proof marker present and structurally valid.
    #[test]
    fn rejects_registered_helper_with_a_tail_call_escape() {
        let error = validate_unreachable_inventory(
            r#"(module
  (import "wasi_snapshot_preview1" "proc_exit" (func $wasi_proc_exit (param i32)))
  (func $returns)
  (func $__rt_fail (param $code i32)
    (if (i32.eqz (local.get $code)) (then (return_call $returns)))
    (call $wasi_proc_exit (i32.const 255))
    unreachable ;; elephc-trap:post-noreturn:runtime-fatal-exit
  )
  (func $f
    (call $__rt_fail (i32.const 0))
    unreachable ;; elephc-trap:post-noreturn:caller-after-tail-call
  )
)"#,
        )
        .expect_err("a tail call must invalidate the helper's non-returning proof");
        assert!(error.contains("lacks an approved non-returning predecessor"));
    }

    /// Verifies the Core 3.0 tail-call, branch, typed-reference branch, and
    /// exception-control families all invalidate the closed-helper proof.
    #[test]
    fn recognizes_every_core_noreturn_escape_family() {
        for operator in [
            "return",
            "return_call",
            "return_call_indirect",
            "return_call_ref",
            "br",
            "br_if",
            "br_table",
            "br_on_null",
            "br_on_non_null",
            "br_on_cast",
            "br_on_cast_fail",
            "throw",
            "throw_ref",
            "try_table",
        ] {
            assert!(
                has_noreturn_escape_operator(&format!("(func $__rt_fail {operator})")),
                "{operator} must invalidate a no-return proof"
            );
        }
        assert!(
            !has_noreturn_escape_operator(
                "(func $__rt_fail (if (i32.const 1) (then nop)) unreachable)"
            ),
            "nested diagnostics-only conditionals must remain admissible"
        );
    }

    /// Verifies a call hidden in a nested WAT block comment cannot prove that
    /// the following real Core trap is post-noreturn.
    #[test]
    fn rejects_noreturn_calls_inside_nested_block_comments() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $__rt_fail (param i32))\n  (func $f\n    (; outer (; (call $__rt_fail (i32.const 1)) ;) ;)\n    unreachable ;; elephc-trap:post-noreturn:comment-proof\n  )\n)",
        )
        .expect_err("block-comment call must not prove non-returnability");
        assert!(error.contains("lacks an approved non-returning predecessor"));
    }

    /// Verifies a conditional call on the trap line does not count as an
    /// immediately dominating non-returning predecessor.
    #[test]
    fn rejects_conditional_same_line_noreturn_calls() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $__rt_fail (param i32))\n  (func $f\n    (if (i32.const 0) (then (call $__rt_fail (i32.const 1)))) unreachable ;; elephc-trap:post-noreturn:conditional-proof\n  )\n)",
        )
        .expect_err("conditional call must not prove the following trap");
        assert!(error.contains("lacks an approved non-returning predecessor"));
    }

    /// Verifies an OOM label cannot bless a path that does not use the OOM boundary.
    #[test]
    fn rejects_unproved_oom_classification() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $f\n    unreachable ;; elephc-trap:deterministic-oom:fake-oom\n  )\n)",
        )
        .expect_err("deterministic OOM requires the shared boundary");
        assert!(error.contains("lacks its immediate OOM failure boundary"));
    }

    /// Verifies the command OOM helper cannot bless a raw Core trap without
    /// first entering the deterministic PHP fatal boundary.
    #[test]
    fn rejects_command_oom_without_failure_boundary() {
        let error = validate_unreachable_inventory(
            r#"(module
  (import "wasi_snapshot_preview1" "proc_exit" (func $wasi_proc_exit (param i32)))
  (func $__rt_oom
    unreachable ;; elephc-trap:deterministic-oom:command-oom
  )
)"#,
        )
        .expect_err("command OOM must immediately call the PHP failure helper");
        assert!(error.contains("lacks its immediate OOM failure boundary"));
    }

    /// Verifies invariant labels are drawn from the audited closed registry.
    #[test]
    fn rejects_unregistered_invariant_classification() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $f\n    unreachable ;; elephc-trap:proven-invariant:invented\n  )\n)",
        )
        .expect_err("invented invariant proof must fail");
        assert!(error.contains("has no matching registered invariant proof"));
    }

    /// Verifies a real invariant id cannot be transplanted into another WAT
    /// function to bless an unrelated trap.
    #[test]
    fn rejects_registered_invariant_at_the_wrong_location() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $f\n    unreachable ;; elephc-trap:proven-invariant:hash-capacity-limit\n  )\n)",
        )
        .expect_err("invariant proof must match its registered runtime location");
        assert!(error.contains("has no matching registered invariant proof"));
    }

    /// Verifies the legacy closed-dispatch invariant classification is no
    /// longer accepted; dispatch fallthroughs must use a registered failure
    /// boundary instead.
    #[test]
    fn rejects_legacy_closed_dispatch_invariant_classification() {
        let error = validate_unreachable_inventory(
            "(module\n  (func $fn_gd_fake\n    nop\n    unreachable ;; elephc-trap:proven-invariant:closed-method-dispatch\n  )\n)",
        )
        .expect_err("dispatch name alone must not establish the closed-ladder invariant");
        assert!(error.contains("has no matching registered invariant proof"));
    }

    /// Verifies the hash-append exceptional tail is admitted only for the
    /// import-free reactor surface explicitly excluded from public commands.
    #[test]
    fn accepts_registered_non_public_reactor_hash_append() {
        validate_unreachable_inventory(
            "(module\n  (func $fn_u_reactor\n    unreachable ;; elephc-trap:non-public:reactor-hash-append-occupied\n  )\n)",
        )
        .expect("registered non-public reactor trap should validate");
    }

    /// Verifies the import-free reactor OOM boundary is explicitly excluded
    /// rather than mislabeled as a deterministic PHP failure.
    #[test]
    fn accepts_registered_non_public_reactor_oom() {
        validate_unreachable_inventory(
            "(module\n  (func $__rt_oom\n    unreachable ;; elephc-trap:non-public:reactor-oom\n  )\n)",
        )
        .expect("registered non-public reactor OOM should validate");
    }

    /// Verifies command detection ignores an import-shaped PHP data string.
    #[test]
    fn ignores_command_import_text_inside_wat_data() {
        validate_unreachable_inventory(
            r#"(module
  (memory 1)
  (data (i32.const 0) "(import \22wasi_snapshot_preview1\22 \22proc_exit\22")
  (func $fn_u_reactor
    unreachable ;; elephc-trap:non-public:reactor-arithmetic-failure
  )
)"#,
        )
        .expect("import-shaped data must not turn a reactor into a command");
    }

    /// Verifies a reactor exclusion marker cannot be transplanted into a WASI
    /// command module.
    #[test]
    fn rejects_non_public_reactor_traps_in_commands() {
        let error = validate_unreachable_inventory(
            r#"(module
  (import "wasi_snapshot_preview1" "proc_exit" (func $wasi_proc_exit (param i32)))
  (func $fn_u_command
    unreachable ;; elephc-trap:non-public:reactor-arithmetic-failure
  )
)"#,
        )
        .expect_err("command modules cannot contain reactor exclusions");
        assert!(error.contains("outside its registered reactor exclusion"));
    }

    /// Verifies the reactor-only raw OOM boundary cannot be transplanted into
    /// a command module.
    #[test]
    fn rejects_non_public_reactor_oom_in_commands() {
        let error = validate_unreachable_inventory(
            r#"(module
  (import "wasi_snapshot_preview1" "proc_exit" (func $wasi_proc_exit (param i32)))
  (func $__rt_oom
    unreachable ;; elephc-trap:non-public:reactor-oom
  )
)"#,
        )
        .expect_err("command modules cannot use the reactor OOM exclusion");
        assert!(error.contains("outside its registered reactor exclusion"));
    }
}
