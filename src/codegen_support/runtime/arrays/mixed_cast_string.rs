//! Purpose:
//! Emits the `__rt_mixed_cast_string`, `__rt_mixed_unbox` runtime helper assembly for mixed cast string.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Mixed helpers use boxed tag/payload cells; tag constants and ownership rules are shared with type checking and codegen.
//! - OWNERSHIP OF THE RESULT IS PER-TAG, and every caller already depends on the split:
//!   tags 1 (string) and 6 (object) are the arms that ALLOCATE: `__rt_str_persist` hands back
//!   a fresh `__rt_heap_alloc` block the caller owns. Tags 0 (int), 2 (float), 3-true (bool) and 9
//!   (resource) all format into the SHARED `_concat_buf` scratch and return a BORROWED
//!   pointer; tags 4 and 5 return the borrowed fixed `Array` literal after warning. Tag
//!   3-false and other unsupported tags return a null pointer with length 0.
//!   `__rt_implode` is the one caller that releases the result, and it does so through
//!   `__rt_heap_free`, which by contract ignores anything that is not a live heap block
//!   (AArch64 range-checks `_heap_buf`, x86_64 checks the heap magic marker), precisely so
//!   concat-buffer scratch can be passed to it. Adding a scratch-returning arm therefore
//!   cannot leak (nothing was allocated) and cannot wild-free (nothing frees scratch).
//! - The tag-6 arm reuses `_class_tostring_ptrs`, the dense `class_id -> __toString` table
//!   `__rt_sprintf_mixed_string` already dispatches through, so a boxed object renders the
//!   same text in `implode()` as in `sprintf("%s")`. It shares tag 1's ownership contract
//!   rather than tag 9's: `__toString` returns storage belonging to the METHOD, so the arm
//!   persists it and releases the original when persist hands back a different block. A class
//!   that publishes no `__toString` keeps the empty-string result -- PHP raises
//!   `Error: Object of class X could not be converted to string` there, which this runtime
//!   cannot yet throw, so that one sub-case stays as it was.
//! - The tag-9 arm reuses `__rt_resource_to_string`, the SAME helper the statically-typed
//!   `PhpType::Resource` path already calls from `lower_resource_to_string` /
//!   `lower_cast_to_string` / `emit_settype_string_conversion`. Boxed and unboxed resources
//!   consequently render through one formatter with one ownership contract.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Converts a boxed Mixed value to a string by dispatching on the unboxed tag.
/// Input: x0 = boxed mixed pointer. Output: x1 = string pointer, x2 = string length (ARM64).
/// Handles int (tag 0 → itoa), string (tag 1 → persisted copy), float (tag 2 → ftoa),
/// bool (tag 3 → "1" or ""), arrays (tags 4/5 → warning plus `Array`), resource
/// (tag 9 → `Resource id #N`), and null/unsupported
/// (→ empty string).
/// Dispatches to `emit_mixed_cast_string_linux_x86_64` on x86_64; ARM64 emits inline.
pub fn emit_mixed_cast_string(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_mixed_cast_string_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_string ---");
    emitter.label_global("__rt_mixed_cast_string");

    emitter.instruction("sub sp, sp, #32");                                     // allocate a small stack frame for nested helper calls
    emitter.instruction("stp x29, x30, [sp, #16]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #16");                                    // establish the helper stack frame
    emitter.instruction("bl __rt_mixed_unbox");                                 // x0=tag, x1=value_lo, x2=value_hi for the boxed payload
    emitter.instruction("cmp x0, #0");                                          // does the mixed payload hold an int?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_int");                // ints cast through itoa
    emitter.instruction("cmp x0, #1");                                          // does the mixed payload already hold a string?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_string");             // strings need detaching from the source mixed owner
    emitter.instruction("cmp x0, #2");                                          // does the mixed payload hold a float?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_float");              // floats cast through ftoa
    emitter.instruction("cmp x0, #3");                                          // does the mixed payload hold a bool?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_bool");               // bools cast to "1" or ""
    emitter.instruction("cmp x0, #4");                                          // does the mixed payload hold an indexed array?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_array");              // arrays warn and cast to the literal "Array"
    emitter.instruction("cmp x0, #5");                                          // does the mixed payload hold an associative array?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_array");              // hashes share PHP's array stringification behavior
    emitter.instruction("cmp x0, #9");                                          // does the mixed payload hold a resource?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_resource");           // resources render as PHP's "Resource id #N"
    emitter.instruction("cmp x0, #6");                                          // does the mixed payload hold an object?
    emitter.instruction("b.eq __rt_mixed_cast_string_from_object");             // objects render through their own __toString
    emitter.instruction("mov x1, xzr");                                         // unsupported and null payloads produce an empty string pointer
    emitter.instruction("mov x2, xzr");                                         // unsupported and null payloads produce an empty string length
    emitter.instruction("b __rt_mixed_cast_string_borrowed");                   // publish borrowed ownership for the normalized empty string

    emitter.label("__rt_mixed_cast_string_from_int");
    emitter.instruction("mov x0, x1");                                          // move the integer payload into the itoa argument register
    emitter.instruction("bl __rt_itoa");                                        // convert the integer payload to decimal text
    emitter.instruction("b __rt_mixed_cast_string_borrowed");                   // publish borrowed ownership for concat scratch

    emitter.label("__rt_mixed_cast_string_from_string");
    emitter.instruction("bl __rt_str_persist");                                 // detach the string payload from the source mixed owner
    emitter.instruction("b __rt_mixed_cast_string_owned");                      // publish the owner returned by string persistence

    emitter.label("__rt_mixed_cast_string_from_resource");
    emitter.instruction("mov x0, x1");                                          // move the native resource payload into the formatter argument register
    emitter.instruction("bl __rt_resource_to_string");                          // format the payload as "Resource id #N" in the shared concat scratch
    emitter.instruction("b __rt_mixed_cast_string_borrowed");                   // publish borrowed ownership for concat scratch

    // -- object elements: the same dense `__toString` table `__rt_sprintf_mixed_string` uses --
    emitter.label("__rt_mixed_cast_string_from_object");
    emitter.instruction("mov x0, x1");                                          // the borrowed object becomes the method receiver
    emitter.instruction("ldr x11, [x0]");                                       // load the object's dense runtime class id
    emitter.instruction("tbnz x11, #63, __rt_mixed_cast_string_object_empty");  // synthetic negative ids index no native metadata
    crate::codegen_support::abi::emit_load_symbol_to_reg(emitter, "x10", "_class_tostring_count", 0);
    emitter.instruction("cmp x11, x10");                                        // is the class id inside the generated table?
    emitter.instruction("b.hs __rt_mixed_cast_string_object_empty");            // out-of-range ids have no native conversion
    crate::codegen_support::abi::emit_symbol_address(emitter, "x10", "_class_tostring_ptrs");
    emitter.instruction("ldr x10, [x10, x11, lsl #3]");                         // resolve the concrete or inherited __toString symbol
    emitter.instruction("cbz x10, __rt_mixed_cast_string_object_empty");        // a class without __toString keeps the empty-string result
    emitter.instruction("blr x10");                                             // call __toString with the borrowed receiver in x0
    // The method's result belongs to the method, not to this frame, so it is persisted exactly
    // as the tag-1 arm persists a boxed string: `__rt_implode` releases what it gets back.
    emitter.instruction("str x1, [sp]");                                        // remember the method's own result pointer
    emitter.instruction("bl __rt_str_persist");                                 // own the bytes independently of the method's storage
    emitter.instruction("ldr x9, [sp]");                                        // reload the method's own result pointer
    emitter.instruction("cmp x1, x9");                                          // did persist adopt that very block?
    emitter.instruction("b.eq __rt_mixed_cast_string_done");                    // yes, there is nothing left to release
    emitter.instruction("stp x1, x2, [sp]");                                    // save the persisted pair across the release
    emitter.instruction("mov x0, x9");                                          // release the independently owned method result
    emitter.instruction("bl __rt_heap_free_safe");                              // borrowed and static pointers are ignored by contract
    emitter.instruction("ldp x1, x2, [sp]");                                    // restore the persisted pair as the result
    emitter.instruction("b __rt_mixed_cast_string_done");                       // return the owned __toString text

    emitter.label("__rt_mixed_cast_string_object_empty");
    emitter.instruction("mov x1, xzr");                                         // no reachable __toString produces an empty string pointer
    emitter.instruction("mov x2, xzr");                                         // no reachable __toString produces an empty string length
    emitter.instruction("b __rt_mixed_cast_string_done");                       // return the normalized empty-string result

    emitter.label("__rt_mixed_cast_string_from_array");
    emitter.instruction("bl __rt_sprintf_warn_array_to_string");                // emit PHP's array-to-string warning for boxed arrays
    crate::codegen_support::abi::emit_symbol_address(emitter, "x1", "_iterable_array_str");
    emitter.instruction("mov x2, #5");                                          // byte length of the fixed "Array" literal
    emitter.instruction("b __rt_mixed_cast_string_borrowed");                   // publish borrowed ownership for fixed data

    emitter.label("__rt_mixed_cast_string_from_float");
    emitter.instruction("fmov d0, x1");                                         // move the unboxed float bits into the FP register file
    emitter.instruction("bl __rt_ftoa");                                        // convert the float payload to decimal text
    emitter.instruction("b __rt_mixed_cast_string_borrowed");                   // publish borrowed ownership for concat scratch

    emitter.label("__rt_mixed_cast_string_from_bool");
    emitter.instruction("cbz x1, __rt_mixed_cast_string_false");                // false casts to the empty string
    emitter.instruction("mov x0, x1");                                          // move the true payload (1) into the itoa argument register
    emitter.instruction("bl __rt_itoa");                                        // convert true to the string "1"
    emitter.instruction("b __rt_mixed_cast_string_borrowed");                   // publish borrowed ownership for concat scratch

    emitter.label("__rt_mixed_cast_string_false");
    emitter.instruction("mov x1, xzr");                                         // false produces an empty string pointer
    emitter.instruction("mov x2, xzr");                                         // false produces an empty string length

    emitter.label("__rt_mixed_cast_string_borrowed");
    emitter.instruction("mov x15, xzr");                                        // publish a borrowed string result to compiled PHP callers
    emitter.instruction("b __rt_mixed_cast_string_done");                       // rejoin the shared helper epilogue

    emitter.label("__rt_mixed_cast_string_owned");
    emitter.instruction("mov x15, #1");                                         // publish the persisted string owner to compiled PHP callers

    emitter.label("__rt_mixed_cast_string_done");
    emitter.instruction("ldp x29, x30, [sp, #16]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #32");                                     // release the helper stack frame
    emitter.instruction("ret");                                                 // return the string cast result in x1/x2
}

