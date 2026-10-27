//! Purpose:
//! Invokes fileinfo's user-stream cast callback without replacing the bytes being identified.
//!
//! Called from:
//! - The fileinfo prelude after reading a stream with no successfully attached filters.
//!
//! Key details:
//! - Return codes distinguish missing methods, invalid values and returning the same stream.
//! - The vtable's result tag preserves the callback's declared native return representation.
//! - An opaque record transfers the callback owner to an explicit PHP finally release.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::data::USER_WRAPPER_VTABLE_CAST_RESULT_OFFSET;
use crate::codegen_support::runtime::RESOURCE_ID_TABLE_SLOTS;
use crate::codegen_support::try_handlers::EXCEPTION_GUARD_OWNER_OFFSET;
use crate::codegen_support::sentinels::emit_branch_if_null_container;

/// Emits the C ABI callback query and transfers its status and owned return into an opaque record.
pub(super) fn emit_fileinfo_cast(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (arg, ret, frame, a, b, c) = if arm {
        ("x0", "x0", "sp", "x9", "x10", "x11")
    } else {
        ("rdi", "rax", "rsp", "r9", "r10", "r11")
    };
    let name = emitter.target.extern_symbol("elephc_runtime_fileinfo_cast");
    emitter.label_global(&name);
    // -- reserve callback state and preserve the native caller linkage --
    if arm {
        emitter.instruction("sub sp, sp, #144");                                // reserve callback values and an aligned caller frame
        emitter.instruction("stp x29, x30, [sp, #128]");                        // preserve the caller frame and return address
        emitter.instruction("add x29, sp, #128");                               // establish the callback frame pointer
    } else {
        emitter.instruction("push rbp");                                        // preserve the SysV caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the callback frame pointer
        emitter.instruction("sub rsp, 128");                                    // reserve aligned callback state without callee-saved scratch registers
    }
    abi::emit_load_int_immediate(emitter, a, 8);
    abi::emit_store_to_address(emitter, a, frame, 8);
    abi::emit_store_zero_to_address(emitter, frame, 16);
    abi::emit_store_zero_to_address(emitter, frame, 24);
    abi::emit_store_zero_to_address(emitter, frame, 56);
    // -- resolve the public resource ID without changing its identity --
    abi::emit_symbol_address(emitter, a, "_resource_id_vals");
    abi::emit_symbol_address(emitter, b, "_resource_id_keys");
    emitter.instruction(if arm { "mov x11, #0" } else { "xor r11d, r11d" });    // begin at resource-ID slot zero
    emitter.label("__rt_finfo_cast_scan");
    emitter.instruction(&if arm { format!("cmp x11, #{RESOURCE_ID_TABLE_SLOTS}") } else { format!("cmp r11, {RESOURCE_ID_TABLE_SLOTS}") }); // bound the identity scan
    emitter.instruction(if arm { "b.hs __rt_finfo_cast_done" } else { "jae __rt_finfo_cast_done" }); // ignore absent resource IDs
    emitter.instruction(if arm { "ldr x12, [x9, x11, lsl #3]" } else { "mov r8, QWORD PTR [r9 + r11 * 8]" }); // borrow the displayed ID
    emitter.instruction(if arm { "cmp x12, x0" } else { "cmp r8, rdi" });       // compare with the caller's resource ID
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_found" } else { "je __rt_finfo_cast_found" }); // resolve the matching payload
    emitter.instruction(if arm { "add x11, x11, #1" } else { "inc r11" });      // advance to the next identity slot
    emitter.instruction(if arm { "b __rt_finfo_cast_scan" } else { "jmp __rt_finfo_cast_scan" }); // continue the bounded scan
    emitter.label("__rt_finfo_cast_found");
    emitter.instruction(if arm { "ldr x0, [x10, x11, lsl #3]" } else { "mov rdi, QWORD PTR [r10 + r11 * 8]" }); // load the native stream descriptor
    abi::emit_store_to_address(emitter, arg, frame, 0);
    abi::emit_load_int_immediate(emitter, a, 0x4000_0000);
    emitter.instruction(if arm { "sub x9, x0, x9" } else { "sub r9, rdi" });    // isolate the synthetic wrapper slot, reversed on x86
    if !arm {
        emitter.instruction("neg r9");                                          // normalize the x86 subtraction to descriptor minus base
    }
    emitter.instruction(if arm { "cmp x9, #256" } else { "cmp r9, 256" });      // bound the wrapper handle table
    emitter.instruction(if arm { "b.hs __rt_finfo_cast_done" } else { "jae __rt_finfo_cast_done" }); // ordinary descriptors have no user callback
    abi::emit_symbol_address(emitter, b, "_user_wrapper_handles");
    emitter.instruction(if arm { "ldr x0, [x10, x9, lsl #3]" } else { "mov rdi, QWORD PTR [r10 + r9 * 8]" }); // borrow the live wrapper object
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rdi, rdi" });      // detect a retired wrapper handle
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_done" } else { "jz __rt_finfo_cast_done" }); // ignore closed handles
    // -- resolve the wrapper's typed stream-cast method and return representation --
    emitter.instruction(if arm { "ldr x12, [x0]" } else { "mov r9, QWORD PTR [rdi]" }); // preserve the class ID across symbol-load scratch use
    abi::emit_load_symbol_to_reg(emitter, b, "_class_name_count", 0);
    emitter.instruction(if arm { "cmp x12, x10" } else { "cmp r9, r10" });      // validate the dense class ID
    emitter.instruction(if arm { "b.hs __rt_finfo_cast_done" } else { "jae __rt_finfo_cast_done" }); // avoid invalid class metadata
    abi::emit_symbol_address(emitter, b, "_user_wrapper_vtable_ptrs");
    emitter.instruction(if arm { "ldr x10, [x10, x12, lsl #3]" } else { "mov r10, QWORD PTR [r10 + r9 * 8]" }); // borrow the class method table
    emitter.instruction(if arm { "cmp x10, #0" } else { "test r10, r10" });     // tolerate an absent wrapper method table
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_done" } else { "jz __rt_finfo_cast_done" }); // leave ordinary classes untouched
    abi::emit_load_from_address(emitter, a, b, USER_WRAPPER_VTABLE_CAST_RESULT_OFFSET);
    abi::emit_store_to_address(emitter, a, frame, 8);
    abi::emit_load_from_address(emitter, c, b, 80);
    emitter.instruction(if arm { "cmp x11, #0" } else { "test r11, r11" });     // vtable slot ten is stream_cast
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_missing" } else { "jz __rt_finfo_cast_missing" }); // let PHP issue the missing-method warning
    // -- invoke the callback once and retain its exact native return registers --
    emitter.instruction(if arm { "mov x1, #0" } else { "xor esi, esi" });       // userspace casts receive PHP_STREAM_AS_STDIO
    emitter.instruction(if arm { "blr x11" } else { "call r11" });              // invoke the callback through its declared method ABI
    abi::emit_store_to_address(emitter, ret, frame, 16);
    abi::emit_load_from_address(emitter, a, frame, 8);
    abi::emit_store_to_address(emitter, a, frame, 32);
    abi::emit_store_to_address(emitter, ret, frame, 40);
    abi::emit_store_zero_to_address(emitter, frame, 48);
    emitter.instruction(if arm { "cmp x9, #1" } else { "cmp r9, 1" });          // string results also carry a byte length
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_string" } else { "je __rt_finfo_cast_string" }); // preserve the returned string span
    emitter.instruction(if arm { "cmp x9, #2" } else { "cmp r9, 2" });          // floats return in the floating-point register bank
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_float" } else { "je __rt_finfo_cast_float" }); // reinterpret the returned float bits
    emitter.instruction(if arm { "cmp x9, #7" } else { "cmp r9, 7" });          // Mixed results already carry their exact PHP tag
    emitter.instruction(if arm { "b.ne __rt_finfo_cast_classify" } else { "jne __rt_finfo_cast_classify" }); // raw results use the metadata tag
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // tolerate a canonical null Mixed pointer
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_done" } else { "jz __rt_finfo_cast_done" }); // null is a quiet cast failure
    for (source, destination) in [(0, 32), (8, 40), (16, 48)] {
        abi::emit_load_from_address(emitter, a, ret, source);
        abi::emit_store_to_address(emitter, a, frame, destination);
    }
    emitter.instruction(if arm { "b __rt_finfo_cast_classify" } else { "jmp __rt_finfo_cast_classify" }); // classify the borrowed boxed payload
    emitter.label("__rt_finfo_cast_string");
    // -- normalize declared strings and floats into the common truthiness slots --
    let (string, length) = abi::string_result_regs(emitter);
    abi::emit_store_to_address(emitter, string, frame, 16);
    abi::emit_store_to_address(emitter, length, frame, 24);
    abi::emit_load_from_address(emitter, a, frame, 16);
    abi::emit_store_to_address(emitter, a, frame, 40);
    abi::emit_load_from_address(emitter, a, frame, 24);
    abi::emit_store_to_address(emitter, a, frame, 48);
    emitter.instruction(if arm { "b __rt_finfo_cast_classify" } else { "jmp __rt_finfo_cast_classify" }); // apply PHP string truthiness
    emitter.label("__rt_finfo_cast_float");
    emitter.instruction(if arm { "fmov x9, d0" } else { "movq r9, xmm0" });     // retain the scalar floating-point result as bits
    abi::emit_store_to_address(emitter, a, frame, 40);
    emitter.label("__rt_finfo_cast_classify");
    // -- distinguish falsy values, valid resources and invalid truthy cast results --
    abi::emit_load_from_address(emitter, a, frame, 32);
    emitter.instruction(if arm { "cmp x9, #9" } else { "cmp r9, 9" });          // only an actual resource tag may identify another stream
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_resource" } else { "je __rt_finfo_cast_resource" }); // validate resource identity separately
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp r9, 4" });          // indexed arrays use element-count truthiness
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_array" } else { "je __rt_finfo_cast_array" }); // handle an empty indexed result
    emitter.instruction(if arm { "cmp x9, #5" } else { "cmp r9, 5" });          // associative arrays share the count header
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_array" } else { "je __rt_finfo_cast_array" }); // handle an empty associative result
    for (reg, offset) in [(if arm { "x0" } else { "rdi" }, 32), (if arm { "x1" } else { "rsi" }, 40), (if arm { "x2" } else { "rdx" }, 48)] {
        abi::emit_load_from_address(emitter, reg, frame, offset);
    }
    emitter.instruction(if arm { "bl __rt_php_truthy" } else { "call __rt_php_truthy" }); // test the callback's exact scalar representation
    emitter.instruction(if arm { "cmp x0, #0" } else { "test rax, rax" });      // false and other falsy values are quiet failures
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_done" } else { "jz __rt_finfo_cast_done" }); // keep identifying the wrapper's original bytes
    emitter.instruction(if arm { "b __rt_finfo_cast_invalid" } else { "jmp __rt_finfo_cast_invalid" }); // a truthy non-stream requires PHP diagnostics
    emitter.label("__rt_finfo_cast_array");
    abi::emit_load_from_address(emitter, a, frame, 40);
    emit_branch_if_null_container(emitter, a, b, "__rt_finfo_cast_done");
    abi::emit_load_from_address(emitter, a, a, 0);
    emitter.instruction(if arm { "cmp x9, #0" } else { "test r9, r9" });        // inspect the container's element count
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_done" } else { "jz __rt_finfo_cast_done" }); // empty arrays are falsy
    emitter.instruction(if arm { "b __rt_finfo_cast_invalid" } else { "jmp __rt_finfo_cast_invalid" }); // nonempty arrays are invalid truthy results
    emitter.label("__rt_finfo_cast_resource");
    abi::emit_load_from_address(emitter, a, frame, 48);
    emitter.instruction(if arm { "cmp x9, #3" } else { "cmp r9, 3" });          // popen resources are streams too
    emitter.instruction(if arm { "b.eq __rt_finfo_cast_stream" } else { "je __rt_finfo_cast_stream" }); // accept process-backed streams
    emitter.instruction(if arm { "cmp x9, #1" } else { "cmp r9, 1" });          // ordinary streams use resource cleanup kind one
    emitter.instruction(if arm { "b.hi __rt_finfo_cast_invalid" } else { "ja __rt_finfo_cast_invalid" }); // reject directory, hash and driver resources
    emitter.label("__rt_finfo_cast_stream");
    abi::emit_load_from_address(emitter, a, frame, 40);
    abi::emit_load_from_address(emitter, b, frame, 0);
    emitter.instruction(if arm { "cmp x9, x10" } else { "cmp r9, r10" });       // a wrapper must not return its own stream
    emitter.instruction(if arm { "b.ne __rt_finfo_cast_done" } else { "jne __rt_finfo_cast_done" }); // another stream never replaces the identification buffer
    abi::emit_load_int_immediate(emitter, a, 3);
    abi::emit_store_to_address(emitter, a, frame, 56);
    emitter.instruction(if arm { "b __rt_finfo_cast_done" } else { "jmp __rt_finfo_cast_done" }); // report the self-cast warning in PHP
    emitter.label("__rt_finfo_cast_invalid");
    abi::emit_load_int_immediate(emitter, a, 2);
    abi::emit_store_to_address(emitter, a, frame, 56);
    emitter.instruction(if arm { "b __rt_finfo_cast_done" } else { "jmp __rt_finfo_cast_done" }); // pack an invalid result before PHP raises its TypeError
    emitter.label("__rt_finfo_cast_missing");
    abi::emit_load_int_immediate(emitter, a, 1);
    abi::emit_store_to_address(emitter, a, frame, 56);
    emitter.label("__rt_finfo_cast_done");
    // -- transfer the callback owner into its record before restoring the caller --
    emit_pack_result(emitter);
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #128]");                        // restore the caller frame and return address
        emitter.instruction("add sp, sp, #144");                                // release the complete aligned callback frame
    } else {
        emitter.instruction("add rsp, 128");                                    // release the aligned callback state
        emitter.instruction("pop rbp");                                         // restore the SysV caller frame pointer
    }
    emitter.instruction("ret");                                                 // transfer the record to the PHP finally owner
    emit_result_accessors(emitter);
}

