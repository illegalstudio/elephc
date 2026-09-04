//! Purpose:
//! The mysqli standard-library surface (MySQL / MariaDB), implemented in
//! elephc-PHP as a second front-end over the existing `elephc_pdo` bridge.
//! Declares `mysqli`, `mysqli_stmt`, `mysqli_result`, `mysqli_sql_exception`,
//! the `MYSQLI_*` constants, and the `mysqli_*` procedural aliases — never the
//! PDO classes: a mysqli-only program must not see `PDO` / `PDOStatement` or
//! throw `PDOException`.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via
//!   `inject_if_used`, after the PDO prelude injection and before name
//!   resolution.
//!
//! Key details:
//! - The surface is BUILT as AST (`build::mysqli_declarations`), not parsed:
//!   the PHP fragments (`constants`, `exception`, `connection`, `result`,
//!   `statement`, `procedural`, assembled per version by `fragments`) are
//!   `cfg(test)` and serve only as the node-by-node parse-parity oracle the
//!   build is checked against, so the compiler never tokenizes PHP on its own
//!   behalf for a mysqli compile.
//! - The prelude is injected only when the program references a mysqli symbol
//!   (see `detect`) or `--with-mysqli` forces it; injection also prepends the
//!   shared `extern "elephc_pdo"` block via
//!   `pdo_prelude::inject_bridge_externs`, which is idempotent so a program
//!   using both PDO and mysqli declares the externs exactly once.
//! - Declaring the shared externs is what makes the checker record the
//!   `elephc_pdo` staticlib, so mysqli programs link the bridge with no new
//!   `BRIDGES` entry.
//! - `extension_loaded('mysqli')` reporting is surface-based: the pipeline
//!   records the "mysqli" surface when this prelude is injected (the shared
//!   archive alone identifies no extension). `mysqlnd` is never reported.

use crate::parser::ast::{Program, Stmt};
use crate::php_version::PhpVersion;

mod build;
#[cfg(test)]
mod connection;
#[cfg(test)]
mod constants;
mod detect;
#[cfg(test)]
mod exception;
#[cfg(test)]
mod fragments;
#[cfg(test)]
mod procedural;
#[cfg(test)]
mod result;
#[cfg(test)]
mod statement;

/// Returns whether the program references the mysqli surface. Exposed so the
/// pipeline (and the test harnesses) can record the "mysqli" PHP surface for
/// `extension_loaded()` reporting using the same detection that decides
/// prelude injection.
pub fn program_uses_mysqli(program: &[Stmt]) -> bool {
    detect::program_uses_mysqli(program)
}

/// Prepends the mysqli prelude (and, idempotently, the shared `elephc_pdo`
/// extern block) to `program` when it references the mysqli surface, so the
/// classes and externs compile through the normal pipeline only for
/// mysqli-using programs. The prelude carries only declarations, which are
/// hoisted, so prepending does not change top-level execution order.
///
/// `force` (set by `--with-mysqli`) bypasses the usage scan so the mysqli
/// surface is always injected and the bridge always linked. The PDO classes are
/// never injected by this path.
///
/// Injection records every prelude declaration in `inventory` under the
/// `"mysqli"` group, so `--with-mysqli` can root the whole surface through
/// reachability (`forced_groups`) exactly like `--with-pdo` — without it, a
/// program with no static mysqli reference would have the forced surface
/// dead-code-eliminated out of the binary.
pub fn inject_if_used(
    program: Program,
    force: bool,
    php_version: PhpVersion,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !detect::program_uses_mysqli(&program) {
        return program;
    }
    // BUILT, not parsed. `build::mysqli_declarations` produces the same AST the PHP
    // fragments parse to — `built_declarations_match_the_php_for_every_version`
    // compares them node by node for every profile — so the tokenizer and parser
    // no longer run over the embedded source on a mysqli compile. Every compile
    // gets a fresh program because later passes mutate the injected AST.
    let mut combined = build::mysqli_declarations(php_version);
    inventory.record_program("mysqli", &combined);
    combined.extend(program);
    // Shared with the PDO prelude; idempotent, so whichever surface injects
    // second finds the block already declared and leaves it alone.
    crate::pdo_prelude::inject_bridge_externs(combined)
}

