//! Purpose:
//! End-to-end tests for `var_dump()` of OBJECTS: the `object(C)#N (n) { … }` header,
//! the per-property body with PHP's visibility-annotated keys, objects nested in
//! arrays/hashes/other objects, uninitialized typed properties, and the
//! `*RECURSION*` guard. Closes the object half of upstream issue #388; the array
//! half is covered by `var_dump_nested_tests`.
//!
//! Called from:
//! - `cargo test --test var_dump_object_tests` through Rust's test harness.
//!
//! Key details:
//! - REGRESSION ANCHOR: `var_dump()` of an object used to print the bare class
//!   name (`object(P)\n`) with NO handle, NO property count and NO body, and an
//!   object nested inside any container rendered as `NULL` (tag 6 fell through to
//!   `__rt_var_dump_emit_null_line`). `top_level_object`, `object_in_indexed_array`
//!   and `object_in_hash` are those repros.
//! - EXPECTATIONS ARE REFERENCE PHP'S OUTPUT, BYTE FOR BYTE. Every expectation
//!   here was taken from `php -d xdebug.mode=off` on the same program against PHP
//!   8.5.6, INCLUDING the `#N` object handle. There is no longer any exception in the
//!   OBJECT-HANDLE PARITY section at the end of this file: the last one,
//!   `hash_init_context_consumes_no_handle_divergence`, was retired when `hash_init()`
//!   grew a real `HashContext` object (`elephc::hash_prelude`) and became
//!   `hash_context_draws_an_object_handle_in_creation_order`, a parity assertion.
//! - THE ONE DIVERGENCE PIN LEFT IN THIS FILE is
//!   `computed_debug_info_is_ignored_and_declared_properties_print_instead`. elephc
//!   honours `__debugInfo()` only when its body is a static property projection (folded
//!   into `_class_vd_desc_*` at compile time); a body that computes values falls back to
//!   the declared-property list. That test asserts elephc's ACTUAL output with PHP's
//!   spelled out beside it, so it can never be mistaken for parity.
//! - ENUM CASES ARE LAZY NOW, AND THAT CELL IS PARITY. Cases used to be created in
//!   bulk in `main`'s prologue, which burnt one handle per case of every referenced
//!   enum before user code ran; `enum_cases_are_eager_so_handles_shift_divergence`
//!   pinned that drift. They are materialized on first access by
//!   `codegen::enum_singletons`, so the LAZY ENUM CASE MATERIALIZATION block below
//!   replaces it with parity assertions. Those tests check `===` BEFORE they check
//!   any `#N`: a scheme that allocated a fresh object per access would produce
//!   correct-looking handles while breaking every enum identity comparison.
//! - THE HANDLE IS REAL AND IT IS SHARED WITH `spl_object_id()`. `#N` and
//!   `spl_object_id()` both read `__rt_object_handle_of`, so they cannot
//!   contradict each other; `spl_object_hash()` is the same handle rendered as
//!   PHP's 32 hex characters. Handles start at 1, are dense, and are reused LIFO
//!   after an object dies, exactly like php-src. Closures, arrow functions,
//!   first-class callables and generators all consume one, because in PHP they
//!   are all objects.
//! - `(n)` is the count of INITIALIZED properties: PHP lists an uninitialized
//!   typed property in the body as `uninitialized(T)` but excludes it from the
//!   count, which is why `__rt_vd_obj_count` recomputes it at runtime instead of
//!   reading the descriptor's static property count.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an
//!   isolated temp dir, compile a plain executable, run it, and assert stdout —
//!   the same harness style as `var_dump_nested_tests` / `opcache_ini_tests`.
//!   Host-target only (macOS aarch64 local).
//! - Compile STDERR is filtered to elephc's OWN diagnostics: on Linux, GNU `ld`
//!   adds static-glibc and `.note.GNU-stack` warnings that Apple's linker never
//!   emits, so an unfiltered assertion would be non-portable.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather
/// than anything elephc emitted: GNU `ld` reports static-glibc notes and the
/// `.note.GNU-stack` deprecation, while Apple's linker stays silent. Anchoring on
/// elephc's own line starts isolates its diagnostics — and still surfaces an
/// UNEXPECTED elephc warning, which an allow-list of known messages would hide.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source`, runs the executable and returns its STDOUT.
///
/// Asserts a clean compile (with no elephc diagnostics) and a clean exit first: a
/// walker that dereferences a wrong-shaped slot shows up as a signal, not as bad
/// text, so the status assertions are load-bearing. An unguarded recursive walker
/// would blow the stack here rather than produce a wrong string.
fn run_php(stem: &str, source: &str) -> String {
    let dir = make_test_dir("elephc_var_dump_object");
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();

    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg(&php);
    let compile = cmd.output().expect("failed to spawn elephc");
    let raw_stderr = String::from_utf8_lossy(&compile.stderr).into_owned();
    assert!(
        compile.status.success(),
        "elephc compile failed:\n{raw_stderr}"
    );
    let diagnostics = elephc_diagnostics(&raw_stderr);
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostics:\n{diagnostics}"
    );

    run_binary(&dir.join(stem))
}

/// Runs a compiled executable and returns its STDOUT, asserting a clean exit.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Compiles `source` with `--heap-debug`, runs it, and returns the run's STDERR.
///
/// `--heap-debug` is the authoritative allocator check for this file: `--gc-stats`
/// under-reports, and a per-access leak that happens to balance against a
/// premature free reads identically to correct behaviour from the counters alone.
/// The instrumentation writes its `allocs=/frees=/live_blocks=` lines and its leak
/// summary to STDERR, so this returns STDERR rather than STDOUT.
fn run_php_heap_debug(stem: &str, source: &str) -> String {
    let dir = make_test_dir("elephc_var_dump_object");
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();

    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg("--heap-debug");
    cmd.arg(&php);
    let compile = cmd.output().expect("failed to spawn elephc");
    let raw_stderr = String::from_utf8_lossy(&compile.stderr).into_owned();
    assert!(
        compile.status.success(),
        "elephc --heap-debug compile failed:\n{raw_stderr}"
    );
    let diagnostics = elephc_diagnostics(&raw_stderr);
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostics:\n{diagnostics}"
    );

    let output = Command::new(dir.join(stem))
        .output()
        .expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?})",
        output.status.code()
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The headline repro: a top-level object used to print `object(P)\n` and nothing
/// else. PHP: `object(P)#1 (2) { … }`.
#[test]
fn top_level_object() {
    let out = run_php(
        "top_level_object",
        "<?php class P { public int $x = 1; public string $n = \"bob\"; }\nvar_dump(new P());\n",
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (2) {\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "  [\"n\"]=>\n",
            "  string(3) \"bob\"\n",
            "}\n",
        )
    );
}

