//! Purpose:
//! Copies native callable descriptor headers and initializes fresh capture storage.
//!
//! Called from:
//! - Compiler callable construction and closure binding.
//!
//! Key details:
//! - The caller supplies the descriptor metadata and manages capture values and handles.
//! - Fresh captures must be zeroed before the heap header publishes the descriptor as callable.

use crate::layout::{CALLABLE_HEAP_KIND, x86_64_heap_kind_word};
use crate::{abi, Arch, Emitter};

pub use super::layout::HEADER_BYTES;

/// Width of one runtime capture pair in the native descriptor.
pub const CAPTURE_BYTES: usize = 16;
/// Shift from runtime capture index to its byte displacement.
pub const CAPTURE_SHIFT: u32 = CAPTURE_BYTES.trailing_zeros();

/// Returns the offset of a selected capture pair.
pub const fn capture_offset(index: usize) -> usize {
    HEADER_BYTES + index * CAPTURE_BYTES
}

/// Selects whether the caller needs a distinct PHP object identity for the descriptor.
#[derive(Clone, Copy)]
pub enum ObjectHandle {
    /// An internal callable descriptor does not publish its own public identity.
    Internal,
    /// Reserve the handle before initializing the allocated descriptor.
    Public,
}

/// Allocates descriptor storage and places its pointer in the selected register.
pub fn emit_allocate_descriptor(
    emitter: &mut impl Emitter, bytes: usize, destination: &str, handle: ObjectHandle,
) {
    let result = emitter.int_result_reg();
    abi::emit_load_int_immediate(emitter, result, bytes as i64);
    abi::emit_call_label(emitter, "__rt_heap_alloc");
    if matches!(handle, ObjectHandle::Public) {
        crate::object_handles::emit_acquire_object_handle(emitter);
    }
    emitter.emit_reg_move(destination, result);
}

/// Builds an owned descriptor with a static template header and cleared captures.
pub fn emit_new_from_static(
    emitter: &mut impl Emitter, bytes: usize, destination: &str, template: &str, handle: ObjectHandle,
) {
    emit_allocate_descriptor(emitter, bytes, destination, handle);
    emit_copy_static_descriptor_to_runtime(emitter, destination, template);
}

/// Copies a known-live descriptor from the result register, preserving its address across allocation.
pub fn emit_new_from_result_with_handle(
    emitter: &mut impl Emitter,
    destination: &str,
    handle: ObjectHandle,
) {
    let result = emitter.int_result_reg();
    let source = emitter.symbol_scratch_reg();
    emitter.emit_push_reg(result);
    emit_allocate_descriptor(emitter, HEADER_BYTES, destination, handle);
    emitter.emit_pop_reg(source);
    emit_copy_descriptor_header_to_runtime(emitter, destination, source);
    emitter.emit_reg_move(result, destination);
}

/// Copies a known-live descriptor from the result register and gives the copy public object identity.
pub fn emit_new_from_result(emitter: &mut impl Emitter, destination: &str) {
    emit_new_from_result_with_handle(emitter, destination, ObjectHandle::Public);
}

/// Stores one or two prepared ABI registers in a previously selected runtime capture.
pub fn emit_store_capture(
    emitter: &mut impl Emitter, descriptor: &str, index: usize, low: &str, high: Option<&str>,
) {
    let offset = capture_offset(index);
    abi::emit_store_to_address(emitter, low, descriptor, offset);
    if let Some(high) = high {
        abi::emit_store_to_address(emitter, high, descriptor, offset + 8);
    }
}

/// Loads one or two words of a previously selected runtime capture.
pub fn emit_load_capture(
    emitter: &mut impl Emitter, descriptor: &str, index: usize, low: &str, high: Option<&str>,
) {
    let offset = capture_offset(index);
    abi::emit_load_from_address(emitter, low, descriptor, offset);
    if let Some(high) = high {
        abi::emit_load_from_address(emitter, high, descriptor, offset + 8);
    }
}

/// Adds semantic flags to the kind word of an initialized runtime descriptor.
pub fn emit_or_kind_flags(emitter: &mut impl Emitter, descriptor: &str, flags: u64) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [{descriptor}]"));            // load the descriptor kind word
            emitter.instruction(&format!("orr x9, x9, #{flags}"));              // add the selected semantic descriptor flags
            emitter.instruction(&format!("str x9, [{descriptor}]"));            // publish the tagged kind word
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("or QWORD PTR [{descriptor}], {flags}")); // add the selected semantic descriptor flags
        }
    }
}

