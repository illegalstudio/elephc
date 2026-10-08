//! Purpose:
//! Regression tests for issue #686: each recursive compiler phase, called on its own, walks
//! `MAX_COMPILER_NESTING` levels on a small thread.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every fixture sets its own 256 KiB thread rather than inheriting one, so the codegen
//!   suite's 32 MiB `RUST_MIN_STACK` cannot mask a phase that lost its budget.
//! - Each fixture reports a small summary, so the deep values are dropped inside that thread
//!   too rather than on the parent's larger stack.
//! - A regression aborts the test PROCESS rather than failing an assertion, which is why these
//!   live in their own binary.
//! - The phase-level budget itself is described in `docs/internals/the-parser.md`.

use std::collections::HashSet;

/// The stack an embedder's worker thread plausibly has. Well under what depth 1024 needs.
const EMBEDDER_STACK_BYTES: usize = 256 * 1024;

/// Source nesting at the compiler's own documented limit.
const NESTING_DEPTH: usize = 1024;

/// Runs `body` on a thread with [`EMBEDDER_STACK_BYTES`] of stack and returns what it REPORTS.
///
/// The report is a small owned summary on purpose. Handing the phase's own result back through
/// `join()` would move a deeply nested AST out to the parent thread and drop it THERE, on
/// libtest's much larger stack — so the recursive `Drop` these fixtures are also about would
/// never run on the small stack at all. Returning a `String` keeps every deep value's whole
/// lifetime, destructor included, inside the thread being tested.
fn on_a_small_embedder_stack(body: impl FnOnce() -> String + Send + 'static) -> String {
    std::thread::Builder::new()
        .name("embedder-small-stack".to_string())
        .stack_size(EMBEDDER_STACK_BYTES)
        .spawn(body)
        .expect("spawning the small embedder stack")
        .join()
        .expect("the embedder thread panicked")
}

/// Summarizes a program without keeping it, so the caller reports a `String` and the AST dies
/// where it was built.
fn summarize(program: elephc::parser::ast::Program) -> String {
    format!("{} statements", program.len())
}

/// Creates a fresh, unpredictably named directory that only this process can write to.
///
/// `DirBuilder::create` fails on a path that already exists instead of reusing it the way
/// `create_dir_all` does, so a directory or symlink planted at the name ahead of time is refused
/// rather than written through; the name mixes the process id with the clock and an attempt
/// counter, and on Unix the directory is created with mode `0700`.
fn exclusive_fixture_dir(prefix: &str) -> std::path::PathBuf {
    for attempt in 0u32..64 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "{prefix}_{}_{nanos}_{attempt}",
            std::process::id()
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        match builder.create(&dir) {
            Ok(()) => return dir,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("creating the fixture directory {}: {error}", dir.display()),
        }
    }
    panic!("no free fixture directory name for {prefix}");
}

/// `$a = [[[…1…]]]` at the compiler's nesting limit.
fn deeply_nested_source() -> String {
    format!(
        "<?php\n$a = {}1{};\necho count($a);\n",
        "[".repeat(NESTING_DEPTH),
        "]".repeat(NESTING_DEPTH)
    )
}

/// Parses the fixture, which every phase below starts from.
fn parse_deeply_nested() -> elephc::parser::ast::Program {
    let source = deeply_nested_source();
    let tokens = elephc::lexer::tokenize(&source).expect("tokenize");
    elephc::parser::parse(&tokens).expect("parse")
}

/// Verifies `parser::parse` walks its own limit on a small stack, with brackets and with
/// parentheses.
///
/// A GUARD rather than a reproduction, and deliberately so: measured, the parser is the
/// shallowest of these walkers and survives 1024 levels of either shape on this stack with its
/// wrapper removed. That is consistent with the issue itself — the aborts were always in the
/// passes below. What this pins is that the entry still carries the budget, so the parser does
/// not become the outlier the day its frames grow.
#[test]
fn parsing_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let bracketed = on_a_small_embedder_stack(|| summarize(parse_deeply_nested()));
    assert_eq!(bracketed, "2 statements");
    let parenthesized = on_a_small_embedder_stack(|| {
        let source = format!(
            "<?php\n$a = {}1{};\necho $a;\n",
            "(".repeat(NESTING_DEPTH),
            ")".repeat(NESTING_DEPTH)
        );
        let tokens = elephc::lexer::tokenize(&source).expect("tokenize");
        summarize(elephc::parser::parse(&tokens).expect("parse"))
    });
    assert_eq!(parenthesized, "2 statements");
}

/// Verifies the magic-constant walker survives the same depth called on its own.
#[test]
fn magic_constant_substitution_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        summarize(elephc::magic_constants::substitute_file_and_scope_constants(
            ast,
            std::path::Path::new("embedder.php"),
        ))
    });
    assert_eq!(report, "2 statements");
}

