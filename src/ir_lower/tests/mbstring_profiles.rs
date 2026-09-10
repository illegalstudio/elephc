//! Purpose:
//! Verifies PHP caller strictness survives lowering of mbstring runtime instructions.
//!
//! Called from:
//! - The focused AST-to-EIR unit test harness.
//!
//! Key details:
//! - Mixed physical-file profiles must survive includes and nested function/closure lowering.
//! - Strict parameter typing remains independent of the strict-PHP extension visibility flag.
//! - Textual EIR exposes the profile that the runtime argument adapter must consume.

use super::*;
use crate::ir::{Immediate, RuntimeCallTarget};

/// Collects the explicit profile carried by every mbstring call in the lowered module.
fn profiles(module: &crate::ir::Module) -> Vec<(String, bool)> {
    let mut result = Vec::new();
    for function in module.functions.iter().chain(&module.class_methods).chain(&module.closures) {
        for instruction in &function.instructions {
            match instruction.immediate {
                Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction { target, strict_types, .. }))
                    if target.uses_mbstring_runtime() =>
                    result.push((function.name.clone(), strict_types.expect("direct lowering knows its source profile"))),
                Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))) if target.uses_mbstring_runtime() =>
                    panic!("mbstring call lost its physical source profile in {}", function.name),
                _ => {},
            }
        }
    }
    result
}

