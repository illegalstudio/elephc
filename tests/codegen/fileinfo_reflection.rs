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

/// Reflection reports PHP's declared fileinfo parameter types, even though the prelude's
/// runtime slots are widened to `mixed` for coercion and argument TypeErrors.
#[test]
fn test_fileinfo_reflection_parameter_signature_bundle() {
    if fileinfo_native::skip("test_fileinfo_reflection_parameter_signature_bundle") { return; }
    let out = compile_and_run(
        r#"<?php
function show(ReflectionParameter $p): string {
    $type = $p->hasType() ? (string) $p->getType() : "<none>";
    if ($p->isDefaultValueAvailable()) {
        return $p->getName() . ":" . $type . "=" . var_export($p->getDefaultValue(), true);
    }
    return $p->getName() . ":" . $type;
}
$r = new ReflectionFunction("finfo_open");
echo "open(", (string) $r->getReturnType(), "):";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$r = new ReflectionFunction("finfo_close");
echo "close:";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$r = new ReflectionFunction("finfo_set_flags");
echo "set_flags:";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$r = new ReflectionFunction("finfo_buffer");
echo "buffer(", (string) $r->getReturnType(), "):";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$r = new ReflectionFunction("finfo_file");
echo "file(", (string) $r->getReturnType(), "):";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$r = new ReflectionFunction("mime_content_type");
echo "mime(", (string) $r->getReturnType(), "):";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$c = new ReflectionClass("finfo");
$r = $c->getMethod("__construct");
echo "construct:", $r->getNumberOfParameters(), ":", $r->isVariadic() ? "variadic" : "fixed", ":";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$r = $c->getMethod("file");
echo "m_file:";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$r = $c->getMethod("buffer");
echo "m_buffer:";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
$r = $c->getMethod("set_flags");
echo "m_set_flags:";
foreach ($r->getParameters() as $p) { echo show($p), ";"; }
echo "\n";
"#,
    );
    assert_eq!(
        out,
        concat!(
            "open(finfo|false):flags:int=0;magic_database:?string=NULL;\n",
            "close:finfo:finfo;\n",
            "set_flags:finfo:finfo;flags:int;\n",
            "buffer(string|false):finfo:finfo;string:string;flags:int=0;context:<none>=NULL;\n",
            "file(string|false):finfo:finfo;filename:string;flags:int=0;context:<none>=NULL;\n",
            "mime(string|false):filename:<none>;\n",
            "construct:2:fixed:flags:int=0;magic_database:?string=NULL;\n",
            "m_file:filename:string;flags:int=0;context:<none>=NULL;\n",
            "m_buffer:string:string;flags:int=0;context:<none>=NULL;\n",
            "m_set_flags:flags:int;\n",
        )
    );
}
/// `open_basedir` confines the magic database for the procedural and object forms, follows
/// symlink targets, and still allows a database created inside the compiled root.
#[test]
fn test_fileinfo_open_basedir_confines_magic_database() {
    if fileinfo_native::skip("test_fileinfo_open_basedir_confines_magic_database") { return; }
    let out = super::fileinfo_php_src::run_case(
        r#"<?php
var_dump(finfo_open(0, "/etc/hosts"));
try {
    new finfo(0, "/etc/hosts");
} catch (Exception $e) {
    echo $e->getMessage(), "\n";
}
symlink("/etc/hosts", __DIR__ . "/link.magic");
var_dump(finfo_open(0, __DIR__ . "/link.magic"));
file_put_contents(__DIR__ . "/mini.magic", "0 string x test\n");
var_dump(finfo_open(0, __DIR__ . "/mini.magic") instanceof finfo);
"#,
        &[(r#"open_basedir"#, r#"'.'"#)],
        &[],
    );
    super::fileinfo_php_src::assert_format(
        &out,
        r#"Warning: finfo_open(): open_basedir restriction in effect. File(/etc/hosts) is not within the allowed path(s): (.) in %s on line %d
bool(false)
finfo::__construct(): open_basedir restriction in effect. File(/etc/hosts) is not within the allowed path(s): (.)

Warning: finfo_open(): open_basedir restriction in effect. File(%s/link.magic) is not within the allowed path(s): (.) in %s on line %d
bool(false)
bool(true)"#,
    );
}

/// A direct `new finfo()` converts a successful load's first warning to an exception, while
/// `finfo_open()` replays it and returns the object. A handler exception during replay
/// propagates instead of being converted to `false`.
#[test]
fn test_fileinfo_constructor_warning_semantics() {
    if fileinfo_native::skip("test_fileinfo_constructor_warning_semantics") { return; }
    let out = super::fileinfo_php_src::run_case(
        r#"<?php
file_put_contents(__DIR__ . "/check.magic", "0 string hello hello-db\n");
try { new finfo(PHP_INT_MAX - 1, __DIR__ . "/check.magic"); }
catch (Exception $e) { echo $e->getMessage(), "\n"; }
var_dump(finfo_open(PHP_INT_MAX - 1, __DIR__ . "/check.magic") instanceof finfo);
set_error_handler(function() { throw new Exception("handler"); });
try { finfo_open(PHP_INT_MAX - 1, __DIR__ . "/check.magic"); }
catch (Exception $e) { echo "caught:", $e->getMessage(), "\n"; }
"#,
        &[],
        &[],
    );
    super::fileinfo_php_src::assert_format(
        &out,
        r#"finfo::__construct(): using regular magic file `%scheck.magic'

Warning: finfo_open(): using regular magic file `%scheck.magic' in %stest.php on line 5
bool(true)
caught:handler"#,
    );
}

/// Dynamic eval reads the declared parameter types from the AOT native metadata once the
/// fileinfo prelude is part of the program.
#[test]
fn test_fileinfo_eval_parameter_types_bundle() {
    if fileinfo_native::skip("test_fileinfo_eval_parameter_types_bundle") { return; }
    let out = compile_and_run(
        r#"<?php
$f = new finfo(FILEINFO_MIME_TYPE);
$code = $argc > 0 ? '$a = new ReflectionParameter("finfo_open", "magic_database"); echo $a->getType(), "|"; $b = new ReflectionParameter("finfo_buffer", "finfo"); echo $b->getType(), "|", $b->isOptional() ? "optional" : "required", "|"; $c = new ReflectionParameter("finfo_open", "flags"); echo $c->getDefaultValue(), "|"; $d = new ReflectionParameter("mime_content_type", "filename"); echo $d->hasType() ? "typed" : "none";' : '';
eval($code);
"#,
    );
    assert_eq!(out, "?string|finfo|required|0|none");
}

/// `finfo_close()`/`finfo_set_flags()` declare the literal `true` return type, while the
/// corresponding internal methods expose it as a tentative contract like PHP.
#[test]
fn test_fileinfo_true_return_types_and_tentative_methods() {
    if fileinfo_native::skip("test_fileinfo_true_return_types_and_tentative_methods") { return; }
    let out = compile_and_run(
        r#"<?php
$f = new finfo();
echo (string) (new ReflectionFunction("finfo_close"))->getReturnType(), "|";
echo (string) (new ReflectionFunction("finfo_set_flags"))->getReturnType(), "|";
$c = new ReflectionClass("finfo");
$m = $c->getMethod("set_flags");
echo $m->hasReturnType() ? "H" : "h", $m->hasTentativeReturnType() ? "T" : "t",
    (string) $m->getTentativeReturnType(), "|";
$m = $c->getMethod("buffer");
echo $m->hasReturnType() ? "H" : "h", $m->hasTentativeReturnType() ? "T" : "t",
    (string) $m->getTentativeReturnType();
$code = $argc > 0 ? 'echo "|", (string) (new ReflectionFunction("finfo_close"))->getReturnType(), "|";' : '';
eval($code);
"#,
    );
    assert_eq!(out, "true|true|hTtrue|hTstring|false|true|");
}