/// Verifies the constant folder survives the same depth called on its own.
#[test]
fn constant_folding_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        summarize(elephc::optimize::fold_constants(ast))
    });
    assert_eq!(report, "2 statements");
}

/// Target-aware folding keeps both recursive folds inside the phase's stack budget.
#[test]
fn target_constant_folding_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let target = elephc::codegen_support::platform::Target::parse("linux-x86_64").unwrap();
        summarize(elephc::optimize::fold_constants_for_target(parse_deeply_nested(), target))
    });
    assert_eq!(report, "2 statements");
}

/// A nonempty autoload registry must budget its recursive reference scan on small threads.
#[test]
fn autoload_reference_scan_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let tokens = elephc::lexer::tokenize("<?php spl_autoload_register(function($name) { require $name . '.php'; });").unwrap();
        let registration = elephc::parser::parse(&tokens).unwrap();
        let (registry, _) = elephc::autoload::Registry::build(std::path::Path::new("."), registration);
        assert!(!registry.is_empty());
        let (program, included) = elephc::autoload::run_collecting_included_with_defines(
            parse_deeply_nested(), std::path::Path::new("."), &registry, &HashSet::new(),
        ).unwrap();
        assert!(included.is_empty());
        summarize(program)
    });
    assert_eq!(report, "2 statements");
}

/// Verifies the type checker survives the same depth called on its own.
#[test]
fn type_checking_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        let ast = elephc::conditional::apply(ast, &HashSet::new());
        let ast = elephc::name_resolver::resolve(ast).expect("name resolve");
        let ast = elephc::optimize::fold_constants(ast);
        match elephc::types::check(&ast) {
            Ok(_) => summarize(ast),
            Err(error) => error.message,
        }
    });
    assert_eq!(report, "2 statements");
}

/// Verifies constant propagation survives the same depth called on its own.
///
/// This is the pass the first cut of the fix missed: it runs AFTER the checker, so a fixture
/// that stopped at type checking could not reach it, and a full compile went through the
/// whole-run budget instead. Called directly, as an embedder running the optimizer would.
#[test]
fn constant_propagation_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        let ast = elephc::conditional::apply(ast, &HashSet::new());
        let ast = elephc::name_resolver::resolve(ast).expect("name resolve");
        let ast = elephc::optimize::fold_constants(ast);
        let check = elephc::types::check(&ast).expect("type check");
        summarize(elephc::optimize::propagate_constants(
            ast,
            check.mixed_storage_local_names(),
            check.buffer_read_sites.clone(),
        ))
    });
    assert_eq!(report, "2 statements");
}

/// Verifies include resolution survives the same depth called on its own.
///
/// Follow-up for #1150: the resolver runs BEFORE the checker, so every fixture above that
/// starts at `name_resolver` skips it entirely, and an embedder that resolves includes itself
/// reaches it with no wrapper of its own in between. The fixture declares no includes; what is
/// being walked is the expression tree the resolver descends on its way to finding none.
#[test]
fn include_resolution_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        match elephc::resolver::resolve(ast, std::path::Path::new(".")) {
            Ok(ast) => summarize(ast),
            Err(error) => error.message,
        }
    });
    assert_eq!(report, "2 statements");
}

/// Verifies dead-code elimination survives the same depth called on its own.
///
/// Follow-up for #1150. Like constant propagation, DCE runs after the checker, so a fixture
/// that stops at type checking cannot reach it; unlike propagation, it also rewrites the tree
/// it walks, which is the shape that grows frames.
#[test]
fn dead_code_elimination_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        let ast = elephc::conditional::apply(ast, &HashSet::new());
        let ast = elephc::name_resolver::resolve(ast).expect("name resolve");
        let ast = elephc::optimize::fold_constants(ast);
        let check = elephc::types::check(&ast).expect("type check");
        summarize(elephc::optimize::eliminate_dead_code(
            ast,
            check.local_binding_decision_spans(),
        ))
    });
    assert_eq!(report, "2 statements");
}

/// Verifies EIR lowering survives the same depth called on its own.
///
/// Follow-up for #1150, and the deepest walker of the set: lowering descends the same tree the
/// checker did and BUILDS one frame per level while it emits. An embedder that drives lowering
/// directly — to inspect EIR, or to run its own backend — gets there without the CLI driver's
/// wrapper.
#[test]
fn eir_lowering_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        let ast = elephc::conditional::apply(ast, &HashSet::new());
        let ast = elephc::name_resolver::resolve(ast).expect("name resolve");
        let ast = elephc::optimize::fold_constants(ast);
        let check = elephc::types::check(&ast).expect("type check");
        let target = elephc::codegen::platform::Target::detect_host();
        match elephc::ir_lower::lower_program(&ast, &check, target, false) {
            Ok(module) => {
                // A count would pin the synthetic-function set, which is not what this is
                // about; that lowering RETURNED at this depth is.
                assert!(!module.functions.is_empty(), "lowering produced no functions");
                "lowered".to_string()
            }
            Err(error) => format!("lowering failed: {error}"),
        }
    });
    assert_eq!(report, "lowered");
}

