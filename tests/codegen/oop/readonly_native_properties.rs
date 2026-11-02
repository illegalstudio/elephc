//! Purpose:
//! Regressions for readonly properties with native buffer and pointer storage.
//!
//! Called from:
//! - The native object-oriented codegen suite.
//!
//! Key details:
//! - Native values retain exact EIR types across readonly initialization probes.
//! - Buffers and raw allocations are explicitly freed after their last use.

use crate::support::{compile_and_run, compile_and_run_with_heap_debug};

const PROMOTED_BUFFER: &str = r#"<?php
class A {
    public function __construct(public readonly buffer<int> $ids) {}
}
buffer<int> $ids = buffer_new<int>(2);
$ids[1] = 42;
$a = new A($ids);
echo $a->ids[1];
buffer_free($ids);
"#;

/// Promoted readonly buffers preserve values populated through a caller's local.
#[test]
fn test_readonly_native_promoted_buffer_initialization() {
    assert_eq!(compile_and_run(PROMOTED_BUFFER), "42");
}

/// Promoted readonly buffer initialization leaves no boxed RHS or object owners behind.
#[test]
fn test_readonly_native_promoted_buffer_heap_clean() {
    verify_heap_clean(PROMOTED_BUFFER, "42");
}

/// Declared readonly packed buffers initialize once while their records stay mutable.
#[test]
fn test_readonly_native_declared_packed_buffer_initialization() {
    verify_heap_clean(r#"<?php
packed class V { public int $x; }
class A {
    public readonly buffer<V> $b;
    public function __construct() { $this->b = buffer_new<V>(1); }
    public function update(): void { $this->b[0]->x = 5; }
}
$a = new A();
$a->update();
echo $a->b[0]->x;
$b = $a->b;
buffer_free($b);
"#, "5");
}

/// Promoted untyped and typed readonly pointers preserve null pointer payloads.
#[test]
fn test_readonly_native_promoted_pointer_initialization() {
    verify_heap_clean(r#"<?php
packed class V { public int $x; }
class A {
    public function __construct(public readonly ptr $p) {}
}
class B {
    public function __construct(public readonly ptr<V> $p) {}
}
$a = new A(ptr_null());
$b = new B(ptr_cast<V>(ptr_null()));
echo ptr_is_null($a->p) ? 'null' : 'set', ':', ptr_is_null($b->p) ? 'null' : 'set';
"#, "null:null");
}

/// A rejected buffer overwrite throws a catchable Error and preserves the original buffer.
#[test]
fn test_readonly_native_buffer_overwrite_preserves_value() {
    verify_heap_clean(r#"<?php
class A {
    public function __construct(public readonly buffer<int> $ids) {}
    public function replace(buffer<int> $ids): void { $this->ids = $ids; }
}
buffer<int> $ids = buffer_new<int>(2);
$ids[1] = 42;
buffer<int> $replacement = buffer_new<int>(2);
$replacement[1] = 99;
$a = new A($ids);
try { $a->replace($replacement); }
catch (Error $error) { echo $error->getMessage(), '|'; }
echo $a->ids[1];
buffer_free($replacement);
buffer_free($ids);
"#, "Cannot modify readonly property A::$ids|42");
}

/// A null pointer still counts as initialized and survives a rejected non-null overwrite.
#[test]
fn test_readonly_native_pointer_overwrite_preserves_value() {
    verify_heap_clean(r#"<?php
extern function calloc(u64 $count, u64 $size): ptr;
extern function free(ptr $p): void;
class A {
    public function __construct(public readonly ptr $p) {}
    public function replace(ptr $p): void { $this->p = $p; }
}
$a = new A(ptr_null());
$replacement = calloc(1, 8);
try { $a->replace($replacement); }
catch (Error $error) { echo $error->getMessage(), '|'; }
echo ptr_is_null($a->p) ? 'null' : 'set';
free($replacement);
"#, "Cannot modify readonly property A::$p|null");
}

/// Native coalesce writes initialize declared storage. A null `ptr` is not PHP `null`, so `??=`
/// keeps an initialized null pointer without attempting the readonly write.
#[test]
fn test_readonly_native_coalesce_initialization() {
    verify_heap_clean(r#"<?php
class A {
    public readonly buffer<int> $ids;
    public readonly ptr $p;
    public function __construct(buffer<int> $ids) {
        $this->ids ??= $ids;
        $this->p ??= ptr_null();
    }
    public function keep(): void { $this->ids ??= buffer_new<int>(2); }
    public function replace(): void { $this->p ??= ptr_null(); }
}
buffer<int> $ids = buffer_new<int>(2);
$ids[1] = 42;
$a = new A($ids);
$a->keep();
echo $a->ids[1], '|';
try { $a->replace(); }
catch (Error $error) { echo $error->getMessage(), '|'; }
echo ptr_is_null($a->p) ? 'null' : 'set';
buffer_free($ids);
"#, "42|null");
}

/// Checks fixture output and balanced heap ownership after native readonly writes.
fn verify_heap_clean(source: &str, expected: &str) {
    let out = compile_and_run_with_heap_debug(source);
    assert!(out.success, "stdout={:?}\nstderr={}", out.stdout, out.stderr);
    assert_eq!(out.stdout, expected, "{}", out.stderr);
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}
