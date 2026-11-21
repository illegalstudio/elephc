//! Purpose:
//! Defines the native Fiber and Generator payload and emits its initial storage.
//!
//! Called from:
//! - The shared coroutine constructor, GC traversal and deep object release.
//!
//! Key details:
//! - The caller provides the initial frame layout selected by the context-switch ABI.
//! - Payload initialization preserves the target's saved registers and failure edge.

use crate::{Arch, Emitter, abi};

/// Byte offset of the `state` field (0=NotStarted 1=Running 2=Suspended 3=Terminated).
pub const FIBER_STATE_OFFSET: i32 = 8;
/// Byte offset of the `stack_base` field (low address of the fiber stack region, includes guard page).
pub const FIBER_STACK_BASE_OFFSET: i32 = 16;
/// Byte offset of the `stack_top` field (high address; initial SP for a fresh fiber, 16-aligned).
pub const FIBER_STACK_TOP_OFFSET: i32 = 24;
/// Byte offset of the `stack_size` field (total bytes of the mmap'd stack region, required by munmap).
pub const FIBER_STACK_SIZE_OFFSET: i32 = 32;
/// Byte offset of the `saved_sp` field (SP saved when this fiber is not running; entry point for first switch).
pub const FIBER_SAVED_SP_OFFSET: i32 = 40;
/// Byte offset of the `callable` field (closure/function pointer supplied to the Fiber constructor).
pub const FIBER_CALLABLE_OFFSET: i32 = 48;
/// Byte offset of the `callable_wrapper` field (generated ABI adapter that runs the Fiber body on first switch).
pub const FIBER_CALLABLE_WRAPPER_OFFSET: i32 = 56;
/// Byte offset of the `caller` field (Fiber* of the resumer, NULL when the fiber has not been started).
pub const FIBER_CALLER_OFFSET: i32 = 64;
/// Byte offset of the low word of `transfer_value` (Mixed cell holding the value in transit between fibers).
pub const FIBER_TRANSFER_VALUE_OFFSET: i32 = 72;
/// Byte offset of the `pending_throw` field (Throwable* scheduled by Fiber::throw; cleared before re-raising).
pub const FIBER_PENDING_THROW_OFFSET: i32 = 88;
/// Byte offset of `own_exc_head` (saved _exc_handler_top for this fiber's exception handler chain).
pub const FIBER_OWN_EXC_HEAD_OFFSET: i32 = 96;
/// Byte offset of `own_call_frame` (saved _exc_call_frame_top for this fiber's cleanup chain).
pub const FIBER_OWN_CALL_FRAME_OFFSET: i32 = 104;
/// Byte offset of the saved runtime-name `__set` reentrancy chain for this fiber.
pub const FIBER_OWN_MAGIC_SET_GUARD_OFFSET: i32 = 232;
/// Byte offset of the saved deep-release GC suppression for this execution context.
pub const FIBER_OWN_GC_RELEASE_SUPPRESSED_OFFSET: i32 = 240;

/// Main-stack counterpart of the coroutine resume pointer.
pub const MAIN_SAVED_SP: &str = "_fiber_main_saved_sp";
/// Parked main-stack magic-set guard head, also cleared at web request reset.
pub const MAIN_SAVED_MAGIC_SET_GUARD: &str = "_fiber_main_saved_magic_set_guard";

/// The same four values are parked in either the main globals or the Fiber object.
const SUSPENDED_WORDS: [(&str, &str, i32, &str); 4] = [
    ("_exc_handler_top", "_fiber_main_saved_exc", FIBER_OWN_EXC_HEAD_OFFSET, "x12"),
    ("_exc_call_frame_top", "_fiber_main_saved_call_frame", FIBER_OWN_CALL_FRAME_OFFSET, "x13"),
    ("_magic_set_guard_head", MAIN_SAVED_MAGIC_SET_GUARD, FIBER_OWN_MAGIC_SET_GUARD_OFFSET, "x14"),
    ("_gc_release_suppressed", "_fiber_main_saved_gc_release_suppressed", FIBER_OWN_GC_RELEASE_SUPPRESSED_OFFSET, "x14"),
];