/// Verifies include DISCOVERY and RESOLUTION walk the nesting limit on a small stack when the
/// program really includes a file (issue #1148).
///
/// `include_resolution_survives_the_nesting_limit_on_a_small_embedder_stack` declares no
/// include, so `has_includes` answers false and the resolver returns before its declaration
/// discovery and its rewriting walk ever run. Here the deep expression sits in a program that
/// requires a file carrying the same depth, so `has_includes`, discovery and resolution all
/// descend both trees, on the budget the resolver's own entry point reserves.
#[test]
fn include_discovery_and_resolution_survive_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let dir = exclusive_fixture_dir("elephc_embedder_include");
        // The included file's deep value is a top-level statement, so it sits AT the limit
        // rather than one level past it inside a function body; the declaration beside it gives
        // discovery something to hoist.
        let library = format!(
            "<?php\nfunction deep_marker() {{ return 1; }}\n$b = {}1{};\n",
            "[".repeat(NESTING_DEPTH),
            "]".repeat(NESTING_DEPTH)
        );
        // `create_new` refuses a path that already exists, a symlink included, so the write can
        // only ever create the fixture's own file.
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join("deep_lib.php"))
            .and_then(|mut file| std::io::Write::write_all(&mut file, library.as_bytes()))
            .expect("writing the included file");
        let source = format!(
            "<?php\nrequire 'deep_lib.php';\n$a = {}1{};\necho count($a);\n",
            "[".repeat(NESTING_DEPTH),
            "]".repeat(NESTING_DEPTH)
        );
        let tokens = elephc::lexer::tokenize(&source).expect("tokenize");
        let ast = elephc::parser::parse(&tokens).expect("parse");
        let report = match elephc::resolver::resolve(ast, &dir) {
            Ok(ast) => summarize(ast),
            Err(error) => error.message,
        };
        let _ = std::fs::remove_dir_all(&dir);
        report
    });
    // The hoisted declaration block, the inlined include, and the main file's own statements.
    assert_eq!(report, "5 statements");
}

/// Verifies the post-typecheck optimizer survives the same depth when an embedder drives it the
/// way the CLI pipeline does, through `PostTypecheckOptimizer` (issue #1149).
///
/// The free functions `propagate_constants` and `eliminate_dead_code` carried the budget, but
/// the pipeline builds one `PostTypecheckOptimizer` and calls its four phases directly, and
/// neither the constructor's program-wide analyses nor those methods had a wrapper of their own.
#[test]
fn post_typecheck_optimizer_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        let ast = elephc::conditional::apply(ast, &HashSet::new());
        let ast = elephc::name_resolver::resolve(ast).expect("name resolve");
        let ast = elephc::optimize::fold_constants(ast);
        let check = elephc::types::check(&ast).expect("type check");
        let optimizer = elephc::optimize::PostTypecheckOptimizer::new_with_type_metadata(
            &ast,
            &check.functions,
            &check.classes,
            &check.interfaces,
        );
        let ast = optimizer.propagate(
            ast,
            check.mixed_storage_local_names(),
            check.buffer_read_sites.clone(),
        );
        let ast = optimizer.prune(ast, check.local_binding_decision_spans());
        let ast = optimizer.normalize(ast, check.local_binding_decision_spans());
        summarize(optimizer.eliminate_dead_code(ast, check.local_binding_decision_spans()))
    });
    assert_eq!(report, "2 statements");
}

/// Verifies the `func_get_args()` desugaring survives the same depth called on its own
/// (issue #1149): its backtrace detector and its rewriter both walk the whole program.
#[test]
fn func_args_desugaring_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        match elephc::func_args::desugar(ast) {
            Ok(ast) => summarize(ast),
            Err(error) => error.message,
        }
    });
    assert_eq!(report, "2 statements");
}

