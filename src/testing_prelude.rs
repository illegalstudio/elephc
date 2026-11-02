//! Purpose:
//! elephc's native test runtime, declared as a compiler prelude: `Elephc\Testing\TestCase`
//! (lifecycle hooks and the supported assertion subset), `Elephc\Testing\Runner` (runs tests,
//! prints the report, computes the exit code), the `AssertionFailedError` /
//! `SkippedTestError` exceptions, and the `PHPUnit\Framework\TestCase` alias that lets a
//! PHPUnit-written suite compile unchanged. `.plans/elephc-test.md` is the design.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`, after
//!   the hash prelude and before name resolution.
//!
//! Key details:
//! - WHY A PRELUDE AND NOT A COMPOSER PACKAGE. Declaration reachability compiles only what the
//!   program reaches, so `elephc test` has to synthesize real `new` expressions for the
//!   discovered test classes inside the compiler; with discovery already there, the runtime
//!   belongs beside it, and a hand-written entry gets the same classes with no dependency to
//!   install. The whole feature compiles through the ordinary class pipeline with NO new
//!   assembly, on every supported target at once.
//! - WHY IT IS BUILT, NOT PARSED. `build` carries the declarations as `synthetic_class`
//!   builders transcribed from the PHP form in `fragments`, which stays in the tree under
//!   `cfg(test)` as the node-by-node parse-parity oracle.
//! - PAY-FOR-USE. Injected only when `detect::program_uses_testing` finds a reference to the
//!   `Elephc\Testing` namespace or to the PHPUnit alias, or when the caller forces it
//!   (`elephc test`). A reference that only an autoloaded file carries is caught by
//!   `inject_after_autoload`, which the pipeline runs after `autoload::run`. The prelude carries only declarations, so prepending it is
//!   order-independent. Its declarations are recorded in the inventory group `testing`.
//! - The report format the runner prints (`ok   Class::method (n assertions)`, `FAIL`,
//!   `ERROR`, `OK (...)`, `Tests: ...`) is a stable surface consumed by Ivory's `make test`
//!   and by `tests/codegen/testing.rs`.

mod build;
mod detect;
pub mod discover;
#[cfg(test)]
mod fragments;

use crate::parser::ast::Program;

/// The inventory group the injected declarations are recorded under.
pub(crate) const INVENTORY_GROUP: &str = "testing";

/// Builds the testing surface: the `Elephc\Testing` namespace and the PHPUnit alias.
pub(crate) fn testing_declarations() -> Program {
    build::testing_declarations()
}

/// Injects the testing prelude when the program references the runtime, leaving every
/// other program untouched.
///
/// `force` comes from `elephc test` (or the codegen harness); otherwise the decision is
/// `detect::program_uses_testing`. The prelude carries only declarations, so prepending it
/// is order-independent: PHP hoists them.
///
/// Runs under `crate::compiler_stack::with_compiler_stack` (issue #1149): its usage scan walks
/// the whole program, so an embedder calling it without the CLI driver's wrapper gets the same
/// stack budget.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        inject_if_used_on_compiler_stack(program, force, inventory)
    })
}

/// The body of [`inject_if_used`], run on the stack `with_compiler_stack` sized.
fn inject_if_used_on_compiler_stack(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !detect::program_uses_testing(&program) {
        return program;
    }
    let mut combined = testing_declarations();
    inventory.record_program(INVENTORY_GROUP, &combined);
    combined.extend(program);
    combined
}