/// Lists the saved main stack words in their fixed-data declaration order.
pub fn main_context_symbols() -> impl Iterator<Item = &'static str> {
    std::iter::once(MAIN_SAVED_SP).chain(SUSPENDED_WORDS.iter().map(|&(_, main, _, _)| main))
}

/// Storage where the native runtime chains are parked while their stack is inactive.
#[derive(Clone, Copy)]
pub enum SuspendedContext<'a> {
    /// Main-thread execution stores its saved chains in runtime globals.
    Main,
    /// A suspended Fiber stores its saved chains in the Fiber object.
    Fiber(&'a str),
}

impl SuspendedContext<'_> {
    /// Parks the current stack's runtime chains before switching to another stack.
    pub fn emit_save(self, emitter: &mut impl Emitter) {
        for (active, main, fiber, arm_register) in SUSPENDED_WORDS {
            let register = if emitter.arch() == Arch::AArch64 { arm_register } else { "r11" };
            emitter.emit_load_symbol_to_reg(register, active, 0);
            match self {
                Self::Main => emitter.emit_store_reg_to_symbol(register, main, 0),
                Self::Fiber(base) => emitter.emit_store_to_address(register, base, fiber as usize),
            }
        }
    }

    /// Restores the selected stack's runtime chains before adopting its saved SP.
    pub fn emit_restore(self, emitter: &mut impl Emitter) {
        for (active, main, fiber, arm_register) in SUSPENDED_WORDS {
            let register = if emitter.arch() == Arch::AArch64 { arm_register } else { "r11" };
            match self {
                Self::Main => emitter.emit_load_symbol_to_reg(register, main, 0),
                Self::Fiber(base) => emitter.emit_load_from_address(register, base, fiber as usize),
            }
            emitter.emit_store_reg_to_symbol(register, active, 0);
        }
    }
}
/// Byte offset of the first `start_args` slot (up to 7 Mixed pointers passed to Fiber::start; parallels AArch64 int arg regs).
pub const FIBER_START_ARGS_OFFSET: i32 = 112;
/// Maximum number of `start_args` slots (one per AArch64 integer argument register minus `$this`).
pub const FIBER_START_ARGS_MAX: i32 = 7;
/// Byte offset of `user_arg_max` (controls how many start_args slots start() may write).
pub const FIBER_USER_ARG_MAX_OFFSET: i32 = 168;
/// Byte offset storing how many start_args slots were supplied by the most recent start() call.
pub const FIBER_START_ARG_COUNT_OFFSET: i32 = 176;
/// Byte offset of the first reserved legacy float slot.
pub const FIBER_FLOAT_ARGS_OFFSET: i32 = 184;
/// Maximum number of reserved legacy float slots.
pub const FIBER_FLOAT_ARGS_MAX: i32 = 6;
/// Total size of the Fiber object payload in bytes (heap-allocated; class_id at offset 0, followed by all runtime-managed fields).
pub const FIBER_OBJECT_SIZE: i32 = 248;

/// Byte offset of `gen_last_key`: boxed Mixed of the most recent yield key.
pub const GEN_LAST_KEY_OFFSET: i32 = 184;
/// Byte offset of `gen_last_value`: boxed Mixed of the most recent yield value.
pub const GEN_LAST_VALUE_OFFSET: i32 = 192;
/// Byte offset of `gen_return_value`: boxed Mixed of the body `return` value.
pub const GEN_RETURN_VALUE_OFFSET: i32 = 200;
/// Byte offset of `gen_auto_key`: next auto-increment integer key (raw i64).
pub const GEN_AUTO_KEY_OFFSET: i32 = 208;
/// Byte offset of `gen_delegated_iter`: inner iterator for `yield from`.
pub const GEN_DELEGATED_ITER_OFFSET: i32 = 216;