/// Verifies every stdlib prelude's usage scan survives the same depth when an embedder injects
/// the preludes itself, in the order the CLI pipeline does (issue #1149).
///
/// None of these programs uses a prelude surface, so every injector must hand the program back
/// untouched; what is being walked is the deep tree each usage scan descends to find nothing.
#[test]
fn prelude_injection_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let php_version = elephc::php_version::PhpVersion::default();
        let mut inventory = elephc::optimize::reachability::PreludeInventory::new();
        let ast = parse_deeply_nested();
        assert!(!elephc::pdo_prelude::program_uses_pdo(&ast));
        assert!(!elephc::mysqli_prelude::program_uses_mysqli(&ast));
        assert!(!elephc::object_cast_prelude::program_uses_object_cast(&ast));
        let _ = elephc::opcache_prelude::collect_preload_symbols(&ast);
        let ast = elephc::pdo_prelude::inject_if_used(ast, false, &mut inventory);
        let ast = elephc::mysqli_prelude::inject_if_used(ast, false, php_version, &mut inventory);
        let ast = elephc::tz_prelude::inject_if_used(ast, false, &mut inventory);
        let ast = elephc::list_id_prelude::inject_if_used(ast, &mut inventory);
        let ast = elephc::var_export_prelude::inject_if_used(ast, &mut inventory);
        let ast = elephc::http_build_query_prelude::inject_if_used(ast, &mut inventory);
        let ast = elephc::image_prelude::inject_if_used(ast, false, &mut inventory);
        let ast = elephc::hash_prelude::inject_if_used(ast, false, &mut inventory);
        let ast = elephc::curl_prelude::inject_if_used(ast, false, &mut inventory);
        let ast = elephc::xml_prelude::inject_if_used(ast, false, &mut inventory);
        let ast = elephc::version_prelude::inject_if_used(ast, php_version, &mut inventory);
        match elephc::object_cast_prelude::inject_if_used(ast, &mut inventory) {
            Ok(ast) => summarize(ast),
            Err(error) => error.message,
        }
    });
    assert_eq!(report, "2 statements");
}

/// Verifies the OPcache prelude's injection survives the same depth called on its own
/// (issue #1149): it probes the program for every OPcache function it could declare.
///
/// The fixture calls none of them, so the program comes back unchanged; what is being walked is
/// the deep tree each probe descends.
#[test]
fn opcache_injection_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let mut inventory = elephc::optimize::reachability::PreludeInventory::new();
        let (ast, _bake_sites) = elephc::opcache_prelude::inject_if_used(
            parse_deeply_nested(),
            elephc::php_version::PhpVersion::default(),
            false,
            None,
            &[],
            &[],
            None,
            false,
            &mut inventory,
        );
        summarize(ast)
    });
    assert_eq!(report, "2 statements");
}

/// Verifies the `--web` prelude's injection survives the same depth called on its own
/// (issue #1149): its usage scan walks the whole user program before the handler wrapper moves
/// the program's statements into the catch-all `try`.
#[test]
fn web_injection_survives_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let mut inventory = elephc::optimize::reachability::PreludeInventory::new();
        let ast = elephc::web_prelude::inject_if_web(
            parse_deeply_nested(),
            true,
            elephc::php_version::PhpVersion::default(),
            &[],
            &mut inventory,
            None,
        );
        let wrapped = matches!(
            ast.last().map(|stmt| &stmt.kind),
            Some(elephc::parser::ast::StmtKind::Try { .. })
        );
        format!("handler wrapper last: {wrapped}")
    });
    assert_eq!(report, "handler wrapper last: true");
}

/// Verifies the PHP-profile scans survive the same depth called on their own (issue #1149).
///
/// The pipeline runs both over the user program between include resolution and prelude
/// injection: the floor check rejects a profile the source could not have run under, and the
/// report names the constructs the profile governs. Neither finds anything in this fixture; what
/// is being walked is the deep tree each scan descends.
#[test]
fn php_profile_scans_survive_the_nesting_limit_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let ast = parse_deeply_nested();
        let profile = elephc::php_version::PhpVersion::default();
        elephc::php_profile::report(&ast, false, profile, elephc::php_profile::Provenance::Flag);
        match elephc::php_profile::floor_violation(&ast, profile) {
            Some(error) => error.message,
            None => summarize(ast),
        }
    });
    assert_eq!(report, "2 statements");
}

/// Verifies source PAST the cap is still diagnosed rather than walked.
///
/// The fixtures above all sit AT `MAX_COMPILER_NESTING`, which is the depth the budget is
/// sized for. One level further has to be refused by the parser's own guard, on the small
/// stack as anywhere else — an embedder that lost the diagnostic would get an abort instead of
/// an error, which is the failure mode #686 was filed for.
#[test]
fn over_cap_nesting_is_diagnosed_on_a_small_embedder_stack() {
    let report = on_a_small_embedder_stack(|| {
        let source = format!(
            "<?php\n$a = {}1{};\necho count($a);\n",
            "[".repeat(NESTING_DEPTH + 1),
            "]".repeat(NESTING_DEPTH + 1)
        );
        let tokens = elephc::lexer::tokenize(&source).expect("tokenize");
        match elephc::parser::parse(&tokens) {
            Ok(program) => summarize(program),
            Err(error) => error.message,
        }
    });
    assert_eq!(report, "maximum compiler nesting depth exceeded");
}
