//! Purpose:
//! EIR tests for claiming transient string arguments before user calls that may return them.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - A concat argument to a callee whose result may be that argument is wrapped in
//!   `EnsureOwned`, and its post-call release is decided at runtime against the result.
//! - Callees that return independent storage keep the rooted `Acquire` path unchanged.
//! - The runtime alias comparison of a string reads the string pointer register on every target.

use crate::ir::{Function, Instruction, Op, ValueDef, ValueId};

/// Returns the instruction that defines `value`, if it is an instruction result.
fn producer(function: &Function, value: ValueId) -> Option<&Instruction> {
    match function.value(value)?.def {
        ValueDef::Instruction { inst, .. } => function.instruction(inst),
        _ => None,
    }
}

/// A may-return callee receives a claimed concat argument whose release compares the result.
#[test]
fn may_return_callee_arguments_are_claimed_and_released_unless_returned() {
    let source = r#"<?php
function id(string $s): string { return $s; }
function measure(string $s): int { return strlen($s); }
$x = id($argv[0] . "b");
$n = measure($argv[0] . "c");
echo $x, $n;
"#;
    for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let module = super::lower_source_at_for_target(
            source,
            std::path::Path::new("main.php"),
            std::path::Path::new("."),
            crate::codegen::platform::Target::parse(target).unwrap(),
        );
        let main = module.functions.iter().find(|function| function.name == "main").unwrap();
        let calls: Vec<&Instruction> =
            main.instructions.iter().filter(|inst| inst.op == Op::Call).collect();
        assert_eq!(calls.len(), 2, "{target}");

        let passthrough = calls[0];
        let claimed = passthrough.operands[0];
        let claim = producer(main, claimed).expect("claimed argument has a producer");
        assert_eq!(claim.op, Op::EnsureOwned, "{target}");
        assert_eq!(producer(main, claim.operands[0]).map(|inst| inst.op), Some(Op::StrConcat), "{target}");
        let result = passthrough.result.unwrap();
        assert!(
            main.instructions.iter().any(|inst| {
                inst.op == Op::ReleaseUnlessAliases && inst.operands == [claimed, result]
            }),
            "{target}: the claimed argument is released unless the call returned it"
        );

        let independent = calls[1];
        assert_ne!(
            producer(main, independent.operands[0]).map(|inst| inst.op),
            Some(Op::EnsureOwned),
            "{target}: a callee with an independent result keeps the rooted argument path"
        );

        let asm = crate::codegen::generate_user_asm_from_ir(&module, false, false).unwrap();
        assert!(asm.contains("__rt_str_claim"), "{target}");
        let compare = if target == "linux-x86_64" { "cmp rax, " } else { "cmp x1, " };
        let lines: Vec<&str> = asm.lines().collect();
        let checks: Vec<&str> = lines
            .windows(2)
            .filter(|pair| {
                pair[1].contains("release_unless_aliases_skip")
                    && (pair[1].trim_start().starts_with("b.eq") || pair[1].trim_start().starts_with("je"))
            })
            .map(|pair| pair[0].trim_start())
            .collect();
        assert!(!checks.is_empty(), "{target}: no runtime alias check emitted");
        assert!(
            checks.iter().all(|line| line.starts_with(compare)),
            "{target}: string alias check must compare the string pointer register: {checks:?}"
        );
    }
}
