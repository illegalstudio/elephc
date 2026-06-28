//! Purpose:
//! Emits the `__rt_object_free_deep`, `__rt_object_free_deep_done` runtime helper assembly for object free deep.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` via `crate::codegen::runtime::arrays`.
//!
//! Key details:
//! - Deep free helpers recursively release owned child storage and must match the heap kind/tag layout exactly.

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::abi;

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits the `__rt_object_free_deep` runtime helper for ARM64.
/// Frees an object instance and recursively releases all heap-backed property payloads
/// (strings, arrays, objects, Mixed cells) via class-level gc descriptors, then returns
/// storage to the heap.
///
/// Input:  x0 = object pointer (heap-backed, non-null, within heap range)
/// Output: none (x0 = 0 on return via `__rt_object_free_deep_done`)
/// Clobbers: x0–x15, lr as needed for helper calls
/// Special cases: Fiber (munmap stack), Generator (boxed Mixed fields), SPL types.
pub fn emit_object_free_deep(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_object_free_deep_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: object_free_deep ---");
    emitter.label_global("__rt_object_free_deep");

    // -- null and heap-range checks --
    emitter.instruction("cbz x0, __rt_object_free_deep_done");                  // skip null objects
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("cmp x0, x9");                                          // is the object below the heap buffer?
    emitter.instruction("b.lo __rt_object_free_deep_done");                     // skip non-heap pointers
    crate::codegen::abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // load the current heap offset
    emitter.instruction("add x10, x9, x10");                                    // compute the current heap end
    emitter.instruction("cmp x0, x10");                                         // is the object at or beyond the heap end?
    emitter.instruction("b.hs __rt_object_free_deep_done");                     // skip invalid pointers

    // -- set up stack frame --
    // Stack layout:
    //   [sp, #0]  = object pointer
    //   [sp, #8]  = descriptor pointer
    //   [sp, #16] = property count
    //   [sp, #24] = loop index
    //   [sp, #32] = saved x29
    //   [sp, #40] = saved x30
    emitter.instruction("sub sp, sp, #48");                                     // allocate stack frame for object cleanup
    emitter.instruction("stp x29, x30, [sp, #32]");                             // save frame pointer and return address
    emitter.instruction("add x29, sp, #32");                                    // set up the new frame pointer
    emitter.instruction("str x0, [sp, #0]");                                    // save the object pointer
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("mov x10, #1");                                         // ordinary deep-free walks suppress nested collector runs
    emitter.instruction("str x10, [x9]");                                       // store release-suppressed = 1 for child cleanup

    // -- run the class's PHP __destruct (if any) before releasing properties --
    // The receiver is still fully constructed here; the helper resolves the
    // destructor from the object's class_id and runs it with $this borrowed.
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer for the destructor call
    emitter.instruction("bl __rt_call_object_destructor");                      // run the class's __destruct hook if one is declared
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer after the destructor returns

    // -- Fiber special case: release the per-fiber stack before the standard struct free path --
    // The Fiber object has zero declared PHP properties. Its payload past the class_id is made
    // of runtime-managed fields, not Mixed/array/string slots, so walking those bytes through
    // the generic property-tag descriptor would read garbage. Detect Fiber by class_id and skip
    // straight to the struct free after returning the heap-allocated stack.
    emitter.instruction("ldr x10, [x0]");                                       // x10 = receiver class_id
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x11", "_fiber_class_id", 0); // x11 = compile-time class id of the built-in Fiber class
    emitter.instruction("cmp x10, x11");                                        // is the receiver a Fiber instance?
    emitter.instruction("b.ne __rt_object_free_deep_not_fiber");                // skip the fiber-specific cleanup path for non-Fiber receivers
    emitter.instruction(&format!("ldr x9, [x0, #{}]", crate::codegen::runtime::FIBER_STACK_BASE_OFFSET)); // x9 = fiber stack_base (mapping start returned by mmap)
    emitter.instruction("cbz x9, __rt_object_free_deep_fiber_no_stack");        // skip when the stack was already released by an earlier free pass
    emitter.instruction(&format!("ldr x10, [x0, #{}]", crate::codegen::runtime::FIBER_STACK_SIZE_OFFSET)); // x10 = total mmap'd length, exactly what munmap needs
    emitter.instruction("mov x0, x9");                                          // pass stack_base as the mapping start to release
    emitter.instruction("mov x1, x10");                                         // pass the mapped length so munmap unmaps the entire region
    emitter.instruction("bl __rt_fiber_free_stack");                            // return the per-fiber stack to the kernel via munmap
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved Fiber object pointer for the struct free path
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen::runtime::FIBER_STACK_BASE_OFFSET)); // null the stack_base so a double-free is a clean no-op
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen::runtime::FIBER_STACK_SIZE_OFFSET)); // null the stack_size to mirror the cleared base pointer
    emitter.label("__rt_object_free_deep_fiber_no_stack");

    // -- release runtime-owned Fiber transfer and pending exception values --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved Fiber object pointer before releasing transfer_value
    emitter.instruction(&format!("ldr x0, [x0, #{}]", crate::codegen::runtime::FIBER_TRANSFER_VALUE_OFFSET)); // x0 = boxed Mixed transfer_value owned by the Fiber
    emitter.instruction("bl __rt_decref_mixed");                                // release the Fiber's retained transfer value, if any
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved Fiber object pointer after transfer cleanup
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen::runtime::FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo after releasing it
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen::runtime::FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to match the empty slot
    emitter.instruction(&format!("ldr x0, [x0, #{}]", crate::codegen::runtime::FIBER_PENDING_THROW_OFFSET)); // x0 = pending Throwable object parked by Fiber::throw/escape
    emitter.instruction("bl __rt_decref_any");                                  // release a pending Throwable if one is still attached
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved Fiber object pointer after pending_throw cleanup
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen::runtime::FIBER_PENDING_THROW_OFFSET)); // clear pending_throw after releasing it

    // -- release the callable descriptor retained by the Fiber object itself --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved Fiber object pointer before descriptor cleanup
    emitter.instruction(&format!("ldr x0, [x0, #{}]", crate::codegen::runtime::FIBER_CALLABLE_OFFSET)); // load the callable descriptor stored on the Fiber
    emitter.instruction("bl __rt_callable_descriptor_release");                 // release dynamic descriptor captures held by the Fiber callable
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved Fiber object pointer after descriptor cleanup
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen::runtime::FIBER_CALLABLE_OFFSET)); // clear callable descriptor after release

    // -- release legacy trailing Fiber capture slots, if any exist.
    // Current Fiber lowering stores captures in the callable descriptor released
    // above, so user_arg_max normally equals FIBER_START_ARGS_MAX and this loop
    // is a no-op. Keeping the guard here makes stale lowered objects harmless.
    let user_arg_max_off = crate::codegen::runtime::FIBER_USER_ARG_MAX_OFFSET;
    let start_args_off = crate::codegen::runtime::FIBER_START_ARGS_OFFSET;
    emitter.instruction(&format!("ldr x9, [x0, #{}]", user_arg_max_off));       // x9 = legacy user_arg_max capture boundary
    emitter.instruction("str x9, [sp, #24]");                                   // park user_arg_max in the spare loop-index slot of the cleanup frame
    for i in 0..crate::codegen::runtime::FIBER_START_ARGS_MAX {
        let skip_label = format!("__rt_object_free_deep_fiber_capture_skip_{}", i);
        emitter.instruction("ldr x9, [sp, #24]");                               // reload user_arg_max — earlier __rt_decref_any clobbers x9 internally
        emitter.instruction(&format!("cmp x9, #{}", i));                        // is slot i still inside the visible start-arg region?
        emitter.instruction(&format!("b.gt {}", skip_label));                   // skip visible start-arg slots; only legacy captures need release
        emitter.instruction("ldr x0, [sp, #0]");                                // reload the saved Fiber object pointer
        emitter.instruction(&format!("ldr x0, [x0, #{}]", start_args_off + i * 8)); // x0 = legacy trailing capture payload
        emitter.instruction("bl __rt_decref_any");                              // release the legacy capture heap payload if present
        emitter.label(&skip_label);
    }
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Fiber pointer one last time before the struct free
    emitter.instruction("b __rt_object_free_deep_struct");                      // skip the property-tag walk and free the Fiber struct itself
    emitter.label("__rt_object_free_deep_not_fiber");

    // -- Generator special case: a fiber-shaped coroutine object whose payload is
    // runtime-managed (coroutine stack + boxed Mixed fields), not PHP property
    // slots. Release the stack and every owned field, then free the struct. --
    emitter.instruction("ldr x10, [x0]");                                       // x10 = receiver class_id
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x11", "_generator_class_id", 0); // x11 = compile-time class id of the built-in Generator class
    emitter.instruction("cmp x10, x11");                                        // is the receiver a Generator coroutine?
    emitter.instruction("b.ne __rt_object_free_deep_not_generator");            // skip generator cleanup for ordinary PHP objects
    emit_generator_coroutine_release_aarch64(emitter);
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved Generator pointer before the struct free
    emitter.instruction("b __rt_object_free_deep_struct");                      // free the coroutine struct without walking property descriptors
    emitter.label("__rt_object_free_deep_not_generator");

    // -- SPL doubly-linked-list family: release custom internal storage, not PHP property slots --
    emitter.instruction("ldr x10, [x0]");                                       // x10 = receiver class_id
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x11", "_spl_dll_class_id", 0); // x11 = class id of SplDoublyLinkedList
    emitter.instruction("cmp x10, x11");                                        // is the receiver a SplDoublyLinkedList?
    emitter.instruction("b.eq __rt_object_free_deep_spl_dll");                  // release SPL list storage for SplDoublyLinkedList
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x11", "_spl_stack_class_id", 0); // x11 = class id of SplStack
    emitter.instruction("cmp x10, x11");                                        // is the receiver a SplStack?
    emitter.instruction("b.eq __rt_object_free_deep_spl_dll");                  // release SPL list storage for SplStack
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x11", "_spl_queue_class_id", 0); // x11 = class id of SplQueue
    emitter.instruction("cmp x10, x11");                                        // is the receiver a SplQueue?
    emitter.instruction("b.ne __rt_object_free_deep_not_spl_dll");              // skip SPL list cleanup for ordinary objects
    emitter.label("__rt_object_free_deep_spl_dll");
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved SPL list object pointer
    emitter.instruction(&format!("ldr x0, [x0, #{}]", crate::codegen::runtime::spl::SPL_DLL_STORAGE_OFFSET)); // load the owned internal Mixed storage array
    emitter.instruction("bl __rt_decref_array");                                // release the internal array and its owned Mixed cells
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved SPL list object pointer after storage release
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen::runtime::spl::SPL_DLL_STORAGE_OFFSET)); // clear the storage pointer after release
    emitter.instruction("b __rt_object_free_deep_no_dyn_props");                // free the custom object storage without generic descriptor walking
    emitter.label("__rt_object_free_deep_not_spl_dll");

    // -- SplFixedArray custom storage: release the owned fixed-size Mixed array --
    emitter.instruction("ldr x10, [x0]");                                       // x10 = receiver class_id
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "x11", "_spl_fixed_array_class_id", 0); // x11 = class id of SplFixedArray
    emitter.instruction("cmp x10, x11");                                        // is the receiver a SplFixedArray?
    emitter.instruction("b.ne __rt_object_free_deep_not_spl_fixed");            // skip fixed-array cleanup for ordinary objects
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved SplFixedArray object pointer
    emitter.instruction(&format!("ldr x0, [x0, #{}]", crate::codegen::runtime::spl::SPL_FIXED_STORAGE_OFFSET)); // load the owned fixed-array storage
    emitter.instruction("bl __rt_decref_array");                                // release fixed-array storage and its owned Mixed cells
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved SplFixedArray object pointer after storage release
    emitter.instruction(&format!("str xzr, [x0, #{}]", crate::codegen::runtime::spl::SPL_FIXED_STORAGE_OFFSET)); // clear storage pointer after release
    emitter.instruction("b __rt_object_free_deep_no_dyn_props");                // free custom fixed-array storage without generic descriptor walking
    emitter.label("__rt_object_free_deep_not_spl_fixed");

    // -- derive property count from the object payload size --
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the object payload size from the heap header
    emitter.instruction("sub x9, x9, #8");                                      // subtract the leading class_id field
    emitter.instruction("lsr x9, x9, #4");                                      // divide by 16 to get the number of property slots
    emitter.instruction("str x9, [sp, #16]");                                   // save the property count for the cleanup loop

    // -- resolve the per-class property tag descriptor --
    emitter.instruction("ldr x10, [x0]");                                       // load the runtime class_id from the object payload
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_class_gc_desc_count");
    emitter.instruction("ldr x11, [x11]");                                      // load the number of emitted class descriptors
    emitter.instruction("cmp x10, x11");                                        // is class_id within the descriptor table?
    emitter.instruction("b.hs __rt_object_free_deep_struct");                   // invalid class ids fall back to a shallow free
    crate::codegen::abi::emit_symbol_address(emitter, "x11", "_class_gc_desc_ptrs");
    emitter.instruction("lsl x12, x10, #3");                                    // scale class_id by 8 bytes per descriptor pointer
    emitter.instruction("ldr x11, [x11, x12]");                                 // load the tag descriptor pointer for this class
    emitter.instruction("str x11, [sp, #8]");                                   // save descriptor pointer for the cleanup loop
    emitter.instruction("str xzr, [sp, #24]");                                  // initialize property index = 0

    // -- walk each property and release heap-backed values based on the descriptor tags --
    emitter.label("__rt_object_free_deep_loop");
    emitter.instruction("ldr x12, [sp, #24]");                                  // reload the current property index
    emitter.instruction("ldr x13, [sp, #16]");                                  // reload the total property count
    emitter.instruction("cmp x12, x13");                                        // have we visited every property slot?
    emitter.instruction("b.ge __rt_object_free_deep_struct");                   // finish once every property has been scanned

    emitter.instruction("ldr x9, [sp, #0]");                                    // reload the object pointer
    emitter.instruction("mov x10, #16");                                        // each property slot occupies 16 bytes
    emitter.instruction("mul x10, x12, x10");                                   // compute the property slot byte offset
    emitter.instruction("add x10, x10, #8");                                    // skip the leading class_id field
    emitter.instruction("ldr x14, [x9, x10]");                                  // load the property payload pointer / low word
    emitter.instruction("ldr x11, [sp, #8]");                                   // reload the descriptor pointer for this property slot
    emitter.instruction("ldrb w15, [x11, x12]");                                // load the compile-time property tag
    emitter.instruction("cmp x15, #1");                                         // is this a compile-time string property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // strings always release through the uniform helper
    emitter.instruction("cmp x15, #4");                                         // is this a compile-time indexed-array property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // arrays always release through the uniform helper
    emitter.instruction("cmp x15, #5");                                         // is this a compile-time associative-array property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // hashes always release through the uniform helper
    emitter.instruction("cmp x15, #6");                                         // is this a compile-time object property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // objects always release through the uniform helper
    emitter.instruction("cmp x15, #7");                                         // is this a compile-time mixed property?
    emitter.instruction("b.eq __rt_object_free_deep_release_runtime");          // mixed payloads may or may not be heap-backed, but decref_any handles both safely
    emitter.instruction("b __rt_object_free_deep_next");                        // scalars and nulls need no cleanup

    emitter.label("__rt_object_free_deep_release_runtime");
    emitter.instruction("mov x0, x14");                                         // move the property payload pointer into the uniform release helper arg reg
    emitter.instruction("str x12, [sp, #24]");                                  // preserve the property index across the helper call
    emitter.instruction("bl __rt_decref_any");                                  // release the heap-backed property payload if needed
    emitter.instruction("ldr x12, [sp, #24]");                                  // restore the property index after the helper call

    emitter.label("__rt_object_free_deep_next");
    emitter.instruction("add x12, x12, #1");                                    // advance to the next property slot
    emitter.instruction("str x12, [sp, #24]");                                  // save the updated property index
    emitter.instruction("b __rt_object_free_deep_loop");                        // continue scanning property slots

    // -- free the object storage itself --
    emitter.label("__rt_object_free_deep_struct");

    // -- if the object carries a #[\AllowDynamicProperties] hashtable, free it --
    // The presence of the dyn_props slot is encoded in the payload size: the
    // base layout is `8 + num_props * 16` (always a multiple of 16 plus 8 for
    // the class_id field), so an extra 8-byte tail signals an ADP slot at
    // offset `size - 16` from the object payload start.
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer for the dyn_props check
    emitter.instruction("ldr w9, [x0, #-16]");                                  // load the object payload size from the heap header
    emitter.instruction("sub x9, x9, #8");                                      // subtract the leading class_id field
    emitter.instruction("and x10, x9, #15");                                    // isolate the low 4 bits of the property region size
    emitter.instruction("cmp x10, #8");                                         // 8 leftover bytes signal a dyn_props pointer slot
    emitter.instruction("b.ne __rt_object_free_deep_no_dyn_props");             // no dyn_props tail → skip hashtable cleanup
    emitter.instruction("sub x9, x9, #8");                                      // back out the dyn_props slot from the property region size
    emitter.instruction("add x9, x9, #8");                                      // re-add the leading class_id offset to land on the dyn_props slot
    emitter.instruction("ldr x11, [x0, x9]");                                   // load the dyn_props hashtable pointer from the slot
    emitter.instruction("cbz x11, __rt_object_free_deep_no_dyn_props");         // null hashtables (lazy init never happened) need no cleanup
    emitter.instruction("mov x0, x11");                                         // pass the hashtable pointer to the uniform decref helper
    emitter.instruction("bl __rt_decref_any");                                  // release the dyn_props hashtable through the uniform helper

    emitter.label("__rt_object_free_deep_no_dyn_props");
    crate::codegen::abi::emit_symbol_address(emitter, "x9", "_gc_release_suppressed");
    emitter.instruction("str xzr, [x9]");                                       // clear release suppression before freeing the object storage
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the object pointer before freeing it
    emitter.instruction("bl __rt_heap_free");                                   // return the object storage to the heap allocator
    emitter.instruction("ldp x29, x30, [sp, #32]");                             // restore frame pointer and return address
    emitter.instruction("add sp, sp, #48");                                     // tear down the object cleanup stack frame

    emitter.label("__rt_object_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller
}