/// Guards the returned owner across allocation, then transfers it into a non-graph record.
fn emit_pack_result(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (ret, frame, scratch, callback) = if arm { ("x0", "sp", "x9", "x14") } else { ("rax", "rsp", "r9", "r11") };
    // -- guard the transferred callback owner before allocating its opaque record --
    abi::emit_store_zero_to_address(emitter, frame, 64 + EXCEPTION_GUARD_OWNER_OFFSET);
    abi::emit_symbol_address(emitter, callback, "__rt_exception_release_owned");
    abi::emit_store_to_address(emitter, callback, frame, 72);
    abi::emit_load_from_address(emitter, scratch, frame, 8);
    emitter.instruction(if arm { "cmp x9, #10" } else { "cmp r9, 10" });        // callable descriptors have a dedicated exceptional release path
    emitter.instruction(if arm { "b.ne __rt_finfo_guard_heap" } else { "jne __rt_finfo_guard_heap" }); // use ordinary typed heap cleanup for other values
    abi::emit_symbol_address(emitter, callback, "__rt_exception_release_callable");
    abi::emit_store_to_address(emitter, callback, frame, 72);
    emitter.instruction(if arm { "b __rt_finfo_guard_owner" } else { "jmp __rt_finfo_guard_owner" }); // guard the returned callable and its captures
    emitter.label("__rt_finfo_guard_heap");
    emitter.instruction(if arm { "cmp x9, #1" } else { "cmp r9, 1" });          // returned strings also transfer an allocation owner
    emitter.instruction(if arm { "b.eq __rt_finfo_guard_owner" } else { "je __rt_finfo_guard_owner" }); // protect an owned string during record allocation
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp r9, 4" });          // scalar return registers must never be treated as heap pointers
    emitter.instruction(if arm { "b.lo __rt_finfo_guard_publish" } else { "jb __rt_finfo_guard_publish" }); // leave scalar guards empty
    emitter.instruction(if arm { "cmp x9, #7" } else { "cmp r9, 7" });          // arrays, objects and Mixed returns are refcounted owners
    emitter.instruction(if arm { "b.hi __rt_finfo_guard_publish" } else { "ja __rt_finfo_guard_publish" }); // raw resources and null carry no heap owner
    emitter.label("__rt_finfo_guard_owner");
    abi::emit_load_from_address(emitter, scratch, frame, 16);
    abi::emit_store_to_address(emitter, scratch, frame, 64 + EXCEPTION_GUARD_OWNER_OFFSET);
    emitter.label("__rt_finfo_guard_publish");
    emitter.instruction(if arm { "add x0, sp, #64" } else { "lea rdi, [rsp + 64]" }); // point at the stack-local activation prefix
    emitter.instruction(if arm { "mov x1, #0" } else { "xor esi, esi" });       // publish this owner as a new unwind group
    emitter.instruction(if arm { "bl __rt_exception_guard_owned" } else { "call __rt_exception_guard_owned" }); // protect the return if allocation invokes throwing GC cleanup
    abi::emit_load_int_immediate(emitter, ret, 32);
    // -- move the owned result into a record and consume its temporary unwind guard --
    emitter.instruction(if arm { "bl __rt_heap_alloc" } else { "call __rt_heap_alloc" }); // allocate the opaque owner record using ordinary heap failure handling
    for (source, destination) in [(56, 0), (8, 8), (16, 16), (24, 24)] {
        abi::emit_load_from_address(emitter, scratch, frame, source);
        abi::emit_store_to_address(emitter, scratch, ret, destination);
    }
    abi::emit_store_to_address(emitter, ret, frame, 104);
    emitter.instruction(if arm { "add x0, sp, #64" } else { "lea rdi, [rsp + 64]" }); // consume exactly the guard whose owner now lives in the record
    emitter.instruction(if arm { "bl __rt_exception_unguard_owned" } else { "call __rt_exception_unguard_owned" }); // transfer ownership to the caller's PHP finally without releasing it
    abi::emit_load_from_address(emitter, ret, frame, 104);
}

