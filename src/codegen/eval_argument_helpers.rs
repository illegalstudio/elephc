//! Purpose:
//! Borrows normalized eval argument cells for native method and constructor bridges.
//!
//! Called from:
//! - `super::eval_method_helpers` and `super::eval_constructor_helpers`.
//!
//! Key details:
//! - Magician owns a dense indexed array of Mixed cells throughout each native invocation.
//! - Reads allocate nothing and retain nothing; the enclosing argument array roots every borrow.
//! - Shape and bounds guards reject malformed bridge inputs before dereferencing an element.
//! - Consuming intrinsic helpers acquire separate owners after argument staging succeeds.

use crate::codegen::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::emit_branch_if_null_container;
use crate::intrinsics::IntrinsicCall;
use crate::types::PhpType;

/// Preserves the PHP array constraint while lowering all other bridge metadata to its ABI type.
pub(super) fn bridge_storage_type(ty: &PhpType) -> PhpType {
    if ty.is_php_array() { ty.clone() } else { ty.codegen_repr() }
}

/// Rejects non-array boxed inputs without transferring ownership or converting their storage.
/// The borrowed cell enters in the integer result register; callers reload it after the tag check.
pub(super) fn emit_require_php_array(emitter: &mut Emitter, fail_label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("bl __rt_mixed_unbox");                         // inspect the boxed argument without acquiring or consuming owners
            emitter.instruction("sub x0, x0, #4");                              // map packed and hash tags to the contiguous range zero through one
            emitter.instruction("cmp x0, #1");                                  // only PHP array payloads satisfy an array declaration
            emitter.instruction(&format!("b.hi {fail_label}"));                 // reject scalar, null, callable, and object payloads
        }
        Arch::X86_64 => {
            emitter.instruction("call __rt_mixed_unbox");                       // inspect the boxed argument without changing its ownership
            emitter.instruction("sub rax, 4");                                  // map packed and hash tags to the contiguous range zero through one
            emitter.instruction("cmp rax, 1");                                  // only PHP array payloads satisfy an array declaration
            emitter.instruction(&format!("ja {fail_label}"));                   // reject scalar, null, callable, and object payloads
        }
    }
}

/// Borrows a normalized by-value string payload, returning false for non-string parameters.
/// Magician applies PHP coercion before staging; its argument array roots the string for the call.
/// Reference slots and object-field initialization must keep their separate owning conversion.
pub(super) fn emit_borrowed_string_arg(
    emitter: &mut Emitter,
    param_type: &PhpType,
    frame_offset: usize,
    fail_label: &str,
) -> bool {
    if param_type.codegen_repr() != PhpType::Str {
        return false;
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [x29, #-{frame_offset}]"));   // reload the argument cell retained by the normalized argument array
            emitter.instruction(&format!("cbz x0, {fail_label}"));              // reject a missing normalized string cell
            emitter.instruction("ldr x9, [x0]");                                // inspect the normalized argument tag before borrowing its payload
            emitter.instruction("cmp x9, #1");                                  // PHP parameter coercion must already have produced a string
            emitter.instruction(&format!("b.ne {fail_label}"));                 // reject malformed bridge input without allocating a conversion
            emitter.instruction("ldr x1, [x0, #8]");                            // borrow the stable string bytes for the native activation
            emitter.instruction("ldr x2, [x0, #16]");                           // preserve the full string length including embedded NUL bytes
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {frame_offset}]")); // reload the argument cell retained by the normalized argument array
            emitter.instruction("test rax, rax");                               // check whether the normalized string cell exists
            emitter.instruction(&format!("jz {fail_label}"));                   // reject a missing normalized string cell
            emitter.instruction("cmp QWORD PTR [rax], 1");                      // PHP parameter coercion must already have produced a string
            emitter.instruction(&format!("jne {fail_label}"));                  // reject malformed bridge input without allocating a conversion
            emitter.instruction("mov rdx, QWORD PTR [rax + 16]");               // preserve the full string length including embedded NUL bytes
            emitter.instruction("mov rax, QWORD PTR [rax + 8]");                // borrow the stable string bytes for the native activation
        }
    }
    true
}

