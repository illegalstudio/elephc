//! Purpose:
//! Shared emitters for boxing PHP values into runtime `Mixed` cells.
//! Keeps ABI register shuffling and ownership-transfer boxing outside AST-specific codegen.
//!
//! Called from:
//! - `crate::codegen` EIR lowerers and `crate::codegen_support` helper emitters.
//!
//! Key details:
//! - Tag values and payload register conventions must match `__rt_mixed_from_value`.
//! - Owned boxing paths transfer or release references without double-freeing payloads.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};
use crate::types::PhpType;


/// Returns the runtime value tag byte for a PhpType.
pub(crate) fn runtime_value_tag(ty: &PhpType) -> u8 {
    match ty {
        PhpType::Int => 0,
        PhpType::Str => 1,
        PhpType::Float => 2,
        PhpType::Bool | PhpType::False => 3,
        PhpType::Array(_) => 4,
        PhpType::AssocArray { .. } => 5,
        PhpType::Object(_) => 6,
        PhpType::Mixed => 7,
        PhpType::Union(_) => 7,
        PhpType::Iterable => 7,
        PhpType::Void => 8,
        PhpType::Resource(_) => 9,
        PhpType::Callable => 10,
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) | PhpType::Never => 0,
        PhpType::TaggedScalar => {
            unreachable!("TaggedScalar carries its runtime tag in the tag register, not a static tag")
        }
    }
}

/// Boxes raw register-based value components into a runtime Mixed cell via __rt_mixed_from_value.
pub(crate) fn emit_box_runtime_payload_as_mixed(
    emitter: &mut Emitter,
    value_tag_reg: &str,
    value_lo_reg: &str,
    value_hi_reg: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, {}", value_tag_reg));         // x0 = runtime value tag for the mixed boxing helper
            emitter.instruction(&format!("mov x1, {}", value_lo_reg));          // x1 = low payload word for the mixed boxing helper
            emitter.instruction(&format!("mov x2, {}", value_hi_reg));          // x2 = high payload word for the mixed boxing helper
            emitter.instruction("bl __rt_mixed_from_value");                    // retain/persist the payload as needed and return a boxed mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", value_tag_reg));        // rax = runtime value tag for the mixed boxing helper
            emitter.instruction(&format!("mov rdi, {}", value_lo_reg));         // rdi = low payload word for the mixed boxing helper
            emitter.instruction(&format!("mov rsi, {}", value_hi_reg));         // rsi = high payload word for the mixed boxing helper
            emitter.instruction("call __rt_mixed_from_value");                  // box the payload into a temporary mixed cell on x86_64
        }
    }
}

