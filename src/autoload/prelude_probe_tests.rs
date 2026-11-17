//! Purpose:
//! Protects the identity allocation boundary between discarded probes and real autoloads.
//!
//! Called from:
//! - `autoload::prelude_probe` unit tests.
//!
//! Key details:
//! - Source and anonymous-class IDs are restored, while global include temporaries stay fresh.
//! - The fixture removes its files even when an assertion fails.

use std::fs;
use std::path::PathBuf;

use crate::parser::ast::{Program, StmtKind};
use crate::span::Span;

use super::{autoloaded_prelude_groups, Registry};

/// Owns the small Composer project used to exercise a real probe and autoload pass.
struct ProbeProject(PathBuf);

impl ProbeProject {
    /// Creates an autoloaded class with a value include and an anonymous class.
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "elephc_probe_counter_{}_{:?}", std::process::id(), std::thread::current().id(),
        ));
        fs::create_dir(&path).unwrap();
        let project = Self(path);
        fs::create_dir(project.0.join("src")).unwrap();
        fs::write(project.0.join("composer.json"),
            r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#).unwrap();
        fs::write(project.0.join("src/value.php"), "<?php return 7;").unwrap();
        fs::write(project.0.join("src/Probe.php"), r#"<?php
namespace App;
$value = require __DIR__ . '/value.php';
$object = new class {};
class Probe {
    public ?\PDO $pdo = null;
    public function inspect() { eval('new class {};'); }
}
"#).unwrap();
        project
    }
}

impl Drop for ProbeProject {
    /// Cleans up the fixture on both success and unwinding.
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

/// Parses the entry or seed program without allocating a physical-file identity.
fn parse(source: &str) -> Program {
    crate::parser::parse(&crate::lexer::tokenize(source).unwrap()).unwrap()
}

/// Reads the resolver's hidden include temporary from the flattened top-level statements.
fn include_temporary_id(program: &[crate::parser::ast::Stmt]) -> Option<usize> {
    program.iter().find_map(|stmt| match &stmt.kind {
        StmtKind::Assign { name, .. } => name.strip_prefix("__elephc_inc_")?.parse().ok(),
        StmtKind::NamespaceBlock { body, .. } | StmtKind::Synthetic(body) => {
            include_temporary_id(body)
        }
        _ => None,
    })
}

/// A probe hands its parsed files to the real walk, which then names them as the probe did, and
/// never gives out one of their identities again.
#[test]
fn probe_hands_its_files_to_the_real_walk_without_recycling_identities() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    let seed = crate::resolver::resolve_collecting_includes_continuing(
        parse("<?php $seed = require 'src/value.php';"), &project.0, &defines,
    ).unwrap().0;
    let previous_temporary = include_temporary_id(&seed).expect("seed include temporary");
    let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
    assert!(!registry.is_empty());
    let anonymous_class = crate::parser::anonymous_class_counter();
    let checkpoint = Span::counters_checkpoint();
    let source_id = Span::fresh_source_id();
    let synthetic_source_id = Span::fresh_synthetic_source_id();
    Span::restore_counters(checkpoint);

    let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);

    assert!(groups.contains("pdo"));
    assert!(crate::parser::anonymous_class_counter() > anonymous_class);
    assert!(Span::fresh_source_id() > source_id);
    assert!(Span::fresh_synthetic_source_id() >= synthetic_source_id);
    let parses = crate::autoload::AUTOLOAD_PARSES.with(std::cell::Cell::get);
    let resolved = crate::name_resolver::resolve(program).unwrap();
    let (loaded, _) = crate::autoload::run_collecting_included_with_defines(
        resolved, &project.0, &registry, &defines,
    ).unwrap();
    assert_eq!(
        crate::autoload::AUTOLOAD_PARSES.with(std::cell::Cell::get),
        parses,
        "the real walk parsed a file the probe already had",
    );
    let actual_temporary = include_temporary_id(&loaded).expect("real autoload include temporary");
    assert!(actual_temporary > previous_temporary, "an earlier include temporary was reused");
    let expected_anonymous = format!("class@anonymous#{anonymous_class}");
    assert!(loaded.iter().any(|stmt| matches!(&stmt.kind,
        StmtKind::ClassDecl { name, .. } if name.ends_with(&expected_anonymous))),
        "the real parse must retain its anonymous-class name");
}

