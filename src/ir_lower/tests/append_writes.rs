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
function hashReturn(string $key, mixed $value): array {
    $items = [$value]; $items[$key] = $value; return $items;
}
echo json_encode(hashReturn('key', $argc));
"#;
    let module = super::lower_source_at_for_target(
        source, std::path::Path::new("main.php"), std::path::Path::new("."),
        crate::codegen::platform::Target::parse(target).unwrap(),
    );
    let returning = module.functions.iter().find(|function| function.name == "hashReturn")
        .expect("missing hash-return fixture");
    assert!(returning.blocks.iter().flat_map(|block| &block.instructions)
        .any(|inst| returning.instruction(*inst).is_some_and(|inst| inst.op == crate::ir::Op::ArrayToHash)),
        "ordinary String-key writes must retain their associative storage contract");
    crate::codegen::generate_user_asm_from_ir(&module, false, false)
        .unwrap_or_else(|error| panic!("{target}: {error:?}"));
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