/// Emits the `__rt_object_free_deep` runtime helper for x86_64 Linux.
/// Mirrors the ARM64 deep-free logic but uses the x86_64 ABI and heap marker layout.
/// Validates the x86_64 heap magic word and heap-kind tag before descending into
/// per-class property cleanup.
///
/// Input:  rax = object pointer (heap-backed, non-null, with x86_64 heap magic marker)
/// Output: none (rax = 0 on return via `__rt_object_free_deep_done`)
/// Clobbers: rax, r10, r11, rcx, r8, r9, and the x86_64 C call-clobbered set
/// Special cases: Fiber, Generator, SPL doubly-linked-list, SplFixedArray.
fn emit_object_free_deep_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: object_free_deep ---");
    emitter.label_global("__rt_object_free_deep");

    emitter.instruction("test rax, rax");                                       // skip null object pointers immediately because they do not own heap storage
    emitter.instruction("jz __rt_object_free_deep_done");                       // null objects need no deep-free work
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the stamped x86_64 heap kind word from the uniform header
    emitter.instruction("mov r11, r10");                                        // preserve the full heap kind word before isolating the ownership marker and heap kind
    emitter.instruction("shr r11, 32");                                         // isolate the high-word heap marker used by the x86_64 heap wrapper
    emitter.instruction(&format!("cmp r11d, 0x{:x}", X86_64_HEAP_MAGIC_HI32));  // ignore foreign pointers that do not carry the elephc x86_64 heap marker
    emitter.instruction("jne __rt_object_free_deep_done");                      // only elephc-owned objects participate in x86_64 deep-free bookkeeping
    emitter.instruction("and r10, 0xff");                                       // isolate the low-byte uniform heap kind tag for a final ownership sanity check
    emitter.instruction("cmp r10, 4");                                          // is this heap-backed payload really an object instance?
    emitter.instruction("jne __rt_object_free_deep_done");                      // other heap kinds must not be released through the object deep-free helper
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before reserving object deep-free spill slots
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame base for the saved object pointer, descriptor pointer, count, and loop index
    emitter.instruction("sub rsp, 32");                                         // reserve local storage for the object pointer, descriptor pointer, property count, and loop index
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // save the object pointer across nested helper calls while releasing properties
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_gc_release_suppressed");
    emitter.instruction("mov QWORD PTR [r10], 1");                              // suppress nested collector runs while this object deep-free walk releases property payloads

    // -- run the class's PHP __destruct (if any) before releasing properties --
    // The receiver is still fully constructed here; the helper resolves the
    // destructor from the object's class_id and runs it with $this borrowed.
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // load the object pointer as $this for the destructor call
    emitter.instruction("call __rt_call_object_destructor");                    // run the class's __destruct hook if one is declared
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the object pointer after the destructor returns

    // -- Fiber special case: release the per-fiber stack before the standard struct free path --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // r10 = receiver class_id
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r11", "_fiber_class_id", 0); // r11 = compile-time class id of the built-in Fiber class
    emitter.instruction("cmp r10, r11");                                        // is the receiver a Fiber instance?
    emitter.instruction("jne __rt_object_free_deep_not_fiber");                 // skip the fiber-specific cleanup path for non-Fiber receivers
    emitter.instruction(&format!("mov rdi, QWORD PTR [rax + {}]", crate::codegen::runtime::FIBER_STACK_BASE_OFFSET)); // rdi = fiber stack_base
    emitter.instruction("test rdi, rdi");                                       // does this Fiber still own a mapped stack?
    emitter.instruction("je __rt_object_free_deep_fiber_no_stack");             // skip when the stack was already released
    emitter.instruction(&format!("mov rsi, QWORD PTR [rax + {}]", crate::codegen::runtime::FIBER_STACK_SIZE_OFFSET)); // rsi = total mapped length
    emitter.instruction("call __rt_fiber_free_stack");                          // return the per-fiber stack to the kernel via munmap
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved Fiber object pointer
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen::runtime::FIBER_STACK_BASE_OFFSET)); // null stack_base for double-free safety
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen::runtime::FIBER_STACK_SIZE_OFFSET)); // null stack_size to mirror the cleared base
    emitter.label("__rt_object_free_deep_fiber_no_stack");

    // -- release runtime-owned Fiber transfer and pending exception values --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved Fiber object pointer before releasing transfer_value
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", crate::codegen::runtime::FIBER_TRANSFER_VALUE_OFFSET)); // rax = boxed Mixed transfer_value owned by the Fiber
    emitter.instruction("call __rt_decref_mixed");                              // release the Fiber's retained transfer value, if any
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved Fiber object pointer after transfer cleanup
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen::runtime::FIBER_TRANSFER_VALUE_OFFSET)); // clear transfer_value.lo after releasing it
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen::runtime::FIBER_TRANSFER_VALUE_OFFSET + 8)); // clear transfer_value.hi to match the empty slot
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", crate::codegen::runtime::FIBER_PENDING_THROW_OFFSET)); // rax = pending Throwable object parked by Fiber::throw/escape
    emitter.instruction("call __rt_decref_any");                                // release a pending Throwable if one is still attached
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved Fiber object pointer after pending_throw cleanup
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen::runtime::FIBER_PENDING_THROW_OFFSET)); // clear pending_throw after releasing it

    // -- release the callable descriptor retained by the Fiber object itself --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved Fiber object pointer before descriptor cleanup
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", crate::codegen::runtime::FIBER_CALLABLE_OFFSET)); // load the callable descriptor stored on the Fiber
    emitter.instruction("call __rt_callable_descriptor_release");               // release dynamic descriptor captures held by the Fiber callable
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved Fiber object pointer after descriptor cleanup
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen::runtime::FIBER_CALLABLE_OFFSET)); // clear callable descriptor after release

    let user_arg_max_off = crate::codegen::runtime::FIBER_USER_ARG_MAX_OFFSET;
    let start_args_off = crate::codegen::runtime::FIBER_START_ARGS_OFFSET;
    emitter.instruction(&format!("mov r10, QWORD PTR [rax + {}]", user_arg_max_off)); // r10 = legacy user_arg_max capture boundary
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // park user_arg_max in the existing loop-index slot
    for i in 0..crate::codegen::runtime::FIBER_START_ARGS_MAX {
        let skip_label = format!("__rt_object_free_deep_fiber_capture_skip_{}", i);
        emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                   // reload user_arg_max after any nested release helper
        emitter.instruction(&format!("cmp r10, {}", i));                        // is slot i still inside the visible start-arg region?
        emitter.instruction(&format!("jg {}", skip_label));                     // skip visible start-arg slots; only legacy captures need release
        emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                    // reload the saved Fiber object pointer
        emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", start_args_off + i * 8)); // rax = legacy trailing capture payload
        emitter.instruction("call __rt_decref_any");                            // release the legacy capture heap payload if present
        emitter.label(&skip_label);
    }
    emitter.instruction("jmp __rt_object_free_deep_struct");                    // skip property-tag walking for runtime-managed Fiber payloads
    emitter.label("__rt_object_free_deep_not_fiber");

    // -- Generator special case: a fiber-shaped coroutine object whose payload is
    // runtime-managed (coroutine stack + boxed Mixed fields), not PHP property
    // slots. Release the stack and every owned field, then free the struct. --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // r10 = receiver class_id
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r11", "_generator_class_id", 0); // r11 = compile-time class id of the built-in Generator class
    emitter.instruction("cmp r10, r11");                                        // is the receiver a Generator coroutine?
    emitter.instruction("jne __rt_object_free_deep_not_generator");             // skip generator cleanup for ordinary PHP objects
    emit_generator_coroutine_release_x86_64(emitter);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved Generator pointer before the struct free
    emitter.instruction("jmp __rt_object_free_deep_struct");                    // free the coroutine struct without walking property descriptors
    emitter.label("__rt_object_free_deep_not_generator");

    // -- SPL doubly-linked-list family: release custom internal storage, not PHP property slots --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // r10 = receiver class_id
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r11", "_spl_dll_class_id", 0); // r11 = class id of SplDoublyLinkedList
    emitter.instruction("cmp r10, r11");                                        // is the receiver a SplDoublyLinkedList?
    emitter.instruction("je __rt_object_free_deep_spl_dll");                    // release SPL list storage for SplDoublyLinkedList
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r11", "_spl_stack_class_id", 0); // r11 = class id of SplStack
    emitter.instruction("cmp r10, r11");                                        // is the receiver a SplStack?
    emitter.instruction("je __rt_object_free_deep_spl_dll");                    // release SPL list storage for SplStack
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r11", "_spl_queue_class_id", 0); // r11 = class id of SplQueue
    emitter.instruction("cmp r10, r11");                                        // is the receiver a SplQueue?
    emitter.instruction("jne __rt_object_free_deep_not_spl_dll");               // skip SPL list cleanup for ordinary objects
    emitter.label("__rt_object_free_deep_spl_dll");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved SPL list object pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", crate::codegen::runtime::spl::SPL_DLL_STORAGE_OFFSET)); // load the owned internal Mixed storage array
    emitter.instruction("call __rt_decref_array");                              // release the internal array and its owned Mixed cells
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved SPL list object pointer after storage release
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen::runtime::spl::SPL_DLL_STORAGE_OFFSET)); // clear the storage pointer after release
    emitter.instruction("jmp __rt_object_free_deep_no_dyn_props");              // free the custom object storage without generic descriptor walking
    emitter.label("__rt_object_free_deep_not_spl_dll");

    // -- SplFixedArray custom storage: release the owned fixed-size Mixed array --
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // r10 = receiver class_id
    crate::codegen::abi::emit_load_symbol_to_reg(emitter, "r11", "_spl_fixed_array_class_id", 0); // r11 = class id of SplFixedArray
    emitter.instruction("cmp r10, r11");                                        // is the receiver a SplFixedArray?
    emitter.instruction("jne __rt_object_free_deep_not_spl_fixed");             // skip fixed-array cleanup for ordinary objects
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved SplFixedArray object pointer
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", crate::codegen::runtime::spl::SPL_FIXED_STORAGE_OFFSET)); // load the owned fixed-array storage
    emitter.instruction("call __rt_decref_array");                              // release fixed-array storage and its owned Mixed cells
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved SplFixedArray object pointer after storage release
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", crate::codegen::runtime::spl::SPL_FIXED_STORAGE_OFFSET)); // clear storage pointer after release
    emitter.instruction("jmp __rt_object_free_deep_no_dyn_props");              // free custom fixed-array storage without generic descriptor walking
    emitter.label("__rt_object_free_deep_not_spl_fixed");

    emitter.instruction("mov r10d, DWORD PTR [rax - 16]");                      // load the object payload size from the uniform heap header
    emitter.instruction("sub r10, 8");                                          // subtract the leading class_id field from the payload size to isolate property storage
    emitter.instruction("shr r10, 4");                                          // divide by 16 because every property slot occupies two qwords
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the total property count for the deep-free loop
    emitter.instruction("mov r10, QWORD PTR [rax]");                            // load the runtime class id from the object payload
    abi::emit_cmp_reg_to_symbol(emitter, "r10", "_class_gc_desc_count");        // is the runtime class id within the emitted descriptor table?
    emitter.instruction("jae __rt_object_free_deep_struct");                    // invalid class ids fall back to a shallow object free on x86_64
    abi::emit_symbol_address(emitter, "r11", "_class_gc_desc_ptrs");            // materialize the base address of the class property-tag descriptor table
    emitter.instruction("mov r11, QWORD PTR [r11 + r10 * 8]");                  // load the property-tag descriptor pointer for this object class
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // save the descriptor pointer for the object-property cleanup loop
    emitter.instruction("mov QWORD PTR [rbp - 32], 0");                         // initialize the object-property loop index to zero

    emitter.label("__rt_object_free_deep_loop");
    emitter.instruction("mov r10, QWORD PTR [rbp - 32]");                       // reload the current object-property index at the top of every loop iteration
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // have we already scanned every property slot owned by this object?
    emitter.instruction("jae __rt_object_free_deep_struct");                    // finish once the property index reaches the saved property count
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the object pointer after any nested helper call
    emitter.instruction("mov rcx, r10");                                        // copy the current property index before scaling it into a byte offset
    emitter.instruction("shl rcx, 4");                                          // convert the property index into a 16-byte property-slot offset
    emitter.instruction("add rcx, 8");                                          // skip the leading class_id field to land on the low word of the property slot
    emitter.instruction("mov rax, QWORD PTR [r11 + rcx]");                      // load the low word of the current property slot as the potential heap-backed child pointer
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the per-class property-tag descriptor pointer after any nested helper call
    emitter.instruction("movzx r8, BYTE PTR [r11 + r10]");                      // load the compile-time property tag for the current property slot
    emitter.instruction("cmp r8, 1");                                           // does the property hold a persisted string pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // strings release through the uniform x86_64 decref_any helper
    emitter.instruction("cmp r8, 4");                                           // does the property hold a nested indexed-array pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // indexed arrays release through the uniform x86_64 decref_any helper
    emitter.instruction("cmp r8, 5");                                           // does the property hold a nested associative-array pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // associative arrays release through the uniform x86_64 decref_any helper
    emitter.instruction("cmp r8, 6");                                           // does the property hold a nested object pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // objects release through the uniform x86_64 decref_any helper
    emitter.instruction("cmp r8, 7");                                           // does the property hold a boxed mixed pointer?
    emitter.instruction("je __rt_object_free_deep_release_runtime");            // mixed cells release through the uniform x86_64 decref_any helper
    emitter.instruction("jmp __rt_object_free_deep_next");                      // scalar, float, and null property slots need no heap cleanup

    emitter.label("__rt_object_free_deep_release_runtime");
    emitter.instruction("call __rt_decref_any");                                // release the heap-backed property payload if the current property slot owns one

    emitter.label("__rt_object_free_deep_next");
    emitter.instruction("add QWORD PTR [rbp - 32], 1");                         // advance the property index to the next slot in the object layout
    emitter.instruction("jmp __rt_object_free_deep_loop");                      // continue scanning property slots until the whole object payload is released

    emitter.label("__rt_object_free_deep_struct");

    // -- if the object carries a #[\AllowDynamicProperties] hashtable, free it --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the object pointer for the dyn_props check
    emitter.instruction("mov r10d, DWORD PTR [rax - 16]");                      // load the object payload size from the heap header
    emitter.instruction("sub r10, 8");                                          // subtract the leading class_id field
    emitter.instruction("mov r11, r10");                                        // copy the property region size before isolating the low nibble
    emitter.instruction("and r11, 15");                                         // isolate the low 4 bits of the property region size
    emitter.instruction("cmp r11, 8");                                          // 8 leftover bytes signal a dyn_props pointer slot
    emitter.instruction("jne __rt_object_free_deep_no_dyn_props");              // no dyn_props tail → skip hashtable cleanup
    emitter.instruction("sub r10, 8");                                          // back out the dyn_props slot from the property region size
    emitter.instruction("add r10, 8");                                          // re-add the leading class_id offset to land on the dyn_props slot
    emitter.instruction("mov r11, QWORD PTR [rax + r10]");                      // load the dyn_props hashtable pointer from the slot
    emitter.instruction("test r11, r11");                                       // null hashtables (lazy init never happened) need no cleanup
    emitter.instruction("jz __rt_object_free_deep_no_dyn_props");               // skip cleanup for null dyn_props slot
    emitter.instruction("mov rax, r11");                                        // pass the hashtable pointer to the uniform decref helper
    emitter.instruction("call __rt_decref_any");                                // release the dyn_props hashtable through the uniform helper

    emitter.label("__rt_object_free_deep_no_dyn_props");
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the object pointer after finishing the optional property cleanup pass
    crate::codegen::abi::emit_symbol_address(emitter, "r10", "_gc_release_suppressed");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // re-enable targeted collector runs now that the object deep-free walk is complete
    emitter.instruction("call __rt_heap_free");                                 // release the object storage itself through the x86_64 heap wrapper
    emitter.instruction("add rsp, 32");                                         // release the spill slots reserved for the object deep-free scan state
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning to generated code

    emitter.label("__rt_object_free_deep_done");
    emitter.instruction("ret");                                                 // return to the caller after releasing the object and any owned heap-backed properties
}