/// Borrowed force-close marker in the zeroed legacy argument area.
pub const FORCE_CLOSE_OFFSET: i32 = FIBER_FLOAT_ARGS_OFFSET + (FIBER_FLOAT_ARGS_MAX - 1) * 8;

/// Loads the coroutine transfer value without changing its owner.
pub fn emit_borrow_transfer(emitter: &mut impl crate::Emitter, fiber: &str, destination: &str) {
    emitter.emit_load_from_address(destination, fiber, FIBER_TRANSFER_VALUE_OFFSET as usize);
}

/// Publishes a prepared value into the transfer slot. The caller supplies its owner.
pub fn emit_publish_transfer(emitter: &mut impl crate::Emitter, fiber: &str, value: &str) {
    emitter.emit_store_to_address(value, fiber, FIBER_TRANSFER_VALUE_OFFSET as usize);
}

/// Clears the unused high word of the transfer slot after a terminal publication.
pub fn emit_clear_transfer_high(emitter: &mut impl crate::Emitter, fiber: &str) {
    emitter.emit_store_zero_to_address(fiber, FIBER_TRANSFER_VALUE_OFFSET as usize + 8);
}

/// Clears both words after the transfer owner leaves the coroutine.
pub fn emit_clear_transfer(emitter: &mut impl crate::Emitter, fiber: &str) {
    emitter.emit_store_zero_to_address(fiber, FIBER_TRANSFER_VALUE_OFFSET as usize);
    emit_clear_transfer_high(emitter, fiber);
}

/// Moves the transfer owner out of the coroutine and leaves the slot empty.
pub fn emit_take_transfer(emitter: &mut impl crate::Emitter, fiber: &str, destination: &str) {
    emit_borrow_transfer(emitter, fiber, destination);
    emit_clear_transfer(emitter, fiber);
}

/// Parks an independently owned exception in the coroutine.
pub fn emit_publish_pending_throw(emitter: &mut impl crate::Emitter, fiber: &str, value: &str) {
    emitter.emit_store_to_address(value, fiber, FIBER_PENDING_THROW_OFFSET as usize);
}

/// Moves a parked exception to the caller, branching away when no owner was parked.
pub fn emit_take_pending_throw(
    emitter: &mut impl crate::Emitter, fiber: &str, destination: &str, absent: &str,
) {
    emitter.emit_load_from_address(destination, fiber, FIBER_PENDING_THROW_OFFSET as usize);
    match emitter.arch() {
        crate::Arch::AArch64 => emitter.instruction(&format!("cbz {destination}, {absent}")), // a null slot has no exception owner
        crate::Arch::X86_64 => {
            emitter.instruction(&format!("test {destination}, {destination}")); // inspect the loaded exception pointer
            emitter.instruction(&format!("je {absent}"));                       // a null slot has no exception owner
        }
    }
    emitter.emit_store_zero_to_address(fiber, FIBER_PENDING_THROW_OFFSET as usize);
}

/// Publishes the borrowed marker while the force-close caller holds its owner.
pub fn emit_publish_force_close(emitter: &mut impl crate::Emitter, fiber: &str, marker: &str) {
    emitter.emit_store_to_address(marker, fiber, FORCE_CLOSE_OFFSET as usize);
}

/// Unpublishes the marker before the force-close caller releases its owner.
pub fn emit_clear_force_close(emitter: &mut impl crate::Emitter, fiber: &str) {
    emitter.emit_store_zero_to_address(fiber, FORCE_CLOSE_OFFSET as usize);
}

/// Borrows the marker while preparing the coroutine's independent pending-throw owner.
pub fn emit_borrow_force_close(emitter: &mut impl crate::Emitter, fiber: &str, destination: &str) {
    emitter.emit_load_from_address(destination, fiber, FORCE_CLOSE_OFFSET as usize);
}

