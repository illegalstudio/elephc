//! Purpose:
//! Checks first-inclusion order, stable main-file seeding, and the shared
//! get_included_files/get_required_files inventory in Magician.
//!
//! Called from:
//! - The interpreter unit-test module.
//!
//! Key details:
//! - Actual nested include fixtures exercise temporary call-site changes.
//! - The ordered inventory must not alter the existing once-deduplication set.

use super::super::*;
use super::support::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Owns one exclusively created directory so parallel fixtures never share files.
struct IncludeFixture(PathBuf);

impl IncludeFixture {
    /// Allocates a fresh directory without deleting a preexisting path.
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "elephc-magician-inventory-{}-{sequence}",
                std::process::id(),
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path.canonicalize().expect("canonical fixture directory")),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create include fixture: {error}"),
            }
        }
    }

    /// Writes an include body only inside this fixture's owned directory.
    fn write(&self, name: &str, source: &str) {
        std::fs::write(self.0.join(name), source).expect("write include fixture");
    }
}

impl Drop for IncludeFixture {
    /// Removes only the exact directory exclusively created by this fixture.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Executes a fragment with a stable main path and returns its observable output.
fn inventory_output(context: &mut ElephcEvalContext, source: &str) -> String {
    let program = parse_fragment(source.as_bytes()).expect("parse inventory fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    let result = execute_program_with_context(context, &program, &mut scope, &mut values)
        .expect("execute inventory fragment");
    assert_eq!(values.get(result), FakeValue::Bool(true));
    values.output
}

/// Nonlexical first loads and the required-files alias use exactly one ordered history.
#[test]
fn execute_program_included_files_keep_first_inclusion_order() {
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/main.php", "/tmp", 1);
    context.mark_included_file("/tmp/z.php");
    context.mark_included_file("/tmp/a.php");
    context.mark_included_file("/tmp/z.php");
    let source = r#"
$included = get_included_files();
$required = get_required_files();
echo count($included) . ":" . $included[0] . ":" . $included[1] . ":" . $included[2] . ":";
echo count($required) === count($included)
    && $required[0] === $included[0]
    && $required[1] === $included[1]
    && $required[2] === $included[2]
    ? "alias" : "bad";
return true;
"#;
    assert_eq!(inventory_output(&mut context, source), "3:/tmp/main.php:/tmp/z.php:/tmp/a.php:alias");
}

/// Repeated regular and once includes execute correctly while recording each path once.
#[test]
fn execute_program_repeated_includes_record_one_ordered_entry() {
    let fixture = IncludeFixture::new();
    fixture.write("piece.php", "<?php echo 'P';");
    let mut context = ElephcEvalContext::new();
    context.set_call_site(
        fixture.0.join("main.php").to_string_lossy().into_owned(),
        fixture.0.to_string_lossy().into_owned(),
        1,
    );
    let source = r#"
include 'piece.php'; include_once 'piece.php'; require_once 'piece.php'; require 'piece.php';
$files = get_included_files();
echo count($files) . ':';
echo basename($files[0]) . ':' . basename($files[1]);
return true;
"#;
    assert_eq!(inventory_output(&mut context, source), "PP2:main.php:piece.php");
}

/// Nested queries retain main and parent order before and after call-site restoration.
#[test]
fn execute_program_nested_include_inventory_keeps_main_first() {
    let fixture = IncludeFixture::new();
    fixture.write("z.php", "<?php include 'a.php';");
    fixture.write("a.php", r#"<?php
$files = get_included_files();
echo basename($files[0]) . ',' . basename($files[1]) . ',' . basename($files[2]) . '|';
"#);
    let mut context = ElephcEvalContext::new();
    let main = fixture.0.join("main.php").to_string_lossy().into_owned();
    context.set_call_site(main.clone(), fixture.0.to_string_lossy().into_owned(), 7);
    let source = r#"
include 'z.php';
$files = get_required_files();
echo basename($files[0]) . ',' . basename($files[1]) . ',' . basename($files[2]);
return true;
"#;
    assert_eq!(
        inventory_output(&mut context, source),
        "main.php,z.php,a.php|main.php,z.php,a.php",
    );
    assert_eq!(context.call_site().0, main);
}

/// Delayed main seeding is independent of whether the include list is already populated.
#[test]
fn include_inventory_seeds_main_once_after_earlier_entries() {
    for mut context in [ElephcEvalContext::new(), ElephcEvalContext::for_abi_version(0)] {
        context.set_call_site("", "", 0);
        context.mark_included_file("/tmp/z.php");
        context.mark_included_file("/tmp/main.php");
        context.mark_included_file("/tmp/a.php");
        context.set_call_site("/tmp/main.php", "/tmp", 1);
        context.set_call_site("/tmp/a.php", "/tmp", 1);
        context.set_call_site("/tmp/main.php", "/tmp", 2);
        context.mark_included_file("/tmp/main.php");
        assert_eq!(context.included_file_names(), ["/tmp/main.php", "/tmp/z.php", "/tmp/a.php"]);
        assert!(context.has_included_file("/tmp/main.php"));
    }
}

/// A seeded main is listed once without changing the preexisting once-set contract.
#[test]
fn include_inventory_main_seed_does_not_replace_once_state() {
    let mut context = ElephcEvalContext::new();
    context.set_call_site("/tmp/main.php", "/tmp", 1);
    assert!(!context.has_included_file("/tmp/main.php"));
    context.mark_included_file("/tmp/main.php");
    assert!(context.has_included_file("/tmp/main.php"));
    assert_eq!(context.included_file_names(), ["/tmp/main.php"]);
}

/// Standalone eval without a source file starts with two equal empty inventories.
#[test]
fn execute_program_without_call_site_reports_empty_include_inventory() {
    let mut context = ElephcEvalContext::new();
    assert_eq!(
        inventory_output(&mut context, "echo count(get_included_files()) . ':' . count(get_required_files()); return true;"),
        "0:0",
    );
}
