//! Purpose:
//! Checker boundaries of binding a packed list to a declared hash parameter (`array<K, V>`).
//!
//! Called from:
//! - The error integration test harness.
//!
//! Key details:
//! - By value a list is rebuilt as the hash, so only element storage with no conversion is
//!   refused. Runtime behavior is covered by `tests/codegen/list_hash_param_binding.rs`.
//! - By reference the callee writes a hash through the caller's own storage. An ordinary local
//!   is promoted in place; storage other code reads as a list is refused instead of crashing.

use super::{expect_error, expect_no_error};

/// The diagnostic every refused by-reference binding carries.
const BY_REFERENCE: &str = "a by-reference parameter uses the caller's own storage";

/// Prefixes `body` with a by-reference hash writer and a by-value hash reader.
fn hashes(body: &str) -> String {
    format!(
        "<?php function put(array<int, int> &$m): void {{ $m[9] = 7; }} \
         function read(array<int, int> $m): int {{ return count($m); }} {body}"
    )
}

/// A plain list local is accepted in both positions, including an empty literal.
#[test]
fn test_list_local_binds_to_declared_hash_parameter() {
    expect_no_error(&hashes(
        "function run(): void { $ids = [4, 5]; put($ids); echo read($ids), read([1, 2]); \
         $empty = []; put($empty); echo read($empty); } run();",
    ));
}

/// A declared `array<T>` local keeps its list storage, so the reference is refused.
#[test]
fn test_error_declared_list_local_cannot_bind_hash_reference() {
    expect_error(
        &hashes("function run(): void { array<int> $ids = [4, 5]; put($ids); } run();"),
        BY_REFERENCE,
    );
}

/// A property, an element, an aliased local and a global are not promotable places.
#[test]
fn test_error_shared_list_storage_cannot_bind_hash_reference() {
    expect_error(
        &hashes("class H { public array<int> $ids = [4, 5]; } \
                 function run(): void { $h = new H(); put($h->ids); } run();"),
        BY_REFERENCE,
    );
    expect_error(
        &hashes("function run(): void { $rows = [[4, 5]]; put($rows[0]); } run();"),
        BY_REFERENCE,
    );
    expect_error(
        &hashes("function run(): void { $ids = [4, 5]; $alias = &$ids; put($ids); echo count($alias); } run();"),
        BY_REFERENCE,
    );
    expect_error(
        &hashes("$g = [4, 5]; function run(): void { global $g; put($g); } run();"),
        BY_REFERENCE,
    );
}

/// One variable can fill two hash reference slots of one call (both alias the promoted local),
/// but cannot become a hash while an earlier `mixed &` slot of that call is boxing it.
#[test]
fn test_error_list_local_boxed_by_an_earlier_slot_cannot_bind_hash_reference() {
    expect_no_error(
        "<?php function two(array<int, int> &$a, array<int, int> &$b): void { $a[5] = 1; $b[6] = 2; } \
         function run(): void { $ids = [4, 5]; two($ids, $ids); } run();",
    );
    expect_error(
        "<?php function two(mixed &$a, array<int, int> &$b): void { $b[6] = 2; } \
         function run(): void { $ids = [4, 5]; two($ids, $ids); } run();",
        BY_REFERENCE,
    );
}

/// A list first bound inside a loop type-checks, for hash and `array<mixed>` parameters alike.
#[test]
fn test_list_local_binds_to_reference_parameters_inside_loops() {
    expect_no_error(&hashes(
        "function run(): void { $ids = [4, 5]; foreach ([1, 2] as $k) { put($ids); echo read($ids); } } run();",
    ));
    expect_no_error(
        "<?php function add(array<mixed> &$a): void { $a[] = 'x'; } \
         function run(): void { $ids = [4, 5]; for ($i = 0; $i < 2; $i++) { add($ids); } echo count($ids); } run();",
    );
}

/// A written `array<mixed> &` parameter is a contract: a declared `array<int>` variable and a
/// typed list property keep their element storage, so the reference is refused.
#[test]
fn test_error_typed_list_storage_cannot_bind_array_mixed_reference() {
    let add = "<?php function add(array<mixed> &$a): void { $a[] = 'x'; } ";
    expect_no_error(&format!("{add}function run(): void {{ $ids = [4, 5]; add($ids); echo count($ids); }} run();"));
    expect_no_error(&format!(
        "{add}function run(): void {{ array<mixed> $any = [4, 'b']; add($any); echo count($any); }} run();"
    ));
    expect_error(
        &format!("{add}function run(): void {{ array<int> $ids = [4, 5]; add($ids); }} run();"),
        BY_REFERENCE,
    );
}

/// Elements with no conversion to the parameter's values are refused in both positions.
#[test]
fn test_error_list_elements_without_hash_conversion_are_refused() {
    expect_error(
        "<?php function put(array<int, float> &$m): void { $m[9] = 1.5; } \
         function run(): void { $ids = [4, 5]; put($ids); } run();",
        BY_REFERENCE,
    );
    expect_error(
        "<?php function read(array<int, float> $m): int { return count($m); } \
         function run(): void { $ids = [4, 5]; echo read($ids); } run();",
        "the element storage differs",
    );
    expect_error(
        "<?php function read(array<int, array<int, int>> $m): int { return count($m); } \
         function run(): void { $rows = [[4, 5]]; echo read($rows); } run();",
        "the element storage differs",
    );
    expect_error(
        "<?php class R { public function read(array<int, array<int, int>> $m): int { return count($m); } } \
         function run(): void { $rows = [[4, 5]]; echo (new R())->read($rows); } run();",
        "the element storage differs",
    );
    expect_error(
        "<?php function collect(array<int, array<int, int>> ...$maps): int { return count($maps); } \
         function run(): void { $rows = [[4, 5]]; echo collect($rows); } run();",
        "the element storage differs",
    );
}