/// Emits a single `__rt_decref_mixed` call to release one boxed Mixed field from a Generator frame on ARM64.
/// Used for last_key, last_value, return_value, and sent_value fields.
///
/// Input:  x0 = Generator frame pointer (reloaded from [sp, #0])
///         offset = byte offset of the boxed Mixed field within the frame
///         name = field name for the emitted comment
/// Output: none
fn emit_generator_mixed_field_release_aarch64(emitter: &mut Emitter, offset: usize, name: &str) {
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the saved Generator frame pointer before field cleanup
    emitter.instruction(&format!("ldr x0, [x0, #{}]", offset));                 // load the Generator frame's boxed Mixed field for release
    emitter.instruction("bl __rt_decref_mixed");                                // release the Generator frame's boxed Mixed field if present
    emitter.comment(&format!("released Generator::{}", name));
}

/// Emits a single `__rt_decref_mixed` call to release one boxed Mixed field from a Generator frame on x86_64.
/// Used for last_key, last_value, return_value, and sent_value fields.
///
/// Input:  rax = Generator frame pointer (reloaded from [rbp - 8])
///         offset = byte offset of the boxed Mixed field within the frame
///         name = field name for the emitted comment
/// Output: none
fn emit_generator_mixed_field_release_x86_64(emitter: &mut Emitter, offset: usize, name: &str) {
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the saved Generator frame pointer before field cleanup
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", offset));     // load the Generator frame's boxed Mixed field for release
    emitter.instruction("call __rt_decref_mixed");                              // release the Generator frame's boxed Mixed field if present
    emitter.comment(&format!("released Generator::{}", name));
}

