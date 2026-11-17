//! Purpose:
//! Checks frame address arithmetic and encoding size across supported targets.
//!
//! Called from:
//! - The shared ABI unit test module.
//!
//! Key details:
//! - Large offsets must stay exact without thousands of immediate instructions.

use super::*;

/// Decodes the restricted AArch64 address sequence without executing generated assembly.
fn decoded_frame_displacement(assembly: &str) -> usize {
    let mut total = 0;
    for (index, instruction) in assembly.lines().map(str::trim).enumerate() {
        if instruction == "mov x9, x29" {
            assert_eq!(index, 0);
            continue;
        }
        let operands: Vec<_> = instruction.strip_prefix("sub ").unwrap().split(", ").collect();
        assert_eq!(operands[0], "x9");
        assert_eq!(operands[1], if index == 0 { "x29" } else { "x9" });
        let immediate = operands[2].strip_prefix('#').unwrap().parse::<usize>().unwrap();
        assert!((1..=4095).contains(&immediate), "{instruction}");
        let shift = match operands.get(3) {
            None => 0,
            Some(&"lsl #12") => 12,
            other => panic!("invalid ADD/SUB immediate shift: {other:?}"),
        };
        assert_eq!(operands.len(), if shift == 0 { 3 } else { 4 });
        total += immediate << shift;
    }
    total
}

/// Far slots preserve their address on every target with bounded ARM64 instruction counts.
#[test]
fn test_frame_slot_addresses_are_exact_and_compact_on_supported_targets() {
    let offsets = [0, 16, 4095, 4096, 4097, 5000, 90_000, 4 << 20, (16 << 20) + 4095, (1 << 30) + 16];
    for target_name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(target_name).unwrap();
        for offset in offsets {
            let mut emitter = Emitter::new(target);
            let register = if target.arch == Arch::AArch64 { "x9" } else { "rax" };
            emit_frame_slot_address(&mut emitter, register, offset);
            let assembly = emitter.output();
            if target.arch == Arch::AArch64 {
                assert_eq!(decoded_frame_displacement(&assembly), offset, "{target_name}: {assembly}");
                let limit = offset.div_ceil(4095 << 12).max(1) + usize::from(offset > 4095 && offset & 4095 != 0);
                assert!(assembly.lines().count() <= limit, "{target_name}: {assembly}");
            } else {
                let expected = if offset == 0 { "    mov rax, rbp\n".into() }
                    else { format!("    lea rax, [rbp - {offset}]\n") };
                assert_eq!(assembly, expected, "{target_name}");
            }
        }
    }
}