/// An object with NO declared properties. Guards the zero-count walk: the body
/// must be empty and the closing brace must still align at column 0.
#[test]
fn empty_object() {
    let out = run_php("empty_object", "<?php class E {}\nvar_dump(new E());\n");
    assert_eq!(out, "object(E)#1 (0) {\n}\n");
}

/// The second repro: an object nested in an INDEXED array rendered as `NULL`.
/// Also pins the indent — the object header sits at the element's indent and its
/// properties one level (2 spaces) deeper.
#[test]
fn object_in_indexed_array() {
    let out = run_php(
        "object_in_indexed",
        "<?php class P { public int $x = 1; public string $n = \"bob\"; }\nvar_dump([1, new P()]);\n",
    );
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [0]=>\n",
            "  int(1)\n",
            "  [1]=>\n",
            "  object(P)#1 (2) {\n",
            "    [\"x\"]=>\n",
            "    int(1)\n",
            "    [\"n\"]=>\n",
            "    string(3) \"bob\"\n",
            "  }\n",
            "}\n",
        )
    );
}

/// The hash half of the same repro, with a scalar entry AFTER the object so a
/// mis-restored indent or seen-stack depth would show up on the following line.
#[test]
fn object_in_hash() {
    let out = run_php(
        "object_in_hash",
        "<?php class P { public int $x = 1; }\nvar_dump([\"k\" => new P(), \"j\" => 2]);\n",
    );
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [\"k\"]=>\n",
            "  object(P)#1 (1) {\n",
            "    [\"x\"]=>\n",
            "    int(1)\n",
            "  }\n",
            "  [\"j\"]=>\n",
            "  int(2)\n",
            "}\n",
        )
    );
}

/// An object holding an ARRAY property — the object walker handing a container
/// back to `__rt_var_dump_value`, which is the mutual-recursion edge that the
/// array-only fix could not exercise.
#[test]
fn object_with_array_property() {
    let out = run_php(
        "object_array_prop",
        "<?php class P { public array $a = [1, 2]; public int $x = 5; }\nvar_dump(new P());\n",
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (2) {\n",
            "  [\"a\"]=>\n",
            "  array(2) {\n",
            "    [0]=>\n",
            "    int(1)\n",
            "    [1]=>\n",
            "    int(2)\n",
            "  }\n",
            "  [\"x\"]=>\n",
            "  int(5)\n",
            "}\n",
        )
    );
}

/// An object holding another OBJECT — object-to-object recursion through the
/// value renderer, and the seen-stack holding two live entries at once.
#[test]
fn object_with_object_property() {
    let out = run_php(
        "object_obj_prop",
        concat!(
            "<?php\n",
            "class Inner { public int $i = 7; }\n",
            "class Outer { public Inner $in; public function __construct() { $this->in = new Inner(); } }\n",
            "var_dump(new Outer());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(Outer)#1 (1) {\n",
            "  [\"in\"]=>\n",
            "  object(Inner)#2 (1) {\n",
            "    [\"i\"]=>\n",
            "    int(7)\n",
            "  }\n",
            "}\n",
        )
    );
}

/// An object TWO container levels deep: the indent must grow by exactly 2 per
/// level for objects the same way it does for arrays.
#[test]
fn object_two_levels_deep() {
    let out = run_php(
        "object_two_deep",
        "<?php class L { public int $v = 3; }\nvar_dump([[new L()]]);\n",
    );
    assert_eq!(
        out,
        concat!(
            "array(1) {\n",
            "  [0]=>\n",
            "  array(1) {\n",
            "    [0]=>\n",
            "    object(L)#1 (1) {\n",
            "      [\"v\"]=>\n",
            "      int(3)\n",
            "    }\n",
            "  }\n",
            "}\n",
        )
    );
}

/// PROPERTY VISIBILITY: PHP annotates the key with the property's visibility —
/// `["a"]`, `["b":protected]`, `["c":"C":private]`. Non-public properties are
/// listed in full; only the KEY differs.
#[test]
fn property_visibility_annotations() {
    let out = run_php(
        "visibility",
        "<?php class C { public int $a = 1; protected int $b = 2; private int $c = 3; }\nvar_dump(new C());\n",
    );
    assert_eq!(
        out,
        concat!(
            "object(C)#1 (3) {\n",
            "  [\"a\"]=>\n",
            "  int(1)\n",
            "  [\"b\":protected]=>\n",
            "  int(2)\n",
            "  [\"c\":\"C\":private]=>\n",
            "  int(3)\n",
            "}\n",
        )
    );
}

/// A private property INHERITED into a subclass keeps the DECLARING class in its
/// key, not the runtime class — the same rule serialize's name mangling follows.
#[test]
fn inherited_private_property_names_declaring_class() {
    let out = run_php(
        "inherited_private",
        concat!(
            "<?php\n",
            "class Base { private int $secret = 4; }\n",
            "class Child extends Base { public int $open = 5; }\n",
            "var_dump(new Child());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(Child)#1 (2) {\n",
            "  [\"secret\":\"Base\":private]=>\n",
            "  int(4)\n",
            "  [\"open\"]=>\n",
            "  int(5)\n",
            "}\n",
        )
    );
}

/// SCALAR PROPERTY TYPES at depth: float, both bools, and a null-valued nullable
/// property (a boxed Mixed cell, which the value renderer has to unbox).
#[test]
fn scalar_property_types() {
    let out = run_php(
        "scalar_props",
        concat!(
            "<?php\n",
            "class T { public float $f = 1.5; public bool $b = true; public bool $g = false; public ?string $s = null; }\n",
            "var_dump(new T());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(T)#1 (4) {\n",
            "  [\"f\"]=>\n",
            "  float(1.5)\n",
            "  [\"b\"]=>\n",
            "  bool(true)\n",
            "  [\"g\"]=>\n",
            "  bool(false)\n",
            "  [\"s\"]=>\n",
            "  NULL\n",
            "}\n",
        )
    );
}

/// An UNINITIALIZED typed property renders `uninitialized(T)` and is EXCLUDED
/// from the header count — `(1)`, not `(2)`, for a class with two declared
/// properties one of which was never written.
#[test]
fn uninitialized_typed_property() {
    let out = run_php(
        "uninit_prop",
        "<?php class U { public int $set = 1; public string $unset; }\nvar_dump(new U());\n",
    );
    assert_eq!(
        out,
        concat!(
            "object(U)#1 (1) {\n",
            "  [\"set\"]=>\n",
            "  int(1)\n",
            "  [\"unset\"]=>\n",
            "  uninitialized(string)\n",
            "}\n",
        )
    );
}