/// Tokenizer use hidden in Composer-loaded methods must trigger prelude injection.
#[test]
fn probe_detects_tokenizer_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    for body in [
        r"return \PhpToken::tokenize($source);",
        r"return \token_get_all($source);",
    ] {
        fs::write(project.0.join("src/Probe.php"), format!(
            "<?php namespace App; class Probe {{ public function scan(string $source): array {{ {body} }} }}",
        )).unwrap();
        let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
        let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
        assert!(groups.contains("tokenizer"), "autoloaded body was missed: {body}");
    }
}

/// A `gmp_*` call only an autoloaded class makes activates its prelude
/// (league/uri's `IPv4\GMPCalculator` lowered to a call of the missing `__elephc_gmp_init`).
#[test]
fn probe_detects_gmp_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    for (body, group) in [
        (r"return \gmp_strval(\gmp_init($source, 16));", "gmp"),
    ] {
        fs::write(project.0.join("src/Probe.php"), format!(
            "<?php namespace App; class Probe {{ public function scan(string $source): string {{ {body} }} }}",
        )).unwrap();
        let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
        let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
        assert!(groups.contains(group), "autoloaded body was missed: {body}");
    }
}

/// A `session_set_save_handler()` call only an autoloaded class makes keeps the callable
/// save-handler adapter (the CLI session gate runs before the autoload pass).
#[test]
fn probe_detects_session_save_handler_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    fs::write(project.0.join("src/Probe.php"), concat!(
        "<?php namespace App; class Probe { public function boot(object $h): bool { ",
        "return \\session_set_save_handler($h); } }",
    )).unwrap();
    let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
    let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
    assert!(groups.contains("session_handler"));
}

/// Fileinfo classes and procedural calls hidden in Composer files both activate the prelude.
#[test]
fn probe_detects_fileinfo_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    for body in [
        r"return new \finfo();",
        r"return \finfo_open();",
        r"return \mime_content_type('input.txt');",
    ] {
        fs::write(project.0.join("src/Probe.php"), format!(
            "<?php namespace App; class Probe {{ public function identify(): mixed {{ {body} }} }}",
        )).unwrap();
        let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
        let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
        assert!(groups.contains("fileinfo"), "autoloaded body was missed: {body}");
    }
}

/// An autoloaded class that can register an exception handler (by name, through a callable it
/// cannot name, or `eval`) keeps the --web catch-all's handler call; one that cannot does not.
#[test]
fn probe_detects_exception_handler_registration_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    for (body, registers) in [
        (r"return set_exception_handler([$this, 'read']);", true),
        (r"return \set_exception_handler(null);", true),
        (r"return $source();", true),
        (r"return eval($source);", true),
        (r"return strlen($source);", false),
        (r"return str_repeat($source, 2);", false),
    ] {
        fs::write(project.0.join("src/Probe.php"), format!(
            "<?php namespace App; class Probe {{ public function read(string $source) {{ {body} }} }}",
        )).unwrap();
        let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
        let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
        assert_eq!(groups.contains(crate::web_prelude::EXCEPTION_HANDLER_GROUP), registers, "{body}");
    }
}

/// `$_REQUEST` observers in Composer-loaded classes retain request population.
#[test]
fn probe_detects_web_request_observers_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    for (body, observes) in [
        (r"return $_REQUEST['q'] ?? 'MISSING';", true),
        (r"return $GLOBALS['_REQUEST']['q'] ?? 'MISSING';", true),
        (r"return $_ENV['PATH'] ?? 'MISSING';", false),
        (r"return $_GET['q'] ?? 'MISSING';", false),
    ] {
        fs::write(project.0.join("src/Probe.php"), format!(
            "<?php namespace App; class Probe {{ public function read() {{ {body} }} }}",
        )).unwrap();
        let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
        let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
        assert_eq!(groups.contains("web_request"), observes, "{body}");
    }
}