/// Exposes the result status and consumes each transferred record before running its destructor.
fn emit_result_accessors(emitter: &mut Emitter) {
    let arm = emitter.target.arch == Arch::AArch64;
    let (arg, ret, frame, scratch) = if arm { ("x0", "x0", "sp", "x9") } else { ("rdi", "rax", "rsp", "r9") };
    let name = emitter.target.extern_symbol("elephc_runtime_fileinfo_cast_status");
    // -- borrow the status without consuming the callback's result owner --
    emitter.label_global(&name);
    abi::emit_load_from_address(emitter, ret, arg, 0);
    emitter.instruction("ret");                                                 // borrow the result status without consuming its owner
    let name = emitter.target.extern_symbol("elephc_runtime_fileinfo_cast_release");
    emitter.label_global(&name);
    // -- retain the typed value while retiring its opaque record before callbacks --
    if arm {
        emitter.instruction("sub sp, sp, #48");                                 // reserve the transferred value and caller linkage
        emitter.instruction("stp x29, x30, [sp, #32]");                         // preserve the caller frame and return address
    } else {
        emitter.instruction("push rbp");                                        // preserve the SysV caller frame pointer
        emitter.instruction("mov rbp, rsp");                                    // establish the release frame pointer
        emitter.instruction("sub rsp, 32");                                     // retain the value across record release and destructor calls
    }
    for (source, destination) in [(8, 0), (16, 8), (24, 16)] {
        abi::emit_load_from_address(emitter, scratch, arg, source);
        abi::emit_store_to_address(emitter, scratch, frame, destination);
    }
    abi::emit_store_zero_to_address(emitter, arg, 16);
    if !arm { emitter.instruction("mov rax, rdi"); }                            // pass the consumed record through the native heap ABI
    emitter.instruction(if arm { "bl __rt_heap_free_safe" } else { "call __rt_heap_free_safe" }); // retire the record before any PHP destructor can reenter
    // -- dispatch the transferred owner to its declared representation's release path --
    abi::emit_load_from_address(emitter, scratch, frame, 0);
    emitter.instruction(if arm { "cmp x9, #1" } else { "cmp r9, 1" });          // raw strings use the validated allocation release helper
    emitter.instruction(if arm { "b.eq __rt_finfo_result_string" } else { "je __rt_finfo_result_string" }); // release the transferred string allocation
    emitter.instruction(if arm { "cmp x9, #10" } else { "cmp r9, 10" });        // callable captures require descriptor cleanup
    emitter.instruction(if arm { "b.eq __rt_finfo_result_callable" } else { "je __rt_finfo_result_callable" }); // preserve the callable ownership protocol
    emitter.instruction(if arm { "cmp x9, #4" } else { "cmp r9, 4" });          // lower tags are non-owning scalars
    emitter.instruction(if arm { "b.lo __rt_finfo_result_done" } else { "jb __rt_finfo_result_done" }); // scalar results carry no transferred heap owner
    emitter.instruction(if arm { "cmp x9, #7" } else { "cmp r9, 7" });          // arrays, objects and Mixed results are refcounted
    emitter.instruction(if arm { "b.hi __rt_finfo_result_done" } else { "ja __rt_finfo_result_done" }); // null and raw descriptors have no heap owner
    abi::emit_load_from_address(emitter, ret, frame, 8);
    emitter.instruction(if arm { "bl __rt_decref_any" } else { "call __rt_decref_any" }); // release the result under PHP's active finally exception state
    emitter.instruction(if arm { "b __rt_finfo_result_done" } else { "jmp __rt_finfo_result_done" }); // finish after the complete typed release
    emitter.label("__rt_finfo_result_string");
    abi::emit_load_from_address(emitter, ret, frame, 8);
    emitter.instruction(if arm { "bl __rt_heap_free_safe" } else { "call __rt_heap_free_safe" }); // release a returned string without touching static data
    emitter.instruction(if arm { "b __rt_finfo_result_done" } else { "jmp __rt_finfo_result_done" }); // retain the normal finally control flow
    emitter.label("__rt_finfo_result_callable");
    abi::emit_load_from_address(emitter, ret, frame, 8);
    emitter.instruction(if arm { "bl __rt_callable_descriptor_release" } else { "call __rt_callable_descriptor_release" }); // release captured callable owners through their descriptor
    emitter.label("__rt_finfo_result_done");
    // -- restore the caller after the complete typed result release --
    if arm {
        emitter.instruction("ldp x29, x30, [sp, #32]");                         // restore the caller linkage after result destruction
        emitter.instruction("add sp, sp, #48");                                 // release the complete aligned release frame
    } else {
        emitter.instruction("add rsp, 32");                                     // release the aligned transferred-value storage
        emitter.instruction("pop rbp");                                         // restore the SysV caller frame pointer
    }
    emitter.instruction("ret");                                                 // leave exception chaining to the enclosing PHP finally
}