/// Returns the mysqli prelude PHP fragments as `(label, source)` pairs, for
/// source-level scans (the prelude parity gates). Test-only, like the fragments
/// themselves: the compilation path builds the surface with
/// `build::mysqli_declarations`, and the oracle below is what keeps the two
/// equal, so a gate over the text is a gate over what ships.
#[cfg(test)]
pub fn fragment_sources() -> &'static [(&'static str, &'static str)] {
    &[
        ("mysqli_prelude(constants)", constants::SRC),
        ("mysqli_prelude(exception)", exception::SRC),
        ("mysqli_prelude(connection)", connection::SRC),
        ("mysqli_prelude(result)", result::SRC),
        ("mysqli_prelude(statement)", statement::SRC),
        ("mysqli_prelude(procedural)", procedural::SRC),
    ]
}

/// Returns the complete assembled mysqli prelude PHP for one version, for the
/// transcription driver (`synthetic_class::transcribe::tests`) and the oracle.
#[cfg(test)]
pub(crate) fn assembled_source_for_version(php_version: PhpVersion) -> String {
    fragments::source_for_version(php_version)
}

/// Parses the assembled mysqli prelude PHP for one version exactly as the
/// compiler used to at injection time; the oracle's reference side.
#[cfg(test)]
pub(crate) fn parsed_prelude_for_version(php_version: PhpVersion) -> Program {
    let source = fragments::source_for_version(php_version);
    let tokens = crate::lexer::tokenize(&source)
        .unwrap_or_else(|e| panic!("PHP {php_version} mysqli prelude must tokenize: {e:?}"));
    crate::parser::parse_internal(&tokens)
        .unwrap_or_else(|e| panic!("PHP {php_version} mysqli prelude must parse: {e:?}"))
}

#[cfg(test)]
mod oracle_tests {
    //! Purpose:
    //! The parse-parity oracle for the built mysqli surface: for every PHP
    //! version, `build::mysqli_declarations` must equal the parse of the PHP the
    //! same version used to ship, declaration by declaration.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Spans are stripped because a built node has none and a parsed one does;
    //!   everything else — order, types, nesting, name qualification — must match.
    //! - `ELEPHC_MYSQLI_ORACLE_DUMP=<dir>` writes both renderings of a diverging
    //!   declaration out to be diffed, since one enormous line is undiffable.

    use super::*;
    use crate::parser::ast::StmtKind;