/// x86_64 variant of `emit_mixed_cast_string`: converts a boxed Mixed value to a string.
/// Input: RDI = boxed mixed pointer. Output: RAX = string pointer, RDX = string length (System V ABI).
/// Handles int (tag 0 → itoa), string (tag 1 → persisted copy), float (tag 2 → ftoa),
/// bool (tag 3 → "1" or ""), arrays (tags 4/5 → warning plus `Array`), resource
/// (tag 9 → `Resource id #N`), and null/unsupported
/// (→ empty string).
fn emit_mixed_cast_string_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: mixed_cast_string ---");
    emitter.label_global("__rt_mixed_cast_string");

    emitter.instruction("push rbp");                                            // preserve the caller frame pointer while mixed string casting uses nested helpers
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the helper
    emitter.instruction("call __rt_mixed_unbox");                               // rax=tag, rdi=value_lo, rdx=value_hi for the boxed payload
    emitter.instruction("cmp rax, 0");                                          // does the mixed payload hold an int?
    emitter.instruction("je __rt_mixed_cast_string_from_int");                  // ints cast through itoa
    emitter.instruction("cmp rax, 1");                                          // does the mixed payload already hold a string?
    emitter.instruction("je __rt_mixed_cast_string_from_string");               // strings need detaching from the source mixed owner
    emitter.instruction("cmp rax, 2");                                          // does the mixed payload hold a float?
    emitter.instruction("je __rt_mixed_cast_string_from_float");                // floats cast through ftoa
    emitter.instruction("cmp rax, 3");                                          // does the mixed payload hold a bool?
    emitter.instruction("je __rt_mixed_cast_string_from_bool");                 // bools cast to \"1\" or \"\"
    emitter.instruction("cmp rax, 4");                                          // does the mixed payload hold an indexed array?
    emitter.instruction("je __rt_mixed_cast_string_from_array");                // arrays warn and cast to the literal \"Array\"
    emitter.instruction("cmp rax, 5");                                          // does the mixed payload hold an associative array?
    emitter.instruction("je __rt_mixed_cast_string_from_array");                // hashes share PHP's array stringification behavior
    emitter.instruction("cmp rax, 9");                                          // does the mixed payload hold a resource?
    emitter.instruction("je __rt_mixed_cast_string_from_resource");             // resources render as PHP's \"Resource id #N\"
    emitter.instruction("cmp rax, 6");                                          // does the mixed payload hold an object?
    emitter.instruction("je __rt_mixed_cast_string_from_object");               // objects render through their own __toString
    emitter.instruction("xor rax, rax");                                        // unsupported and null payloads produce an empty string pointer
    emitter.instruction("xor rdx, rdx");                                        // unsupported and null payloads produce an empty string length
    emitter.instruction("jmp __rt_mixed_cast_string_borrowed");                 // publish borrowed ownership for the normalized empty string

    emitter.label("__rt_mixed_cast_string_from_int");
    emitter.instruction("mov rax, rdi");                                        // move the integer payload into the itoa input register
    emitter.instruction("call __rt_itoa");                                      // convert the integer payload to decimal text
    emitter.instruction("jmp __rt_mixed_cast_string_borrowed");                 // publish borrowed ownership for concat scratch

    emitter.label("__rt_mixed_cast_string_from_string");
    emitter.instruction("mov rax, rdi");                                        // move the unboxed string pointer into the ABI string result register
    emitter.instruction("call __rt_str_persist");                               // detach the string payload from the source mixed owner
    emitter.instruction("jmp __rt_mixed_cast_string_owned");                    // publish the owner returned by string persistence

    emitter.label("__rt_mixed_cast_string_from_resource");
    emitter.instruction("mov rax, rdi");                                        // move the native resource payload into the formatter input register
    emitter.instruction("call __rt_resource_to_string");                        // format the payload as \"Resource id #N\" in the shared concat scratch
    emitter.instruction("jmp __rt_mixed_cast_string_borrowed");                 // publish borrowed ownership for concat scratch

    // -- object elements: the same dense `__toString` table `__rt_sprintf_mixed_string` uses --
    emitter.label("__rt_mixed_cast_string_from_object");
    emitter.instruction("sub rsp, 16");                                         // scratch for the method result across nested helper calls
    emitter.instruction("mov r8, QWORD PTR [rdi]");                             // load the object's dense runtime class id
    emitter.instruction("test r8, r8");                                         // reject synthetic negative class ids
    emitter.instruction("js __rt_mixed_cast_string_object_empty");              // synthetic ids index no native metadata
    emitter.instruction("cmp r8, QWORD PTR [rip + _class_tostring_count]");     // is the class id inside the generated table?
    emitter.instruction("jae __rt_mixed_cast_string_object_empty");             // out-of-range ids have no native conversion
    emitter.instruction("lea r10, [rip + _class_tostring_ptrs]");               // address the dense __toString function-pointer table
    emitter.instruction("mov r10, QWORD PTR [r10 + r8 * 8]");                   // resolve the concrete or inherited __toString symbol
    emitter.instruction("test r10, r10");                                       // did the class publish a conversion method?
    emitter.instruction("jz __rt_mixed_cast_string_object_empty");              // a class without __toString keeps the empty-string result
    emitter.instruction("call r10");                                            // call __toString with the borrowed receiver in rdi
    // The method's result belongs to the method, not to this frame, so it is persisted exactly
    // as the tag-1 arm persists a boxed string: `__rt_implode` releases what it gets back.
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // remember the method's own result pointer
    emitter.instruction("call __rt_str_persist");                               // own the bytes independently of the method's storage
    emitter.instruction("cmp rax, QWORD PTR [rsp]");                            // did persist adopt that very block?
    emitter.instruction("je __rt_mixed_cast_string_object_owned");              // yes, there is nothing left to release
    emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                        // save the persisted length across the release
    emitter.instruction("mov rcx, QWORD PTR [rsp]");                            // the method's own result pointer
    emitter.instruction("mov QWORD PTR [rsp], rax");                            // keep the persisted pointer for the return
    emitter.instruction("mov rax, rcx");                                        // release the independently owned method result
    emitter.instruction("call __rt_heap_free_safe");                            // borrowed and static pointers are ignored by contract
    emitter.instruction("mov rax, QWORD PTR [rsp]");                            // restore the persisted pointer as the result
    emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");                        // restore the persisted length as the result
    emitter.label("__rt_mixed_cast_string_object_owned");
    emitter.instruction("add rsp, 16");                                         // release the object-arm scratch
    emitter.instruction("jmp __rt_mixed_cast_string_done");                     // return the owned __toString text

    emitter.label("__rt_mixed_cast_string_object_empty");
    emitter.instruction("add rsp, 16");                                         // release the object-arm scratch
    emitter.instruction("xor rax, rax");                                        // no reachable __toString produces an empty string pointer
    emitter.instruction("xor rdx, rdx");                                        // no reachable __toString produces an empty string length
    emitter.instruction("jmp __rt_mixed_cast_string_done");                     // return the normalized empty-string result

    emitter.label("__rt_mixed_cast_string_from_array");
    emitter.instruction("call __rt_sprintf_warn_array_to_string_x64");          // emit PHP's array-to-string warning for boxed arrays
    crate::codegen_support::abi::emit_symbol_address(emitter, "rax", "_iterable_array_str");
    emitter.instruction("mov edx, 5");                                          // byte length of the fixed \"Array\" literal
    emitter.instruction("jmp __rt_mixed_cast_string_borrowed");                 // publish borrowed ownership for fixed data

    emitter.label("__rt_mixed_cast_string_from_float");
    emitter.instruction("movq xmm0, rdi");                                      // move the unboxed float bits into the FP register file
    emitter.instruction("call __rt_ftoa");                                      // convert the float payload to decimal text
    emitter.instruction("jmp __rt_mixed_cast_string_borrowed");                 // publish borrowed ownership for concat scratch

    emitter.label("__rt_mixed_cast_string_from_bool");
    emitter.instruction("test rdi, rdi");                                       // false casts to the empty string
    emitter.instruction("je __rt_mixed_cast_string_false");                     // skip integer conversion when the bool payload is false
    emitter.instruction("mov rax, rdi");                                        // move the true payload (1) into the itoa input register
    emitter.instruction("call __rt_itoa");                                      // convert true to the string \"1\"
    emitter.instruction("jmp __rt_mixed_cast_string_borrowed");                 // publish borrowed ownership for concat scratch

    emitter.label("__rt_mixed_cast_string_false");
    emitter.instruction("xor rax, rax");                                        // false produces an empty string pointer
    emitter.instruction("xor rdx, rdx");                                        // false produces an empty string length

    emitter.label("__rt_mixed_cast_string_borrowed");
    emitter.instruction("xor r11d, r11d");                                      // publish a borrowed string result to compiled PHP callers
    emitter.instruction("jmp __rt_mixed_cast_string_done");                     // rejoin the shared helper epilogue

    emitter.label("__rt_mixed_cast_string_owned");
    emitter.instruction("mov r11d, 1");                                         // publish the persisted string owner to compiled PHP callers

    emitter.label("__rt_mixed_cast_string_done");
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the string cast result in rax/rdx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Emits `__rt_mixed_cast_string` for one target and returns the assembly text.
    fn emit_for(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        emit_mixed_cast_string(&mut emitter);
        emitter.output()
    }

    /// Pins the AArch64 tag-9 arm: a boxed resource must reach
    /// `__rt_resource_to_string` with its native payload in `x0`, and rejoin the shared
    /// epilogue so the display string comes back in the standard `x1`/`x2` pair.
    ///
    /// Without this arm every boxed resource fell through to the null/unsupported tail
    /// and `"$r"`, `"" . $r`, `(string) $r` and `strval($r)` all produced the EMPTY
    /// string while PHP 8.5.6 produced `Resource id #5`.
    #[test]
    fn test_mixed_cast_string_arm64_dispatches_the_resource_tag() {
        let asm = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(
            asm.contains("    cmp x0, #9\n    b.eq __rt_mixed_cast_string_from_resource\n"),
            "{asm}"
        );
        assert!(asm.contains("__rt_mixed_cast_string_from_resource:\n"), "{asm}");
        assert!(asm.contains("bl __rt_resource_to_string"), "{asm}");
        assert_eq!(asm.matches("bl __rt_resource_to_string").count(), 1, "{asm}");
        assert_eq!(asm.matches("sub sp, sp, #32").count(), 1, "{asm}");
    }

    /// Pins the x86_64 tag-9 arm, the half a comparable fix silently lost before: the
    /// #601 implode ownership work was removed on BOTH arches precisely because only the
    /// AArch64 sequence had ever been asserted. Same dispatch, same single call, same
    /// unchanged frame.
    #[test]
    fn test_mixed_cast_string_x86_64_dispatches_the_resource_tag() {
        let asm = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(
            asm.contains("    cmp rax, 9\n    je __rt_mixed_cast_string_from_resource\n"),
            "{asm}"
        );
        assert!(asm.contains("__rt_mixed_cast_string_from_resource:\n"), "{asm}");
        assert!(asm.contains("mov rax, rdi"), "{asm}");
        assert!(asm.contains("call __rt_resource_to_string"), "{asm}");
        assert_eq!(asm.matches("call __rt_resource_to_string").count(), 1, "{asm}");
        assert_eq!(asm.matches("push rbp").count(), 1, "{asm}");
        assert_eq!(asm.matches("pop rbp").count(), 1, "{asm}");
    }

    /// Verifies boxed indexed and associative arrays share PHP's warning and `Array` result.
    #[test]
    fn test_mixed_cast_string_dispatches_array_tags_on_both_targets() {
        let arm64 = emit_for(Target::new(Platform::MacOS, Arch::AArch64));
        assert!(arm64.contains("cmp x0, #4\n    b.eq __rt_mixed_cast_string_from_array"));
        assert!(arm64.contains("cmp x0, #5\n    b.eq __rt_mixed_cast_string_from_array"));
        assert!(arm64.contains("bl __rt_sprintf_warn_array_to_string"));
        assert!(arm64.contains("_iterable_array_str"));

        let x64 = emit_for(Target::new(Platform::Linux, Arch::X86_64));
        assert!(x64.contains("cmp rax, 4\n    je __rt_mixed_cast_string_from_array"));
        assert!(x64.contains("cmp rax, 5\n    je __rt_mixed_cast_string_from_array"));
        assert!(x64.contains("call __rt_sprintf_warn_array_to_string_x64"));
        assert!(x64.contains("_iterable_array_str"));
    }

    /// Pins the OWNERSHIP contract of the resource arm on both targets: it must return
    /// BORROWED `_concat_buf` scratch, exactly like the int/float/bool arms, and must
    /// never route through `__rt_str_persist` the way the tag-1 string arm does.
    ///
    /// This is the invariant `__rt_implode` depends on. Implode records whatever
    /// `__rt_mixed_cast_string` returns and hands it to `__rt_heap_free`, which ignores
    /// pointers outside the live heap by contract. A resource arm that allocated instead
    /// would be freed correctly there but LEAKED at the ~20 other call sites that never
    /// release the result; an arm that allocated and was NOT recorded would leak
    /// everywhere. Scratch is the only contract every existing caller already honours.
    #[test]
    fn test_mixed_cast_string_resource_arm_borrows_scratch_on_both_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = emit_for(target);
            let arm = asm
                .split("__rt_mixed_cast_string_from_resource:\n")
                .nth(1)
                .unwrap_or_else(|| panic!("missing resource arm for {target:?}:\n{asm}"));
            // Stop at the NEXT helper label, whichever it is. Naming a later arm here meant
            // that anything inserted in between was measured as though it were the resource
            // arm, and the object arm's `__rt_str_persist` tripped this assertion.
            let arm = arm
                .split_once("\n__rt_mixed_cast_string_")
                .map(|(resource_arm, _)| resource_arm)
                .unwrap_or(arm);
            assert!(
                !arm.contains("__rt_str_persist"),
                "the resource arm must not allocate an owned copy ({target:?}):\n{arm}"
            );
            assert!(
                !arm.contains("__rt_heap_alloc") && !arm.contains("__rt_heap_free"),
                "the resource arm must not touch the heap allocator ({target:?}):\n{arm}"
            );
        }
    }

    /// Publishes owned status only for the tag-1 branch that persists a string payload.
    #[test]
    fn test_mixed_cast_string_publishes_per_tag_ownership_on_both_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = emit_for(target);
            let string_arm = asm
                .split("__rt_mixed_cast_string_from_string:\n")
                .nth(1)
                .expect("string arm must be emitted")
                .split("__rt_mixed_cast_string_from_resource:\n")
                .next()
                .expect("string arm must precede resource arm");
            assert!(string_arm.contains("__rt_str_persist"), "{target:?}: {asm}");
            assert!(
                string_arm.contains("mixed_cast_string_owned"),
                "{target:?}: {asm}"
            );
            match target.arch {
                Arch::AArch64 => {
                    assert!(asm.contains("mov x15, xzr"), "{target:?}: {asm}");
                    assert!(asm.contains("mov x15, #1"), "{target:?}: {asm}");
                }
                Arch::X86_64 => {
                    assert!(asm.contains("xor r11d, r11d"), "{target:?}: {asm}");
                    assert!(asm.contains("mov r11d, 1"), "{target:?}: {asm}");
                }
            }
        }
    }
}