/// Injects the testing prelude after autoloading when a file the autoloader brought in is
/// the first to reference the runtime (an entry calling a helper class that builds a
/// `Runner`, for instance), and the pre-resolution pass therefore did not inject it.
///
/// The declarations are name-resolved on the way in, exactly as `autoload::run` resolves the
/// files it splices, because this position is past the pipeline's own name resolution. A
/// program that already carries the prelude (the `testing` inventory group exists) is
/// returned unchanged.
pub fn inject_after_autoload(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Result<Program, crate::errors::CompileError> {
    crate::compiler_stack::with_compiler_stack(|| {
        if inventory.groups.contains_key(INVENTORY_GROUP)
            || !detect::program_uses_testing(&program)
        {
            return Ok(program);
        }
        let mut combined = crate::name_resolver::resolve(testing_declarations())?;
        inventory.record_program(INVENTORY_GROUP, &combined);
        combined.extend(program);
        Ok(combined)
    })
}

/// Parses the PHP form of the prelude exactly as the compiler would at injection time; the
/// oracle's reference side.
#[cfg(test)]
pub(crate) fn parsed_prelude() -> Program {
    let tokens =
        crate::lexer::tokenize(fragments::SRC).expect("testing prelude PHP must tokenize");
    crate::parser::parse_internal(&tokens).expect("testing prelude PHP must parse")
}

#[cfg(test)]
mod oracle_tests {
    //! Purpose:
    //! The parse-parity oracle for the built testing surface: `build::testing_declarations`
    //! must equal the parse of the PHP form, declaration by declaration.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Spans are stripped because a built node has none and a parsed one does; everything
    //!   else (order, types, nesting, name qualification) must match.
    //! - `ELEPHC_TESTING_ORACLE_DUMP=<dir>` writes both renderings of a diverging declaration
    //!   out to be diffed, since one enormous line is undiffable.

    use super::*;
    use crate::parser::ast::{Stmt, StmtKind};

    /// The built declarations are the parse of the PHP form, node for node.
    #[test]
    fn built_declarations_match_the_php() {
        let parsed = parsed_prelude();
        let built = build::testing_declarations();
        assert_eq!(
            built.len(),
            parsed.len(),
            "declaration COUNT differs: built {} vs parsed {}",
            built.len(),
            parsed.len()
        );
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            let decl = declaration_label(parsed_stmt);
            let left = strip_spans(&format!("{built_stmt:?}"));
            let right = strip_spans(&format!("{parsed_stmt:?}"));
            if left != right {
                if let Ok(dir) = std::env::var("ELEPHC_TESTING_ORACLE_DUMP") {
                    std::fs::write(format!("{dir}/built_{decl}.txt"), left.replace("}, ", "},\n"))
                        .expect("dump built");
                    std::fs::write(
                        format!("{dir}/parsed_{decl}.txt"),
                        right.replace("}, ", "},\n"),
                    )
                    .expect("dump parsed");
                }
                panic!("built declaration `{decl}` differs from its PHP form");
            }
        }
    }

    /// The surface is fixed: the runtime namespace with its five classes, then the alias
    /// namespace with one. A class disappearing silently is exactly the failure the PHP-text
    /// form could not catch.
    #[test]
    fn declares_the_runtime_and_the_alias() {
        let program = build::testing_declarations();
        let namespaces: Vec<(String, Vec<String>)> = program
            .iter()
            .map(|stmt| match &stmt.kind {
                StmtKind::NamespaceBlock { name, body } => (
                    name.as_ref().map(|name| name.as_str().to_string()).unwrap_or_default(),
                    body.iter().map(declaration_label).collect(),
                ),
                other => panic!("unexpected top-level declaration: {other:?}"),
            })
            .collect();
        assert_eq!(
            namespaces,
            vec![
                (
                    "Elephc\\Testing".to_string(),
                    vec![
                        "AssertionFailedError".to_string(),
                        "SkippedTestError".to_string(),
                        "IncompleteTestError".to_string(),
                        "TestCase".to_string(),
                        "Runner".to_string(),
                    ]
                ),
                ("PHPUnit\\Framework".to_string(), vec!["TestCase".to_string()]),
            ]
        );
    }

    /// Every assertion is `protected` and `void`, takes a trailing `string $message = ''`,
    /// and `TestCase` is abstract: the shape a PHPUnit-written suite relies on.
    #[test]
    fn assertions_share_the_phpunit_shape() {
        let program = build::testing_declarations();
        let StmtKind::NamespaceBlock { body, .. } = &program[0].kind else {
            panic!("the runtime namespace must come first");
        };
        let StmtKind::ClassDecl {
            methods,
            is_abstract,
            ..
        } = &body[3].kind
        else {
            panic!("TestCase must be the fourth declaration");
        };
        assert!(*is_abstract, "TestCase is abstract");
        let assertions: Vec<&crate::parser::ast::ClassMethod> = methods
            .iter()
            .filter(|method| method.name.starts_with("assert"))
            .collect();
        assert_eq!(assertions.len(), 24, "the supported assertion subset");
        for method in assertions {
            assert_eq!(
                method.visibility,
                crate::parser::ast::Visibility::Protected,
                "{} must be protected",
                method.name
            );
            assert_eq!(
                method.return_type,
                Some(crate::parser::ast::TypeExpr::Void),
                "{} must return void",
                method.name
            );
            let (last_name, last_type, last_default, _) =
                method.params.last().expect("an assertion takes parameters");
            assert_eq!(last_name, "message", "{} ends with $message", method.name);
            assert_eq!(last_type, &Some(crate::parser::ast::TypeExpr::Str));
            assert!(last_default.is_some(), "{} defaults $message", method.name);
        }
    }

    /// Names the declaration a statement carries, for diagnostics.
    fn declaration_label(stmt: &Stmt) -> String {
        match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::FunctionDecl { name, .. } => name.clone(),
            StmtKind::NamespaceBlock { name, .. } => {
                name.as_ref().map(|name| name.as_str().to_string()).unwrap_or_default()
            }
            _ => "<other>".to_string(),
        }
    }

    /// Removes span payloads so a synthetic node and a parsed node compare on structure.
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

    /// `elephc test` forces the surface through reachability via the prelude inventory:
    /// injection must record every declaration under the `testing` group so
    /// `forced_groups` can root it.
    #[test]
    fn inject_records_the_testing_prelude_group() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = inject_if_used(Vec::new(), true, &mut inventory);
        assert!(!program.is_empty(), "forced injection must produce the prelude");
        let group = inventory
            .groups
            .get(INVENTORY_GROUP)
            .expect("injection must record the testing prelude group");
        for class in [
            "Elephc\\Testing\\TestCase",
            "Elephc\\Testing\\Runner",
            "Elephc\\Testing\\AssertionFailedError",
            "PHPUnit\\Framework\\TestCase",
        ] {
            assert!(
                group.classes.contains(&php_symbol_key(class)),
                "{class} must be recorded in the testing group"
            );
        }
        assert!(
            group.methods.contains(&(
                php_symbol_key("Elephc\\Testing\\TestCase"),
                php_symbol_key("assertSame"),
                false
            )),
            "methods must be recorded with their class"
        );
    }

    /// A program that never references the runtime is returned untouched.
    #[test]
    fn unrelated_programs_are_not_injected() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let tokens = crate::lexer::tokenize("<?php echo strlen('x');").expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let injected = inject_if_used(program.clone(), false, &mut inventory);
        assert_eq!(injected.len(), program.len());
        assert!(inventory.groups.get(INVENTORY_GROUP).is_none());
    }

    /// A reference that appears only after autoloading injects the resolved prelude once; a
    /// program that already carries it is left alone.
    #[test]
    fn post_autoload_injection_fills_the_gap_once() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let tokens = crate::lexer::tokenize(
            "<?php namespace App; final class Suite { public function run(): int { $r = new \\Elephc\\Testing\\Runner(); return $r->finish(); } }",
        )
        .expect("tokenize");
        let program = crate::name_resolver::resolve(crate::parser::parse(&tokens).expect("parse"))
            .expect("resolve");
        let injected = inject_after_autoload(program.clone(), &mut inventory).expect("inject");
        assert!(injected.len() > program.len(), "the prelude is prepended");
        assert!(inventory.groups.contains_key(INVENTORY_GROUP));
        let again = inject_after_autoload(injected.clone(), &mut inventory).expect("inject");
        assert_eq!(again.len(), injected.len(), "a second pass adds nothing");
    }

    /// A suite that imports the runtime class gets the prelude prepended.
    #[test]
    fn referencing_programs_are_injected() {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let tokens = crate::lexer::tokenize(
            "<?php use Elephc\\Testing\\TestCase; final class T extends TestCase {}",
        )
        .expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let injected = inject_if_used(program.clone(), false, &mut inventory);
        assert_eq!(injected.len(), program.len() + testing_declarations().len());
        assert!(inventory.groups.get(INVENTORY_GROUP).is_some());
    }
}