/// Transfers independent owners for staged boxed arguments consumed by an intrinsic helper.
/// Call after fallible preparation and boundary installation, before materializing ABI registers.
pub(super) fn emit_consumed_intrinsic_arguments(
    emitter: &mut Emitter,
    intrinsic: IntrinsicCall,
    parameters: &[PhpType],
) {
    for &index in intrinsic.consumed_mixed_parameters() {
        assert_eq!(parameters[index].codegen_repr(), PhpType::Mixed);
        let offset: usize = parameters[index + 1..].iter()
            .map(super::eval_ref_arg_helpers::eval_arg_temp_slot_size).sum();
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x0, [sp, #{offset}]"));       // load the staged argument whose owner passes to the intrinsic
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, QWORD PTR [rsp + {offset}]")); // load the staged argument whose owner passes to the intrinsic
            }
        }
        abi::emit_call_label(emitter, "__rt_incref");
    }
}

/// Loads a borrowed argument into the bridge's fixed frame spill without creating a key or owner.
pub(super) fn emit_borrowed_argument(
    emitter: &mut Emitter,
    index: usize,
    array_frame_offset: usize,
    result_frame_offset: usize,
    fail_label: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x0, [x29, #-{array_frame_offset}]")); // load the caller-owned argument array box
            emitter.instruction(&format!("cbz x0, {fail_label}"));              // reject a missing argument array
            emitter.instruction("ldr x9, [x0]");                                // read the normalized argument container tag
            emitter.instruction("cmp x9, #4");                                  // normalized call arguments use indexed storage
            emitter.instruction(&format!("b.ne {fail_label}"));                 // do not interpret hash or scalar payloads as indexed storage
            emitter.instruction("ldr x0, [x0, #8]");                            // borrow the indexed payload rooted by the argument box
            emit_branch_if_null_container(emitter, "x0", "x9", fail_label);
            emitter.instruction("ldr x9, [x0, #-8]");                           // read the indexed element representation
            emitter.instruction("ubfx x9, x9, #8, #7");                         // isolate the element value tag
            emitter.instruction("cmp x9, #7");                                  // a borrowed argument must already be a boxed Mixed cell
            emitter.instruction(&format!("b.ne {fail_label}"));                 // reject raw scalar element storage
            abi::emit_load_int_immediate(emitter, "x10", index as i64);
            emitter.instruction("ldr x9, [x0]");                                // read the number of bound arguments
            emitter.instruction("cmp x10, x9");                                 // check the requested argument before loading its slot
            emitter.instruction(&format!("b.hs {fail_label}"));                 // reject a missing argument without reading past the array
            emitter.instruction("add x0, x0, #24");                             // address the packed Mixed pointer slots
            emitter.instruction("ldr x0, [x0, x10, lsl #3]");                   // borrow the argument cell without incrementing its refcount
            emitter.instruction(&format!("str x0, [x29, #-{result_frame_offset}]")); // keep the borrow available for coercion and reference writeback
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, QWORD PTR [rbp - {array_frame_offset}]")); // load the caller-owned argument array box
            emitter.instruction("test rax, rax");                               // check whether an argument array exists
            emitter.instruction(&format!("jz {fail_label}"));                   // reject a missing argument array
            emitter.instruction("cmp QWORD PTR [rax], 4");                      // normalized call arguments use indexed storage
            emitter.instruction(&format!("jne {fail_label}"));                  // do not interpret hash or scalar payloads as indexed storage
            emitter.instruction("mov rax, QWORD PTR [rax + 8]");                // borrow the indexed payload rooted by the argument box
            emit_branch_if_null_container(emitter, "rax", "r10", fail_label);
            emitter.instruction("mov r10, QWORD PTR [rax - 8]");                // read the indexed element representation
            emitter.instruction("shr r10, 8");                                  // shift the element value tag to the low bits
            emitter.instruction("and r10, 127");                                // discard heap flags outside the value tag
            emitter.instruction("cmp r10, 7");                                  // a borrowed argument must already be a boxed Mixed cell
            emitter.instruction(&format!("jne {fail_label}"));                  // reject raw scalar element storage
            abi::emit_load_int_immediate(emitter, "r11", index as i64);
            emitter.instruction("cmp r11, QWORD PTR [rax]");                    // check the argument index against the logical length
            emitter.instruction(&format!("jae {fail_label}"));                  // reject a missing argument without reading past the array
            emitter.instruction("mov rax, QWORD PTR [rax + r11 * 8 + 24]");     // borrow the argument cell without incrementing its refcount
            emitter.instruction(&format!("mov QWORD PTR [rbp - {result_frame_offset}], rax")); // preserve the borrow for coercion and reference writeback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Array guards preserve boxed metadata and reject every tag outside the two array layouts.
    #[test]
    fn php_array_bridge_guards_cover_every_supported_target() {
        let array = PhpType::php_array();
        assert_eq!(bridge_storage_type(&array), array);
        assert_eq!(bridge_storage_type(&PhpType::Int), PhpType::Int);
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_require_php_array(&mut emitter, "invalid_array");
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_mixed_unbox").count(), 1, "{name}");
            assert!(!asm.contains("incref") && !asm.contains("decref"), "{name}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("sub x0, x0, #4"), "{name}");
                assert!(asm.contains("cmp x0, #1"), "{name}");
                assert!(asm.contains("b.hi invalid_array"), "{name}");
            } else {
                assert!(asm.contains("sub rax, 4"), "{name}");
                assert!(asm.contains("cmp rax, 1"), "{name}");
                assert!(asm.contains("ja invalid_array"), "{name}");
            }
        }
    }

    /// All targets borrow normalized strings without allocation and leave other parameter types alone.
    #[test]
    fn native_string_argument_borrows_are_guarded_and_allocation_free() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            assert!(!emit_borrowed_string_arg(&mut emitter, &PhpType::Int, 40, "invalid"));
            assert!(emitter.output().is_empty(), "{name}");
            let mut emitter = Emitter::new(target);
            assert!(emit_borrowed_string_arg(&mut emitter, &PhpType::Str, 40, "invalid"));
            let asm = emitter.output();
            assert!(!asm.contains("__rt_"), "{name}: {asm}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("cmp x9, #1"), "{name}");
                assert!(asm.contains("ldr x1, [x0, #8]"), "{name}");
                assert!(asm.contains("ldr x2, [x0, #16]"), "{name}");
            } else {
                assert!(asm.contains("cmp QWORD PTR [rax], 1"), "{name}");
                assert!(asm.contains("mov rdx, QWORD PTR [rax + 16]"), "{name}");
                assert!(asm.contains("mov rax, QWORD PTR [rax + 8]"), "{name}");
            }
        }
    }

    /// Every target retains consumed offsets and values without retaining borrowed array arguments.
    #[test]
    fn intrinsic_consumers_acquire_only_their_declared_boxed_arguments() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for class in ["SplDoublyLinkedList", "SplStack", "SplQueue", "SplFixedArray"] {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                let intrinsic = IntrinsicCall::instance_method(class, "offsetSet").unwrap();
                emit_consumed_intrinsic_arguments(&mut emitter, intrinsic, &[PhpType::Mixed, PhpType::Mixed]);
                let offsets = if emitter.target.arch == Arch::AArch64 {
                    ["[sp, #16]", "[sp, #0]"]
                } else { ["[rsp + 16]", "[rsp + 0]"] };
                let asm = emitter.output();
                assert_eq!(asm.matches("__rt_incref").count(), 2, "{name}:{class}");
                assert!(offsets.iter().all(|offset| asm.contains(offset)), "{name}:{class}");
            }
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let intrinsic = IntrinsicCall::instance_method("SplFixedArray", "__unserialize").unwrap();
            emit_consumed_intrinsic_arguments(&mut emitter, intrinsic, &[PhpType::Array(Box::new(PhpType::Mixed))]);
            assert!(!emitter.output().contains("__rt_incref"), "{name}");
        }
    }

    /// Every supported target checks argument storage and bounds without allocating or retaining.
    #[test]
    fn native_argument_borrows_are_guarded_and_allocation_free() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_borrowed_argument(&mut emitter, 3, 32, 40, "invalid_arguments");
            let asm = emitter.output();
            assert!(!asm.contains("__elephc_eval_value_int"), "{name}");
            assert!(!asm.contains("__elephc_eval_value_array_get"), "{name}");
            assert!(!asm.contains("incref"), "{name}");
            if target.arch == Arch::AArch64 {
                assert!(asm.contains("cmp x9, #4"), "{name}");
                assert!(asm.contains("cmp x9, #7"), "{name}");
                assert!(asm.contains("b.hs invalid_arguments"), "{name}");
                assert!(asm.contains("str x0, [x29, #-40]"), "{name}");
            } else {
                assert!(asm.contains("cmp QWORD PTR [rax], 4"), "{name}");
                assert!(asm.contains("cmp r10, 7"), "{name}");
                assert!(asm.contains("jae invalid_arguments"), "{name}");
                assert!(asm.contains("mov QWORD PTR [rbp - 40], rax"), "{name}");
            }
        }
    }
}