/// Random, BcMath, GMP, and DateTimeZone::listIdentifiers in an autoloaded method
/// are the only uses that must turn those preludes on.
#[test]
fn probe_detects_random_bcmath_gmp_and_list_identifiers() {
    let project = ProbeProject::new();
    let cases = [
        (
            "random",
            r"return (new \Random\Randomizer(new \Random\Engine\Mt19937(42)))->getInt(1, 6);",
        ),
        ("random", r"\mt_srand(42); return \mt_rand(1, 100);"),
        ("random", r"return \array_rand([10, 20]);"),
        ("random", r"mt_srand(42); return mt_rand(1, 100);"),
        (
            "bcmath",
            r#"return (string) (new \BcMath\Number("1.5"))->add("2.25");"#,
        ),
        (
            "gmp",
            r#"return \gmp_strval(\gmp_add("1", "1"));"#,
        ),
        (
            "list_id",
            r"return \count(\DateTimeZone::listIdentifiers(\DateTimeZone::EUROPE)) > 10 ? 1 : 0;",
        ),
        (
            "list_id",
            r"return \count(\timezone_identifiers_list());",
        ),
    ];
    for (group, body) in cases {
        let groups = groups_for_method_body(&project, body);
        assert!(
            groups.contains(group),
            "autoloaded body was missed for {group}: {body}"
        );
    }
    let untouched = groups_for_method_body(&project, "return 1;");
    for group in ["random", "bcmath", "gmp", "list_id"] {
        assert!(
            !untouched.contains(group),
            "{group} was reported for an unrelated autoloaded method"
        );
    }
}

/// Writes one autoloaded method and returns the groups the probe reports for it.
fn groups_for_method_body(project: &ProbeProject, body: &str) -> crate::fast_hash::FastSet<&'static str> {
    fs::write(
        project.0.join("src/Probe.php"),
        format!(
            "<?php namespace App; class Probe {{ public function run() {{ {body} }} }}"
        ),
    )
    .unwrap();
    let defines = crate::fast_hash::FastSet::default();
    let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
    autoloaded_prelude_groups(&program, &project.0, &registry, &defines)
}

/// Autoloaded observers retain web environment population while getenv remains independent.
#[test]
fn probe_detects_web_environment_observers_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    for (body, observes) in [
        (r"return $_ENV['PATH'];", true),
        (r"return get_defined_vars();", true),
        (r"return eval($source);", true),
        (r"return getenv('PATH');", false),
        (r"return $source();", false),
    ] {
        fs::write(project.0.join("src/Probe.php"), format!(
            "<?php namespace App; class Probe {{ public function read(string $source) {{ {body} }} }}",
        )).unwrap();
        let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
        let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
        assert_eq!(groups.contains("web_environment"), observes, "{body}");
    }
}

/// WeakMap and WeakReference named only by Composer-loaded classes keep their core prelude, and
/// so does introspection or a dynamic `new` there, which may reach either class.
#[test]
fn probe_detects_weak_references_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = crate::fast_hash::FastSet::default();
    for (body, group) in [
        (r"public function __construct(private \WeakMap $seen) {}", "weak_map"),
        (r"public function make(): object { return new \WeakMap(); }", "weak_map"),
        (r"private ?\WeakReference $last = null;", "weak_reference"),
        (r"public function all(): array { return \get_declared_classes(); }", "weak_map"),
        (r"public function all(): array { return \get_declared_classes(); }", "weak_reference"),
        (r"public function make(string $c): object { return new $c(); }", "weak_map"),
        (r"public function make(string $c): object { return new $c(); }", "weak_reference"),
    ] {
        fs::write(project.0.join("src/Probe.php"), format!(
            "<?php namespace App; class Probe {{ {body} }}",
        )).unwrap();
        let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
        let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
        assert!(groups.contains(group), "autoloaded {group} use was missed: {body}");
    }
}