/// Branches when the borrowed force-close marker is absent.
pub fn emit_branch_without_force_close(emitter: &mut impl crate::Emitter, fiber: &str, done: &str) {
    match emitter.arch() {
        crate::Arch::AArch64 => {
            emit_borrow_force_close(emitter, fiber, "x9");
            emitter.instruction(&format!("cbz x9, {done}"));                    // continue when no cancellation marker is installed
        }
        crate::Arch::X86_64 => {
            emitter.instruction(&format!("cmp QWORD PTR [{fiber} + {FORCE_CLOSE_OFFSET}], 0")); // inspect the borrowed marker
            emitter.instruction(&format!("je {done}"));                         // continue when no cancellation marker is installed
        }
    }
}

/// An owned boxed value stored in a Generator coroutine.
#[derive(Clone, Copy)]
pub enum GeneratorOwner {
    /// Last yielded key.
    Key,
    /// Last yielded value.
    Value,
    /// Value returned by the body.
    Return,
}

impl GeneratorOwner {
    /// Resolves the storage slot inside the native Generator payload.
    fn offset(self) -> usize {
        match self {
            Self::Key => GEN_LAST_KEY_OFFSET as usize,
            Self::Value => GEN_LAST_VALUE_OFFSET as usize,
            Self::Return => GEN_RETURN_VALUE_OFFSET as usize,
        }
    }

    /// Reads the boxed owner without changing the Generator.
    pub fn emit_borrow(self, emitter: &mut impl crate::Emitter, generator: &str, destination: &str) {
        emitter.emit_load_from_address(destination, generator, self.offset());
    }

    /// Publishes an owner into a fresh Generator slot.
    pub fn emit_publish(self, emitter: &mut impl crate::Emitter, generator: &str, value: &str) {
        emitter.emit_store_to_address(value, generator, self.offset());
    }

    /// Publishes the new owner before retiring the old one, which can run PHP.
    pub fn emit_replace(
        self, emitter: &mut impl crate::Emitter, generator: &str, value: &str, previous: &str,
    ) {
        self.emit_borrow(emitter, generator, previous);
        self.emit_publish(emitter, generator, value);
        emitter.emit_reg_move(emitter.int_result_reg(), previous);
        emitter.emit_call_label("__rt_decref_mixed");
    }
}

/// Allocates and initializes the coroutine payload and the first frame of its native stack.
/// The compiler supplies the stack capacity, frame offsets and failure handling.
pub fn emit_payload_and_stack(
    emitter: &mut impl Emitter,
    stack_bytes: i32,
    initial_frame_bytes: i32,
    initial_entry_offset: i32,
) {
    match emitter.arch() {
        Arch::AArch64 => emit_arm64(emitter, stack_bytes, initial_frame_bytes, initial_entry_offset),
        Arch::X86_64 => emit_x86_64(emitter, stack_bytes, initial_frame_bytes, initial_entry_offset),
    }
}

