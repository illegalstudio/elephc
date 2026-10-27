//! Purpose:
//! Regression coverage for lexical enum trait constants and catchable access errors.
//!
//! Called from:
//! - The object-oriented codegen integration suite.
//!
//! Key details:
//! - Parameterized materialization cases require clean heaps; receiver controls compare output separately.

use crate::support::{compile_and_run_capture, compile_and_run_with_heap_debug_per_argv};

/// Keeps each materialization/access branch independent while compiling its source only once.
fn assert_clean_cases(source: &str, cases: &[(&[&str], &str)]) {
    let argvs = cases.iter().map(|(args, _)| *args).collect::<Vec<_>>();
    let outputs = compile_and_run_with_heap_debug_per_argv(source, &argvs);
    for (output, (args, expected)) in outputs.iter().zip(cases) {
        assert!(output.success, "{args:?}: {}", output.stderr);
        assert_eq!(output.stdout, *expected, "{args:?}: {}", output.stderr);
        assert!(output.stderr.contains("leak summary: clean"), "{args:?}: {}", output.stderr);
    }
}

/// Verifies the reviewed magic semantics against PHP with strict heap cleanup.
#[test]
fn test_enum_trait_constant_review_magic() {
    let source = r#"<?php
namespace A { trait T { const CLS = __CLASS__; const TR = __TRAIT__; const NS = __NAMESPACE__; const FN = __FUNCTION__; const METH = __METHOD__; const LINE = __LINE__; const FILE = __FILE__; const DIR = __DIR__; const DATA = [__CLASS__, __TRAIT__]; } trait Nested { use T; } }
namespace B { enum E { use \A\Nested; case One; } enum F { use \A\Nested; case One; } }
namespace C { class Reader { static function read() { echo \B\E::CLS, '|', \B\E::TR, '|', \B\E::NS, '|', \B\E::FN, '|', \B\E::METH, '|', \B\E::LINE, '|', json_encode(\B\F::DATA), ":", \B\F::FILE === __FILE__ ? "file" : "bad-file", ":", \B\F::DIR === __DIR__ ? "dir" : "bad-dir"; } } Reader::read(); }
"#;
    assert_clean_cases(source, &[
        (&[], "B\\E|A\\T|A|||2|[\"B\\\\F\",\"A\\\\T\"]:file:dir"),
    ]);
}

/// Verifies the reviewed lazy parent semantics against PHP with strict heap cleanup.
#[test]
fn test_enum_trait_constant_review_lazy_parent() {
    let source = r#"<?php
trait T { const DIRECT = parent::X; const NESTED = ['before', parent::X]; const SAFE = true ? 'yes' : parent::X; const PARENT_NAME = parent::class; }
enum E { use T; case One; const array TYPED = [parent::X]; }
function readParent() { return E::DIRECT; }
echo 'declared|';
try {
    if ($argv[1] === 'direct') { echo E::DIRECT; }
    elseif ($argv[1] === 'nested') { echo json_encode(E::NESTED); }
    elseif ($argv[1] === 'typed') { echo json_encode(E::TYPED); }
    elseif ($argv[1] === 'function') { echo readParent(); }
    elseif ($argv[1] === 'name') { echo E::PARENT_NAME; }
    elseif ($argv[1] === 'safe') { echo E::SAFE; }
    else { echo 'unused'; }
} catch (Error $e) { echo $e->getMessage(); unset($e); }
"#;
    assert_clean_cases(source, &[
        (&["unused"], "declared|unused"),
        (&["direct"], "declared|Cannot access \"parent\" when current class scope has no parent"),
        (&["function"], "declared|Cannot access \"parent\" when current class scope has no parent"),
        (&["nested"], "declared|Cannot access \"parent\" when current class scope has no parent"),
        (&["typed"], "declared|Cannot access \"parent\" when current class scope has no parent"),
        (&["name"], "declared|Cannot use \"parent\" when current class scope has no parent"),
        (&["safe"], "declared|yes"),
    ]);
}

/// Verifies the reviewed visibility semantics against PHP with strict heap cleanup.
#[test]
fn test_enum_trait_constant_review_visibility() {
    let source = r#"<?php
trait T {
    private const A = 1;
    protected const B = 2;
    private const BROKEN = parent::X;
    public const PUBLIC_VALUE = self::A + self::B;
    public static function read(): int { return self::A + self::B; }
    public static function broken() { return self::BROKEN; }
}
enum E { use T; case One; }
function outsideRead() { return E::A; }
class Other { public static function read() { return E::B; } }
try {
    if ($argv[1] === 'private') { echo E::A; }
    elseif ($argv[1] === 'protected') { echo E::B; }
    elseif ($argv[1] === 'function') { echo outsideRead(); }
    elseif ($argv[1] === 'class') { echo Other::read(); }
    elseif ($argv[1] === 'hidden-parent') { echo E::BROKEN; }
    elseif ($argv[1] === 'inside-parent') { echo E::broken(); }
    else { echo E::read(), ':', E::PUBLIC_VALUE, '|'; echo E::A; }
} catch (Error $e) { echo $e->getMessage(); unset($e); }
"#;
    assert_clean_cases(source, &[
        (&["private"], "Cannot access private constant E::A"),
        (&["protected"], "Cannot access protected constant E::B"),
        (&["function"], "Cannot access private constant E::A"),
        (&["class"], "Cannot access protected constant E::B"),
        (&["hidden-parent"], "Cannot access private constant E::BROKEN"),
        (&["inside-parent"], "Cannot access \"parent\" when current class scope has no parent"),
        (&["public-then-private"], "3:3|Cannot access private constant E::A"),
    ]);
}

/// Verifies the reviewed unused reads semantics against PHP with strict heap cleanup.
#[test]
fn test_enum_trait_constant_review_unused_reads() {
    let source = r#"<?php
trait T { private const A = 1; const P = parent::X; }
enum E { use T; case One; }
try { E::A; echo 'bad-private'; } catch (Error $e) { echo $e->getMessage(), '|'; unset($e); }
try { E::P; echo 'bad-parent'; } catch (Error $e) { echo $e->getMessage(); unset($e); }
"#;
    assert_clean_cases(source, &[
        (&[], "Cannot access private constant E::A|Cannot access \"parent\" when current class scope has no parent"),
    ]);
}

/// Keeps lexical enum access in a closure functional; the independent enum-receiver owner leak is baseline-pinned.
#[test]
fn test_enum_trait_constant_review_closure_scope_control() {
    let output = compile_and_run_capture(r#"<?php
trait T { private const A = 1; public function read(): int { return (fn() => self::A)(); } }
enum E { use T; case One; }
echo E::One->read();
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "1");
    // The exact old and new compilers both retain the same 40-byte enum-case receiver owner.
    // Preserve that separate witness; the strict materialization cases above require zero leaks.
}
/// Preserves the review's instance-method example independently of the baseline enum-case retention.
#[test]
fn test_enum_trait_constant_review_instance_scope_control() {
    let output = compile_and_run_capture(r#"<?php
trait T { private const A = 1; protected const B = 2; public function read(): int { return self::A + self::B; } }
enum E { use T; case One; }
echo E::One->read(), '|';
try { echo E::A; } catch (Error $e) { echo $e->getMessage(), '|'; unset($e); }
try { echo E::B; } catch (Error $e) { echo $e->getMessage(); unset($e); }
"#);
    assert!(output.success, "{}", output.stderr);
    assert_eq!(output.stdout, "3|Cannot access private constant E::A|Cannot access protected constant E::B");
    // Receiver retention is identical on the old compiler, separately from constant-value ownership.
}