/// Branches on either of two single-bit flags stored in a runtime descriptor kind word.
pub fn emit_branch_on_kind_flags(
    emitter: &mut impl Emitter,
    descriptor: &str,
    first: (u64, &str),
    second: (u64, &str),
) {
    debug_assert!(first.0.is_power_of_two());
    debug_assert!(second.0.is_power_of_two());
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [{descriptor}]"));            // load the descriptor kind word
            emitter.instruction(&format!("tbnz x9, #{}, {}", first.0.trailing_zeros(), first.1)); // branch when the first semantic flag is present
            emitter.instruction(&format!("tbnz x9, #{}, {}", second.0.trailing_zeros(), second.1)); // branch when the second semantic flag is present
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test QWORD PTR [{descriptor}], {}", first.0)); // test the first semantic descriptor flag
            emitter.instruction(&format!("jnz {}", first.1));                   // branch when the first semantic flag is present
            emitter.instruction(&format!("test QWORD PTR [{descriptor}], {}", second.0)); // test the second semantic descriptor flag
            emitter.instruction(&format!("jnz {}", second.1));                 // branch when the second semantic flag is present
        }
    }
}

/// Replaces the kind word of an initialized runtime descriptor.
pub fn emit_store_kind(emitter: &mut impl Emitter, descriptor: &str, kind: u64) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{kind}"));                   // materialize the descriptor kind
            emitter.instruction(&format!("str x9, [{descriptor}]"));            // publish the replacement kind word
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [{descriptor}], {kind}")); // publish the replacement kind word
        }
    }
}

/// Materializes the descriptor for a staged borrowed object receiver and its independent owner.
pub fn emit_new_with_staged_receiver(
    emitter: &mut impl Emitter, template: &str, receiver_offset: usize, destination: &str,
) {
    let receiver = emitter.int_result_reg();
    let descriptor = match emitter.arch() { Arch::AArch64 => "x11", Arch::X86_64 => "rcx" };
    emitter.emit_load_temporary_stack_slot(receiver, receiver_offset);
    abi::emit_call_label(emitter, "__rt_incref");
    emitter.emit_push_reg(receiver);
    emit_new_from_static(emitter, capture_offset(1), receiver, template, ObjectHandle::Internal);
    emitter.emit_push_reg(receiver);
    emitter.emit_load_temporary_stack_slot(descriptor, 0);
    emitter.emit_load_temporary_stack_slot(receiver, 16);
    emit_store_capture(emitter, descriptor, 0, receiver, None);
    emitter.emit_pop_reg(destination);
    emitter.emit_release_temporary_stack(16);
}

/// Copies only the fixed header, preserving any runtime capture slots.
pub fn emit_copy_runtime_header(
    emitter: &mut impl Emitter,
    dest: &str,
    source: &str,
    scratch: &str,
) {
    for offset in (0..HEADER_BYTES).step_by(8) {
        abi::emit_load_from_address(emitter, scratch, source, offset);
        abi::emit_store_to_address(emitter, scratch, dest, offset);
    }
}

/// Copies a static descriptor's header and initializes the capture storage of a fresh copy.
pub fn emit_copy_static_descriptor_to_runtime(
    emitter: &mut impl Emitter,
    dest: &str,
    descriptor_label: &str,
) {
    let source = emitter.symbol_scratch_reg();
    abi::emit_symbol_address(emitter, source, descriptor_label);
    emit_copy_descriptor_header_to_runtime(emitter, dest, source);
}

/// Copies a descriptor at a known address and initializes the fresh callable allocation.
/// The source register must differ from the secondary scratch register used for the copy.
pub fn emit_copy_descriptor_header_to_runtime(emitter: &mut impl Emitter, dest: &str, source: &str) {
    let scratch = emitter.secondary_scratch_reg();
    emit_copy_runtime_header(emitter, dest, source, scratch);
    emit_initialize_runtime_descriptor(emitter, dest);
}