/// Emits the AArch64 payload and initial native stack in the constructor's saved registers.
fn emit_arm64(
    emitter: &mut impl Emitter,
    stack_bytes: i32,
    initial_frame_bytes: i32,
    initial_entry_offset: i32,
) {
    // -- allocate the Fiber object payload --
    emitter.instruction(&format!("mov x0, #{}", FIBER_OBJECT_SIZE));            // size in bytes for the Fiber object payload
    emitter.instruction("bl __rt_heap_alloc");                                  // x0 = pointer to the object payload (header at x0-8)
    emitter.instruction("mov x21, x0");                                         // x21 = Fiber object pointer (kept until return)
    emitter.instruction("mov x9, #4");                                          // heap kind 4 = object instance
    emitter.instruction("str x9, [x21, #-8]");                                  // stamp the kind in the uniform heap header
    emitter.instruction("bl __rt_object_handle_acquire");                       // bind the new Fiber object to its PHP object handle (x0 still holds it)
    emitter.instruction("str x20, [x21]");                                      // store the runtime class_id at the object header

    // -- zero-initialise every Fiber field before populating the meaningful ones --
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_STATE_OFFSET));   // state placeholder (overwritten below with NotStarted)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_STACK_BASE_OFFSET)); // stack_base placeholder (overwritten after stack alloc)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_STACK_TOP_OFFSET)); // stack_top placeholder (overwritten after stack alloc)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_STACK_SIZE_OFFSET)); // stack_size placeholder (overwritten after stack alloc)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_SAVED_SP_OFFSET)); // saved_sp placeholder (overwritten after fake-frame setup)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_CALLABLE_OFFSET)); // callable.lo placeholder (overwritten with x19 below)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_CALLABLE_WRAPPER_OFFSET)); // callable wrapper placeholder (overwritten with x22 below)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_CALLER_OFFSET));  // caller starts NULL (no resumer until start/resume)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_TRANSFER_VALUE_OFFSET)); // transfer_value.lo cleared
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_TRANSFER_VALUE_OFFSET + 8)); // transfer_value.hi cleared
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_PENDING_THROW_OFFSET)); // pending_throw cleared
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_OWN_EXC_HEAD_OFFSET)); // own_exc_head cleared (no installed handlers yet)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_OWN_CALL_FRAME_OFFSET)); // own_call_frame cleared (no activation records on the fresh fiber stack yet)
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_OWN_MAGIC_SET_GUARD_OFFSET)); // no active magic-set recursion guard on a fresh fiber
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_OWN_GC_RELEASE_SUPPRESSED_OFFSET)); // a fresh coroutine has no suspended deep release
    for i in 0..FIBER_START_ARGS_MAX {
        emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_START_ARGS_OFFSET + i * 8)); // start_args[i] cleared
    }
    emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_START_ARG_COUNT_OFFSET)); // start_arg_count cleared until Fiber::start() copies values
    for i in 0..FIBER_FLOAT_ARGS_MAX {
        emitter.instruction(&format!("str xzr, [x21, #{}]", FIBER_FLOAT_ARGS_OFFSET + i * 8)); // clear reserved legacy float slot storage
    }
    // user_arg_max defaults to FIBER_START_ARGS_MAX so start() may fill every
    // visible start_args slot. Captures now live in callable descriptors.
    emitter.instruction(&format!("mov x9, #{}", FIBER_START_ARGS_MAX));         // default user_arg_max = full slot count
    emitter.instruction(&format!("str x9, [x21, #{}]", FIBER_USER_ARG_MAX_OFFSET)); // user_arg_max stored on the freshly built fiber

    // -- record the captured callable --
    emitter.instruction(&format!("str x19, [x21, #{}]", FIBER_CALLABLE_OFFSET)); // callable.lo = descriptor pointer
    emitter.instruction(&format!("str x22, [x21, #{}]", FIBER_CALLABLE_WRAPPER_OFFSET)); // callable wrapper = Fiber entry ABI adapter

    // -- allocate the per-fiber stack via mmap; alloc returns base/top/total --
    emitter.instruction(&format!("mov x0, #{}", stack_bytes));                  // request the default usable fiber stack size in bytes
    emitter.instruction("bl __rt_fiber_alloc_stack");                           // x0 = stack_base (mapping start), x1 = stack_top, x2 = total mapped length
    emitter.instruction("cbz x0, __rt_fiber_construct_stack_failed");           // abort construction if mmap failed instead of writing a fake frame at NULL
    emitter.instruction(&format!("str x0, [x21, #{}]", FIBER_STACK_BASE_OFFSET)); // stack_base = mmap mapping start (includes the guard page)
    emitter.instruction(&format!("str x1, [x21, #{}]", FIBER_STACK_TOP_OFFSET)); // stack_top = initial SP target (16-byte aligned high address)
    emitter.instruction(&format!("str x2, [x21, #{}]", FIBER_STACK_SIZE_OFFSET)); // stack_size = total mapped length, needed verbatim by munmap on free

    // -- carve out a fake initial frame at the very top of the stack --
    emitter.instruction(&format!("sub x10, x1, #{}", initial_frame_bytes));     // x10 = initial saved_sp (room for the switch save area)
    emitter.instruction(&format!("str x10, [x21, #{}]", FIBER_SAVED_SP_OFFSET)); // saved_sp points at the bottom of the fake frame

    // -- zero the fake frame so callee-saved registers come back as zero on first switch --
    emitter.instruction("mov x11, x10");                                        // x11 = cursor through the fake frame
    emitter.instruction(&format!("mov x12, #{}", initial_frame_bytes / 16));    // number of 16-byte chunks to zero
    emitter.label("__rt_fiber_construct_zero_loop");
    emitter.instruction("stp xzr, xzr, [x11], #16");                            // zero a 16-byte slice and advance the cursor
    emitter.instruction("subs x12, x12, #1");                                   // decrement the chunk counter
    emitter.instruction("b.ne __rt_fiber_construct_zero_loop");                 // continue until the entire frame is zero

    // -- install __rt_fiber_entry as the saved x30 so the first switch returns into it --
    abi::emit_symbol_address(emitter, "x9", "__rt_fiber_entry");                // x9 = absolute address of the entry trampoline
    emitter.instruction(&format!("str x9, [x10, #{}]", initial_entry_offset));  // saved x30 slot = entry trampoline address
}

