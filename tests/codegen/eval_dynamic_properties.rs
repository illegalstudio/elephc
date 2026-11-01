//! Purpose:
//! End-to-end value tests for DYNAMIC properties — created by `$object->name = ...` rather than
//! declared — on objects the interpreter owns, where every reader has to agree about the same
//! object.
//!
//! Called from:
//! - `cargo test --test codegen_tests eval_dynamic_properties` through Rust's test harness.
//!
//! Key details:
//! - An interpreter-declared object keeps its properties in the interpreter's own per-object
//!   store, while `print_r`, `var_dump`, `json_encode`, `get_object_vars` and the reflection
//!   listings all walk the runtime object's slots. A value written to only one of the two is
//!   silently absent from whichever half does not look there, which is why the readers are
//!   exercised together rather than one per test.
//! - The runtime-include case matters on its own: that is the shape an autoloader produces, and
//!   the include path is deliberately not a literal so the file is chosen at run time.
//! - Every expected string is `php -n` 8.5.6's output. PHP also emits `Deprecated: Creation of
//!   dynamic property C::$p is deprecated` for a class without `#[AllowDynamicProperties]`;
//!   that goes to stderr, not stdout, so it is absent from these expectations.

use crate::support::*;

/// Verifies a plain runtime object and an interpreter-declared one answer alike.
///
/// `php -n` 8.5.6 prints `std=A;own=B;issetstd=y;issetown=y;varsstd=1;varsown=1`.
#[test]
fn test_eval_dynamic_property_answers_alike_on_both_object_kinds() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
$o = new stdClass(); $o->p = "A";
class Own {}
$u = new Own(); $u->q = "B";
echo "std=", $o->p, ";own=", $u->q, ";";
echo "issetstd=", isset($o->p) ? "y" : "n", ";issetown=", isset($u->q) ? "y" : "n", ";";
echo "varsstd=", count(get_object_vars($o)), ";varsown=", count(get_object_vars($u));
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "std=A;own=B;issetstd=y;issetown=y;varsstd=1;varsown=1");
}

/// Verifies `isset`, `empty`, `unset` and `property_exists` agree about a dynamic property.
///
/// `php -n` 8.5.6 prints `Iemip`. The final letter is the one that catches a half-finished
/// removal: `unset()` has to clear both stores, or the property stays findable.
#[test]
fn test_eval_isset_empty_and_unset_agree_about_a_dynamic_property() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class D4 { public $declared = "d"; }
$o = new D4();
$o->dyn = "v";
echo isset($o->dyn) ? "I" : "i";
echo empty($o->dyn) ? "E" : "e";
echo isset($o->missing) ? "M" : "m";
unset($o->dyn);
echo isset($o->dyn) ? "I" : "i";
echo property_exists($o, "dyn") ? "P" : "p";
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "Iemip");
}

/// Verifies `json_encode()` and `print_r()` both show the declared and the dynamic property.
///
/// `php -n` 8.5.6 prints the JSON object then the `print_r` block. `json_encode()` used to
/// print `{}` and `print_r()` used to show the declared name with an empty value.
#[test]
fn test_eval_json_encode_and_print_r_show_both_properties() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class D5 { public $declared = "d"; }
$o = new D5();
$o->dyn2 = "w";
echo json_encode($o);
echo ":";
print_r($o);
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(
        out,
        "{\"declared\":\"d\",\"dyn2\":\"w\"}:D5 Object\n(\n    [declared] => d\n    [dyn2] => w\n)\n"
    );
}

/// Verifies reflection sees the same dynamic property that plain access does.
///
/// `php -n` 8.5.6 prints `D:value:H`. This is the container-shaped read of the same object.
#[test]
fn test_eval_reflection_sees_the_same_dynamic_property() {
    let out = compile_and_run(
        r#"<?php
$code = <<<'PHPCODE'
class D8 { public $declared = "d"; }
$o = new D8();
$o->dynamic = "value";
$ref = new ReflectionObject($o);
$p = $ref->getProperty("dynamic");
echo $p->isDynamic() ? "D" : "d"; echo ":";
echo $p->getValue($o); echo ":";
echo $ref->hasProperty("dynamic") ? "H" : "h";
PHPCODE;
eval($code);
"#,
    );
    assert_eq!(out, "D:value:H");
}