/// Clears capture storage before publishing a fresh allocation as a managed callable node.
pub fn emit_initialize_runtime_descriptor(emitter: &mut impl Emitter, dest: &str) {
    // -- recycled captures must be null before any later allocation or ownership cleanup --
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr w9, [{dest}, #-16]"));            // bound initialization by the allocator's actual payload capacity
            emitter.instruction(&format!("mov x10, #{HEADER_BYTES}"));          // preserve the copied fixed descriptor header
            emitter.label("1");
            emitter.instruction("cmp x10, x9");                                 // stop before crossing the allocated descriptor payload
            emitter.instruction("b.hs 2f");                                     // descriptors without captures need no slot initialization
            emitter.instruction(&format!("str xzr, [{dest}, x10]"));            // leave every not-yet-written capture null for nested cleanup
            emitter.instruction("add x10, x10, #8");                            // initialize the next capture word, including metadata halves
            emitter.instruction("b 1b");                                        // finish initialization before publishing the graph kind
            emitter.label("2");
            emitter.instruction(&format!("mov x9, #{CALLABLE_HEAP_KIND}"));     // register callable ownership independently from raw helper buffers
            emitter.instruction(&format!("str x9, [{dest}, #-8]"));             // publish the canonical managed callable kind
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r11d, DWORD PTR [{dest} - 16]")); // bound initialization by the allocator's actual payload capacity
            emitter.instruction(&format!("mov r10d, {HEADER_BYTES}"));          // preserve the copied fixed descriptor header
            emitter.label("1");
            emitter.instruction("cmp r10, r11");                                // stop before crossing the allocated descriptor payload
            emitter.instruction("jae 2f");                                      // descriptors without captures need no slot initialization
            emitter.instruction(&format!("mov QWORD PTR [{dest} + r10], 0"));   // leave every not-yet-written capture null for nested cleanup
            emitter.instruction("add r10, 8");                                  // initialize the next capture word, including metadata halves
            emitter.instruction("jmp 1b");                                      // finish initialization before publishing the graph kind
            emitter.label("2");
            emitter.instruction(&format!("mov r10, 0x{:x}", x86_64_heap_kind_word(CALLABLE_HEAP_KIND))); // preserve canonical heap magic while registering callable ownership
            emitter.instruction(&format!("mov QWORD PTR [{dest} - 8], r10"));   // publish the canonical managed callable kind
        }
    }
}

/// Offset of the non-owning foreign context capture in a callback adapter.
pub const FOREIGN_CONTEXT_CAPTURE_OFFSET: usize = HEADER_BYTES;
/// Offset of the independently retained callback capture in a callback adapter.
pub const CALLBACK_CAPTURE_OFFSET: usize = FOREIGN_CONTEXT_CAPTURE_OFFSET + 16;

/// Native spill location for a staged callback-adapter capture.
#[derive(Clone, Copy)]
pub enum CaptureSlot {
    /// Fixed offset below the frame pointer.
    Frame(usize),
    /// Fixed offset from the current stack pointer.
    Stack(usize),
    /// First word of a temporary stack lease.
    Top,
}

impl CaptureSlot {
    /// Reloads a staged capture without changing its ownership.
    fn emit_load(self, emitter: &mut impl Emitter, scratch: &str) {
        match self {
            Self::Frame(offset) => abi::load_at_offset(emitter, scratch, offset),
            Self::Stack(offset) => match emitter.arch() {
                Arch::AArch64 => emitter.instruction(&format!("ldr {scratch}, [sp, #{offset}]")), // reload the staged capture with its explicit stack displacement
                Arch::X86_64 => emitter.emit_load_temporary_stack_slot(scratch, offset),
            },
            Self::Top => emitter.emit_load_temporary_stack_slot(scratch, 0),
        }
    }
}

/// Allocates a fresh callback adapter and installs its two staged captures.
///
/// The caller keeps the foreign context valid and supplies an owned callback to transfer.
pub fn emit_callback_adapter(
    emitter: &mut impl Emitter,
    template: &str,
    scratch: &str,
    context: CaptureSlot,
    callback: CaptureSlot,
) {
    let result = emitter.int_result_reg();
    emit_new_from_static(emitter, capture_offset(2), result, template, ObjectHandle::Internal);
    context.emit_load(emitter, scratch);
    abi::emit_store_to_address(emitter, scratch, result, FOREIGN_CONTEXT_CAPTURE_OFFSET);
    callback.emit_load(emitter, scratch);
    abi::emit_store_to_address(emitter, scratch, result, CALLBACK_CAPTURE_OFFSET);
}