/// Retains weak/strict source modes in ordinary, function, and arrow-function calls.
#[test]
fn mbstring_lowering_retains_caller_strictness() {
    for strict in [false, true] {
        let source = format!(r#"<?php
{}
function measured(string $text): int {{ return mb_strlen($text); }}
$measure = fn(string $text): int => mb_strwidth($text);
echo measured("猫"), $measure("猫"), mb_strlen("猫");
"#, if strict { "declare(strict_types=1);" } else { "" });
        let module = lower_source(&source);
        let profiles = profiles(&module);
        assert!(profiles.len() >= 3, "{profiles:?}");
        assert!(profiles.iter().all(|(_, actual)| *actual == strict), "{profiles:?}");
        assert!(print_module(&module).contains(&format!("strict_types={}", u8::from(strict))));
    }
    assert!(!crate::source::current_strict_types(), "statement lowering restores its caller's source profile");
}

/// Keeps included weak and strict function bodies distinct from the entry file's strictness.
#[test]
fn mbstring_lowering_retains_included_file_strictness() {
    let directory = std::env::temp_dir().join(format!("elephc-mbstring-profiles-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let weak = directory.join("weak.php");
    let strict = directory.join("strict.php");
    std::fs::write(&weak, "<?php function weak_measure(string $s): int { return mb_strlen($s); }").unwrap();
    std::fs::write(&strict, "<?php declare(strict_types=1); function strict_measure(string $s): int { return mb_strlen($s); }").unwrap();
    let source = "<?php declare(strict_types=1); require 'weak.php'; require 'strict.php'; echo weak_measure('x'), strict_measure('y'), mb_strlen('z');";
    let module = lower_source_at(source, &directory.join("main.php"), &directory);
    std::fs::remove_dir_all(&directory).unwrap();
    let profiles = profiles(&module);
    assert!(profiles.iter().any(|(name, mode)| name.ends_with("weak_measure") && !mode), "{profiles:?}");
    assert!(profiles.iter().any(|(name, mode)| name.ends_with("strict_measure") && *mode), "{profiles:?}");
    assert_eq!(profiles.iter().filter(|(_, mode)| !mode).count(), 1, "{profiles:?}");
}

/// Leaves mixed inputs intact across positional, named, and associative-spread parameter planning.
#[test]
fn mbstring_lowering_preserves_concrete_argument_values() {
    let module = lower_source(r#"<?php
function positional_measure(mixed $text): int { return mb_strlen($text); }
function named_measure(mixed $text): int { return mb_strlen(encoding: "UTF-8", string: $text); }
function spread_measure(mixed $text): int { return mb_strlen(...["string" => $text]); }
echo positional_measure(123), named_measure(123), spread_measure(123);
"#);
    let text = print_module(&module);
    assert!(!text.contains(" = cast "), "parameter coercion escaped the shared runtime planner: {text}");
    assert_eq!(profiles(&module).len(), 3, "{text}");
    assert!(crate::ir::RuntimeFnId::MbStrlen.effects().contains(crate::ir::Effects::WRITES_GLOBAL));
}

/// Derives optional runtime requirements from finite callable targets while excluding unrelated calls.
#[test]
fn mbstring_lowering_retains_callable_requirements() {
    for source in [
        "<?php $list = mb_list_encodings(...); echo count($list());",
        "<?php $name = $argc > 0 ? 'mb_list_encodings' : 'mb_encoding_aliases'; echo count(call_user_func($name));",
    ] {
        assert!(lower_source(source).required_runtime_features.mbstring, "{source}");
    }
    let module = lower_source("<?php $name = $argc > 0 ? 'trim' : 'strtoupper'; echo call_user_func($name, 'hi');");
    assert!(!module.required_runtime_features.mbstring);
}

/// Enables managed Oniguruma for direct and reachable matching callables without enabling PCRE2.
#[test]
fn mbregex_lowering_requires_managed_provider() {
    use crate::codegen::{link_requirements_for_runtime_features, LinkRequirement};
    for source in [
        "<?php echo mb_ereg_match('a', 'abc');",
        "<?php $match = mb_ereg_match(...); echo $match('a', 'abc');",
        "<?php $name = $argc > 0 ? 'mb_ereg_match' : 'mb_check_encoding'; echo call_user_func($name, 'a', 'abc');",
    ] {
        let features = lower_source(source).required_runtime_features;
        assert!(features.mbstring && features.mbregex && !features.regex && !features.mbstring_mime,
            "{source}: {features:?}");
        let requirements = link_requirements_for_runtime_features(features);
        assert!(requirements.contains(&LinkRequirement::NativePackage("oniguruma")));
        assert!(requirements.contains(&LinkRequirement::Bridge("elephc_mbstring")));
        assert!(!requirements.contains(&LinkRequirement::NativePackage("pcre2")));
    }
    for source in ["<?php echo mb_strlen('hello');", "<?php echo mb_regex_encoding(), mb_regex_set_options();"] {
        let features = lower_source(source).required_runtime_features;
        assert!(features.mbstring && !features.mbregex && !features.regex && !features.mbstring_mime,
            "{features:?}");
        assert!(!link_requirements_for_runtime_features(features).contains(&LinkRequirement::NativePackage("oniguruma")));
    }
}

/// Selects the MIME provider for direct and callable output handlers without broad mbstring coupling.
#[test]
fn mbstring_output_handler_lowering_requires_mime_provider() {
    for source in [
        "<?php echo mb_output_handler('text', 9);",
        "<?php $handler = mb_output_handler(...); echo $handler('text', 9);",
    ] {
        let features = lower_source(source).required_runtime_features;
        assert!(features.mbstring && features.mbstring_mime, "{source}: {features:?}");
    }
    assert!(!lower_source("<?php echo mb_strlen('text');")
        .required_runtime_features.mbstring_mime);
}

/// Assembles actual capture and query calls after frontend lowering on every supported target.
#[test]
#[ignore = "requires clang with ELF AArch64/x86_64 and Apple AArch64 assembler support"]
fn mbstring_capture_calls_assemble_on_all_supported_targets() {
    let source = r#"<?php
function captures(mixed $matches): void {
    $alias =& $matches;
    var_dump(mb_ereg("(?<key>a)", "a", $alias));
    echo $matches["key"], "\n";
    var_dump(mb_eregi(matches: $alias, string: "A", pattern: "a"));
}
captures(null);
var_dump(mb_ereg("a", "a"));
mb_parse_str("key=value&list[]=a&list[]=b", $query);
echo $query["list"][1], "\n";
"#;
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let directory = std::env::temp_dir().join(format!("mbstring-capture-calls-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    for (name, triple) in [
        ("linux-x86_64", "x86_64-linux-gnu"),
        ("linux-aarch64", "aarch64-linux-gnu"),
        ("macos-aarch64", "arm64-apple-macos11"),
        ("ios-arm64", "arm64-apple-ios13"),
        ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
    ] {
        let mut module = lower_source_at_for_target(source, Path::new("main.php"), Path::new("."), Target::parse(name).unwrap());
        crate::ir_passes::optimize_module(&mut module);
        crate::ir::validate_module(&module).expect("optimized capture IR must remain valid");
        let assembly = crate::codegen::generate_user_asm_from_ir(&module, false, true).unwrap();
        let input = directory.join(format!("{name}.s"));
        std::fs::write(&input, assembly).unwrap();
        let output = std::process::Command::new("clang").args(["-target", triple, "-c"]).arg(&input)
            .arg("-o").arg(directory.join(format!("{name}.o"))).output().expect("clang is required for this explicit target check");
        assert!(output.status.success(), "{name}: {}\n{}", input.display(), String::from_utf8_lossy(&output.stderr));
    }
    std::fs::remove_dir_all(directory).unwrap();
}