/// Boxes the current expression result in the ABI result registers into a runtime Mixed cell.
pub(crate) fn emit_box_current_value_as_mixed(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed | PhpType::Union(_) => {}
        PhpType::Iterable => emit_box_iterable_as_mixed(emitter),
        PhpType::TaggedScalar => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x9, x0");                              // stage the tagged scalar payload while the tag moves into the helper tag register
                emitter.instruction("mov x0, x1");                              // pass the dynamic runtime tag as the mixed boxing helper tag argument
                emitter.instruction("mov x1, x9");                              // pass the tagged scalar payload as the mixed boxing helper low word
                emitter.instruction("mov x2, xzr");                             // tagged scalar payloads do not use a second word
                emitter.instruction("bl __rt_mixed_from_value");                // box the tagged scalar payload into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // pass the tagged scalar payload as the mixed boxing helper low word
                emitter.instruction("mov rax, rdx");                            // pass the dynamic runtime tag as the mixed boxing helper tag argument
                emitter.instruction("xor rsi, rsi");                            // tagged scalar payloads do not use a second word
                emitter.instruction("call __rt_mixed_from_value");              // box the tagged scalar payload into a mixed cell
            }
        },
        PhpType::Int
        | PhpType::Bool
        | PhpType::False
        | PhpType::Void
        | PhpType::Never
        | PhpType::Resource(_) => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // move the current scalar payload into the mixed helper argument register
                emitter.instruction("mov x2, xzr");                             // scalar mixed payloads do not use a second word
                emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); // materialize the static value tag for this scalar
                emitter.instruction("bl __rt_mixed_from_value");                // box the scalar payload into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the current scalar payload into the mixed helper low-word register
                emitter.instruction("xor rsi, rsi");                            // scalar mixed payloads do not use a second word
                abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(ty) as i64);
                emitter.instruction("call __rt_mixed_from_value");              // box the scalar payload into a mixed cell
            }
        },
        PhpType::Float => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fmov x1, d0");                             // move the current float bits into the mixed helper payload register
                emitter.instruction("mov x2, xzr");                             // float payloads only use the low word
                emitter.instruction("mov x0, #2");                              // runtime tag 2 = float
                emitter.instruction("bl __rt_mixed_from_value");                // box the float payload into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("movq rdi, xmm0");                          // move the current float bits into the mixed helper payload register
                emitter.instruction("xor rsi, rsi");                            // float payloads only use the low word
                abi::emit_load_int_immediate(emitter, "rax", 2);
                emitter.instruction("call __rt_mixed_from_value");              // box the float payload into a mixed cell
            }
        },
        PhpType::Str => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #1");                              // runtime tag 1 = string
                emitter.instruction("bl __rt_mixed_from_value");                // persist the string payload and box it into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the current string pointer into the mixed helper low-word register
                emitter.instruction("mov rsi, rdx");                            // move the current string length into the mixed helper high-word register
                abi::emit_load_int_immediate(emitter, "rax", 1);
                emitter.instruction("call __rt_mixed_from_value");              // box the string payload into a mixed cell
            }
        },
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the current heap pointer into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // heap-backed payloads only use the low word
                    emitter.instruction(&format!("mov x0, #{}", runtime_value_tag(ty))); // materialize the heap payload tag for the mixed helper
                    emitter.instruction("bl __rt_mixed_from_value");            // retain the heap child and box it into a mixed cell
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the current heap pointer into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // heap-backed payloads only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", runtime_value_tag(ty) as i64);
                    emitter.instruction("call __rt_mixed_from_value");          // box the heap child into a mixed cell
                }
            }
        }
        PhpType::Callable => match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x1, x0");                              // move the callable descriptor into the mixed helper payload register
                emitter.instruction("mov x2, xzr");                             // callable descriptor payloads only use the low word
                emitter.instruction("mov x0, #10");                             // runtime tag 10 = callable descriptor
                emitter.instruction("bl __rt_mixed_from_value");                // retain the callable descriptor and box it into a mixed cell
            }
            Arch::X86_64 => {
                emitter.instruction("mov rdi, rax");                            // move the callable descriptor into the mixed helper payload register
                emitter.instruction("xor rsi, rsi");                            // callable descriptor payloads only use the low word
                abi::emit_load_int_immediate(emitter, "rax", 10);
                emitter.instruction("call __rt_mixed_from_value");              // retain the callable descriptor and box it into a mixed cell
            }
        },
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("mov x1, x0");                          // move the raw pointer into the mixed helper payload register
                    emitter.instruction("mov x2, xzr");                         // raw pointers only use the low word
                    emitter.instruction("mov x0, #0");                          // treat unsupported raw pointers as integer-like payloads for now
                    emitter.instruction("bl __rt_mixed_from_value");            // box the raw pointer bits into a mixed cell
                }
                Arch::X86_64 => {
                    emitter.instruction("mov rdi, rax");                        // move the raw pointer into the mixed helper payload register
                    emitter.instruction("xor rsi, rsi");                        // raw pointers only use the low word
                    abi::emit_load_int_immediate(emitter, "rax", 0);
                    emitter.instruction("call __rt_mixed_from_value");          // box the raw pointer bits into a mixed cell
                }
            }
        }
    }
}