/// THE RECURSION GUARD. `$a->self = $a;` is a genuine cycle: an unguarded walker
/// descends forever and dies on the stack rather than printing anything. PHP
/// prints `*RECURSION*` in place of the value, and continues with the remaining
/// properties — so this also pins that the guard does not abort the walk.
#[test]
fn self_referential_object_renders_recursion() {
    let out = run_php(
        "recursion",
        concat!(
            "<?php\n",
            "class R { public ?R $self = null; public int $x = 1; }\n",
            "$a = new R();\n",
            "$a->self = $a;\n",
            "var_dump($a);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(R)#1 (2) {\n",
            "  [\"self\"]=>\n",
            "  *RECURSION*\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "}\n",
        )
    );
}

/// A cycle through TWO objects: the guard has to hold every object on the path,
/// not just the immediately enclosing one, or `$b->back` would recurse forever.
#[test]
fn mutual_object_cycle_renders_recursion() {
    let out = run_php(
        "mutual_cycle",
        concat!(
            "<?php\n",
            "class A { public ?B $b = null; }\n",
            "class B { public ?A $back = null; }\n",
            "$a = new A();\n",
            "$b = new B();\n",
            "$a->b = $b;\n",
            "$b->back = $a;\n",
            "var_dump($a);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(A)#1 (1) {\n",
            "  [\"b\"]=>\n",
            "  object(B)#2 (1) {\n",
            "    [\"back\"]=>\n",
            "    *RECURSION*\n",
            "  }\n",
            "}\n",
        )
    );
}

/// TWO SIBLING references to the SAME object are NOT recursion — the guard is a
/// stack of the objects on the current path, so it must be popped on the way out.
/// A guard implemented as a "already seen anywhere" set would wrongly print
/// `*RECURSION*` for the second element here.
#[test]
fn sibling_references_to_same_object_both_render() {
    let out = run_php(
        "siblings",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$a = new P();\n",
            "var_dump([$a, $a]);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [0]=>\n",
            "  object(P)#1 (1) {\n",
            "    [\"x\"]=>\n",
            "    int(1)\n",
            "  }\n",
            "  [1]=>\n",
            "  object(P)#1 (1) {\n",
            "    [\"x\"]=>\n",
            "    int(1)\n",
            "  }\n",
            "}\n",
        )
    );
}

/// TWO CONSECUTIVE top-level dumps: the second must start at column 0. A walk
/// that left `_vd_indent` or the seen-stack depth dirty would show up here and
/// nowhere else.
#[test]
fn consecutive_top_level_dumps_reset_state() {
    let out = run_php(
        "consecutive",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "var_dump(new P());\n",
            "var_dump(new P());\n",
            "var_dump(7);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "}\n",
            "object(P)#1 (1) {\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "}\n",
            "int(7)\n",
        )
    );
}

/// An object dumped AFTER a self-referential one: the `*RECURSION*` early-exit
/// must still pop the seen stack it pushed, or every later object would report
/// recursion too.
#[test]
fn recursion_does_not_leak_guard_state() {
    let out = run_php(
        "recursion_then_plain",
        concat!(
            "<?php\n",
            "class R { public ?R $self = null; }\n",
            "class P { public int $x = 1; }\n",
            "$a = new R();\n",
            "$a->self = $a;\n",
            "var_dump($a);\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(R)#1 (1) {\n",
            "  [\"self\"]=>\n",
            "  *RECURSION*\n",
            "}\n",
            "object(P)#2 (1) {\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "}\n",
        )
    );
}

// ---------------------------------------------------------------------------
// OBJECT-HANDLE PARITY
//
// PHP's `#N` is the object HANDLE: a small dense integer starting at 1, exposed
// by `spl_object_id()`, and REUSED LIFO once an object is destroyed. Every
// expectation below is byte-for-byte `php -d xdebug.mode=off` output on the same
// program, taken from PHP 8.5.6.
//
// The four tests this section replaces (`object_handle_is_omitted_never_fabricated`,
// `handle_omitted_after_free_where_php_would_reuse_one`,
// `closure_shifts_php_handle_numbering`,
// `spl_object_id_is_a_heap_pointer_not_a_php_handle`) pinned the ABSENCE of the
// handle and the pointer-shaped `spl_object_id`. They were replaced deliberately
// when handles landed: the scenarios they described are exactly the ones the
// tests below now assert PHP's real answer for, and the closure and LIFO-reuse
// cases — the two a naive counter gets wrong — are still the load-bearing ones.
// ---------------------------------------------------------------------------

/// The first object in a program is `#1`. Reference PHP 8.5.6 output, exactly.
#[test]
fn first_object_is_handle_one() {
    let out = run_php(
        "handle_first_object",
        "<?php class P { public int $x = 1; }\nvar_dump(new P());\n",
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "}\n",
        )
    );
}