/// Verifies the same holds for a class a file included at RUN TIME declared.
///
/// `php -n` 8.5.6 prints `loaded;A;y;1;{"declared":"d","dyn":"A"}`. This is the autoload-shaped
/// path: the include path is not a literal, so the file and its class arrive at run time.
#[test]
fn test_runtime_include_dynamic_property_is_visible_to_every_reader() {
    let out = compile_and_run_files(
        &[
            (
                "main.php",
                "<?php\n$name = \"piece\" . \".php\";\ninclude $name;\n",
            ),
            (
                "piece.php",
                r#"<?php
class S3Store { public $declared = "d"; }
echo "loaded;";
$o = new S3Store();
$o->dyn = "A";
echo $o->dyn, ";";
echo isset($o->dyn) ? "y" : "n", ";";
echo count(get_object_vars($o)) - 1, ";";
echo json_encode($o);
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "loaded;A;y;1;{\"declared\":\"d\",\"dyn\":\"A\"}");
}

/// A class an autoloader includes from another function frame reads `$_SERVER` lazily, the way
/// php's `auto_globals_jit` creates it (Laravel's `SetRequestForConsole::bootstrap()` under
/// artisan died with "Undefined variable $_SERVER"). Expected output verified against php 8.5.
#[test]
fn test_runtime_included_class_reads_cli_server_superglobal_from_another_frame() {
    let out = compile_and_run(
        r#"<?php
$dir = sys_get_temp_dir() . '/elephc_sg_' . getmypid();
@mkdir($dir);
file_put_contents($dir . '/SgProbe.php', '<?php class SgProbe { function f() { $s = $_SERVER; return count($s) > 0 ? "server-ok" : "server-empty"; } }');
spl_autoload_register(function ($c) use ($dir) { require $dir . '/' . $c . '.php'; });
class SgKernel { function run() { $out = ''; foreach (['SgProbe'] as $cls) { $out .= (new $cls)->f(); } return $out; } }
$k = new SgKernel;
echo $k->run(), "\n";
unlink($dir . '/SgProbe.php'); rmdir($dir);
"#,
    );
    assert_eq!(out, "server-ok\n");
}

/// A compiled closure bound to a class the INTERPRETER declared writes that class's private
/// properties under the bound scope. The class has no compiled id, so the scope used to be
/// dropped and the write reached the interpreter from global scope: a runtime-name write fataled
/// "Cannot access private property" (Symfony's DeepClone hydrators against an interpreted
/// `Definition`) and a literal-name write was lost. Expected output verified against php 8.5.
#[test]
fn test_closure_bound_to_an_interpreted_class_writes_its_private_properties() {
    let out = compile_and_run(
        r#"<?php
$file = sys_get_temp_dir() . '/elephc_bind_scope_' . getmypid() . '.php';
file_put_contents($file, '<?php namespace Dc; class D { private ?string $class = null; private array $tags = []; function c() { return $this->class; } function t() { return $this->tags; } }');
require $file;
unlink($file);
$c = "Dc\\D";
$literal = Closure::bind(static function ($o) { $o->class = 'A'; }, null, $c);
$o = new $c; $literal($o); echo $o->c(), "\n";
$write = Closure::bind(static function ($o, $values) { foreach ($values as $n => $v) { $o->$n = $v; } }, null, $c);
$o = new $c; $write($o, ['class' => 'B', 'tags' => ['x']]); echo $o->c(), ' ', implode(',', $o->t()), "\n";
$read = Closure::bind(static function ($o, $n) { return $o->$n; }, null, $c);
echo $read($o, 'class'), "\n";
$scoped = Closure::bind(static function ($o) { $o->class = 'C'; }, null, $o);
$scoped($o); echo $o->c(), "\n";
try {
    (static function ($o, $n) { $o->$n = 'D'; })($o, 'class');
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "A\nB x\nB\nC\nCannot access private property Dc\\D::$class\n"
    );
}