/// Releases the pushed temporary refcounted value after an array push operation.
pub(crate) fn emit_release_pushed_refcounted_temp_after_array_push(
    emitter: &mut Emitter,
    ty: &PhpType,
) {
    if !ty.is_refcounted() {
        return;
    }

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the updated array pointer while releasing the pushed temporary
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the pushed temporary pointer saved below the array result
            abi::emit_decref_if_refcounted(emitter, ty);
            emitter.instruction("ldr x0, [sp], #16");                           // restore the updated array pointer after releasing the pushed temporary
            emitter.instruction("add sp, sp, #16");                             // discard the saved pushed temporary pointer
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve a temporary slot for the updated array pointer
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // preserve the updated array pointer while releasing the pushed temporary
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the pushed temporary pointer saved below the array result
            abi::emit_decref_if_refcounted(emitter, ty);
            emitter.instruction("mov rax, QWORD PTR [rsp]");                    // restore the updated array pointer after releasing the pushed temporary
            emitter.instruction("add rsp, 32");                                 // discard the array-result slot and the pushed temporary slot
        }
    }
}

/// Boxes an owned current result into Mixed and releases the original owner afterward.
pub(crate) fn emit_box_current_owned_value_as_mixed(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Str => emit_box_current_owned_string_as_mixed(emitter),
        PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Iterable
        | PhpType::Object(_)
        | PhpType::Callable => {
            emit_box_current_owned_refcounted_as_mixed_for_container(emitter, ty);
        }
        _ => emit_box_current_value_as_mixed(emitter, ty),
    }
}

/// Transfers the owned string result into a freshly allocated Mixed string cell.
fn emit_box_current_owned_string_as_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("stp x1, x2, [sp, #-16]!");                     // preserve the owned string payload while allocating the Mixed cell
            emitter.instruction("mov x0, #24");                                 // mixed cells store tag plus two payload words
            emitter.instruction("bl __rt_heap_alloc");                          // allocate a fresh Mixed cell payload
            emitter.instruction("mov x9, #5");                                  // heap kind 5 = boxed Mixed cell
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the Mixed heap header
            emitter.instruction("mov x10, #1");                                 // runtime tag 1 = string
            emitter.instruction("str x10, [x0]");                               // store the string runtime tag
            emitter.instruction("ldp x11, x12, [sp], #16");                     // restore the transferred string pointer and length
            emitter.instruction("stp x11, x12, [x0, #8]");                      // move the string payload into the Mixed cell
        }
        Arch::X86_64 => {
            emitter.instruction("sub rsp, 16");                                 // reserve spill space for the owned string payload
            emitter.instruction("mov QWORD PTR [rsp], rax");                    // preserve the owned string pointer while allocating the Mixed cell
            emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                // preserve the owned string length while allocating the Mixed cell
            emitter.instruction("mov rax, 24");                                 // mixed cells store tag plus two payload words
            emitter.instruction("call __rt_heap_alloc");                        // allocate a fresh Mixed cell payload
            emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the Mixed heap marker
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the Mixed heap header
            emitter.instruction("mov QWORD PTR [rax], 1");                      // store runtime tag 1 = string
            emitter.instruction("mov r10, QWORD PTR [rsp]");                    // reload the transferred string pointer
            emitter.instruction("mov QWORD PTR [rax + 8], r10");                // move the string pointer into the Mixed payload
            emitter.instruction("mov r10, QWORD PTR [rsp + 8]");                // reload the transferred string length
            emitter.instruction("mov QWORD PTR [rax + 16], r10");               // move the string length into the Mixed payload
            emitter.instruction("add rsp, 16");                                 // discard the temporary string payload spill
        }
    }
}