/// Clones a validated receiver descriptor and its one or two capture slots from the binding frame.
pub fn emit_clone_bound_descriptor(emitter: &mut impl Emitter) {
    match emitter.arch() {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", capture_offset(1)));    // default to header plus the $this capture
            emitter.instruction("cmp x10, #2");                                 // does the descriptor carry called-class id too?
            emitter.instruction("b.ne __rt_closure_bind_allocate");             // keep the 80-byte allocation for top-level closures
            emitter.instruction(&format!("mov x0, #{}", capture_offset(2)));    // include the hidden 16-byte called-class capture
            emitter.label("__rt_closure_bind_allocate");
            emitter.instruction("bl __rt_heap_alloc");                          // x0 = fresh descriptor block
            emit_initialize_runtime_descriptor(emitter, "x0");
            emitter.instruction("bl __rt_object_handle_acquire");               // Closure::bind creates a NEW Closure in PHP, so it takes a new object handle
            emitter.instruction("str x0, [sp, #16]");                           // save the new descriptor pointer

            // -- copy the descriptor payload --
            emitter.instruction("ldr x1, [sp, #0]");                            // x1 = source descriptor
            emitter.instruction("ldp x2, x3, [x1, #0]");                        // copy header words 0-1 (kind, entry)
            emitter.instruction("stp x2, x3, [x0, #0]");                        // store header words 0-1
            emitter.instruction("ldp x2, x3, [x1, #16]");                       // copy header words 2-3 (name, name_len)
            emitter.instruction("stp x2, x3, [x0, #16]");                       // store header words 2-3
            emitter.instruction("ldp x2, x3, [x1, #32]");                       // copy header words 4-5 (signature, environment)
            emitter.instruction("stp x2, x3, [x0, #32]");                       // store header words 4-5
            emitter.instruction("ldp x2, x3, [x1, #48]");                       // copy header words 6-7 (invocation, invoker)
            emitter.instruction("stp x2, x3, [x0, #48]");                       // store header words 6-7
            emitter.instruction("ldp x2, x3, [x1, #64]");                       // copy the single capture slot (value, tag/len)
            emitter.instruction("stp x2, x3, [x0, #64]");                       // store the capture slot into the copy
            emitter.instruction("ldr x10, [sp, #32]");                          // reload the capture count for the optional slot
            emitter.instruction("cmp x10, #2");                                 // is called-class id present?
            emitter.instruction("b.ne __rt_closure_bind_captures_copied");      // the $this-only descriptor is complete
            emitter.instruction("ldp x2, x3, [x1, #80]");                       // copy called-class id and its unused high word
            emitter.instruction("stp x2, x3, [x0, #80]");                       // preserve late-static dispatch in the bound closure
            emitter.label("__rt_closure_bind_captures_copied");
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", capture_offset(1)));    // default to header plus the $this capture
            emitter.instruction("cmp QWORD PTR [rsp+32], 2");                   // does the descriptor carry called-class id too?
            emitter.instruction("jne __rt_closure_bind_allocate");              // keep the 80-byte allocation for top-level closures
            emitter.instruction(&format!("mov rax, {}", capture_offset(2)));    // include the hidden 16-byte called-class capture
            emitter.label("__rt_closure_bind_allocate");
            emitter.instruction("call __rt_heap_alloc");                        // rax = fresh descriptor block
            emit_initialize_runtime_descriptor(emitter, "rax");
            emitter.instruction("call __rt_object_handle_acquire");             // Closure::bind creates a NEW Closure in PHP, so it takes a new object handle
            emitter.instruction("mov [rsp+16], rax");                           // save the new descriptor pointer

            // -- copy the descriptor payload --
            emitter.instruction("mov rsi, [rsp+0]");                            // rsi = source descriptor
            emitter.instruction("mov rdi, rax");                                // rdi = destination descriptor
            emitter.instruction("mov rcx, 10");                                 // $this-only descriptor is ten 8-byte words
            emitter.instruction("cmp QWORD PTR [rsp+32], 2");                   // is called-class id present?
            emitter.instruction("jne __rt_closure_bind_copy");                  // copy the 80-byte descriptor
            emitter.instruction("mov rcx, 12");                                 // 96 bytes = twelve words with called-class id
            emitter.label("__rt_closure_bind_copy");
            emitter.instruction("cld");                                         // copy forward
            emitter.instruction("rep movsq");                                   // copy the descriptor payload word by word
        }
    }
}

/// The method tag in an aligned two-value callable selector.
pub const MIXED_METHOD_TAG_OFFSET: usize = 0;
/// The method's two payload words within the callable selector.
pub const MIXED_METHOD_PAYLOAD_OFFSET: usize = 16;
/// The receiver tag begins at the second aligned value.
pub const MIXED_RECEIVER_TAG_OFFSET: usize = 32;
/// The receiver's two payload words within the callable selector.
pub const MIXED_RECEIVER_PAYLOAD_OFFSET: usize = MIXED_RECEIVER_TAG_OFFSET + MIXED_METHOD_PAYLOAD_OFFSET;
/// Total bytes of callable selector storage.
pub const MIXED_SELECTOR_BYTES: usize = 2 * MIXED_RECEIVER_TAG_OFFSET;