/// Emits the X86_64 payload and initial native stack in the constructor's saved registers.
fn emit_x86_64(
    emitter: &mut impl Emitter,
    stack_bytes: i32,
    initial_frame_bytes: i32,
    initial_entry_offset: i32,
) {
    // -- allocate the Fiber object payload --
    emitter.instruction(&format!("mov rax, {}", FIBER_OBJECT_SIZE));            // size in bytes for the Fiber object payload
    emitter.instruction("call __rt_heap_alloc");                                // rax = pointer to the object payload
    emitter.instruction("mov r14, rax");                                        // r14 = Fiber object pointer kept until return
    emitter.instruction(&format!("mov r10, 0x{:x}", crate::layout::x86_64_heap_kind_word(crate::layout::OBJECT_HEAP_KIND))); // materialize the object heap kind word
    emitter.instruction("mov QWORD PTR [r14 - 8], r10");                        // stamp the allocation as an object instance
    emitter.instruction("call __rt_object_handle_acquire");                     // bind the new Fiber object to its PHP object handle (rax still holds it)
    emitter.instruction("mov QWORD PTR [r14], r13");                            // store the runtime class_id at the object header

    // -- zero-initialise every Fiber field before populating meaningful ones --
    emitter.instruction("lea r10, [r14 + 8]");                                  // r10 = first runtime-managed Fiber field after class_id
    emitter.instruction(&format!("mov r11, {}", (FIBER_OBJECT_SIZE - 8) / 8));  // r11 = number of qword fields to clear
    emitter.label("__rt_fiber_construct_zero_object_loop");
    emitter.instruction("mov QWORD PTR [r10], 0");                              // clear the current runtime-managed Fiber qword field
    emitter.instruction("add r10, 8");                                          // advance to the next qword field
    emitter.instruction("sub r11, 1");                                          // consume one field from the clear count
    emitter.instruction("jne __rt_fiber_construct_zero_object_loop");           // continue until every runtime-managed field is zeroed

    emitter.instruction(&format!("mov r10, {}", FIBER_START_ARGS_MAX));         // default user_arg_max = full slot count
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], r10", FIBER_USER_ARG_MAX_OFFSET)); // user_arg_max stored on the freshly built fiber
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], r12", FIBER_CALLABLE_OFFSET)); // callable.lo = descriptor pointer
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], r15", FIBER_CALLABLE_WRAPPER_OFFSET)); // callable wrapper = Fiber entry ABI adapter

    // -- allocate the per-fiber stack via mmap; alloc returns base/top/total --
    emitter.instruction(&format!("mov edi, {}", stack_bytes));                  // request the default usable fiber stack size in bytes
    emitter.instruction("call __rt_fiber_alloc_stack");                         // rax = stack_base, rdx = stack_top, rcx = total mapped length
    emitter.instruction("test rax, rax");                                       // did mmap return a real stack mapping?
    emitter.instruction("jz __rt_fiber_construct_stack_failed");                // abort construction before writing a fake frame at NULL
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], rax", FIBER_STACK_BASE_OFFSET)); // stack_base = mmap mapping start
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], rdx", FIBER_STACK_TOP_OFFSET)); // stack_top = initial SP target
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], rcx", FIBER_STACK_SIZE_OFFSET)); // stack_size = total mapped length

    // -- carve out and zero a fake initial frame at the top of the stack --
    emitter.instruction(&format!("lea r10, [rdx - {}]", initial_frame_bytes));  // r10 = initial saved_sp for the switch restore path
    emitter.instruction(&format!("mov QWORD PTR [r14 + {}], r10", FIBER_SAVED_SP_OFFSET)); // saved_sp points at the fake switch frame
    emitter.instruction("mov r11, r10");                                        // r11 = cursor through the fake frame
    emitter.instruction(&format!("mov rcx, {}", initial_frame_bytes / 8));      // rcx = number of qwords to zero in the fake frame
    emitter.label("__rt_fiber_construct_zero_frame_loop");
    emitter.instruction("mov QWORD PTR [r11], 0");                              // zero one saved-register or saved-return slot
    emitter.instruction("add r11, 8");                                          // advance to the next fake-frame qword
    emitter.instruction("sub rcx, 1");                                          // consume one fake-frame qword
    emitter.instruction("jne __rt_fiber_construct_zero_frame_loop");            // continue until the fake frame is zeroed
    abi::emit_symbol_address(emitter, "r11", "__rt_fiber_entry");              // r11 = absolute address of the entry trampoline
    emitter.instruction(&format!("mov QWORD PTR [r10 + {}], r11", initial_entry_offset)); // saved return address = entry trampoline
}

