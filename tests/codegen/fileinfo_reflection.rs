//! Purpose:
//! Checks profile-aware fileinfo deprecation metadata through AOT and dynamic eval reflection.
//!
//! Called from:
//! - The codegen integration harness on supported executable targets.
//!
//! Key details:
//! - Literal reflection targets activate fileinfo without a direct function or class reference.
//! - Each eval bundle compiles related owner, attribute and declaring-function cases together.

use crate::support::*;
use elephc::php_version::PhpVersion;

/// Reflection-only references activate declarations and honor the selected deprecation profile.
#[test]
fn test_fileinfo_reflection_only_deprecation_profiles() {
    if fileinfo_native::skip("test_fileinfo_reflection_only_deprecation_profiles") { return; }
    let source = r#"<?php
$ref = new ReflectionFunction("finfo_close");
echo $ref->isDeprecated() ? "deprecated" : "current";
"#;
    assert_eq!(compile_and_run_with_php_version(source, PhpVersion::Php84), "current");
    assert_eq!(compile_and_run_with_php_version(source, PhpVersion::Php85), "deprecated");
}

/// Compiles one profile's AOT, native eval and parameter-owner reflection cases together.
fn fileinfo_eval_reflection_output(version: PhpVersion) -> String {
    compile_and_run_with_php_version(r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
echo "aot:", (new ReflectionFunction("finfo_close"))->isDeprecated() ? "D" : "-", ";";
$code = $argc > 0 ? '$ref = new ReflectionFunction("finfo_close"); echo "eval:", $ref->isDeprecated() ? "D" : "-"; $attrs = $ref->getAttributes(Deprecated::class); echo "|", count($attrs); if (count($attrs) > 0) { $args = $attrs[0]->getArguments(); echo "|", $args["since"], "|", $args["message"]; } echo ";param:", (new ReflectionParameter("finfo_close", "finfo"))->getDeclaringFunction()->isDeprecated() ? "D" : "-";' : '';
eval($code);
"#, version)
}

/// PHP 8.5 retains Deprecated and its named arguments through native eval registration.
#[test]
fn test_fileinfo_eval_deprecation_php85_bundle() {
    if fileinfo_native::skip("test_fileinfo_eval_deprecation_php85_bundle") { return; }
    assert_eq!(fileinfo_eval_reflection_output(PhpVersion::Php85),
        "aot:D;eval:D|1|8.5|as finfo objects are freed automatically;param:D");
}

/// PHP 8.4 retains no deprecation attribute or flag on native fileinfo functions.
#[test]
fn test_fileinfo_eval_deprecation_php84_bundle() {
    if fileinfo_native::skip("test_fileinfo_eval_deprecation_php84_bundle") { return; }
    assert_eq!(fileinfo_eval_reflection_output(PhpVersion::Php84), "aot:-;eval:-|0;param:-");
}
