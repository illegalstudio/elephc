//! Purpose:
//! Calls the read-only Mixed add/sub/mul intrinsics with a borrowed integer operand cell.
//! Keeps a temporary integer off the managed heap without changing the result representation.
//!
//! Called from:
//! - `crate::codegen::lower_inst::arithmetic::lower_mixed_numeric_binop()` for constants.
//! - `crate::codegen_support::runtime::strings::str_inc_dec` for increment/decrement deltas.
//!
//! Key details:
//! - The left operand is a borrowed Mixed pointer; the right operand enters as a raw I64.
//! - Only the three intrinsics below may read this non-owning, headerless stack cell.
//! - They read/cast the integer payload without retaining or exposing its address and return
//!   a fresh owned heap cell. Releasing, storing or returning the operand pointer is invalid.
//! - The 24-byte cell occupies a 32-byte scratch span aligned to 16 bytes, ending after the call.

use crate::codegen_support::{emit::Emitter, platform::Arch};
use crate::ir::MixedNumericOp;

use super::{
    emit_call_label, emit_release_temporary_stack, emit_reserve_temporary_stack,
    emit_store_to_sp, emit_temporary_stack_address,
};

/// Calls a nonescaping numeric intrinsic with an integer RHS in the normal second operand register.
///
/// Inputs are `x0 = left Mixed*, x1 = integer` on AArch64 and `rax = left Mixed*,
/// rdi = integer` on x86_64. The left owner remains with the caller. The returned
/// `x0`/`rax` is the same fresh owned Mixed result as an ordinary boxed call.
/// Unsupported operations return `false` without emitting anything.
#[must_use]
pub fn emit_mixed_numeric_integer_rhs_call(emitter: &mut Emitter, op: MixedNumericOp) -> bool {
    let helper = match op {
        MixedNumericOp::Add => "__rt_mixed_numeric_add",
        MixedNumericOp::Sub => "__rt_mixed_numeric_sub",
        MixedNumericOp::Mul => "__rt_mixed_numeric_mul",
        _ => return false,
    };
    let rhs = match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    };

    // -- lend an integer cell only to the audited read-only numeric intrinsic --
    emit_reserve_temporary_stack(emitter, 32);
    emit_store_to_sp(emitter, rhs, 8);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str xzr, [sp]");                               // tag zero identifies the borrowed integer payload
            emitter.instruction("str xzr, [sp, #16]");                          // initialize the unused high word before unboxing can read it
        }
        Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rsp], 0");                      // tag zero identifies the borrowed integer payload
            emitter.instruction("mov QWORD PTR [rsp + 16], 0");                 // initialize the unused high word before unboxing can read it
        }
    }
    emit_temporary_stack_address(emitter, rhs, 0);
    emit_call_label(emitter, helper);
    emit_release_temporary_stack(emitter, 32);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::{platform::Target, RuntimeFeatures};
    use std::{fs, path::PathBuf, process::Command, time::{SystemTime, UNIX_EPOCH}};

    /// Owns only the generated assembly and objects of one cross-assembly run.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        /// Removes the test-owned directory on success or assertion failure.
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Unsupported arithmetic cannot borrow a cell through an unaudited consumer.
    #[test]
    fn borrowed_numeric_rejects_pow_without_emission() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            assert!(!emit_mixed_numeric_integer_rhs_call(&mut emitter, MixedNumericOp::Pow));
            assert!(emitter.output().is_empty(), "{name}");
        }
    }

    /// Assembles the call transaction with aligned frames on every target, direct and PIC.
    #[test]
    fn borrowed_numeric_call_assembles_on_all_supported_targets() {
        if Command::new("clang").arg("--version").output().is_err() {
            eprintln!("Clang unavailable; executable target CI provides assembly coverage");
            return;
        }
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = Scratch(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target")
            .join(format!("borrowed-numeric-{}-{nonce}", std::process::id())));
        fs::create_dir_all(&dir.0).unwrap();
        for (name, triple) in [
            ("macos-aarch64", "arm64-apple-macos11"), ("ios-arm64", "arm64-apple-ios13.0"),
            ("ios-sim-arm64", "arm64-apple-ios13.0-simulator"),
            ("linux-aarch64", "aarch64-linux-gnu"), ("linux-x86_64", "x86_64-linux-gnu"),
        ] {
            for pic in [false, true] {
                for op in [MixedNumericOp::Add, MixedNumericOp::Sub, MixedNumericOp::Mul] {
                    let target = Target::parse(name).unwrap();
                    let mut emitter = if pic { Emitter::new_pic(target) } else { Emitter::new(target) };
                    assert!(emit_mixed_numeric_integer_rhs_call(&mut emitter, op));
                    let body = emitter.output();
                    let (prefix, suffix) = if target.arch == Arch::AArch64 {
                        ("stp x29, x30, [sp, #-16]!\nmov x29, sp\n", "ldp x29, x30, [sp], #16\nret\n")
                    } else {
                        (".intel_syntax noprefix\npush rbp\nmov rbp, rsp\n", "pop rbp\nret\n")
                    };
                    let source = dir.0.join(format!("{name}-{pic}-{op:?}.s"));
                    fs::write(&source, format!(".text\n{prefix}{body}{suffix}")).unwrap();
                    let output = Command::new("clang").args(["-target", triple, "-c"]).arg(&source)
                        .arg("-o").arg(source.with_extension("o")).output().unwrap();
                    assert!(output.status.success(), "{name}, PIC {pic}, {op:?}: {}",
                        String::from_utf8_lossy(&output.stderr));
                }
            }
        }
    }

    /// Numeric consumers may not route borrowed operand pointers to retaining or escaping helpers.
    #[test]
    fn borrowed_numeric_consumers_keep_the_read_only_call_contract() {
        let allowed = ["__rt_mixed_unbox", "__rt_mixed_cast_int", "__rt_mixed_cast_float",
            "__rt_mixed_from_value", "__rt_mixed_str_operand_is_float"];
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for pic in [false, true] {
                let runtime = crate::codegen::generate_runtime_with_features_pic(
                    65_536, target, RuntimeFeatures::none(), pic);
                let (label, mnemonic) = if target.arch == Arch::AArch64 {
                    ("__rt_mixed_numeric_common:\n", "bl ")
                } else {
                    ("__rt_mixed_numeric_common_linux_x86_64:\n", "call ")
                };
                let (_, body) = runtime.split_once(label).expect("numeric common entry");
                let (body, _) = body.split_once("\n    ret\n").expect("numeric return");
                let calls: Vec<_> = body.lines().filter_map(|line| line.trim().strip_prefix(mnemonic)).collect();
                assert!(!calls.is_empty(), "{name}, PIC {pic}");
                for callee in calls {
                    assert!(allowed.contains(&callee),
                        "{name}, PIC {pic}: re-audit stack operand lifetime for {callee}");
                }
            }
        }
    }
}
