//! Purpose:
//! Keeps DOM XML loading available after implicit PHP variable rebinding.
//!
//! Called from:
//! - Codegen CI with the managed libxml2 package.
//!
//! Key details:
//! - Each program independently requires loader detection, with no SimpleXML call to mask it.

use crate::support::*;

/// Production CLI injection type-checks the whole XML surface without forced harness preludes.
#[test]
fn test_simplexml_production_cli_prelude_typechecks() {
    let dir = make_cli_test_dir("elephc_xml_prelude_check");
    for (name, source, force) in [
        ("parser", "<?php $parser = xml_parser_create();", false),
        ("writer", "<?php $writer = new XMLWriter();", false),
        ("simplexml", "<?php $xml = simplexml_load_string('<r/>');", false),
        ("dom", "<?php $doc = new DOMDocument(); $doc->loadXML('<r/>');", false),
        ("forced", "<?php echo 1;", true),
    ] {
        let path = dir.join(format!("{name}.php"));
        fs::write(&path, source).unwrap();
        let mut command = elephc_cli_command(&dir);
        command.arg("--check");
        if force { command.arg("--with-xml"); }
        let output = command.arg(&path).output().expect("run production CLI frontend");
        assert!(output.status.success(), "{name}: {}", String::from_utf8_lossy(&output.stderr));
        assert!(!path.with_extension("s").exists(), "--check must stop before codegen");
    }
    fs::remove_dir_all(dir).unwrap();
}

/// List assignment invalidates a previous unrelated receiver before the DOM loader call.
#[test]
fn test_simplexml_dom_loader_after_list_rebinding() {
    if skip_without_xml_native("test_simplexml_dom_loader_after_list_rebinding") { return; }
    let out = compile_and_run(r#"<?php
class Config { public function loadXML(string $source): bool { return false; } }
$doc = new Config();
[$doc] = [new DOMDocument()];
var_dump($doc->loadXML('<r/>'));
"#);
    assert_eq!(out, "bool(true)\n");
}

/// Foreach destructuring resets receiver facts both inside the loop and on its exit.
#[test]
fn test_simplexml_dom_loader_after_foreach_list_rebinding() {
    if skip_without_xml_native("test_simplexml_dom_loader_after_foreach_list_rebinding") { return; }
    let out = compile_and_run(r#"<?php
class Config { public function loadXML(string $source): bool { return false; } }
$doc = new Config();
foreach ([[new DOMDocument()]] as [$doc]) {}
var_dump($doc->loadXML('<r/>'));
"#);
    assert_eq!(out, "bool(true)\n");
}

/// A static declaration restores its retained DOM object over an earlier ordinary local.
#[test]
fn test_simplexml_dom_loader_after_static_rebinding() {
    if skip_without_xml_native("test_simplexml_dom_loader_after_static_rebinding") { return; }
    let out = compile_and_run(r#"<?php
class Config { public function loadXML(string $source): bool { return false; } }
function loadStaticDocument(bool $seed): void {
    $doc = new Config();
    $alias =& $doc;
    static $doc = null;
    $alias = new Config();
    if (!$seed) { var_dump($doc->loadXML('<r/>')); }
    $doc = new DOMDocument();
}
loadStaticDocument(true);
loadStaticDocument(false);
"#);
    assert_eq!(out, "bool(true)\n");
}