/// Boxes an owned refcounted value into a Mixed cell, then releases the original owner.
fn emit_box_current_owned_refcounted_as_mixed_for_container(emitter: &mut Emitter, ty: &PhpType) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the owned source heap value while boxing it into Mixed
            emit_box_current_value_as_mixed(emitter, ty);
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the boxed Mixed result while releasing the original owner
            emitter.instruction("ldr x0, [sp, #16]");                           // reload the original heap value retained by the Mixed box
            abi::emit_decref_if_refcounted(emitter, ty);
            emitter.instruction("ldr x0, [sp], #16");                           // restore the boxed Mixed result
            emitter.instruction("add sp, sp, #16");                             // discard the saved original heap value pointer
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");
            emit_box_current_value_as_mixed(emitter, ty);
            abi::emit_push_reg(emitter, "rax");
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // reload the original heap value retained by the Mixed box
            abi::emit_decref_if_refcounted(emitter, ty);
            abi::emit_pop_reg(emitter, "rax");
            emitter.instruction("add rsp, 16");                                 // discard the saved original heap value pointer
        }
    }
}

/// Boxes an iterable by probing its concrete heap kind and mapping it to a Mixed tag.
fn emit_box_iterable_as_mixed(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // preserve the iterable heap pointer while probing its concrete heap kind
            emitter.instruction("bl __rt_heap_kind");                           // classify the raw iterable pointer by its heap-kind tag
            emitter.instruction("mov x9, x0");                                  // keep the heap kind available for tag normalization
            emitter.instruction("cmp x0, #2");                                  // is the heap kind at least the indexed-array tag?
            emitter.instruction("cset x10, hs");                                // record whether the iterable is in the supported heap-backed range lower bound
            emitter.instruction("cmp x0, #4");                                  // is the heap kind no greater than the object tag?
            emitter.instruction("cset x11, ls");                                // record whether the iterable is in the supported heap-backed range upper bound
            emitter.instruction("and x10, x10, x11");                           // combine the lower and upper bound checks into one predicate
            emitter.instruction("add x9, x9, #2");                              // map heap kind 2/3/4 to mixed tag 4/5/6
            emitter.instruction("mov x0, #8");                                  // default unknown iterable payloads to the null mixed tag
            emitter.instruction("cmp x10, #0");                                 // did the heap kind fall inside the supported iterable range?
            emitter.instruction("csel x0, x9, x0, ne");                         // choose the mapped concrete mixed tag when the range check succeeded
            emitter.instruction("ldr x1, [sp], #16");                           // restore the iterable heap pointer as the mixed payload low word
            emitter.instruction("mov x2, xzr");                                 // iterable payloads do not use a high payload word
            emitter.instruction("bl __rt_mixed_from_value");                    // retain the concrete heap payload and return an owned mixed cell
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rax");                                  // preserve the iterable heap pointer while probing its concrete heap kind
            emitter.instruction("call __rt_heap_kind");                         // classify the raw iterable pointer by its heap-kind tag
            emitter.instruction("mov r10, rax");                                // keep the heap kind available for tag normalization
            emitter.instruction("cmp rax, 2");                                  // is the heap kind at least the indexed-array tag?
            emitter.instruction("setae r11b");                                  // record whether the iterable is in the supported heap-backed range lower bound
            emitter.instruction("cmp rax, 4");                                  // is the heap kind no greater than the object tag?
            emitter.instruction("setbe dl");                                    // record whether the iterable is in the supported heap-backed range upper bound
            emitter.instruction("and dl, r11b");                                // combine the lower and upper bound checks into one predicate byte
            emitter.instruction("add r10, 2");                                  // map heap kind 2/3/4 to mixed tag 4/5/6
            emitter.instruction("mov rax, 8");                                  // default unknown iterable payloads to the null mixed tag
            emitter.instruction("test dl, dl");                                 // did the heap kind fall inside the supported iterable range?
            emitter.instruction("cmovne rax, r10");                             // choose the mapped concrete mixed tag when the range check succeeded
            abi::emit_pop_reg(emitter, "rdi");                                   // restore the iterable heap pointer as the mixed payload low word
            emitter.instruction("xor rsi, rsi");                                // iterable payloads do not use a high payload word
            emitter.instruction("call __rt_mixed_from_value");                  // retain the concrete heap payload and return an owned mixed cell
        }
    }
}