/// Releases all runtime-owned storage of a fiber-shaped Generator on AArch64:
/// the coroutine stack (munmap), the boxed `transfer_value`, the pending
/// Throwable, every boxed `start_args` cell, and the persistent
/// `last_key`/`last_value`/`return_value` cells.
///
/// Input: the Generator pointer is saved at `[sp, #0]` (reloaded per step,
/// because nested decref helpers clobber `x0`).
fn emit_generator_coroutine_release_aarch64(emitter: &mut Emitter) {
    use crate::codegen::runtime::generators::coro;
    let stack_base = crate::codegen::runtime::FIBER_STACK_BASE_OFFSET;
    let stack_size = crate::codegen::runtime::FIBER_STACK_SIZE_OFFSET;
    let transfer = crate::codegen::runtime::FIBER_TRANSFER_VALUE_OFFSET;
    let pending = crate::codegen::runtime::FIBER_PENDING_THROW_OFFSET;
    let start_args = crate::codegen::runtime::FIBER_START_ARGS_OFFSET as usize;

    // -- return the coroutine stack to the kernel --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Generator pointer for stack release
    emitter.instruction(&format!("ldr x9, [x0, #{}]", stack_base));             // x9 = coroutine stack base (mmap start)
    emitter.instruction("cbz x9, __rt_object_free_deep_gen_no_stack");          // skip when the stack was already released
    emitter.instruction(&format!("ldr x10, [x0, #{}]", stack_size));            // x10 = mapped length for munmap
    emitter.instruction("mov x0, x9");                                          // pass the stack base as the mapping start
    emitter.instruction("mov x1, x10");                                         // pass the mapped length to unmap
    emitter.instruction("bl __rt_fiber_free_stack");                            // munmap the coroutine stack
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Generator pointer after the munmap
    emitter.instruction(&format!("str xzr, [x0, #{}]", stack_base));            // null stack_base so a double-free is a clean no-op
    emitter.instruction(&format!("str xzr, [x0, #{}]", stack_size));            // null stack_size to mirror the cleared base
    emitter.label("__rt_object_free_deep_gen_no_stack");

    // -- release the boxed transfer value and any pending Throwable --
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Generator pointer before transfer cleanup
    emitter.instruction(&format!("ldr x0, [x0, #{}]", transfer));               // x0 = boxed Mixed transfer_value
    emitter.instruction("bl __rt_decref_mixed");                                // release the transfer value if present
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Generator pointer after transfer cleanup
    emitter.instruction(&format!("str xzr, [x0, #{}]", transfer));              // clear transfer_value after releasing it
    emitter.instruction(&format!("ldr x0, [x0, #{}]", pending));                // x0 = pending Throwable object, if any
    emitter.instruction("bl __rt_decref_any");                                  // release a pending Throwable if still attached
    emitter.instruction("ldr x0, [sp, #0]");                                    // reload the Generator pointer after pending cleanup
    emitter.instruction(&format!("str xzr, [x0, #{}]", pending));               // clear pending_throw after releasing it

    // -- release the owned boxed start arguments forwarded into the body --
    for i in 0..crate::codegen::runtime::FIBER_START_ARGS_MAX as usize {
        emit_generator_mixed_field_release_aarch64(
            emitter,
            start_args + i * 8,
            &format!("start_args[{}]", i),
        );
    }

    // -- release the generator's persistent key/value/return cells --
    emit_generator_mixed_field_release_aarch64(emitter, coro::GEN_LAST_KEY_OFFSET as usize, "last_key");
    emit_generator_mixed_field_release_aarch64(emitter, coro::GEN_LAST_VALUE_OFFSET as usize, "last_value");
    emit_generator_mixed_field_release_aarch64(emitter, coro::GEN_RETURN_VALUE_OFFSET as usize, "return_value");
}

