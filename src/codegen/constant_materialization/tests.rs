//! Purpose:
//! Verifies scalar constant reconstruction and immediate instruction emission.
//!
//! Called from:
//! - Library tests through `codegen::constant_materialization`.
//!
//! Key details:
//! - Generates every supported target from hand-built EIR, bypassing folding.
//! - Checks legal forms, full-width fallback, reversed comparisons, and float bits.

use super::*;
use crate::codegen::generate_user_asm_from_ir;
use crate::codegen::platform::Target;
use crate::ir::{Builder, CmpPredicate, Function, Immediate, IrType, LocalKind, Module, Op, Ownership, Terminator};
use crate::types::PhpType;

/// Emits one integer operation with an unknown operand and a chosen constant side.
fn operation_asm(target: Target, op: Op, constant: i64, left: bool) -> String {
    let mut module = Module::new(target);
    let mut function = Function::new("main".into(), IrType::I64, PhpType::Int);
    function.flags.is_main = true;
    {
        let mut b = Builder::new(&mut function);
        let entry = b.create_named_block("entry", vec![]);
        b.set_entry(entry);
        b.position_at_end(entry);
        let slot = b.add_local(None, IrType::I64, PhpType::Int, LocalKind::PhpLocal);
        let input = b.emit_load_local(slot, IrType::I64, PhpType::Int);
        let number = b.emit_const_i64(constant);
        let args = if left { vec![number, input] } else { vec![input, number] };
        let immediate = match op {
            Op::ICmp => Some(Immediate::CmpPredicate(CmpPredicate::Slt)),
            Op::ICheckedAddToInt | Op::ICheckedSubToInt | Op::ICheckedMulToInt => Some(Immediate::Bool(true)),
            _ => None,
        };
        let result = b.emit(op, args, immediate, IrType::I64,
            if op == Op::ICmp { PhpType::Bool } else { PhpType::Int }, Ownership::NonHeap).unwrap();
        b.terminate(Terminator::Return { value: Some(result) });
    }
    module.add_function(function);
    generate_user_asm_from_ir(&module, false, false).unwrap()
}

/// Returns true when some instruction matches `pattern`, where `x*` stands for any one
/// 64-bit AArch64 register and `r*` for any x86_64 register: allocated operands are read in
/// place instead of being copied into the result register first.
fn has_instruction(asm: &str, pattern: &str) -> bool {
    asm.lines().map(str::trim).any(|line| matches_pattern(line, pattern))
}

/// Matches one instruction line against a pattern with register wildcards.
fn matches_pattern(line: &str, pattern: &str) -> bool {
    let wildcard = [pattern.find("x*"), pattern.find("r*")].into_iter().flatten().min();
    let Some(at) = wildcard else {
        return line == pattern;
    };
    let (head, tail) = (&pattern[..at], &pattern[at + 2..]);
    let first = &pattern[at..at + 1];
    let Some(rest) = line.strip_prefix(head).and_then(|rest| rest.strip_prefix(first)) else {
        return false;
    };
    let register = rest.bytes().take_while(u8::is_ascii_alphanumeric).count();
    (first == "r" || rest.bytes().take(register).all(|b| b.is_ascii_digit()))
        && register > 0
        && matches_pattern(&rest[register..], tail)
}

/// Checks arithmetic and mask immediates without a standalone constant definition.
#[test]
fn rematerialize_integer_assembly_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let arm = target.arch == Arch::AArch64;
        for left in [false, true] {
            let asm = operation_asm(target, Op::IAdd, 4096, left);
            assert!(if arm { has_instruction(&asm, "add x0, x*, #1, lsl #12") } else { asm.contains("add rax, 4096") }, "{name}: {asm}");
            assert!(!asm.contains(if arm { "mov x0, #4096" } else { "mov rax, 4096" }), "{asm}");
            let asm = operation_asm(target, Op::IBitAnd, 255, left);
            assert!(if arm { has_instruction(&asm, "and x0, x*, #0xff") } else { asm.contains("and rax, 255") }, "{name}: {asm}");
        }
        for (op, arm_form, x86_form) in [
            (Op::ISub, "add x0, x*, #7", "sub rax, -7"),
            (Op::IBitOr, "orr x0, x*, #0xfffffffffffffff9", "or rax, -7"),
        ] {
            let asm = operation_asm(target, op, -7, false);
            assert!(if arm { has_instruction(&asm, arm_form) } else { asm.contains(x86_form) }, "{asm}");
        }
        let asm = operation_asm(target, Op::IBitXor, 0x00ff_00ff_00ff_00ff, false);
        assert!(if arm { has_instruction(&asm, "eor x0, x*, #0xff00ff00ff00ff") } else { asm.contains("xor rax, r10") }, "{name}: {asm}");
        let asm = operation_asm(target, Op::IAdd, i64::MAX, false);
        assert!(if arm { has_instruction(&asm, "add x0, x*, x10") } else { asm.contains("add rax, r10") }, "{name}: {asm}");
        // Profitable constant multiplies are strength-reduced; the rest keep the multiplier.
        let asm = operation_asm(target, Op::IMul, 3, false);
        assert!(asm.contains(if arm { "add x0, x0, x0, lsl #1" } else { "lea rax, [rax + rax*2]" }), "{name}: {asm}");
        let asm = operation_asm(target, Op::IMul, 11, false);
        assert!(if arm { has_instruction(&asm, "mul x0, x*, x10") } else { asm.contains("imul rax, 11") }, "{name}: {asm}");
    }
}