/// Allocates a one-word unwind sentinel without assigning it a PHP object handle.
pub fn emit_unwind_marker(emitter: &mut impl crate::Emitter) {
    crate::heap_alloc::emit_fresh_with_kind(emitter, crate::HeapKind::new(crate::layout::OBJECT_HEAP_KIND));
    match emitter.arch() {
        crate::Arch::AArch64 => {
            emitter.instruction("mov x9, #-3");                                 // keep the sentinel outside catchable PHP class ids
            emitter.instruction("str x9, [x0]");                                // publish the one-word marker payload
        }
        crate::Arch::X86_64 => {
            emitter.instruction("mov QWORD PTR [rax], -3");                     // publish the one-word marker payload
        }
    }
}

/// Allocates an ordinary Error owner and transfers three staged words from the current stack.
pub fn emit_compact_error(emitter: &mut impl crate::Emitter) {
    crate::values::object_storage::emit_allocate_object(emitter);
    // -- store the class and immutable message metadata in their existing order --
    for offset in [0, 8, 16] {
        match emitter.arch() {
            crate::Arch::AArch64 => {
                emitter.instruction(&format!("ldr x9, [sp, #{offset}]"));       // recover one staged error word
                emitter.instruction(&format!("str x9, [x0, #{offset}]"));       // publish the word in the compact payload
            }
            crate::Arch::X86_64 => {
                emitter.instruction(&format!("mov r10, QWORD PTR [rsp + {offset}]")); // recover one staged error word
                emitter.instruction(&format!("mov QWORD PTR [rax + {offset}], r10")); // publish the word in the compact payload
            }
        }
    }
}