    /// THE ORACLE FOR THE TRANSCRIPTION: the built AST must equal the parse of
    /// the PHP the same profile ships, for every profile. The three hand-written
    /// version conditionals (`$reportMode` default, `fetch_column`,
    /// `execute_query`) are exactly what a transcription cannot check by itself,
    /// and a conditional in the wrong branch still compiles — this names the
    /// declaration that diverged instead.
    #[test]
    fn built_declarations_match_the_php_for_every_version() {
        for version in PhpVersion::ALL {
            let parsed = parsed_prelude_for_version(version);
            let built = build::mysqli_declarations(version);
            assert_eq!(
                built.len(),
                parsed.len(),
                "PHP {version}: declaration COUNT differs — built {} vs parsed {}",
                built.len(),
                parsed.len()
            );
            for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
                let decl = declaration_label(parsed_stmt);
                let left = strip_spans(&format!("{built_stmt:?}"));
                let right = strip_spans(&format!("{parsed_stmt:?}"));
                if left != right {
                    if let Ok(dir) = std::env::var("ELEPHC_MYSQLI_ORACLE_DUMP") {
                        let spelling = version.spelling().replace('.', "_");
                        std::fs::write(
                            format!("{dir}/built_{spelling}_{decl}.txt"),
                            left.replace("}, ", "},\n"),
                        )
                        .expect("dump built");
                        std::fs::write(
                            format!("{dir}/parsed_{spelling}_{decl}.txt"),
                            right.replace("}, ", "},\n"),
                        )
                        .expect("dump parsed");
                    }
                    panic!("PHP {version}: built `{decl}` differs from its PHP");
                }
            }
        }
    }

    /// The built program must carry the internal source mode the parsed one got
    /// from `parse_internal`, or name resolution would treat the prelude's
    /// `__elephc_*` calls as user code.
    #[test]
    fn built_declarations_are_stamped_internal() {
        for stmt in build::mysqli_declarations(PhpVersion::default()) {
            assert_eq!(
                stmt.source_mode,
                crate::source::SourceMode::Internal,
                "{} must be built under the internal source mode",
                declaration_label(&stmt)
            );
        }
    }

    /// Names a top-level declaration for assertion messages and dump files.
    fn declaration_label(stmt: &Stmt) -> String {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, .. }
            | StmtKind::ClassDecl { name, .. }
            | StmtKind::ExternFunctionDecl { name, .. }
            | StmtKind::ConstDecl { name, .. } => name.clone(),
            other => format!("{other:?}").chars().take(40).collect(),
        }
    }

    /// Removes span payloads so a built node and a parsed node compare on
    /// structure alone.
    fn strip_spans(rendered: &str) -> String {
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered;
        while let Some(at) = rest.find("Span {") {
            cleaned.push_str(&rest[..at]);
            cleaned.push_str("Span");
            let after = &rest[at..];
            let close = after.find('}').map(|end| end + 1).unwrap_or(after.len());
            rest = &after[close..];
        }
        cleaned.push_str(rest);
        cleaned
    }
}

#[cfg(test)]
mod inventory_tests {
    use super::*;
    use crate::names::php_symbol_key;

    /// `--with-mysqli` forces the surface through reachability via the prelude
    /// inventory: injection must record every mysqli declaration under the
    /// "mysqli" group so `forced_groups` can root it (same contract as PDO —
    /// without this, a program whose only trace is `extension_loaded('mysqli')`
    /// gets the whole surface dead-code-eliminated).
    #[test]
    fn inject_records_the_mysqli_prelude_group() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program =
            inject_if_used(Vec::new(), true, PhpVersion::default(), &mut inventory);
        assert!(!program.is_empty(), "forced injection must produce the prelude");
        let group = inventory
            .groups
            .get("mysqli")
            .expect("injection must record the mysqli prelude group");
        for class in ["mysqli", "mysqli_stmt", "mysqli_result", "mysqli_sql_exception"] {
            assert!(
                group.classes.contains(&php_symbol_key(class)),
                "mysqli group missing class {class}"
            );
        }
        for function in ["mysqli_connect", "mysqli_query", "mysqli_stmt_bind_param"] {
            assert!(
                group.functions.contains(&php_symbol_key(function)),
                "mysqli group missing function {function}"
            );
        }
        assert!(
            group.methods.iter().any(|(class, method, _)| {
                class == &php_symbol_key("mysqli")
                    && method == &php_symbol_key("real_connect")
            }),
            "mysqli group missing method mysqli::real_connect"
        );
    }

    /// The inventory group must be identical whichever way the surface is
    /// produced: recording the BUILT program yields the same classes, functions
    /// and methods as recording the PARSED PHP did before the migration, for
    /// every profile (the gated members included).
    #[test]
    fn built_and_parsed_programs_record_the_same_inventory() {
        for version in PhpVersion::ALL {
            let mut from_built = crate::optimize::reachability::PreludeInventory::new();
            from_built.record_program("mysqli", &build::mysqli_declarations(version));
            let mut from_parsed = crate::optimize::reachability::PreludeInventory::new();
            from_parsed.record_program("mysqli", &parsed_prelude_for_version(version));
            let built = &from_built.groups["mysqli"];
            let parsed = &from_parsed.groups["mysqli"];
            assert_eq!(built.classes, parsed.classes, "PHP {version}: classes");
            assert_eq!(built.functions, parsed.functions, "PHP {version}: functions");
            assert_eq!(built.methods, parsed.methods, "PHP {version}: methods");
        }
    }
}
