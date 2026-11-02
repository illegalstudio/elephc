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

/// A probe preserves real-file and anonymous-class naming without recycling its include temporary.
#[test]
fn probe_restores_local_identities_but_keeps_include_temporaries_monotonic() {
    let project = ProbeProject::new();
    let defines = std::collections::HashSet::new();
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
    assert_eq!(crate::parser::anonymous_class_counter(), anonymous_class);
    assert_eq!(Span::fresh_source_id(), source_id);
    assert_eq!(Span::fresh_synthetic_source_id(), synthetic_source_id);
    let resolved = crate::name_resolver::resolve(program).unwrap();
    let (loaded, _) = crate::autoload::run_collecting_included_with_defines(
        resolved, &project.0, &registry, &defines,
    ).unwrap();
    let actual_temporary = include_temporary_id(&loaded).expect("real autoload include temporary");
    assert!(actual_temporary > previous_temporary + 1,
        "the probe's discarded include temporary must not be reused");
    let expected_anonymous = format!("class@anonymous#{anonymous_class}");
    assert!(loaded.iter().any(|stmt| matches!(&stmt.kind,
        StmtKind::ClassDecl { name, .. } if name.ends_with(&expected_anonymous))),
        "the real parse must retain its anonymous-class name");
}

/// Tokenizer use hidden in Composer-loaded methods must trigger prelude injection.
#[test]
fn probe_detects_tokenizer_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = std::collections::HashSet::new();
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

/// Fileinfo classes and procedural calls hidden in Composer files both activate the prelude.
#[test]
fn probe_detects_fileinfo_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = std::collections::HashSet::new();
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

/// Autoloaded observers retain web environment population while getenv remains independent.
#[test]
fn probe_detects_web_environment_observers_in_autoloaded_classes() {
    let project = ProbeProject::new();
    let defines = std::collections::HashSet::new();
    for (body, observes) in [
        (r"return $_ENV['PATH'];", true),
        (r"return get_defined_vars();", true),
        (r"return eval($source);", true),
        (r"return getenv('PATH');", false),
    ] {
        fs::write(project.0.join("src/Probe.php"), format!(
            "<?php namespace App; class Probe {{ public function read(string $source) {{ {body} }} }}",
        )).unwrap();
        let (registry, program) = Registry::build(&project.0, parse("<?php new App\\Probe();"));
        let groups = autoloaded_prelude_groups(&program, &project.0, &registry, &defines);
        assert_eq!(groups.contains("web_environment"), observes, "{body}");
    }
}