/// Left constants reverse the condition, and negative bounds use a legal signed comparison.
#[test]
fn rematerialize_comparison_assembly_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let arm = target.arch == Arch::AArch64;
        let asm = operation_asm(target, Op::ICmp, -4096, true);
        assert!(has_instruction(&asm, if arm { "cmn x*, #1, lsl #12" } else { "cmp r*, -4096" }), "{name}: {asm}");
        assert!(asm.contains(if arm { "cset x0, gt" } else { "setg al" }), "{name}: {asm}");
        for n in [i64::MIN, i64::MAX, 0xffff_ffff] {
            let asm = operation_asm(target, Op::ICmp, n, false);
            assert!(has_instruction(&asm, if arm { "cmp x*, x10" } else { "cmp r*, r10" }), "{name}, {n}: {asm}");
        }
    }
}

/// Checked fast paths use flags, with constant reconstruction confined to overflow conversion.
#[test]
fn rematerialize_checked_immediate_keeps_overflow_path() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let arm = target.arch == Arch::AArch64;
        for op in [Op::ICheckedAddToInt, Op::ICheckedSubToInt, Op::ICheckedMulToInt] {
            let asm = operation_asm(target, op, 3, false);
            let expected = match (arm, op) {
                (true, Op::ICheckedAddToInt) => "adds x0, x0, #3",
                (true, Op::ICheckedSubToInt) => "subs x0, x0, #3",
                (true, _) => "smulh x9, x0, x10",
                (false, Op::ICheckedAddToInt) => "add rax, 3",
                (false, Op::ICheckedSubToInt) => "sub rax, 3",
                (false, _) => "imul rax, 3",
            };
            assert!(asm.contains(expected), "{name}: {asm}");
            assert!(asm.contains("__rt_php_float_to_int"), "{asm}");
            if !arm || op != Op::ICheckedMulToInt {
                let compute = asm.find(expected).unwrap();
                let reconstruct = asm.find(if arm { "mov x10, #3" } else { "mov r10, 3" }).unwrap();
                assert!(compute < reconstruct, "{name}: {asm}");
            }
        }
    }
}

/// Immediate and register forms use one not-taken overflow branch into the cold tail.
#[test]
fn rematerialize_checked_arithmetic_has_one_fast_path_branch() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for op in [Op::ICheckedAddToInt, Op::ICheckedSubToInt, Op::ICheckedMulToInt] {
            for constant in [3, -3, 0x1234_5678_9abc_def0] {
                let asm = operation_asm(target, op, constant, false);
                let enter = match (target.arch, op) {
                    (Arch::AArch64, Op::ICheckedMulToInt) => "b.ne ",
                    (Arch::AArch64, _) => "b.vs ",
                    (Arch::X86_64, _) => "jo ",
                };
                let branches: Vec<_> = asm.lines().map(str::trim).filter(|line| {
                    line.contains("checked_int_to_int_") && !line.ends_with(':')
                }).collect();
                assert_eq!(branches.len(), 2, "{name}: {asm}");
                assert!(branches[0].starts_with(enter), "{name}: {asm}");
                assert!(branches[0].contains("checked_int_to_int_overflow"), "{name}: {asm}");
                assert!(branches[1].contains("checked_int_to_int_done"), "{name}: {asm}");
                let label = |prefix: &str| asm.lines().position(|line| {
                    line.contains(prefix) && line.ends_with(':')
                }).unwrap();
                let convert = asm.lines().position(|line| line.contains("__rt_php_float_to_int")).unwrap();
                // The in-range result falls through `done`; the cold conversion lives after it.
                assert!(label("checked_int_to_int_done") < label("checked_int_to_int_overflow"), "{name}: {asm}");
                assert!(label("checked_int_to_int_overflow") < convert, "{name}: {asm}");
            }
        }
    }
}

/// Float recipes preserve signed zero and NaN payloads without clobbering x86 integer inputs.
#[test]
fn rematerialize_float_payloads_and_scratch_contract() {
    for name in ["linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        for bits in [0, 1 << 63, 0x7ff8_0000_0000_1234] {
            let mut emitter = Emitter::new(target);
            let mut data = DataSection::new();
            let float_reg = abi::float_result_reg(&emitter);
            emit_constant(&mut emitter, &mut data, ScalarConstant::Float(bits), float_reg);
            let asm = emitter.output();
            if target.arch == Arch::X86_64 {
                assert!(asm.contains("movsd xmm0, QWORD PTR [rip + _float_"), "{asm}");
                assert!(!asm.contains("r11"), "{asm}");
                assert!(data.emit(target).contains(&format!("0x{bits:016x}")));
            } else {
                assert!(asm.contains("fmov d0, x9"), "{asm}");
            }
        }
    }
    let target = Target::parse("linux-x86_64").unwrap();
    let mut emitter = Emitter::new_pic(target);
    emit_constant(&mut emitter, &mut DataSection::new(), ScalarConstant::Float(1 << 63), "xmm1");
    let asm = emitter.output();
    assert!(asm.contains("@GOTPCREL"), "{asm}");
    assert!(asm.contains("push r11") && asm.contains("pop r11"), "{asm}");
}
