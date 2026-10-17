//! Purpose:
//! Validates write-only append lowering and assembly emission on every supported target.
//!
//! Called from:
//! - The EIR lowering unit test harness.
//!
//! Key details:
//! - Each target has an independently scheduled test with the same source fixture.
//! - Existing array-push operations provide target-aware COW and next-key handling.

/// Runs the append regression fixture through target-specific EIR and assembly generation.
fn verify_append_write_target(target: &str) {
    let source = r#"<?php
class Box { public array $items = []; public static array $shared = []; }
$items = [];
echo ($items[] += $argc);
$items[]['k'] .= 'x';
$items[][] = $argc;
$items[]++;
echo ++$items[];
$items[][1] = 'sparse';
$items[][1] .= 'compound';
$items[][$argc] = [7];
$items[][-1] = 'negative';
$items[][1.5] = 'float';
$items[][true] = 'boolean';
$items[][null] = 'null';
$negative = -$argc;
$sparse = []; $sparse[$negative] = [8];
echo json_encode($sparse);
try { ++$items[]->x; } catch (Error $error) { echo $error->getMessage(); }
$nested = [[]];
$nested[0][] += $argc;
$nested[0][]['k'] = 'v';
$groups = [];
$groups['k'][] = $argc;
$groups['j'][] += $argc;
echo count($groups['k']), count($groups['j']);
$box = new Box();
$box->items[] += $argc;
Box::$shared[] += $argc;
echo ++$box->items[], ++Box::$shared[];
$append = fn() => ($box->items[] += 2);
echo $append();
function writeStringKey(&...$values): void {
    $values["1"] = 'Q'; echo strlen($values[1]), $values[0], $values[1];
}
$a = 'A'; $b = 'B'; writeStringKey($a, $b); echo $b;
function makeParentName(string $suffix): string { return 'k' . $suffix; }
function writeCopiedKeys(string $suffix, &...$values): void {
    $copy = $values; $copy["1"] = 'Q'; echo $copy[0], $copy[1];
    $values["key"]["x"] = 1; $values["key"]["y"] = 2;
    $values["group"][] = 3; $values["group"][] = 4;
    $values[makeParentName($suffix)]["z"] = 5;
    echo $values[0], $values[1], json_encode($values);
}
$copied = writeCopiedKeys(...); $copied('ey', $a, $b);
function writeConsumingParent(&...$values): void { $values["key"]["x"] = 1; }
$consuming = writeConsumingParent(...); $consuming($a, $b);
$plain = ['A', 'B', 'C']; $plain["key"]["x"] = 1;
echo json_encode($plain);
function writeTypedParents(string $key, mixed $boxed): void {
    $strings = ['A', 'B']; $strings[$key]['x'] = 1;
    $strings[makeParentName('ey')]['y'] = 2;
    $ints = [5, 7]; $ints[$boxed]['x'] = 3;
    echo json_encode($strings), json_encode($ints);
}
writeTypedParents('key', 'key');
function writeNumericParents(string $key): void {
    $strings = ['A', 'B']; $copy = $strings;
    try { $strings[$key]['x'] = 1; } catch (TypeError $error) { echo $error->getMessage(); }
    try { $strings[$key][] = 1; } catch (Error $error) { echo $error->getMessage(); }
    $ints = [5, 7];
    try { $ints[$key]['x'] = 2; } catch (Error $error) { echo $error->getMessage(); }
    echo json_encode($strings), json_encode($copy), json_encode($ints);
}
writeNumericParents('1');
function writeScalarChildren(string $key, mixed $inner): void {
    $strings = ['AB', 'CD'];
    try { $strings[$key][$inner] = 'Z'; } catch (TypeError $error) { echo $error->getMessage(); }
    $false = [false, 'q']; $false[$key]['x'] = 1;
    echo json_encode($strings), json_encode($false);
}
writeScalarChildren('0', '1abc');
class OffsetText { public function __toString(): string { echo 'S'; return ''; } }
function writeSeventhReview(mixed $inner): void {
    $strings = ['AB'];
    try { $strings[0][-3] = new OffsetText(); } catch (Error $error) { echo $error->getMessage(); }
    $false = [false];
    set_error_handler(function($level, $message) { throw new Error('handler'); });
    try { $false[0][$inner] = 1; }
    catch (Throwable $error) { echo $error->getMessage(), json_encode($false); }
    restore_error_handler();
}
writeSeventhReview($argc);
function hashReturn(string $key, mixed $value): array {
    $items = [$value]; $items[$key] = $value; return $items;
}
echo json_encode(hashReturn('key', $argc));
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let main = module.functions.iter().find(|function| function.flags.is_main).unwrap();
    assert!(main.instructions.iter().any(|inst| inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::MixedArrayCellHasLiveOwner))),
        "{target}: pending compound appends query their selected cell without another retain");
    assert!(main.instructions.iter().any(|inst| inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::PendingAppendFetchForWrite))),
        "{target}: pending compound appends preserve the pre-callback parent selection");
    assert!(main.blocks.iter().any(|block| block.name.starts_with("append.update.cancel")),
        "{target}: detached parents bypass leaf materialization and the operator");
    let returning = module.functions.iter().find(|function| function.name == "hashReturn")
        .expect("missing hash-return fixture");
    assert!(returning.blocks.iter().flat_map(|block| &block.instructions)
        .any(|inst| returning.instruction(*inst).is_some_and(|inst| inst.op == crate::ir::Op::ArrayToHash)),
        "ordinary String-key writes must retain their associative storage contract");
    let guarded = module.functions.iter().find(|function| function.name == "writeNumericParents")
        .expect("missing scalar-parent fixture");
    assert!(guarded.blocks.iter().flat_map(|block| &block.instructions)
        .any(|id| guarded.instruction(*id).is_some_and(|inst| inst.op == crate::ir::Op::TypePredicate
            && inst.immediate == Some(crate::ir::Immediate::TypePredicate(crate::ir::PhpTypePredicate::String)))),
        "{target}: selected boxed strings must be guarded before nested writes");
    let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
    let selected_owner_count = if target == "linux-x86_64" {
        "cmp r11, 3"
    } else {
        "cmp x9, #3"
    };
    assert!(asm.contains(selected_owner_count),
        "{target}: cell and payload counts must leave exactly one PHP owner");
    let scalar = module.functions.iter().find(|function| function.name == "writeScalarChildren").unwrap();
    assert!(scalar.instructions.iter().any(|inst| inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::StringWriteOffset))), "{target}: runtime string offsets are normalized");
    assert!(scalar.instructions.iter().any(|inst| inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::WriteDiagnostic))), "{target}: false conversion uses PHP's diagnostic channel");
    let reviewed = module.functions.iter().find(|function| function.name == "writeSeventhReview").unwrap();
    assert!(reviewed.instructions.iter().any(|inst| inst.op == crate::ir::Op::CatchBind),
        "{target}: a throwing deprecation handler must be deferred until its keyed write completes");
    assert!(reviewed.instructions.iter().any(|inst| inst.immediate == Some(crate::ir::Immediate::RuntimeCall(
        crate::ir::RuntimeCallTarget::ExceptionChain))), "{target}: invalid offsets chain the pending handler error");
    let nested = asm.split("@fn name=writeConsumingParent ").nth(1)
        .expect("missing reference-variadic assembly body")
        .split("@endfn name=writeConsumingParent").next().unwrap();
    assert!(!nested.contains("ref_cell_store_same_pointer"),
        "{target}: consuming parent fetches publish without retiring their old owner twice");
}

/// Keeps all target cases separate so one fixture does not accumulate matrix timeouts.
macro_rules! append_target_test {
    ($name:ident, $target:literal) => {
        /// Verifies append assignments and updates emit on this supported target.
        #[test]
        fn $name() { verify_append_write_target($target); }
    };
}

append_target_test!(append_review_macos, "macos-aarch64");
append_target_test!(append_review_ios, "ios-arm64");
append_target_test!(append_review_ios_sim, "ios-sim-arm64");
append_target_test!(append_review_linux_arm, "linux-aarch64");
append_target_test!(append_review_linux_x86, "linux-x86_64");
