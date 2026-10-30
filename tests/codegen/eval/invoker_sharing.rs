//! Purpose:
//! Pins that eval callable invokers are emitted once per distinct body: callees of the same
//! shape share one invoker, whatever their names, bodies or declaration sites. Parameter names
//! are part of the shape: the invoker carries them as strings.
//!
//! Called from:
//! - `cargo test --test codegen_tests codegen::eval::invoker_sharing` through Rust's test harness.
//!
//! Key details:
//! - The counts compare two programs that differ only by the added callees, so the builtin
//!   surface every runtime-sourced eval carries cancels out.

use crate::support::*;

/// Compiles one fixture to assembly and returns the user assembly text.
fn user_asm(source: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "elephc_invoker_sharing_{}_{}",
        std::process::id(),
        TEST_ID.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    let (asm, _, _) = compile_source_to_asm_with_options(source, &dir, 8_388_608, false, false);
    let _ = fs::remove_dir_all(&dir);
    asm
}

/// Returns the eval callable invokers the assembly defines, each name once per definition.
fn invoker_definitions(asm: &str) -> Vec<&str> {
    asm.lines()
        .filter_map(|line| line.trim().strip_suffix(':'))
        .map(|label| label.trim_start_matches('_'))
        .filter(|label| {
            label.strip_prefix("elephc_eval_callable_invoker_")
                .is_some_and(|id| id.bytes().all(|byte| byte.is_ascii_digit()))
        })
        .collect()
}

const OPAQUE_EVAL: &str = "$code = $argc > 5 ? 'echo 1;' : 'echo 2;';\neval($code);\n";

/// Six methods with one signature add at most one invoker, and every invoker is defined once.
#[test]
fn test_eval_invokers_are_shared_between_callees_of_one_shape() {
    let base = user_asm(&format!("<?php\nclass SharedShape {{}}\n{OPAQUE_EVAL}"));
    let methods = (0..6)
        .map(|index| format!("    public function m{index}(int $value): int {{ return $value + {index}; }}\n"))
        .collect::<String>();
    let shared = user_asm(&format!("<?php\nclass SharedShape {{\n{methods}}}\n{OPAQUE_EVAL}"));
    let base_invokers = invoker_definitions(&base);
    let shared_invokers = invoker_definitions(&shared);
    assert!(
        shared_invokers.len() <= base_invokers.len() + 1,
        "six same-shape methods added {} invokers",
        shared_invokers.len() - base_invokers.len()
    );
    let mut unique = shared_invokers.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), shared_invokers.len(), "an invoker label is defined twice");
}

/// Methods sharing one invoker still reach their own bodies when eval calls them.
#[test]
fn test_eval_calls_through_a_shared_invoker_reach_each_callee() {
    let output = compile_and_run(r#"<?php
class SharedShapeRun {
    public function first(int $a): int { return $a + 1; }
    public function second(int $b): int { return $b * 10; }
    public function third(int $c = 7): int { return $c - 2; }
}
$code = $argc > 0 ? '
    $o = new SharedShapeRun();
    echo $o->first(1), ",", $o->second(2), ",", $o->third(), ",", $o->third(4), "\n";
    $f = [$o, "second"];
    echo $f(3), ",", call_user_func([$o, "first"], 5), "\n";
' : '';
eval($code);
"#);
    assert_eq!(output, "2,20,5,2\n30,6\n");
}
