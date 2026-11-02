//! Purpose:
//! Checks target parity for associative array_push lowering and receiver publication.
//!
//! Called from:
//! - The ir_lower unit test harness.
//!
//! Key details:
//! - Every supported target must emit the shared hash append and count paths.
//! - Runtime string-key canonicalization must lower through target-neutral cast and comparison ops.

/// Typed hashes, aliases, runtime unpacks and Mixed entries emit on every supported target.
#[test]
fn associative_array_push_emits_on_every_supported_target() {
    let source = r#"<?php
$values = ["a" => 1];
$copy = $values;
echo array_push($values, $argc, 2), array_push(array: $values);
$alias =& $values;
array_push($alias, ...[3, 4]);
$mixed = ["a" => 1, "b" => "s"];
array_push($mixed, false, null, [1]);
$tail = [$argc, 5];
array_push($mixed, ...$tail);
function targetNumericStringKeys(): Generator { yield "10" => 6; }
array_push($mixed, ...targetNumericStringKeys());
$indexed = [1, 2];
array_push($indexed, ...targetNumericStringKeys());
class TargetPushBox { public array $items = ["a" => 1]; }
$box = new TargetPushBox();
array_push($box->items, ...$tail);
$held = $box;
array_push($box->items, (function () use (&$box) { $box = new TargetPushBox(); return 2; })());
$box->items = ["a" => 1, PHP_INT_MAX - 1 => 2];
try { array_push($box->items, 3, 4); } catch (Error $e) { echo $e->getMessage(); }
class TargetReadonlyPushBox {
    public readonly array $items;
    public function __construct() { $this->items = ["a" => 1]; }
}
$readonly = new TargetReadonlyPushBox();
try { array_push($readonly->items, 2); } catch (Error $e) { echo $e->getMessage(); }
class TargetReferencePushBox {
    public array $backing = ["a" => 1];
    public private(set) array $items {
        &get { return $this->backing; }
        set { $this->backing = $value; }
    }
}
$reference = new TargetReferencePushBox();
array_push($reference->items, 2);
$rows = ["inner" => ["a" => 1]];
array_push($rows["inner"], 2);
class TargetNativePushBox { public $items = ["a" => 1]; }
$native = new TargetNativePushBox();
array_push($native->items, 2);
$referenceRows = [["a" => 1]];
$rowsAlias =& $referenceRows;
array_push($referenceRows[0], 2);
$captured = [1];
$readCaptured = function () use (&$captured) { return json_encode($captured); };
array_push($captured, 2);
echo $readCaptured();
$capturedRows = [[1]];
$writeCapturedRows = function () use (&$capturedRows) { array_push($capturedRows[0], 2); };
$writeCapturedRows();
echo $capturedRows[0][0], $capturedRows[0][1];
function targetReturnedPushRows() {
    $rows = [[1]];
    array_push($rows[0], 2);
    return $rows;
}
echo targetReturnedPushRows()[0][0];
function targetLocalPushPage(int $n, int $start): buffer<int> {
    $page = buffer_new<int>($n);
    for ($i = 0; $i < $n; $i++) { $page[$i] = $start + $i; }
    return $page;
}
class TargetBufferPushPool { public array<buffer<int> > $pages = []; }
$bufferPool = new TargetBufferPushPool();
array_push($bufferPool->pages, targetLocalPushPage(2, 4));
foreach ($bufferPool->pages as $page) { buffer_free($page); }
echo json_encode($values), json_encode($copy), json_encode($mixed);
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
            .unwrap_or_else(|error| panic!("{target}: {error:?}"));
        assert!(asm.contains("__rt_hash_next_index"), "{target}: no associative append");
        assert!(asm.contains("__rt_hash_set"), "{target}: no hash insertion");
    }
}