/// Three simultaneously live objects number `#1`, `#2`, `#3` in ALLOCATION order.
#[test]
fn several_live_objects_number_ascending() {
    let out = run_php(
        "handle_ascending",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$a = new P();\n",
            "$b = new P();\n",
            "$c = new P();\n",
            "var_dump($a);\n",
            "var_dump($b);\n",
            "var_dump($c);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#3 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// Handles are reused LIFO: after `unset($a); unset($b);` the next two objects
/// are `#2` then `#1`, NOT `#3` and `#4`. This is the case a monotonic counter
/// gets wrong, and reference PHP 8.5.6 prints exactly these two headers.
#[test]
fn handle_is_reused_lifo_after_free() {
    let out = run_php(
        "handle_lifo_reuse",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$a = new P();\n",
            "$b = new P();\n",
            "unset($a);\n",
            "unset($b);\n",
            "$c = new P();\n",
            "$d = new P();\n",
            "var_dump($c);\n",
            "var_dump($d);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// Dumping the SAME object twice prints the same handle both times — the handle
/// is object identity, not a per-dump counter.
#[test]
fn two_dumps_of_the_same_object_agree() {
    let out = run_php(
        "handle_same_object_twice",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$o = new P();\n",
            "var_dump($o);\n",
            "var_dump($o);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// A CLOSURE is an object in PHP and consumes a handle, so the `P` allocated
/// after one is `#2`, not `#1`. This is the case that sank the previous attempt:
/// an elephc closure is a callable descriptor rather than a class-id-headed
/// object, so `lower_closure_new` now gives even a capture-free closure runtime
/// descriptor storage purely so it can hold — and release — a handle.
#[test]
fn closure_shifts_handle_numbering() {
    let out = run_php(
        "handle_closure_shift",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$f = function () { return 1; };\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#2 (1) {\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "}\n",
        )
    );
}

/// An ARROW FUNCTION is a Closure too and shifts the numbering the same way.
#[test]
fn arrow_function_shifts_handle_numbering() {
    let out = run_php(
        "handle_arrow_shift",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$f = fn(): int => 2;\n",
            "var_dump(new P());\n",
            "echo $f(), \"\\n\";\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "2\n",
        )
    );
}

/// A FIRST-CLASS CALLABLE `h(...)` also produces a Closure and takes a handle.
#[test]
fn first_class_callable_shifts_handle_numbering() {
    let out = run_php(
        "handle_fcc_shift",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "function h(): int { return 7; }\n",
            "$f = h(...);\n",
            "var_dump(new P());\n",
            "echo $f(), \"\\n\";\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "7\n",
        )
    );
}

/// A GENERATOR is an object in PHP: calling a generator function materializes a
/// Generator instance, which takes handle `#1` here.
#[test]
fn generator_shifts_handle_numbering() {
    let out = run_php(
        "handle_generator_shift",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "function gen() { yield 1; }\n",
            "$g = gen();\n",
            "var_dump(new P());\n",
            "foreach ($g as $v) { echo $v, \"\\n\"; }\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "1\n",
        )
    );
}

/// An EXCEPTION object consumes a handle like any other object, so it is `#1`
/// and the plain object created after it is `#2`.
#[test]
fn exception_object_consumes_a_handle() {
    let out = run_php(
        "handle_exception",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$e = new RuntimeException(\"boom\");\n",
            "var_dump(spl_object_id($e));\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "int(1)\n",
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// `spl_object_id()` returns the SAME number `var_dump` prints, and
/// `spl_object_hash()` is PHP's 32-character rendering of it: 16 zero-padded hex
/// digits then 16 zeros. Printing a `#N` that disagreed with `spl_object_id`
/// would be worse than printing none, so this is the consistency anchor.
#[test]
fn spl_object_id_and_hash_match_the_printed_handle() {
    let out = run_php(
        "handle_spl_object_id",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$o = new P();\n",
            "var_dump(spl_object_id($o));\n",
            "var_dump($o);\n",
            "var_dump(spl_object_hash($o));\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "int(1)\n",
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "string(32) \"00000000000000010000000000000000\"\n",
        )
    );
}

/// 1024 simultaneously live objects reach handle `#1024`. The handle is NOT
/// stored in the allocator header's spare kind-word bits — those would cap it at
/// 255, which is why `255` / `256` are asserted explicitly across that boundary.
#[test]
fn a_thousand_live_objects_are_not_capped_at_255() {
    let out = run_php(
        "handle_thousand_objects",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$keep = [];\n",
            "for ($i = 0; $i < 1024; $i++) { $keep[] = new P(); }\n",
            "echo spl_object_id($keep[0]), \"\\n\";\n",
            "echo spl_object_id($keep[254]), \"\\n\";\n",
            "echo spl_object_id($keep[255]), \"\\n\";\n",
            "var_dump($keep[1023]);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "1\n",
            "255\n",
            "256\n",
            "object(P)#1024 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

// ---------------------------------------------------------------------------
// LAZY ENUM CASE MATERIALIZATION
//
// These replace `enum_cases_are_eager_so_handles_shift_divergence`, which pinned
// elephc's old answer (`#3`) with PHP's (`#2`) spelled out in prose. Enum cases
// are now materialized on FIRST ACCESS, so the divergence is gone and the cell is
// asserted as parity instead.
//
// Every expectation below was taken from `php -d xdebug.mode=off` on PHP 8.5.6
// running the same program. Two PHP behaviours are load-bearing and were measured
// rather than assumed:
//   - A PURE enum materializes ONLY the case you touch.
//   - A BACKED enum materializes EVERY case, in declaration order, on the first
//     touch of ANY case — `enum E: int {A,B,C} $e = E::B;` leaves the next object
//     at `#4`, while the pure-enum spelling leaves it at `#2`.
// `E::cases()` and `from()`/`tryFrom()` materialize every case as well, including
// a `tryFrom()` that matches nothing.
//
// IDENTITY IS TESTED BEFORE HANDLES, deliberately. A lazy scheme that allocated a
// fresh object per access would print handles that look plausible while silently
// breaking `===`, `match`, and `in_array(..., true)` — a far worse failure than
// the handle drift these tests replace. `enum_case_identity_holds_across_every_access_path`
// is the guard for that and would fail loudly before any `#N` assertion did.
// ---------------------------------------------------------------------------

/// PARITY (was `enum_cases_are_eager_so_handles_shift_divergence`) — TOUCHING ONE
/// CASE OF A PURE ENUM CREATES EXACTLY ONE OBJECT.
///
/// The headline repro. PHP creates `E::A` on first access and caches it, so the
/// `P` after it is `#2`; elephc used to create all three cases in `main`'s
/// prologue and print `#4`. The three-case enum makes the old drift (+3) impossible
/// to confuse with an off-by-one.
#[test]
fn enum_case_materializes_lazily_on_first_access() {
    let out = run_php(
        "handle_enum_lazy_first_access",
        concat!(
            "<?php\n",
            "enum E { case A; case B; case C; }\n",
            "class P { public int $x = 1; }\n",
            "$case = E::A;\n",
            "var_dump(new P());\n",
            "echo $case->name, \"\\n\";\n",
        ),
    );
    assert_eq!(
        out,
        concat!("object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n", "A\n",)
    );
}

/// PARITY — AN ENUM THAT IS DECLARED BUT NEVER EVALUATED COSTS NOTHING.
///
/// Both enums here are *referenced* by code the program never runs (a function
/// that is never called, a class constant that is never read), which is exactly
/// what the old reachability scan counted as "needs singletons". PHP allocates
/// nothing, so the `P` is `#1`.
#[test]
fn untouched_enum_cases_burn_no_handle() {
    let out = run_php(
        "handle_enum_untouched",
        concat!(
            "<?php\n",
            "enum E { case A; case B; case C; }\n",
            "enum F: string { case X = 'x'; case Y = 'y'; }\n",
            "class P { public int $x = 1; }\n",
            "class K { const Q = F::X; }\n",
            "function nope() { return [E::A, F::Y]; }\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(out, "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n");
}

/// PARITY — A PURE ENUM MATERIALIZES ONLY THE CASE THAT WAS TOUCHED.
///
/// Touching the MIDDLE case is the discriminating spelling: an implementation
/// that materialized "every case up to this one" would print `#3`, and one that
/// materialized all three would print `#4`. PHP prints `#2`.
#[test]
fn pure_enum_materializes_only_the_touched_case() {
    let out = run_php(
        "handle_enum_pure_one_case",
        concat!(
            "<?php\n",
            "enum E { case A; case B; case C; }\n",
            "class P { public int $x = 1; }\n",
            "$e = E::B;\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(out, "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n");
}

/// PARITY — A BACKED ENUM MATERIALIZES EVERY CASE ON THE FIRST TOUCH OF ANY CASE.
///
/// This is NOT the pure-enum rule, and the difference was measured, not assumed:
/// php-src builds a backed enum's whole case set together, so touching `E::B`
/// alone hands `E::A` handle 1 and leaves the `P` at `#4`. The identical program
/// with `enum E { … }` instead of `enum E: int { … }` prints `#2`
/// (`pure_enum_materializes_only_the_touched_case`). Touching the LAST case still
/// numbers the cases in DECLARATION order, which the second half asserts.
#[test]
fn backed_enum_materializes_every_case_on_first_touch() {
    let out = run_php(
        "handle_enum_backed_all",
        concat!(
            "<?php\n",
            "enum E: int { case A = 1; case B = 2; case C = 3; }\n",
            "class P { public int $x = 1; }\n",
            "$e = E::C;\n",
            "var_dump(new P());\n",
            "echo spl_object_id(E::A), \" \", spl_object_id(E::B), \" \", spl_object_id(E::C), \"\\n\";\n",
        ),
    );
    assert_eq!(
        out,
        concat!("object(P)#4 (1) {\n  [\"x\"]=>\n  int(1)\n}\n", "1 2 3\n",)
    );
}

/// PARITY — `E::cases()` MATERIALIZES EVERY CASE, IN DECLARATION ORDER, AND
/// REUSES THE ONES THAT ALREADY EXIST.
///
/// The sharp cell. Two cases are touched out of order first (`C` then `A`, taking
/// handles 1 and 2), so a `cases()` that re-created everything would renumber
/// them, and one that appended in call order would return `C` first. PHP does
/// neither: the returned array is in DECLARATION order, `A` and `C` keep the
/// handles they already had, and only the missing `B` is created — taking handle
/// 3 even though it sits in the middle of the array.
#[test]
fn enum_cases_materializes_the_rest_in_declaration_order() {
    let out = run_php(
        "handle_enum_cases_order",
        concat!(
            "<?php\n",
            "enum E { case A; case B; case C; }\n",
            "$x = E::C;\n",
            "$y = E::A;\n",
            "$cs = E::cases();\n",
            "foreach ($cs as $c) { echo $c->name, \"#\", spl_object_id($c), \" \"; }\n",
            "echo \"\\n\";\n",
            "var_dump($cs[0] === E::A, $cs[2] === $x);\n",
        ),
    );
    assert_eq!(
        out,
        concat!("A#2 B#3 C#1 \n", "bool(true)\n", "bool(true)\n",)
    );
}

/// PARITY — A FIRST-EVER `E::cases()` NUMBERS THE CASES 1..n IN DECLARATION ORDER.
///
/// The companion to the test above, with nothing materialized beforehand: the
/// whole set is created by the `cases()` call itself, so the `P` after it is `#4`.
#[test]
fn first_enum_cases_call_numbers_from_one() {
    let out = run_php(
        "handle_enum_cases_fresh",
        concat!(
            "<?php\n",
            "enum E { case A; case B; case C; }\n",
            "class P { public int $x = 1; }\n",
            "$a = E::cases();\n",
            "var_dump(new P());\n",
            "echo spl_object_id(E::A), \" \", spl_object_id(E::B), \" \", spl_object_id(E::C), \"\\n\";\n",
        ),
    );
    assert_eq!(
        out,
        concat!("object(P)#4 (1) {\n  [\"x\"]=>\n  int(1)\n}\n", "1 2 3\n",)
    );
}

/// PARITY — `tryFrom()` MATERIALIZES EVERY CASE EVEN WHEN IT MATCHES NOTHING.
///
/// The unrolled backing-value scan compares against compile-time literals and
/// only reads a case slot on a match, so a lookup that misses would naturally
/// create nothing. PHP creates all three anyway — the `P` is `#4`, not `#1` — so
/// the materialization is requested explicitly at the head of `from`/`tryFrom`.
#[test]
fn try_from_materializes_every_case_even_on_no_match() {
    let out = run_php(
        "handle_enum_tryfrom_miss",
        concat!(
            "<?php\n",
            "enum S: string { case A = 'a'; case B = 'b'; case C = 'c'; }\n",
            "class P { public int $x = 1; }\n",
            "$v = S::tryFrom('zzz');\n",
            "var_dump($v === null);\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "object(P)#4 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// PARITY — A CASE USED AS A CLASS CONSTANT OR A DEFAULT PARAMETER IS STILL LAZY.
///
/// Neither initializer runs until the constant is read / the parameter defaults
/// in, so the `P` created before both is `#1` and the case then takes handle 1.
/// The `===` on each line is what proves the lazy slot is a cache rather than a
/// per-read allocation.
#[test]
fn class_constant_and_default_parameter_cases_are_lazy() {
    let out = run_php(
        "handle_enum_const_default",
        concat!(
            "<?php\n",
            "enum E { case A; case B; case C; }\n",
            "class P { public int $x = 1; }\n",
            "class K { const D = E::B; }\n",
            "function withDefault(E $e = E::C): E { return $e; }\n",
            "var_dump(new P());\n",
            "echo spl_object_id(K::D), \"\\n\";\n",
            "var_dump(K::D === E::B, withDefault() === E::C);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "1\n",
            "bool(true)\n",
            "bool(true)\n",
        )
    );
}

/// PARITY — A `match` MATERIALIZES ONLY THE ARMS IT ACTUALLY EVALUATES.
///
/// `$e = E::B` takes handle 1. The `match` then evaluates `E::A` (handle 2) and
/// stops at the `E::B` arm, so `E::C` is still unborn and takes handle 3 only when
/// the final `echo` names it. An eager scheme printed `A=1 B=2 C=3` here.
#[test]
fn match_materializes_only_the_arms_it_evaluates() {
    let out = run_php(
        "handle_enum_match_arms",
        concat!(
            "<?php\n",
            "enum E { case A; case B; case C; }\n",
            "$e = E::B;\n",
            "echo \"B=\", spl_object_id(E::B), \"\\n\";\n",
            "$r = match ($e) { E::A => 'a', E::B => 'b', E::C => 'c' };\n",
            "echo $r, \" A=\", spl_object_id(E::A), \" B=\", spl_object_id(E::B), \" C=\", spl_object_id(E::C), \"\\n\";\n",
        ),
    );
    assert_eq!(out, concat!("B=1\n", "b A=2 B=1 C=3\n",));
}

/// PARITY — SEPARATE ENUMS MATERIALIZE IN ACCESS ORDER, NOT DECLARATION ORDER.
///
/// The second-declared enum is touched first and takes handle 1. The old eager
/// initializer walked the enums SORTED BY NAME, so it always gave `E1` the lower
/// handles regardless of what user code touched first; this is the cell that
/// caught that.
#[test]
fn separate_enums_materialize_in_access_order() {
    let out = run_php(
        "handle_enum_access_order",
        concat!(
            "<?php\n",
            "enum E1 { case A; case B; }\n",
            "enum E2 { case X; case Y; }\n",
            "class P { public int $x = 1; }\n",
            "$b = E2::Y;\n",
            "$a = E1::A;\n",
            "echo spl_object_id(E2::Y), \" \", spl_object_id(E1::A), \"\\n\";\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(
        out,
        concat!("1 2\n", "object(P)#3 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",)
    );
}

/// IDENTITY — EVERY ACCESS PATH REACHES THE SAME OBJECT.
///
/// THIS IS THE TEST THAT MATTERS MOST. A lazy scheme that allocated a fresh object
/// per access would satisfy every `#N` assertion above while silently breaking all
/// of PHP's enum semantics, so this asserts identity across the whole matrix:
/// a direct `E::A`, a class constant, a default parameter, `from()`, `tryFrom()`,
/// `cases()` by index, a `match` subject, a value round-tripped through a function
/// typed by the enum, and `->name` / `->value` reads. Every line must be `true`.
#[test]
fn enum_case_identity_holds_across_every_access_path() {
    let out = run_php(
        "handle_enum_identity_paths",
        concat!(
            "<?php\n",
            "enum E: int { case A = 1; case B = 2; case C = 3; }\n",
            "class K { const D = E::B; }\n",
            "function withDefault(E $e = E::C): E { return $e; }\n",
            "function roundTrip(E $e): E { return $e; }\n",
            "$cs = E::cases();\n",
            "var_dump(\n",
            "  E::A === E::A,\n",
            "  K::D === E::B,\n",
            "  withDefault() === E::C,\n",
            "  E::from(2) === E::B,\n",
            "  E::tryFrom(3) === E::C,\n",
            "  $cs[0] === E::A,\n",
            "  $cs[2] === E::C,\n",
            "  roundTrip(E::B) === E::B,\n",
            "  (match (E::A) { E::A => true, default => false }),\n",
            "  E::B->name === 'B',\n",
            "  E::B->value === 2\n",
            ");\n",
        ),
    );
    assert_eq!(out, "bool(true)\n".repeat(11));
}

/// IDENTITY — REPEATED READS IN A LOOP RETURN ONE OBJECT, NOT ONE PER ITERATION.
///
/// The direct refutation of "allocate on every access": a thousand reads must all
/// report the SAME handle, and the object created afterwards must take the very
/// next one (`#2`) rather than `#1001`. Deliberately array-free — accumulating the
/// ids in an array would drag in an unrelated indexed-array keying bug (a
/// call-result used as a key also creates a phantom `[0]` slot), which would make
/// this test fail for a reason that has nothing to do with enum identity.
#[test]
fn repeated_case_reads_return_one_object() {
    let out = run_php(
        "handle_enum_repeat_reads",
        concat!(
            "<?php\n",
            "enum E { case A; case B; }\n",
            "class P { public int $x = 1; }\n",
            "$first = spl_object_id(E::A);\n",
            "$same = true;\n",
            "for ($i = 0; $i < 1000; $i++) {\n",
            "  if (spl_object_id(E::A) !== $first) { $same = false; }\n",
            "}\n",
            "var_dump($same);\n",
            "echo $first, \"\\n\";\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "1\n",
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// IDENTITY — A MATERIALIZED CASE SURVIVES HEAP CHURN THAT REUSES ITS NEIGHBOURS.
///
/// A lazily created singleton has to live for the whole program. A backed enum
/// materializes siblings that NOTHING references, which is precisely the shape a
/// premature release would reclaim: if `E::B`/`E::C` were freed, the churn below
/// would reuse their handles and the reads afterwards would read a dangling slot.
/// The handles must still be 2 and 3, and the values must still read back.
#[test]
fn materialized_cases_survive_heap_churn() {
    let out = run_php(
        "handle_enum_churn",
        concat!(
            "<?php\n",
            "enum E: string { case A = 'a'; case B = 'b'; case C = 'c'; }\n",
            "class P { public int $x = 1; }\n",
            "$a = E::A;\n",
            "$junk = [];\n",
            "for ($i = 0; $i < 50; $i++) { $junk[] = new P(); }\n",
            "$junk = null;\n",
            "for ($i = 0; $i < 50; $i++) { $z = new P(); }\n",
            "echo E::B->value, E::C->value, \"\\n\";\n",
            "var_dump(E::B === E::from('b'), E::C === E::tryFrom('c'));\n",
            "echo spl_object_id(E::B), \" \", spl_object_id(E::C), \"\\n\";\n",
        ),
    );
    assert_eq!(
        out,
        concat!("bc\n", "bool(true)\n", "bool(true)\n", "2 3\n",)
    );
}

/// PARITY — AN ENUM WITH AN INTERFACE, METHODS AND CONSTANTS IS STILL LAZY.
///
/// Implementing an interface and declaring a method/constant does not force any
/// case into existence: the `P` before the first read is `#1`, and the two cases
/// of this backed enum then take handles 1 and 2 so the second `P` is `#3`.
#[test]
fn enum_with_interface_methods_and_constants_is_lazy() {
    let out = run_php(
        "handle_enum_interface",
        concat!(
            "<?php\n",
            "interface HasLabel { public function label(): string; }\n",
            "enum S: string implements HasLabel {\n",
            "  const FALLBACK = 'none';\n",
            "  case A = 'a';\n",
            "  case B = 'b';\n",
            "  public function label(): string { return $this->name . ':' . $this->value; }\n",
            "}\n",
            "class P { public int $x = 1; }\n",
            "var_dump(new P());\n",
            "echo S::A->label(), \" \", S::FALLBACK, \"\\n\";\n",
            "var_dump(S::A instanceof HasLabel, S::A === S::from('a'));\n",
            "echo spl_object_id(new P()), \"\\n\";\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "A:a none\n",
            "bool(true)\n",
            "bool(true)\n",
            "3\n",
        )
    );
}

/// HEAP HEALTH — A LAZY SINGLETON IS NEITHER LEAKED PER ACCESS NOR FREED EARLY.
///
/// `--heap-debug` is the authoritative allocator check; `--gc-stats` under-reports
/// and would read the same whether a lazily created singleton leaked once per
/// access or was freed while still reachable. The three shapes together pin both
/// failure modes with exact block counts rather than a summary word:
///
/// - An enum nobody touches allocates NOTHING: `leak summary: clean`. This is the
///   line the eager initializer could never produce.
/// - A pure enum whose single case is read 200 times ends with exactly ONE live
///   block. A per-access allocation would end with 200; a premature free would end
///   with none and would have handed the block back for reuse while the global slot
///   still pointed at it.
/// - A backed enum read 600 times ends with exactly THREE — its whole case set,
///   materialized once. The case objects are process-lifetime singletons owned by
///   their global slots, so they are live at exit BY DESIGN; PHP's enum cases live
///   just as long.
#[test]
fn lazy_enum_singletons_are_heap_exact() {
    let untouched = run_php_heap_debug(
        "handle_enum_heap_untouched",
        concat!(
            "<?php\n",
            "enum E: int { case A = 1; case B = 2; case C = 3; }\n",
            "$n = 0;\n",
            "for ($i = 0; $i < 200; $i++) { $n += $i; }\n",
            "echo $n, \"\\n\";\n",
        ),
    );
    assert!(
        untouched.contains("leak summary: clean"),
        "an untouched enum must allocate nothing, got:\n{untouched}"
    );

    let one_case = run_php_heap_debug(
        "handle_enum_heap_one_case",
        concat!(
            "<?php\n",
            "enum E { case A; case B; case C; }\n",
            "$n = 0;\n",
            "for ($i = 0; $i < 200; $i++) { $n += strlen(E::A->name); }\n",
            "echo $n, \"\\n\";\n",
        ),
    );
    assert!(
        one_case.contains("live_blocks=1"),
        "200 reads of one pure case must leave exactly one live block, got:\n{one_case}"
    );

    let all_cases = run_php_heap_debug(
        "handle_enum_heap_backed",
        concat!(
            "<?php\n",
            "enum E: int { case A = 1; case B = 2; case C = 3; }\n",
            "$n = 0;\n",
            "for ($i = 0; $i < 200; $i++) {\n",
            "  $n += E::A->value + E::B->value + E::C->value;\n",
            "}\n",
            "echo $n, \"\\n\";\n",
        ),
    );
    assert!(
        all_cases.contains("live_blocks=3"),
        "600 reads of a three-case backed enum must leave exactly three live blocks, got:\n{all_cases}"
    );
}

/// PARITY (was `eval_in_scope_remints_object_handles_divergence`) — AN `eval()` IN
/// SCOPE DOES NOT RENUMBER LIVE OBJECTS.
///
/// This test used to pin `#5` and blame the AOT eval bridge for "re-materializing
/// live objects on the way in and out". That diagnosis was wrong, and the shape of
/// the program is what disproves it: `$o` is created and dumped around an `eval()`
/// that never mentions it, and the `#5` was visible on an object that existed
/// BEFORE the `eval()` ran — no staging behaviour can renumber the past.
///
/// The real cause was eager enum-case materialization. A module containing `eval()`
/// treated EVERY enum as reachable (eval can name any case at runtime), so the four
/// prelude cases — `PropertyHookType::{Get,Set}` and `SortDirection::{Ascending,
/// Descending}` — were created in `main`'s prologue and took handles 1..4 before
/// user code started, leaving `$o` at `#5`. With cases materialized on first access
/// (`codegen::enum_singletons`) nothing touches those four, `$o` is `#1`, and the
/// bridge was never at fault.
#[test]
fn eval_in_scope_preserves_object_handles() {
    let out = run_php(
        "handle_eval_scope",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$o = new P();\n",
            "eval('$q = 1;');\n",
            "var_dump($o);\n",
        ),
    );
    assert_eq!(
        out,
        concat!("object(P)#1 (1) {\n", "  [\"x\"]=>\n", "  int(1)\n", "}\n",)
    );
}

/// Verifies a `HashContext` DRAWS AN OBJECT HANDLE from the shared pool, in creation
/// order with ordinary objects.
///
/// This test used to be the last remaining member of the "PHP counts something as an
/// object that elephc does not" family, asserting `object(P)#1` because the incremental
/// hashing state was a resource with no class id. Closures, arrow functions, first-class
/// callables, `Closure::bind` results and generators had already been brought into the
/// pool; the hash context was the holdout, and its docblock was written as a tripwire
/// that would fail "the moment `hash_init` grows an object representation". It did.
///
/// `hash_init()` now returns a real `HashContext` allocated through the standard object
/// path, so it takes handle `#1` and the plain `P` created after it is `#2` — byte for
/// byte what reference PHP 8.5.6 prints for this program. The assertion is therefore
/// PARITY now, not a documented divergence; a regression to the resource model would
/// print `#1` here and fail.
#[test]
fn hash_context_draws_an_object_handle_in_creation_order() {
    let out = run_php(
        "handle_hash_context",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$c = hash_init(\"md5\");\n",
            "var_dump(new P());\n",
            "echo hash_final($c), \"\\n\";\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#2 (1) {\n",
            "  [\"x\"]=>\n",
            "  int(1)\n",
            "}\n",
            "d41d8cd98f00b204e9800998ecf8427e\n",
        )
    );
}

/// Verifies `var_dump()` HONOURS `__debugInfo()` when its body is a property projection.
///
/// PHP replaces the whole property body with the array `__debugInfo()` returns — a
/// different key set, a different order, and a different count in the `(n)` header.
/// elephc folds the projection into the class's `_class_vd_desc_*` table at compile
/// time (`codegen_support::runtime::data::user::var_dump_debug_info_projection`), so no
/// method is actually called at dump time; the observable result is identical for this
/// shape. `secret` must NOT appear, and the header must read `(1)` and not `(2)` — a
/// naive implementation that filtered rows but kept the declared count would print a
/// count that disagrees with the body.
#[test]
fn var_dump_honours_a_debug_info_property_projection() {
    let out = run_php(
        "debuginfo_projection",
        concat!(
            "<?php\n",
            "class C {\n",
            "    public string $algo = \"md5\";\n",
            "    public int $secret = 7;\n",
            "    public function __debugInfo(): array { return [\"algo\" => $this->algo]; }\n",
            "}\n",
            "var_dump(new C());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(C)#1 (1) {\n",
            "  [\"algo\"]=>\n",
            "  string(3) \"md5\"\n",
            "}\n",
        )
    );
}

/// Verifies a `__debugInfo()` projection may RENAME and REORDER, and may be empty.
///
/// Renaming is what proves the key text comes from the array key rather than from the
/// property, and reordering proves rows are emitted in projection order rather than
/// layout order. `return [];` is a legal projection meaning "print no properties", which
/// PHP renders as `(0) {}` — the count must follow.
#[test]
fn debug_info_projection_may_rename_reorder_and_be_empty() {
    let out = run_php(
        "debuginfo_rename_reorder",
        concat!(
            "<?php\n",
            "class C {\n",
            "    public int $a = 1;\n",
            "    public int $b = 2;\n",
            "    public function __debugInfo(): array { return [\"second\" => $this->b, \"first\" => $this->a]; }\n",
            "}\n",
            "class E {\n",
            "    public int $hidden = 9;\n",
            "    public function __debugInfo(): array { return []; }\n",
            "}\n",
            "var_dump(new C());\n",
            "var_dump(new E());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(C)#1 (2) {\n",
            "  [\"second\"]=>\n",
            "  int(2)\n",
            "  [\"first\"]=>\n",
            "  int(1)\n",
            "}\n",
            // `#1` again, not `#2`: the `C` temporary is released once `var_dump`
            // returns and its handle goes back to the pool, so `E` reuses it. Reference
            // PHP 8.5.6 prints `#1` here too — verified, not assumed.
            "object(E)#1 (0) {\n",
            "}\n",
        )
    );
}

/// Verifies a computed `__debugInfo()` result replaces the declared-property dump like php-src.
#[test]
fn computed_debug_info_is_used_instead_of_declared_properties() {
    let out = run_php(
        "debuginfo_computed_divergence",
        concat!(
            "<?php\n",
            "class C {\n",
            "    public array $items = [1, 2];\n",
            "    public function __debugInfo(): array { return [\"n\" => count($this->items)]; }\n",
            "}\n",
            "var_dump(new C());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(C)#1 (1) {\n",
            "  [\"n\"]=>\n",
            "  int(2)\n",
            "}\n",
        )
    );
}

// ---------------------------------------------------------------------------
// ENUM CASES — `var_dump()` renders `enum(E::C)`, never an object body.
//
// REGRESSION ANCHOR (issue B16): every enum case used to fall through the tag-6
// object path and print its backing storage, e.g.
// `object(Status)#1 (1) { ["name"]=> string(6) "Active" }` for a pure enum and a
// two-property body with `value` FIRST for a backed one. PHP prints one line:
// `enum(Status::Active)`.
//
// The test is at the value level, not the builtin level: `__rt_vd_val_obj`
// consults `__rt_obj_enum_name_offset` before writing anything object-shaped, so
// a nested enum gets the same treatment at any depth and at the right indent.
// Every expectation below is reference PHP 8.4.20's output for the same program.
// ---------------------------------------------------------------------------

/// A PURE enum case at top level. PHP: `enum(Status::Active)`.
#[test]
fn top_level_pure_enum_case() {
    let out = run_php(
        "vd_enum_pure",
        "<?php enum Status { case Active; case Idle; }\nvar_dump(Status::Active);\n",
    );
    assert_eq!(out, "enum(Status::Active)\n");
}

/// A STRING-BACKED enum case. PHP prints the case name, never the backing value.
#[test]
fn top_level_string_backed_enum_case() {
    let out = run_php(
        "vd_enum_backed_str",
        "<?php enum Suit: string { case Hearts = 'H'; }\nvar_dump(Suit::Hearts);\n",
    );
    assert_eq!(out, "enum(Suit::Hearts)\n");
}

/// An INT-BACKED enum case. Pinned separately from the string-backed one because
/// the two lay their `name`/`value` slots out identically but hold different
/// payload shapes, so a walker reading the wrong slot fails on exactly one.
#[test]
fn top_level_int_backed_enum_case() {
    let out = run_php(
        "vd_enum_backed_int",
        "<?php enum Lvl: int { case Low = 3; }\nvar_dump(Lvl::Low);\n",
    );
    assert_eq!(out, "enum(Lvl::Low)\n");
}

/// Enum cases NESTED in an array — one indexed entry and one string key. Pins the
/// indent: the `enum(...)` line sits where the element's value line would.
#[test]
fn enum_cases_nested_in_an_array() {
    let out = run_php(
        "vd_enum_in_array",
        concat!(
            "<?php\n",
            "enum Status { case Active; }\n",
            "enum Suit: string { case Hearts = 'H'; }\n",
            "var_dump([Status::Active, 'k' => Suit::Hearts]);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "array(2) {\n",
            "  [0]=>\n",
            "  enum(Status::Active)\n",
            "  [\"k\"]=>\n",
            "  enum(Suit::Hearts)\n",
            "}\n",
        )
    );
}

/// An enum case held in an OBJECT PROPERTY. The property line indents like any
/// other value line, and the sibling `int` property proves the walk continues.
#[test]
fn enum_case_in_an_object_property() {
    let out = run_php(
        "vd_enum_in_object",
        concat!(
            "<?php\n",
            "enum Status { case Active; }\n",
            "class H {\n",
            "    public $e;\n",
            "    public $n = 7;\n",
            "    function __construct() { $this->e = Status::Active; }\n",
            "}\n",
            "var_dump(new H());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(H)#1 (2) {\n",
            "  [\"e\"]=>\n",
            "  enum(Status::Active)\n",
            "  [\"n\"]=>\n",
            "  int(7)\n",
            "}\n",
        )
    );
}

/// `var_dump()` is variadic: several values in ONE call, enums mixed with scalars,
/// each dumped independently in source order.
#[test]
fn several_values_in_one_var_dump_call() {
    let out = run_php(
        "vd_enum_variadic",
        concat!(
            "<?php\n",
            "enum Status { case Active; }\n",
            "enum Suit: string { case Hearts = 'H'; }\n",
            "var_dump(Status::Active, 42, Suit::Hearts);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "enum(Status::Active)\n",
            "int(42)\n",
            "enum(Suit::Hearts)\n",
        )
    );
}

/// An enum case reached through a `mixed`-typed local, i.e. the boxed Mixed
/// (tag 7) path rather than a statically typed object operand. Both funnel into
/// `__rt_var_dump_value`, and this pins that they agree.
#[test]
fn enum_case_through_a_mixed_local() {
    let out = run_php(
        "vd_enum_mixed_local",
        concat!(
            "<?php\n",
            "enum Status { case Idle; }\n",
            "function pick(mixed $v): mixed { return $v; }\n",
            "var_dump(pick(Status::Idle));\n",
        ),
    );
    assert_eq!(out, "enum(Status::Idle)\n");
}