/// x86_64 counterpart of `emit_generator_coroutine_release_aarch64`. The
/// Generator pointer is saved at `[rbp - 8]` (reloaded per step).
fn emit_generator_coroutine_release_x86_64(emitter: &mut Emitter) {
    use crate::codegen::runtime::generators::coro;
    let stack_base = crate::codegen::runtime::FIBER_STACK_BASE_OFFSET;
    let stack_size = crate::codegen::runtime::FIBER_STACK_SIZE_OFFSET;
    let transfer = crate::codegen::runtime::FIBER_TRANSFER_VALUE_OFFSET;
    let pending = crate::codegen::runtime::FIBER_PENDING_THROW_OFFSET;
    let start_args = crate::codegen::runtime::FIBER_START_ARGS_OFFSET as usize;

    // -- return the coroutine stack to the kernel --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Generator pointer for stack release
    emitter.instruction(&format!("mov rdi, QWORD PTR [rax + {}]", stack_base)); // rdi = coroutine stack base (mmap start)
    emitter.instruction("test rdi, rdi");                                       // does the Generator still own a mapped stack?
    emitter.instruction("je __rt_object_free_deep_gen_no_stack");               // skip when the stack was already released
    emitter.instruction(&format!("mov rsi, QWORD PTR [rax + {}]", stack_size)); // rsi = mapped length for munmap
    emitter.instruction("call __rt_fiber_free_stack");                          // munmap the coroutine stack
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Generator pointer after the munmap
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", stack_base));   // null stack_base for double-free safety
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", stack_size));   // null stack_size to mirror the cleared base
    emitter.label("__rt_object_free_deep_gen_no_stack");

    // -- release the boxed transfer value and any pending Throwable --
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Generator pointer before transfer cleanup
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", transfer));   // rax = boxed Mixed transfer_value
    emitter.instruction("call __rt_decref_mixed");                              // release the transfer value if present
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Generator pointer after transfer cleanup
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", transfer));     // clear transfer_value after releasing it
    emitter.instruction(&format!("mov rax, QWORD PTR [rax + {}]", pending));    // rax = pending Throwable object, if any
    emitter.instruction("call __rt_decref_any");                                // release a pending Throwable if still attached
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // reload the Generator pointer after pending cleanup
    emitter.instruction(&format!("mov QWORD PTR [rax + {}], 0", pending));      // clear pending_throw after releasing it

    // -- release the owned boxed start arguments forwarded into the body --
    for i in 0..crate::codegen::runtime::FIBER_START_ARGS_MAX as usize {
        emit_generator_mixed_field_release_x86_64(
            emitter,
            start_args + i * 8,
            &format!("start_args[{}]", i),
        );
    }

    // -- release the generator's persistent key/value/return cells --
    emit_generator_mixed_field_release_x86_64(emitter, coro::GEN_LAST_KEY_OFFSET as usize, "last_key");
    emit_generator_mixed_field_release_x86_64(emitter, coro::GEN_LAST_VALUE_OFFSET as usize, "last_value");
    emit_generator_mixed_field_release_x86_64(emitter, coro::GEN_RETURN_VALUE_OFFSET as usize, "return_value");
}
