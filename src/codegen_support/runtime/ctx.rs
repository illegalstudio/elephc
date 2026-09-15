//! Purpose:
//! Owns the per-context runtime state layout (`_rt_ctx`) for the sandbox-threads spike.
//! Defines the ctx-register convention, emits the ctx data block, and emits the
//! `__rt_ctx_init` helper that publishes the ctx pointer into the reserved register.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` (helper emission).
//! - `crate::codegen_support::runtime::data::fixed` (ctx data block emission).
//! - `src/codegen/frame.rs` main prologue (ctx register publication).
//!
//! Key details:
//! - The ctx register is x28 on AArch64 and r14 on x86_64: both are callee-saved,
//!   both are saved/restored whole by `__rt_fiber_switch`, and both are excluded
//!   from the linear-scan allocator pools (`callee_int_pool` uses x21-x27 on
//!   AArch64 and rbx only on x86_64), so no register-allocated value ever
//!   collides with the ctx pointer.
//! - x18 is NOT used: Apple AArch64 reserves it for the OS.
//! - The layout embeds only the heap-path state for the spike: concat scratch
//!   (buf + off), heap bump offset, free-list head, and the small-bin heads.
//!   `_heap_buf` itself stays a global symbol during the spike because its base
//!   address is identical for every ctx and it is never reset per context.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Concat scratch capacity in bytes, mirroring `strings::CONCAT_BUF_CAPACITY`.
/// Duplicated here so the ctx layout is self-contained for the spike.
pub(crate) const CTX_CONCAT_BUF_CAPACITY: usize = 65536;

/// Small-bin head count, mirroring the 4 class heads emitted as `_heap_small_bins`.
pub(crate) const CTX_HEAP_SMALL_BIN_COUNT: usize = 4;

/// Byte offset of `_concat_off` inside `_rt_ctx`.
///
/// Scalar fields lead the layout and the 64 KiB concat buffer closes it: every
/// scalar offset stays small enough for the AArch64 unsigned-imm12 encoding
/// (`ldr xN, [x28, #imm]`), which caps at 4095.
pub(crate) const CTX_CONCAT_OFF_OFFSET: usize = 0;

/// Byte offset of `_heap_off` inside `_rt_ctx`.
pub(crate) const CTX_HEAP_OFF_OFFSET: usize = CTX_CONCAT_OFF_OFFSET + 8;

/// Byte offset of `_heap_free_list` inside `_rt_ctx`.
pub(crate) const CTX_HEAP_FREE_LIST_OFFSET: usize = CTX_HEAP_OFF_OFFSET + 8;

/// Byte offset of `_heap_small_bins` inside `_rt_ctx`.
pub(crate) const CTX_HEAP_SMALL_BINS_OFFSET: usize = CTX_HEAP_FREE_LIST_OFFSET + 8;

/// Byte offset of the heap ARENA BASE inside `_rt_ctx`.
///
/// The allocator computes `address = base + heap_off`. Keeping the offset
/// per-context while the base stayed a single global symbol made the state
/// per-context but the MEMORY shared: two contexts both starting at
/// `heap_off = 0` hand out the same address. So the base travels in the context
/// too, and a second context points at its own arena.
///
/// The main context's base is `_heap_buf`, the `.comm` arena sized by
/// `--heap-size`; `__rt_ctx_init` installs it.
pub(crate) const CTX_HEAP_BASE_OFFSET: usize =
    CTX_HEAP_SMALL_BINS_OFFSET + CTX_HEAP_SMALL_BIN_COUNT * 8;

/// Byte offset of the heap CAPACITY inside `_rt_ctx`.
///
/// Travels with the base for the same reason: arenas may differ in size, so the
/// exhaustion check has to read the limit of the arena it is bumping.
pub(crate) const CTX_HEAP_MAX_OFFSET: usize = CTX_HEAP_BASE_OFFSET + 8;

/// Byte offset of `_concat_buf` inside `_rt_ctx`.
///
/// The 64 KiB scratch buffer closes the layout: accesses derive its address
/// once via `emit_ctx_address` (an `add xN, x28, #imm` also within imm12) and
/// then index freely from there, so the large offset never reaches an
/// immediate-offset load.
/// The active Throwable a context is unwinding, formerly the `_exc_value` global.
///
/// This is the first field of the family that BLOCKS M1: a thread that throws must not
/// publish into, or walk, the main thread's exception state. Sharing it is not a
/// corruption risk to be measured, it is structurally wrong — two contexts unwinding at
/// once would see one another's Throwable.
pub(crate) const CTX_EXC_VALUE_OFFSET: usize = CTX_HEAP_MAX_OFFSET + 8;

/// Top of this context's catch-handler chain, formerly `_exc_handler_top`.
pub(crate) const CTX_EXC_HANDLER_TOP_OFFSET: usize = CTX_EXC_VALUE_OFFSET + 8;

/// Top of this context's call-frame chain for unwinding, formerly `_exc_call_frame_top`.
pub(crate) const CTX_EXC_CALL_FRAME_TOP_OFFSET: usize = CTX_EXC_HANDLER_TOP_OFFSET + 8;

/// This context's current Fiber, formerly `_fiber_current`.
///
/// Fibers are cooperative WITHIN a context: two OS threads each running their own fiber
/// must not share the "which fiber is current" cell, or a switch on one thread would
/// restore the other thread's registers.
pub(crate) const CTX_FIBER_CURRENT_OFFSET: usize = CTX_EXC_CALL_FRAME_TOP_OFFSET + 8;

/// Main-stack state parked while a Fiber of this context runs (`_fiber_main_saved_*`).
pub(crate) const CTX_FIBER_MAIN_SAVED_SP_OFFSET: usize = CTX_FIBER_CURRENT_OFFSET + 8;
pub(crate) const CTX_FIBER_MAIN_SAVED_EXC_OFFSET: usize = CTX_FIBER_MAIN_SAVED_SP_OFFSET + 8;
pub(crate) const CTX_FIBER_MAIN_SAVED_CALL_FRAME_OFFSET: usize =
    CTX_FIBER_MAIN_SAVED_EXC_OFFSET + 8;

/// The call-stack floor every function prologue compares `sp` against (`_stack_limit`),
/// and the OS-thread floor measured at process start (`_stack_limit_main`).
///
/// A thread on its own mmap'd stack compared against the MAIN thread's floor either never
/// trips the guard or trips it immediately, depending on which way the stacks happen to
/// lie in the address space. There is no value of a shared floor that is correct for two
/// stacks, which is why this pair blocks M1 rather than merely risking corruption.
pub(crate) const CTX_STACK_LIMIT_OFFSET: usize = CTX_FIBER_MAIN_SAVED_CALL_FRAME_OFFSET + 8;
pub(crate) const CTX_STACK_LIMIT_MAIN_OFFSET: usize = CTX_STACK_LIMIT_OFFSET + 8;

/// Allocator bookkeeping, formerly `_gc_allocs` / `_gc_frees` / `_gc_live` / `_gc_peak`.
///
/// PER-CONTEXT, decided rather than inherited. Each context owns its own arena — no
/// pointer crosses an arena boundary, which is the sandbox-threads rule the whole design
/// rests on — so a per-context count is the only number that describes something real. A
/// process-wide total would describe no single arena, and making it one would put an
/// atomic on the allocator's hottest path to produce it. With one context the two answers
/// are identical, so nothing observable changes today.
pub(crate) const CTX_GC_ALLOCS_OFFSET: usize = CTX_STACK_LIMIT_MAIN_OFFSET + 8;
pub(crate) const CTX_GC_FREES_OFFSET: usize = CTX_GC_ALLOCS_OFFSET + 8;
pub(crate) const CTX_GC_LIVE_OFFSET: usize = CTX_GC_FREES_OFFSET + 8;
pub(crate) const CTX_GC_PEAK_OFFSET: usize = CTX_GC_LIVE_OFFSET + 8;

/// The cycle collector's re-entrancy flag, formerly `_gc_collecting`.
///
/// This one is a LOCK, not a counter, and it has no per-context/process choice to make:
/// a shared flag would let one context's collection suppress another's, or let one clear
/// a flag the other is relying on.
pub(crate) const CTX_GC_COLLECTING_OFFSET: usize = CTX_GC_PEAK_OFFSET + 8;

/// Output-buffering and print_r capture state, formerly the `_ob_*` and `_print_r_*`
/// globals.
///
/// Per-context for the plainest of reasons: `ob_start()` opens a buffer on the CALLING
/// context's stack, and `print_r($x, true)` captures that context's output. Sharing either
/// would interleave two contexts' output into one buffer.
///
/// The three scalars join the scalar block; the six 512-byte handle arrays follow it,
/// still inside the first 4 KiB window so their base addresses stay one instruction.
pub(crate) const CTX_PRINT_R_MODE_OFFSET: usize = CTX_GC_COLLECTING_OFFSET + 8;
pub(crate) const CTX_PRINT_R_OFF_OFFSET: usize = CTX_PRINT_R_MODE_OFFSET + 8;
pub(crate) const CTX_OB_LEVEL_OFFSET: usize = CTX_PRINT_R_OFF_OFFSET + 8;

/// One entry per nested output buffer, 64 levels of 8 bytes.
pub(crate) const CTX_OB_TABLE_SIZE: usize = 512;
pub(crate) const CTX_OB_PTRS_OFFSET: usize = CTX_OB_LEVEL_OFFSET + 8;
pub(crate) const CTX_OB_LENS_OFFSET: usize = CTX_OB_PTRS_OFFSET + CTX_OB_TABLE_SIZE;
pub(crate) const CTX_OB_CAPS_OFFSET: usize = CTX_OB_LENS_OFFSET + CTX_OB_TABLE_SIZE;
pub(crate) const CTX_OB_HANDLER_STUBS_OFFSET: usize = CTX_OB_CAPS_OFFSET + CTX_OB_TABLE_SIZE;
pub(crate) const CTX_OB_HANDLER_ENVS_OFFSET: usize =
    CTX_OB_HANDLER_STUBS_OFFSET + CTX_OB_TABLE_SIZE;
pub(crate) const CTX_OB_NAME_PTRS_OFFSET: usize = CTX_OB_HANDLER_ENVS_OFFSET + CTX_OB_TABLE_SIZE;
pub(crate) const CTX_OB_NAME_LENS_OFFSET: usize = CTX_OB_NAME_PTRS_OFFSET + CTX_OB_TABLE_SIZE;
pub(crate) const CTX_OB_CHUNK_SIZES_OFFSET: usize = CTX_OB_NAME_LENS_OFFSET + CTX_OB_TABLE_SIZE;
pub(crate) const CTX_OB_FLAGS_OFFSET: usize = CTX_OB_CHUNK_SIZES_OFFSET + CTX_OB_TABLE_SIZE;
pub(crate) const CTX_OB_STARTED_OFFSET: usize = CTX_OB_FLAGS_OFFSET + CTX_OB_TABLE_SIZE;

/// The address of this context's slot-state word in `_rt_ctx_state`, or 0 for a context
/// that did not come from the pool (the main one).
///
/// Written by `__rt_ctx_acquire`, which already holds the address, so `__rt_ctx_release`
/// needs no arithmetic to find it. The alternative — dividing the pointer's distance from
/// the pool base by `CTX_SIZE` — buys nothing and costs a `udiv`.
pub(crate) const CTX_POOL_STATE_PTR_OFFSET: usize = CTX_OB_STARTED_OFFSET + CTX_OB_TABLE_SIZE;

/// Per-descriptor resource tables, formerly `_eof_flags`, `_popen_files`,
/// `_dir_handles`, `_glob_handles`, `_bzstream_handles` and the two stream-filter
/// tables.
///
/// PER-CONTEXT, by the plan's own criterion: "per-context if a thread may open
/// resources". A spawned task runs arbitrary PHP, so it can call `opendir()` — so it can.
///
/// The consequence is worth stating rather than discovering: a descriptor opened in one
/// context is not visible in another's tables. That is the transfer contract, not a gap —
/// a resource handle joins the values that cannot cross a context boundary, refused at
/// compile time like the rest. Sharing the tables instead would mean two contexts writing
/// the same slot for the same fd number, which is a data race, not a feature.
pub(crate) const CTX_EOF_FLAGS_SIZE: usize = 256;
pub(crate) const CTX_HANDLE_TABLE_SIZE: usize = 2048;
pub(crate) const CTX_FILTER_TABLE_SIZE: usize = 256;

pub(crate) const CTX_EOF_FLAGS_OFFSET: usize = CTX_POOL_STATE_PTR_OFFSET + 8;
pub(crate) const CTX_POPEN_FILES_OFFSET: usize = CTX_EOF_FLAGS_OFFSET + CTX_EOF_FLAGS_SIZE;
pub(crate) const CTX_DIR_HANDLES_OFFSET: usize =
    CTX_POPEN_FILES_OFFSET + CTX_HANDLE_TABLE_SIZE;
pub(crate) const CTX_GLOB_HANDLES_OFFSET: usize =
    CTX_DIR_HANDLES_OFFSET + CTX_HANDLE_TABLE_SIZE;
pub(crate) const CTX_BZSTREAM_HANDLES_OFFSET: usize =
    CTX_GLOB_HANDLES_OFFSET + CTX_HANDLE_TABLE_SIZE;
pub(crate) const CTX_STREAM_READ_FILTERS_OFFSET: usize =
    CTX_BZSTREAM_HANDLES_OFFSET + CTX_HANDLE_TABLE_SIZE;
pub(crate) const CTX_STREAM_WRITE_FILTERS_OFFSET: usize =
    CTX_STREAM_READ_FILTERS_OFFSET + CTX_FILTER_TABLE_SIZE;

/// The C-string scratch pair, formerly `_cstr_buf` / `_cstr_buf2` (4 KiB each).
///
/// `__rt_cstr` copies a PHP string into one of these to hand a NUL-terminated pointer to
/// libc. Two contexts calling any C-string builtin at the same time would overwrite each
/// other's scratch mid-call, so the pair is per-context — the same reason the concat
/// arena is.
///
/// ALIGNED TO 4096 ON PURPOSE. AArch64 `add xN, x28, #imm` takes a 12-bit immediate, or a
/// 12-bit one shifted left by 12 — so an offset is encodable in ONE instruction when it is
/// below 4096 or an exact multiple of it. Every scalar above stays in the first window;
/// each buffer starts on a 4 KiB boundary. Placing a buffer at, say, 4280 would silently
/// cost a second instruction on every use, or fail to assemble.
pub(crate) const CTX_CSTR_BUF_SIZE: usize = 4096;
pub(crate) const CTX_CSTR_BUF_OFFSET: usize =
    (CTX_STREAM_WRITE_FILTERS_OFFSET + CTX_FILTER_TABLE_SIZE + 4095) & !4095;
pub(crate) const CTX_CSTR_BUF2_OFFSET: usize = CTX_CSTR_BUF_OFFSET + CTX_CSTR_BUF_SIZE;

pub(crate) const CTX_CONCAT_BUF_OFFSET: usize = CTX_CSTR_BUF2_OFFSET + CTX_CSTR_BUF_SIZE;

/// The print_r capture buffer, formerly `_print_r_buf` (64 KiB), closing the layout.
///
/// Its offset is a multiple of 4096 because every buffer before it is, which is what the
/// layout test pins: a buffer landing off that grid would cost a second instruction on
/// every address materialization, or fail to assemble outright.
pub(crate) const CTX_PRINT_R_BUF_OFFSET: usize =
    CTX_CONCAT_BUF_OFFSET + CTX_CONCAT_BUF_CAPACITY;

/// The length-growing stream filters' 64 KiB scratch, formerly `_stream_filter_buf`.
/// Per-context for the same reason as the C-string pair: it is scratch a filter encodes
/// into mid-call, and two contexts filtering at once would overwrite each other.
pub(crate) const CTX_STREAM_FILTER_BUF_OFFSET: usize =
    CTX_PRINT_R_BUF_OFFSET + CTX_CONCAT_BUF_CAPACITY;

/// Total byte size of one `_rt_ctx` instance (16-byte aligned).
///
/// Covers the leading scalar fields, the four small-bin heads, and the closing
/// 64 KiB concat scratch buffer.
/// How many execution contexts a binary can hold at once.
///
/// Eight is a starting point, not a measurement: M1's `Parallel\TaskGroup` has no
/// implementation to size against yet. The cost is `CTX_POOL_SLOTS * CTX_SIZE` of
/// zero-filled BSS — about 1.7 MiB at eight — which never reaches the image, since the
/// pool is a common symbol like every other runtime global.
///
/// Slot 0 is the MAIN context: `__rt_ctx_init` takes it unconditionally, so the process
/// always has one even if nothing ever spawns.
pub(crate) const CTX_POOL_SLOTS: usize = 8;

pub(crate) const CTX_SIZE: usize =
    (CTX_STREAM_FILTER_BUF_OFFSET + CTX_CONCAT_BUF_CAPACITY + 15) & !15;

/// Returns the reserved ctx-pointer register name for the target.
///
/// x28 (AArch64) / r14 (x86_64): callee-saved, saved whole by the fiber switch,
/// and excluded from both register-allocator pools, so the ctx pointer survives
/// every call and every fiber switch without extra spills.
pub fn ctx_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x28",
        Arch::X86_64 => "r14",
    }
}

/// Emits the `_rt_ctx` data block: one instance of the context struct.
///
/// A common symbol, like every other runtime global: the block is 64 KiB of
/// zeroes, and `.space` inside `.data` writes all of them into the image (it
/// made a ctx binary ~66 KB larger than its legacy twin). A future pool emitter
/// will replace this with an array plus a free-list for per-thread contexts.
// The ctx data block currently ships through `runtime::data::fixed`'s string
// assembly path; this emitter-shaped variant stays for the pool iteration.
#[allow(dead_code)]
pub fn emit_rt_ctx_data(emitter: &mut Emitter, single: bool) {
    if single {
        emitter.raw(&crate::codegen_support::data_section::comm_directive(
            "_rt_ctx",
            CTX_SIZE,
            emitter.target,
        ));
    }
}

/// Zeroes the heap allocator state (bump offset, free-list head, small-bin
/// heads), ctx-relative in ctx-register mode and via the legacy globals
/// otherwise.
///
/// Used by the `--web` per-request arena reset: the whole arena is reclaimed at
/// once after every refcounted per-request value has already been released.
pub fn emit_heap_arena_reset_state(emitter: &mut Emitter) {
    let fields = [
        CTX_HEAP_OFF_OFFSET,
        CTX_HEAP_FREE_LIST_OFFSET,
        CTX_HEAP_SMALL_BINS_OFFSET,
        CTX_HEAP_SMALL_BINS_OFFSET + 8,
        CTX_HEAP_SMALL_BINS_OFFSET + 16,
        CTX_HEAP_SMALL_BINS_OFFSET + 24,
    ];
    for offset in fields {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("str xzr, [x28, #{}]", offset)); // zero one per-context heap allocator field
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov QWORD PTR [r14 + {}], 0", offset)); // zero one per-context heap allocator field
            }
        }
    }
}

/// Publishes the `_rt_ctx` base into the reserved ctx register without a call.
///
/// Used by entry points that must (re-)establish the per-context state pointer
/// without the full zeroing pass of `__rt_ctx_init` — notably the fiber entry
/// trampoline, whose fresh stack arrives with a zeroed save area.
pub fn emit_ctx_publish(emitter: &mut Emitter) {
    abi::emit_symbol_address(emitter, ctx_reg(emitter), "_rt_ctx");
}

/// Spills the FOREIGN caller's ctx register into a frame slot, ahead of the
/// publish that overwrites it.
///
/// The ctx register is callee-saved on both targets (`x28` AArch64 / `r14`
/// x86_64), so any entry point a host calls — a cdylib/staticlib export, an FFI
/// callback trampoline — owes the host that register back untouched. Publishing
/// without this pair silently hands the host elephc's `_rt_ctx` pointer instead
/// of its own value.
pub fn emit_ctx_save_foreign(emitter: &mut Emitter, frame_offset: usize) {
    let ctx = ctx_reg(emitter).to_string();
    abi::store_at_offset(emitter, &ctx, frame_offset);
}

/// Restores the foreign caller's ctx register from its frame slot.
///
/// Must run on EVERY return path of an entry that called
/// `emit_ctx_save_foreign` — including the error and exception returns.
pub fn emit_ctx_restore_foreign(emitter: &mut Emitter, frame_offset: usize) {
    let ctx = ctx_reg(emitter).to_string();
    abi::load_at_offset(emitter, &ctx, frame_offset);
}

/// Zeroes every mutable ctx field (concat offset, heap bump, free list, bins)
/// through the already-published ctx register, on both targets.
///
/// Split out of `emit_rt_ctx_init` so library-entry points can reuse the
/// reset without emitting a call: `elephc_init` publishes the pointer inline
/// and must zero the same fields the helper does.
pub fn emit_ctx_zero_fields(emitter: &mut Emitter) {
    let ctx = ctx_reg(emitter);
    emit_ctx_zero_fields_at(emitter, ctx);
}

/// Zeroes the mutable ctx fields through an ARBITRARY base register.
///
/// `__rt_ctx_acquire` prepares a slot for a caller that has not published it yet, so it
/// cannot go through the ctx register — that one still belongs to the acquiring context.
pub fn emit_ctx_zero_fields_at(emitter: &mut Emitter, ctx: &str) {
    for offset in [
        CTX_CONCAT_OFF_OFFSET,
        CTX_HEAP_OFF_OFFSET,
        CTX_HEAP_FREE_LIST_OFFSET,
        // A pooled context that starts a new request must not inherit the previous
        // one's Throwable or handler chain — that is the M1 reuse case this loop exists
        // for, and an exception cell is exactly the kind of state that would survive.
        CTX_EXC_VALUE_OFFSET,
        CTX_EXC_HANDLER_TOP_OFFSET,
        CTX_EXC_CALL_FRAME_TOP_OFFSET,
        CTX_FIBER_CURRENT_OFFSET,
        CTX_FIBER_MAIN_SAVED_SP_OFFSET,
        CTX_FIBER_MAIN_SAVED_EXC_OFFSET,
        CTX_FIBER_MAIN_SAVED_CALL_FRAME_OFFSET,
        CTX_GC_ALLOCS_OFFSET,
        CTX_GC_FREES_OFFSET,
        CTX_GC_LIVE_OFFSET,
        CTX_GC_PEAK_OFFSET,
        CTX_GC_COLLECTING_OFFSET,
        CTX_PRINT_R_MODE_OFFSET,
        CTX_PRINT_R_OFF_OFFSET,
        CTX_OB_LEVEL_OFFSET,
        // `_stack_limit` / `_stack_limit_main` are NOT in this list on purpose. They are
        // published by `__rt_stack_limit_init` from the real stack bounds, and a zeroed
        // floor would make every prologue's `cmp sp, floor` succeed at any depth — the
        // guard would be silently off rather than conservatively on.
    ] {
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("str xzr, [{}, #{}]", ctx, offset)); // reset one mutable ctx field to zero
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov QWORD PTR [{} + {}], 0", ctx, offset)); // reset one mutable ctx field to zero
            }
        }
    }
    for bin in 0..CTX_HEAP_SMALL_BIN_COUNT {
        let offset = CTX_HEAP_SMALL_BINS_OFFSET + bin * 8;
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("str xzr, [{}, #{}]", ctx, offset)); // reset one small-bin head to empty
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov QWORD PTR [{} + {}], 0", ctx, offset)); // reset one small-bin head to empty
            }
        }
    }
}

/// Emits `__rt_ctx_init`: publishes the `_rt_ctx` base into the ctx register and
/// zeroes the mutable ctx fields (concat offset, heap bump, free list, bins).
///
/// Contract:
/// - Must be called exactly once per execution context before any ctx-relative
///   access runs (main prologue today; a spawned thread's entry later). Calling
///   it again on a LIVE context resets the allocator (zeroed bump offset,
///   empty free list) — catastrophic mid-request, intended only at fresh
///   context boundaries (pool reuse, per-request reset).
/// - Clobber set, pinned: the ctx register (`x28`/`r14`) is REWRITTEN (that is
///   its purpose) and the AArch64 `adrp`/x86_64 `lea` sequences borrow the
///   standard symbol scratch (`x9` AArch64; none on x86_64 — RIP-relative).
///   No argument register (`x0`-`x7`, `rdi`-`r9`) is touched, so a prologue
///   may call this helper BEFORE argc/argv have been spilled without
///   corrupting them. Keep this property: new field stores must stay on the
///   ctx register, not on argument registers.
/// - The data section's `.space` zero-fill covers the initial zeroing, but the
///   explicit zero stores keep `__rt_ctx_init` correct for a REUSED context
///   (the M1 thread-pool case where a fresh request reuses a pooled ctx).
/// - It also installs the MAIN context's arena: base `_heap_buf`, capacity
///   `_heap_max`. This is the one place in a ctx build allowed to name those
///   symbols; every other heap site reads the pair out of the context, which is
///   what lets a second context own different memory.
pub fn emit_rt_ctx_init(emitter: &mut Emitter) {
    let ctx = ctx_reg(emitter);
    emitter.blank();
    emitter.comment("--- runtime: ctx_init (publish per-context state pointer) ---");
    emitter.label_global("__rt_ctx_init");
    abi::emit_symbol_address(emitter, ctx, "_rt_ctx");
    // Zero the mutable fields so a REUSED context (thread pool, request reuse)
    // starts pristine even though the data section zero-fills only the first use.
    emit_ctx_zero_fields(emitter);
    emit_ctx_install_default_arena(emitter);
    emitter.instruction("ret");
}

/// Emits `__rt_ctx_acquire` and `__rt_ctx_release`: the pool the M1 bridge will call.
///
/// `__rt_ctx_acquire(base, size) -> ctx*` claims a free slot, zeroes its mutable fields,
/// installs the caller-supplied arena and returns the slot pointer — or 0 when the pool is
/// exhausted. `__rt_ctx_release(ctx*)` marks the slot free again.
///
/// THE CLAIM IS ATOMIC, deliberately, even though nothing spawns a thread yet. A plain
/// load/store pair would pass every test that can be written today and would be a race the
/// first time two threads acquire at once — cheap to prevent, expensive to find. AArch64
/// uses an `ldaxr`/`stlxr` pair, x86_64 a `lock cmpxchg`; both give the acquire/release
/// ordering a slot hand-off needs.
///
/// NEITHER TOUCHES THE CTX REGISTER. Acquire prepares a slot for a caller that has not
/// published it — the acquiring thread still owns its own context, and stomping the
/// register here would lose it. Publication is the caller's job, which is why
/// `emit_ctx_zero_fields_at` takes a base register.
///
/// THE ARENA IS A PARAMETER rather than `_heap_buf`, because that is the entire point: a
/// second context must own different memory, or two contexts bump-allocate into one arena
/// and hand out the same addresses. `__rt_ctx_init` keeps naming `_heap_buf`, for slot 0.
pub fn emit_rt_ctx_pool(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: ctx_acquire (claim a pooled execution context) ---");
    emitter.label_global("__rt_ctx_acquire");
    match emitter.target.arch {
        Arch::AArch64 => {
            // x0 = arena base, x1 = arena size in bytes.
            abi::emit_symbol_address(emitter, "x9", "_rt_ctx_state");               // x9 = this slot's state word
            abi::emit_symbol_address(emitter, "x10", "_rt_ctx_pool");               // x10 = this slot's base
            emitter.instruction(&format!("mov x11, #{}", CTX_POOL_SLOTS));          // x11 = slots left to try
            emitter.label("__rt_ctx_acquire_slot");
            emitter.instruction("cbz x11, __rt_ctx_acquire_exhausted");             // every slot taken
            emitter.label("__rt_ctx_acquire_try");
            emitter.instruction("ldaxr x12, [x9]");                                 // read the state with acquire ordering
            emitter.instruction("cbnz x12, __rt_ctx_acquire_next");                 // already claimed: move on, write nothing
            emitter.instruction("mov x13, #1");                                     // the claimed marker
            emitter.instruction("stlxr w14, x13, [x9]");                            // publish the claim with release ordering
            emitter.instruction("cbnz w14, __rt_ctx_acquire_try");                  // lost the exclusive: retry this slot
            emit_ctx_zero_fields_at(emitter, "x10");                                // hand back a pristine context
            emitter.instruction(&format!("str x9, [x10, #{}]", CTX_POOL_STATE_PTR_OFFSET)); // remember the slot for release
            emitter.instruction(&format!("str x0, [x10, #{}]", CTX_HEAP_BASE_OFFSET)); // this context's own arena base
            emitter.instruction("add x12, x0, x1");                                 // arena ceiling = base + size
            emitter.instruction(&format!("str x12, [x10, #{}]", CTX_HEAP_MAX_OFFSET)); // this context's own arena ceiling
            emitter.instruction("mov x0, x10");                                     // return the slot pointer
            emitter.instruction("ret");
            emitter.label("__rt_ctx_acquire_next");
            emitter.instruction("add x9, x9, #8");                                  // next slot's state word
            emitter.instruction(&format!("add x10, x10, #{}", CTX_SIZE));           // next slot's base (stride is 4 KiB-aligned)
            emitter.instruction("sub x11, x11, #1");
            emitter.instruction("b __rt_ctx_acquire_slot");
            emitter.label("__rt_ctx_acquire_exhausted");
            emitter.instruction("mov x0, #0");                                      // the pool is full; the caller decides what that means
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            // rdi = arena base, rsi = arena size in bytes.
            abi::emit_symbol_address(emitter, "r8", "_rt_ctx_state");                // r8 = this slot's state word
            abi::emit_symbol_address(emitter, "r9", "_rt_ctx_pool");                 // r9 = this slot's base
            emitter.instruction(&format!("mov r10, {}", CTX_POOL_SLOTS));            // r10 = slots left to try
            emitter.label("__rt_ctx_acquire_slot");
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_ctx_acquire_exhausted");                    // every slot taken
            emitter.instruction("xor eax, eax");                                     // expect the slot to be free
            emitter.instruction("mov edx, 1");                                       // the claimed marker
            emitter.instruction("lock cmpxchg QWORD PTR [r8], rdx");                 // claim it, or learn it was taken
            emitter.instruction("jnz __rt_ctx_acquire_next");                        // someone else holds it
            emit_ctx_zero_fields_at(emitter, "r9");                                  // hand back a pristine context
            emitter.instruction(&format!("mov QWORD PTR [r9 + {}], r8", CTX_POOL_STATE_PTR_OFFSET)); // remember the slot for release
            emitter.instruction(&format!("mov QWORD PTR [r9 + {}], rdi", CTX_HEAP_BASE_OFFSET)); // this context's own arena base
            emitter.instruction("lea rax, [rdi + rsi]");                             // arena ceiling = base + size
            emitter.instruction(&format!("mov QWORD PTR [r9 + {}], rax", CTX_HEAP_MAX_OFFSET)); // this context's own arena ceiling
            emitter.instruction("mov rax, r9");                                      // return the slot pointer
            emitter.instruction("ret");
            emitter.label("__rt_ctx_acquire_next");
            emitter.instruction("add r8, 8");                                        // next slot's state word
            emitter.instruction(&format!("add r9, {}", CTX_SIZE));                   // next slot's base
            emitter.instruction("sub r10, 1");
            emitter.instruction("jmp __rt_ctx_acquire_slot");
            emitter.label("__rt_ctx_acquire_exhausted");
            emitter.instruction("xor eax, eax");                                     // the pool is full; the caller decides what that means
            emitter.instruction("ret");
        }
    }

    emitter.blank();
    emitter.comment("--- runtime: ctx_release (return a pooled execution context) ---");
    emitter.label_global("__rt_ctx_release");
    match emitter.target.arch {
        Arch::AArch64 => {
            // x0 = a pointer __rt_ctx_acquire returned. A context that never came from the
            // pool carries a zero back-pointer and is silently ignored, so releasing the
            // main context is a no-op rather than a corruption.
            emitter.instruction(&format!("ldr x9, [x0, #{}]", CTX_POOL_STATE_PTR_OFFSET)); // the slot's state word
            emitter.instruction("cbz x9, __rt_ctx_release_done");                   // not pooled: nothing to hand back
            emitter.instruction(&format!("str xzr, [x0, #{}]", CTX_POOL_STATE_PTR_OFFSET)); // forget the slot before freeing it
            emitter.instruction("stlr xzr, [x9]");                                  // publish the release with release ordering
            emitter.label("__rt_ctx_release_done");
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            // rdi = a pointer __rt_ctx_acquire returned; a zero back-pointer means unpooled.
            emitter.instruction(&format!("mov r8, QWORD PTR [rdi + {}]", CTX_POOL_STATE_PTR_OFFSET)); // the slot's state word
            emitter.instruction("test r8, r8");
            emitter.instruction("jz __rt_ctx_release_done");                         // not pooled: nothing to hand back
            emitter.instruction(&format!("mov QWORD PTR [rdi + {}], 0", CTX_POOL_STATE_PTR_OFFSET)); // forget the slot before freeing it
            emitter.instruction("mov QWORD PTR [r8], 0");                            // x86 stores already carry release ordering
            emitter.label("__rt_ctx_release_done");
            emitter.instruction("ret");
        }
    }
}

/// Points this context's arena at the process-wide `_heap_buf`/`_heap_max` pair.
///
/// Split out so every entry that builds the MAIN context — the executable's
/// `__rt_ctx_init` and the library's `elephc_init` — installs the same arena
/// without duplicating the sequence. A spawned context will instead receive a
/// base and a capacity from its thread bridge.
///
/// Clobber set: the scratch the symbol materialization uses (`x9` on AArch64,
/// none on x86_64) plus `x10`/`r10` to carry the capacity. No argument register,
/// so the main prologue may still call this before argc/argv are spilled.
pub fn emit_ctx_install_default_arena(emitter: &mut Emitter) {
    let ctx = ctx_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x10", "_heap_buf");
            emitter.instruction(&format!("str x10, [{}, #{}]", ctx, CTX_HEAP_BASE_OFFSET)); // this context's arena base
            abi::emit_load_symbol_to_reg(emitter, "x10", "_heap_max", 0);
            emitter.instruction(&format!("str x10, [{}, #{}]", ctx, CTX_HEAP_MAX_OFFSET)); // this context's arena capacity
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_heap_buf");
            emitter.instruction(&format!(
                "mov QWORD PTR [{} + {}], r10",
                ctx, CTX_HEAP_BASE_OFFSET
            )); // this context's arena base
            abi::emit_load_symbol_to_reg(emitter, "r10", "_heap_max", 0);
            emitter.instruction(&format!(
                "mov QWORD PTR [{} + {}], r10",
                ctx, CTX_HEAP_MAX_OFFSET
            )); // this context's arena capacity
        }
    }
}

/// The legacy global symbols a ctx build serves out of `_rt_ctx` instead.
///
/// This is the routing table the `abi::` symbol accessors consult: in a ctx build an
/// access to one of these names becomes a ctx-relative access at the paired offset, and
/// `data/fixed.rs` stops declaring the symbol — so a path that somehow still names it
/// fails the LINK rather than reading another context's state. Same tripwire the concat
/// and heap families already use.
///
/// It maps NAMES because that is what the accessors receive. Every access must go
/// through them for the table to be complete, which
/// `exception_state_is_only_reached_through_the_abi_accessors` enforces at the source
/// level — fifteen sites had to be converted before this table could be trusted.
const PER_CONTEXT_SYMBOLS: &[(&str, usize)] = &[
    ("_exc_value", CTX_EXC_VALUE_OFFSET),
    ("_exc_handler_top", CTX_EXC_HANDLER_TOP_OFFSET),
    ("_exc_call_frame_top", CTX_EXC_CALL_FRAME_TOP_OFFSET),
    ("_fiber_current", CTX_FIBER_CURRENT_OFFSET),
    ("_fiber_main_saved_sp", CTX_FIBER_MAIN_SAVED_SP_OFFSET),
    ("_fiber_main_saved_exc", CTX_FIBER_MAIN_SAVED_EXC_OFFSET),
    ("_fiber_main_saved_call_frame", CTX_FIBER_MAIN_SAVED_CALL_FRAME_OFFSET),
    ("_stack_limit", CTX_STACK_LIMIT_OFFSET),
    ("_stack_limit_main", CTX_STACK_LIMIT_MAIN_OFFSET),
    ("_gc_allocs", CTX_GC_ALLOCS_OFFSET),
    ("_gc_frees", CTX_GC_FREES_OFFSET),
    ("_gc_live", CTX_GC_LIVE_OFFSET),
    ("_gc_peak", CTX_GC_PEAK_OFFSET),
    ("_gc_collecting", CTX_GC_COLLECTING_OFFSET),
    // Buffers, not scalars. They route the same way because every consumer asks for the
    // ADDRESS through `emit_symbol_address`, which consults this table — no dedicated
    // helper needed, the lesson the GC family taught.
    ("_cstr_buf", CTX_CSTR_BUF_OFFSET),
    ("_cstr_buf2", CTX_CSTR_BUF2_OFFSET),
    ("_print_r_mode", CTX_PRINT_R_MODE_OFFSET),
    ("_print_r_off", CTX_PRINT_R_OFF_OFFSET),
    ("_print_r_buf", CTX_PRINT_R_BUF_OFFSET),
    ("_ob_level", CTX_OB_LEVEL_OFFSET),
    ("_ob_ptrs", CTX_OB_PTRS_OFFSET),
    ("_ob_lens", CTX_OB_LENS_OFFSET),
    ("_ob_caps", CTX_OB_CAPS_OFFSET),
    ("_ob_handler_stubs", CTX_OB_HANDLER_STUBS_OFFSET),
    ("_ob_handler_envs", CTX_OB_HANDLER_ENVS_OFFSET),
    ("_ob_name_ptrs", CTX_OB_NAME_PTRS_OFFSET),
    ("_ob_name_lens", CTX_OB_NAME_LENS_OFFSET),
    ("_ob_chunk_sizes", CTX_OB_CHUNK_SIZES_OFFSET),
    ("_ob_flags", CTX_OB_FLAGS_OFFSET),
    ("_ob_started", CTX_OB_STARTED_OFFSET),
    ("_eof_flags", CTX_EOF_FLAGS_OFFSET),
    ("_popen_files", CTX_POPEN_FILES_OFFSET),
    ("_dir_handles", CTX_DIR_HANDLES_OFFSET),
    ("_glob_handles", CTX_GLOB_HANDLES_OFFSET),
    ("_bzstream_handles", CTX_BZSTREAM_HANDLES_OFFSET),
    ("_stream_read_filters", CTX_STREAM_READ_FILTERS_OFFSET),
    ("_stream_write_filters", CTX_STREAM_WRITE_FILTERS_OFFSET),
    ("_stream_filter_buf", CTX_STREAM_FILTER_BUF_OFFSET),
];

/// The ctx field offset serving `symbol`, when this build routes it.
///
/// Returns `None` in a legacy build and for every symbol that is still process-global,
/// so a caller can fall through to its ordinary symbol addressing unchanged.
pub fn per_context_symbol_offset(emitter: &Emitter, symbol: &str) -> Option<usize> {
    if !emitter.ctx_register {
        return None;
    }
    PER_CONTEXT_SYMBOLS
        .iter()
        .find(|(name, _)| *name == symbol)
        .map(|(_, offset)| *offset)
}

/// Loads a ctx field into `reg` through the reserved ctx register.
///
/// `field_offset` must be one of the `CTX_*_OFFSET` constants from this module.
pub fn emit_ctx_load(emitter: &mut Emitter, reg: &str, field_offset: usize) {
    let ctx = ctx_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr {}, [{}, #{}]", reg, ctx, field_offset));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov {}, QWORD PTR [{} + {}]", reg, ctx, field_offset));
        }
    }
}

/// Stores `reg` into a ctx field through the reserved ctx register.
pub fn emit_ctx_store(emitter: &mut Emitter, reg: &str, field_offset: usize) {
    let ctx = ctx_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("str {}, [{}, #{}]", reg, ctx, field_offset));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [{} + {}], {}", ctx, field_offset, reg));
        }
    }
}

/// Stores a literal zero into a ctx field, the ctx-relative form of
/// `abi::emit_store_zero_to_symbol`.
///
/// Separate from `emit_ctx_store` because neither target needs a register to write a
/// zero: AArch64 has `xzr` and x86_64 takes an immediate, and borrowing a scratch here
/// would be a clobber the caller did not ask for.
pub fn emit_ctx_store_zero(emitter: &mut Emitter, field_offset: usize) {
    let ctx = ctx_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("str xzr, [{}, #{}]", ctx, field_offset));
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [{} + {}], 0", ctx, field_offset));
        }
    }
}

/// Computes the address of a ctx field into `dest` through the ctx register.
pub fn emit_ctx_address(emitter: &mut Emitter, dest: &str, field_offset: usize) {
    let ctx = ctx_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            // `add xN, xM, #imm` takes a 12-bit immediate, optionally shifted left by 12 —
            // so one instruction covers an offset below 4096, or an exact multiple of it,
            // and nothing else. The context has grown past 4 KiB of scalars and handle
            // tables, so an offset like 4712 is ordinary now and must not be left to the
            // assembler to reject. Split it into the two encodable halves rather than
            // contorting the layout to keep every field on a 4 KiB grid.
            if field_offset < 4096 {
                emitter.instruction(&format!("add {}, {}, #{}", dest, ctx, field_offset));
            } else {
                let page = field_offset & !0xfff;
                let rest = field_offset & 0xfff;
                emitter.instruction(&format!("add {}, {}, #{}", dest, ctx, page)); // 4 KiB-aligned part, encodable with the lsl #12 form
                if rest != 0 {
                    emitter.instruction(&format!("add {}, {}, #{}", dest, dest, rest)); // remainder, below 4096
                }
            }
        }
        Arch::X86_64 => {
            // x86_64 takes a full 32-bit displacement, so one `lea` covers any ctx field.
            emitter.instruction(&format!("lea {}, [{} + {}]", dest, ctx, field_offset));
        }
    }
}

/// Loads the free-list head value into `reg`, ctx-relative in ctx-register
/// mode and from the legacy global symbol otherwise.
///
/// Mirrors `emit_heap_off_load` for `_heap_free_list` so every helper that
/// consumes the free-list head shares one addressing-mode switch.
pub fn emit_free_list_head_load(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "ldr {}, [x28, #{}]",
                reg, CTX_HEAP_FREE_LIST_OFFSET
            )); // load the per-context free-list head
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov {}, QWORD PTR [r14 + {}]",
                reg, CTX_HEAP_FREE_LIST_OFFSET
            )); // load the per-context free-list head
        }
    }
}

/// Stores the heap bump offset `value` back into per-context state.
///
/// Companion to `emit_heap_off_load` for the bump-shrink paths (tail trimming, bump
/// resets) that write the allocator cursor back. It took a scratch register until the
/// legacy arm went: addressing a global needed one, addressing a ctx field does not, and
/// leaving the parameter would have callers reserving a register for nothing.
pub fn emit_heap_off_store(emitter: &mut Emitter, value: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "str {}, [x28, #{}]",
                value, CTX_HEAP_OFF_OFFSET
            )); // store the per-context heap bump offset
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov QWORD PTR [r14 + {}], {}",
                CTX_HEAP_OFF_OFFSET, value
            )); // store the per-context heap bump offset
        }
    }
}

/// Stores an immediate into the concat scratch write offset, ctx-relative in
/// ctx-register mode and to the legacy global symbol otherwise.
///
/// Used by the reset paths (web request reset, boundary isolation) that park
/// the scratch cursor at a known position rather than publishing a computed
/// value.
///
/// Clobbers: a NON-ZERO value needs a register to travel through on AArch64
/// (there is no store-immediate form), and that register is `x10` — the same
/// scratch the legacy arm's `emit_store_imm_to_symbol` uses, so both arms have
/// the same clobber set. Zero stores through `xzr` and clobbers nothing.
// Consumed by the M0 concat-family migration (see emit_concat_off_load).
#[allow(dead_code)]
pub fn emit_concat_off_store_imm(emitter: &mut Emitter, value: i64) {
    match emitter.target.arch {
        Arch::AArch64 => {
            // `str #imm, [...]` does not exist: anything but zero must be
            // materialized first, or the assembler rejects the helper (the
            // cdylib boundary parks the cursor at CONCAT_SCRATCH_CAPACITY).
            let source = if value == 0 {
                "xzr"
            } else {
                abi::emit_load_int_immediate(emitter, "x10", value);
                "x10"
            };
            emitter.instruction(&format!(
                "str {}, [x28, #{}]",
                source, CTX_CONCAT_OFF_OFFSET
            )); // store the immediate per-context concat offset
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov QWORD PTR [r14 + {}], {}",
                CTX_CONCAT_OFF_OFFSET, value
            )); // store the immediate per-context concat offset
        }
    }
}

/// Loads the concat scratch write offset into `reg`, ctx-relative in
/// ctx-register mode and from the legacy global symbol otherwise.
///
/// Contract: the legacy arm must not disturb any OTHER register — the
/// migrated string producers keep live state (sign flags, cursors) in the
/// x9/r11-class scratch the ABI loader would clobber, so the legacy AArch64
/// path resolves the symbol through the DESTINATION register itself and the
/// x86_64 path loads RIP-relative with no scratch at all.
// Consumed by the M0 concat-family migration (see emit_concat_off_load).
#[allow(dead_code)]
pub fn emit_concat_off_load(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "ldr {}, [x28, #{}]",
                reg, CTX_CONCAT_OFF_OFFSET
            )); // load the per-context concat scratch write offset
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov {}, QWORD PTR [r14 + {}]",
                reg, CTX_CONCAT_OFF_OFFSET
            )); // load the per-context concat scratch write offset
        }
    }
}

/// Stores the concat scratch write offset value back, ctx-relative in
/// ctx-register mode and to the legacy global symbol otherwise.
///
/// Contract: the legacy arm must not disturb the caller's live scratch — the
/// migrated string producers keep write cursors in x9/r11-class scratch across
/// the publish. The AArch64 legacy path therefore materializes the address in
/// **x6** (the historical concat-scratch addressing register, not used by any
/// call site around its publish) and the x86_64 path stores RIP-relative with
/// no scratch at all. Call sites must not keep a live value in x6 across this
/// helper on AArch64.
///
/// The x6 borrow is a contract in a comment, and the mechanical check behind it
/// (`legacy_aarch64_runtime_has_no_dangling_x6_stores`) covers the runtime text
/// only — user codegen that calls this helper is outside its reach. The one
/// misuse it CAN catch here is a call site passing x6 as the value itself,
/// which would silently store the address in place of the offset.
// Consumed by the M0 concat-family migration (see emit_concat_off_load).
#[allow(dead_code)]
pub fn emit_concat_off_store(emitter: &mut Emitter, value: &str) {
    debug_assert!(
        emitter.ctx_register || emitter.target.arch != Arch::AArch64 || value != "x6",
        "the AArch64 legacy arm materializes the _concat_off address in x6, so x6 \
         cannot also carry the value being stored"
    );
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "str {}, [x28, #{}]",
                value, CTX_CONCAT_OFF_OFFSET
            )); // store the per-context concat scratch write offset
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov QWORD PTR [r14 + {}], {}",
                CTX_CONCAT_OFF_OFFSET, value
            )); // store the per-context concat scratch write offset
        }
    }
}

/// Materializes the concat scratch BUFFER base address into `reg`,
/// ctx-relative in ctx-register mode and from the legacy global symbol
/// otherwise.
///
/// The buffer closes the `_rt_ctx` layout at a 64 KiB+ offset, so the ctx form
/// derives the address from the ctx register (an imm12-window `add`/`lea`) —
/// never an immediate-offset load on the far offset itself.
// Consumed by the M0 concat-family migration (see emit_concat_off_load).
#[allow(dead_code)]
pub fn emit_concat_buf_address(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "add {}, x28, #{}",
                reg, CTX_CONCAT_BUF_OFFSET
            )); // base of the per-context concat scratch buffer
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "lea {}, [r14 + {}]",
                reg, CTX_CONCAT_BUF_OFFSET
            )); // base of the per-context concat scratch buffer
        }
    }
}

/// Materializes the free-list head SLOT address into `reg`, ctx-relative in
/// ctx-register mode and from the legacy global symbol otherwise.
///
/// Distinct from `emit_free_list_head_load` (which loads the head VALUE): the
/// ordered-insertion and merge paths keep a mutable pointer to the previous
/// next-slot, so they need the slot's address rather than its contents.
pub fn emit_free_list_address(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "add {}, x28, #{}",
                reg, CTX_HEAP_FREE_LIST_OFFSET
            )); // address of the per-context free-list head slot
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "lea {}, [r14 + {}]",
                reg, CTX_HEAP_FREE_LIST_OFFSET
            )); // address of the per-context free-list head slot
        }
    }
}

/// Materializes the small-bin head array address into `reg`, ctx-relative in
/// ctx-register mode and from the legacy global symbol otherwise.
pub fn emit_small_bins_address(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "add {}, x28, #{}",
                reg, CTX_HEAP_SMALL_BINS_OFFSET
            )); // base of the per-context small-bin head array
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "lea {}, [r14 + {}]",
                reg, CTX_HEAP_SMALL_BINS_OFFSET
            )); // base of the per-context small-bin head array
        }
    }
}

/// Materializes the heap ARENA BASE into `reg` — the address the allocator adds
/// `heap_off` to.
///
/// In ctx mode this is a LOAD from the context, not a symbol materialization:
/// the base is what makes one context's memory its own. In legacy mode it stays
/// the `_heap_buf` symbol address, which is what every caller expected before.
///
/// The only ctx-mode site that may still name `_heap_buf` is `__rt_ctx_init`,
/// which installs the main context's base — pinned by
/// `ctx_runtime_names_the_heap_arena_symbol_once`.
pub fn emit_heap_base_address(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "ldr {}, [x28, #{}]",
                reg, CTX_HEAP_BASE_OFFSET
            )); // load this context's heap arena base
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov {}, QWORD PTR [r14 + {}]",
                reg, CTX_HEAP_BASE_OFFSET
            )); // load this context's heap arena base
        }
    }
}

/// Loads the heap CAPACITY in bytes into `reg`.
///
/// Legacy mode reads the `_heap_max` global; ctx mode reads the field, because
/// two contexts may hold arenas of different sizes and the exhaustion check has
/// to be about the arena actually being bumped.
///
/// Clobbers `reg` and NOTHING else. The AArch64 legacy arm resolves the symbol
/// through the destination register rather than calling
/// `abi::emit_load_symbol_to_reg`, which borrows x9: the call sites this
/// replaced kept live values there, and going through x9 turned every
/// allocation into "heap memory exhausted" — the limit check read a corrupted
/// working register instead of the capacity.
pub fn emit_heap_max_load(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "ldr {}, [x28, #{}]",
                reg, CTX_HEAP_MAX_OFFSET
            )); // load this context's heap capacity
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov {}, QWORD PTR [r14 + {}]",
                reg, CTX_HEAP_MAX_OFFSET
            )); // load this context's heap capacity
        }
    }
}

/// Loads the current heap bump offset into `reg`, ctx-relative in ctx-register
/// mode and from the legacy global symbol otherwise.
///
/// This is the shared range-check front end: every refcount/deep-free helper
/// that validates `ptr < _heap_buf + _heap_off` goes through it, so the
/// ctx-mode runtime keeps one consistent source for the live heap end.
pub fn emit_heap_off_load(emitter: &mut Emitter, reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!(
                "ldr {}, [x28, #{}]",
                reg, CTX_HEAP_OFF_OFFSET
            )); // load the per-context heap bump offset
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov {}, QWORD PTR [r14 + {}]",
                reg, CTX_HEAP_OFF_OFFSET
            )); // load the per-context heap bump offset
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::RuntimeFeatures;
    use crate::codegen_support::emit::Emitter;
    use crate::codegen_support::platform::{Arch, Platform, Target};

    /// `ctx_reg` resolves to the reserved callee-saved register on each target.
    #[test]
    fn ctx_register_is_reserved_callee_saved_per_target() {
        let arm = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        assert_eq!(ctx_reg(&arm), "x28");
        let x86 = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        assert_eq!(ctx_reg(&x86), "r14");
    }

    /// The ctx layout is self-consistent: scalars first, buffer last, every
    /// scalar offset inside the AArch64 imm12 window, size 16-byte aligned.
    #[test]
    fn ctx_layout_offsets_are_ordered_and_size_is_aligned() {
        // Scalars lead so their offsets stay within the unsigned-imm12 window.
        assert_eq!(CTX_CONCAT_OFF_OFFSET, 0);
        assert_eq!(CTX_HEAP_OFF_OFFSET, 8);
        assert_eq!(CTX_HEAP_FREE_LIST_OFFSET, 16);
        assert_eq!(CTX_HEAP_SMALL_BINS_OFFSET, 24);
        // The arena the allocator bumps into travels with the state that indexes
        // it, right after the bins.
        assert_eq!(CTX_HEAP_BASE_OFFSET, 24 + CTX_HEAP_SMALL_BIN_COUNT * 8);
        assert_eq!(CTX_HEAP_MAX_OFFSET, CTX_HEAP_BASE_OFFSET + 8);
        // The exception family follows the heap state: three scalars, and the reason they
        // are here at all is that a thread which throws must not walk the main thread's
        // handler chain.
        assert_eq!(CTX_EXC_VALUE_OFFSET, CTX_HEAP_MAX_OFFSET + 8);
        assert_eq!(CTX_EXC_HANDLER_TOP_OFFSET, CTX_EXC_VALUE_OFFSET + 8);
        assert_eq!(CTX_EXC_CALL_FRAME_TOP_OFFSET, CTX_EXC_HANDLER_TOP_OFFSET + 8);
        // The fiber/stack family follows: fiber state, then the two call-stack floors.
        // Both block M1 for the same reason exceptions do — a second context cannot share
        // "which fiber is current", and no single floor value is correct for two stacks.
        assert_eq!(CTX_FIBER_CURRENT_OFFSET, CTX_EXC_CALL_FRAME_TOP_OFFSET + 8);
        assert_eq!(CTX_FIBER_MAIN_SAVED_SP_OFFSET, CTX_FIBER_CURRENT_OFFSET + 8);
        assert_eq!(CTX_FIBER_MAIN_SAVED_EXC_OFFSET, CTX_FIBER_MAIN_SAVED_SP_OFFSET + 8);
        assert_eq!(
            CTX_FIBER_MAIN_SAVED_CALL_FRAME_OFFSET,
            CTX_FIBER_MAIN_SAVED_EXC_OFFSET + 8
        );
        assert_eq!(CTX_STACK_LIMIT_OFFSET, CTX_FIBER_MAIN_SAVED_CALL_FRAME_OFFSET + 8);
        assert_eq!(CTX_STACK_LIMIT_MAIN_OFFSET, CTX_STACK_LIMIT_OFFSET + 8);
        // Allocator bookkeeping, then the collector's re-entrancy lock.
        assert_eq!(CTX_GC_ALLOCS_OFFSET, CTX_STACK_LIMIT_MAIN_OFFSET + 8);
        assert_eq!(CTX_GC_FREES_OFFSET, CTX_GC_ALLOCS_OFFSET + 8);
        assert_eq!(CTX_GC_LIVE_OFFSET, CTX_GC_FREES_OFFSET + 8);
        assert_eq!(CTX_GC_PEAK_OFFSET, CTX_GC_LIVE_OFFSET + 8);
        assert_eq!(CTX_GC_COLLECTING_OFFSET, CTX_GC_PEAK_OFFSET + 8);
        // Every scalar stays inside the first 4 KiB window, which is what keeps
        // `add xN, x28, #off` and `ldr xN, [x28, #off]` single instructions.
        assert_eq!(CTX_PRINT_R_MODE_OFFSET, CTX_GC_COLLECTING_OFFSET + 8);
        assert_eq!(CTX_OB_LEVEL_OFFSET, CTX_PRINT_R_OFF_OFFSET + 8);
        assert_eq!(CTX_OB_PTRS_OFFSET, CTX_OB_LEVEL_OFFSET + 8);
        assert_eq!(CTX_OB_NAME_PTRS_OFFSET, CTX_OB_HANDLER_ENVS_OFFSET + CTX_OB_TABLE_SIZE);
        assert_eq!(CTX_OB_STARTED_OFFSET, CTX_OB_FLAGS_OFFSET + CTX_OB_TABLE_SIZE);
        // Ten handle tables of 512 bytes do not fit under 4 KiB beside the scalars, and
        // that is fine: `emit_ctx_address` splits an offset above the imm12 window into two
        // encodable adds. What must hold is only that the buffers start after them.
        assert_eq!(CTX_POOL_STATE_PTR_OFFSET, CTX_OB_STARTED_OFFSET + CTX_OB_TABLE_SIZE);
        assert_eq!(CTX_EOF_FLAGS_OFFSET, CTX_POOL_STATE_PTR_OFFSET + 8);
        // `__rt_ctx_acquire` steps between slots with `add xN, xN, #CTX_SIZE`, which AArch64
        // encodes in one instruction only for a multiple of 4096. Pinned here because the
        // helper would otherwise fail to assemble the moment the layout drifts off that grid.
        assert_eq!(CTX_SIZE % 4096, 0, "the slot stride must stay encodable as an add immediate");
        assert_eq!(
            CTX_STREAM_WRITE_FILTERS_OFFSET,
            CTX_STREAM_READ_FILTERS_OFFSET + CTX_FILTER_TABLE_SIZE
        );
        assert!(
            CTX_CSTR_BUF_OFFSET >= CTX_STREAM_WRITE_FILTERS_OFFSET + CTX_FILTER_TABLE_SIZE
        );
        // The buffers follow, each on a 4 KiB boundary so its address stays one `add`.
        assert_eq!(CTX_CSTR_BUF_OFFSET % 4096, 0);
        assert_eq!(CTX_CSTR_BUF2_OFFSET % 4096, 0);
        assert_eq!(CTX_CONCAT_BUF_OFFSET % 4096, 0);
        assert_eq!(CTX_CSTR_BUF2_OFFSET, CTX_CSTR_BUF_OFFSET + CTX_CSTR_BUF_SIZE);
        assert_eq!(CTX_CONCAT_BUF_OFFSET, CTX_CSTR_BUF2_OFFSET + CTX_CSTR_BUF_SIZE);
        assert_eq!(CTX_PRINT_R_BUF_OFFSET, CTX_CONCAT_BUF_OFFSET + CTX_CONCAT_BUF_CAPACITY);
        assert_eq!(CTX_PRINT_R_BUF_OFFSET % 4096, 0);
        assert_eq!(
            CTX_STREAM_FILTER_BUF_OFFSET,
            CTX_PRINT_R_BUF_OFFSET + CTX_CONCAT_BUF_CAPACITY
        );
        // Every scalar offset must be encodable as ldr [x28, #imm] (imm12 ≤ 4095).
        for offset in [
            CTX_CONCAT_OFF_OFFSET,
            CTX_HEAP_OFF_OFFSET,
            CTX_HEAP_FREE_LIST_OFFSET,
            CTX_HEAP_SMALL_BINS_OFFSET,
            CTX_HEAP_BASE_OFFSET,
            CTX_HEAP_MAX_OFFSET,
            CTX_EXC_VALUE_OFFSET,
            CTX_EXC_HANDLER_TOP_OFFSET,
            CTX_EXC_CALL_FRAME_TOP_OFFSET,
            CTX_FIBER_CURRENT_OFFSET,
            CTX_FIBER_MAIN_SAVED_SP_OFFSET,
            CTX_FIBER_MAIN_SAVED_EXC_OFFSET,
            CTX_FIBER_MAIN_SAVED_CALL_FRAME_OFFSET,
            CTX_STACK_LIMIT_OFFSET,
            CTX_STACK_LIMIT_MAIN_OFFSET,
            CTX_GC_ALLOCS_OFFSET,
            CTX_GC_FREES_OFFSET,
            CTX_GC_LIVE_OFFSET,
            CTX_GC_PEAK_OFFSET,
            CTX_GC_COLLECTING_OFFSET,
        ] {
            assert!(offset + 8 <= 4096, "scalar ctx offset {offset} escapes the imm12 window");
        }
        assert!(CTX_SIZE >= CTX_CONCAT_BUF_OFFSET + CTX_CONCAT_BUF_CAPACITY);
        assert_eq!(CTX_SIZE % 16, 0);
        // The context's size is a per-THREAD cost once M1 pools them, so it is pinned
        // rather than left to drift. Three 64 KiB scratch buffers (concat, print_r capture,
        // stream-filter) dominate it; the scalars and every handle table together are under
        // 15 KiB. Allocating the scratch lazily is the obvious lever if this ever matters.
        assert_eq!(CTX_SIZE, 221_184, "the per-context footprint changed");
        // The concat scratch dominates the context, so a ctx instance is ~64 KiB.
        assert!(CTX_SIZE > CTX_CONCAT_BUF_CAPACITY);
    }

    /// `emit_rt_ctx_data` declares exactly one `_rt_ctx` of `CTX_SIZE` bytes, as
    /// a COMMON symbol: `.space` inside `.data` writes 64 KiB of zeroes into
    /// every ctx binary, which nearly doubled a small program's image.
    #[test]
    fn rt_ctx_data_emits_single_zero_filled_instance() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_rt_ctx_data(&mut emitter, true);
        let asm = emitter.output();
        assert!(asm.contains(&format!(".comm _rt_ctx, {}", CTX_SIZE)), "{asm}");
        assert!(!asm.contains(".space"), "the ctx block must not be image bytes: {asm}");
    }

    /// `__rt_ctx_init` publishes `_rt_ctx` into the ctx register and zeroes every
    /// mutable field on AArch64.
    #[test]
    fn ctx_init_publishes_pointer_and_zeroes_fields_aarch64() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_rt_ctx_init(&mut emitter);
        let asm = emitter.output();
        // Publishes the ctx base into x28 via the platform symbol helper (adrp+add).
        assert!(asm.contains("adrp x28, _rt_ctx"), "{asm}");
        assert!(asm.contains("add x28, x28, _rt_ctx"), "{asm}");
        // Zeroes concat off, heap off, free list, and all four small-bin heads.
        for offset in [
            CTX_CONCAT_OFF_OFFSET,
            CTX_HEAP_OFF_OFFSET,
            CTX_HEAP_FREE_LIST_OFFSET,
        ] {
            assert!(asm.contains(&format!("str xzr, [x28, #{}]", offset)), "{asm}");
        }
        for bin in 0..CTX_HEAP_SMALL_BIN_COUNT {
            let offset = CTX_HEAP_SMALL_BINS_OFFSET + bin * 8;
            assert!(asm.contains(&format!("str xzr, [x28, #{}]", offset)), "{asm}");
        }
    }

    /// `__rt_ctx_init` publishes `_rt_ctx` into r14 and zeroes every mutable field
    /// on x86_64.
    #[test]
    fn ctx_init_publishes_pointer_and_zeroes_fields_x86_64() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_rt_ctx_init(&mut emitter);
        let asm = emitter.output();
        assert!(asm.contains("lea r14, [rip + _rt_ctx]"), "{asm}");
        for offset in [
            CTX_CONCAT_OFF_OFFSET,
            CTX_HEAP_OFF_OFFSET,
            CTX_HEAP_FREE_LIST_OFFSET,
        ] {
            assert!(
                asm.contains(&format!("mov QWORD PTR [r14 + {}], 0", offset)),
                "{asm}"
            );
        }
        for bin in 0..CTX_HEAP_SMALL_BIN_COUNT {
            let offset = CTX_HEAP_SMALL_BINS_OFFSET + bin * 8;
            assert!(
                asm.contains(&format!("mov QWORD PTR [r14 + {}], 0", offset)),
                "{asm}"
            );
        }
    }

    /// Ctx-relative loads/stores/addresses go through the ctx register on both
    /// targets and never reference a global symbol.
    #[test]
    fn ctx_access_helpers_route_through_ctx_register() {
        for (target, scratch, load, store, address) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "x9",
                "ldr x9, [x28, #72]",
                "str x9, [x28, #72]",
                "add x9, x28, #72",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "r9",
                "mov r9, QWORD PTR [r14 + 72]",
                "mov QWORD PTR [r14 + 72], r9",
                "lea r9, [r14 + 72]",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_ctx_load(&mut emitter, scratch, 72);
            emit_ctx_store(&mut emitter, scratch, 72);
            emit_ctx_address(&mut emitter, scratch, 72);
            let asm = emitter.output();
            assert!(asm.contains(load), "{asm}");
            assert!(asm.contains(store), "{asm}");
            assert!(asm.contains(address), "{asm}");
            assert!(!asm.contains("adrp"), "ctx access must not materialize symbols: {asm}");
        }
    }

    /// The heap bump offset is read through the ctx register, with no symbol in sight.
    ///
    /// This test used to check BOTH arms of an addressing switch — ctx through x28, legacy
    /// through `adrp`/`add`/`ldr` against `_heap_off`. There is one arm now. What is left
    /// is worth keeping on its own: a symbol materialization reappearing here would mean
    /// the allocator had found the global again, which is the shape that let a half-routed
    /// free path exhaust an 8 MiB heap during the spike.
    #[test]
    fn heap_off_load_reads_through_the_ctx_register() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_heap_off_load(&mut emitter, "x10");
        let asm = emitter.output();
        assert!(
            asm.contains(&format!("ldr x10, [x28, #{}]", CTX_HEAP_OFF_OFFSET)),
            "{asm}"
        );
        assert!(!asm.contains("adrp"), "no symbol may be materialized here: {asm}");

        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_heap_off_load(&mut emitter, "r11");
        let asm = emitter.output();
        assert!(
            asm.contains(&format!("mov r11, QWORD PTR [r14 + {}]", CTX_HEAP_OFF_OFFSET)),
            "{asm}"
        );
        assert!(!asm.contains("rip +"), "no symbol may be materialized here: {asm}");
    }

    /// The full generated runtime honors the ctx-register feature end to end:
    /// `__rt_ctx_init` is present, `__rt_heap_alloc` reads heap state through
    /// x28, and the `_rt_ctx` data block is declared.
    #[test]
    fn ctx_feature_generates_ctx_addressed_runtime_end_to_end() {
        use crate::codegen_support::driver_support::generate_runtime_with_features;
        let target = Target::new(Platform::MacOS, Arch::AArch64);
        let asm = generate_runtime_with_features(
            8 * 1024 * 1024,
            target,
            RuntimeFeatures {
                ctx_register: true,
                ..RuntimeFeatures::none()
            },
        );
        // The publication helper is emitted and installs x28.
        assert!(asm.contains("__rt_ctx_init:"), "ctx runtime must emit __rt_ctx_init");
        assert!(asm.contains("adrp x28, _rt_ctx"), "{asm}");
        // The allocator reads per-context heap state through x28.
        assert!(
            asm.contains(&format!("ldr x10, [x28, #{}]", CTX_HEAP_OFF_OFFSET)),
            "ctx runtime allocator must bump through x28"
        );
        // The data block backs it, zero-filled rather than written to the image.
        assert!(asm.contains(&format!(".comm _rt_ctx, {}", CTX_SIZE)), "{asm}");
    }

    /// Collects every instruction of the ALL-features ctx-mode runtime that
    /// touches the reserved ctx register outside a sanctioned shape.
    ///
    /// Sanctioned shapes: the publish/install sequences, ctx-relative
    /// loads/stores/addresses, and whole-register save/restore pairs (fiber
    /// switch, frame spill). Anything else is a hand-written helper using the
    /// ctx register as ordinary scratch, which silently corrupts the
    /// per-context state pointer and no other test would catch.
    ///
    /// Runs on ALL features: a helper that scratches the ctx register inside a
    /// feature-gated family would otherwise only corrupt state on the builds
    /// that enable it — the hardest failure to attribute.
    fn ctx_register_scratch_offenders(
        platform: Platform,
        arch: Arch,
        ctx_reg: &str,
    ) -> Vec<String> {
        use crate::codegen_support::driver_support::generate_runtime_with_features;
        let target = Target::new(platform, arch);
        let comment = target.line_comment_prefix();
        let asm = generate_runtime_with_features(
            8 * 1024 * 1024,
            target,
            RuntimeFeatures {
                ctx_register: true,
                ..RuntimeFeatures::all()
            },
        );
        let mut offenders: Vec<String> = Vec::new();
        for raw in asm.lines() {
            let line = raw.trim();
            // INSTRUCTIONS only: skip blanks, comments, directives and labels.
            // Reading the instruction stream is the whole point of the audit —
            // an earlier version filtered lines on the comment prefix and so
            // scanned nothing but comment text, which made it always green.
            if line.is_empty() || line.starts_with(comment) || line.starts_with('.') {
                continue;
            }
            let line = line.split(comment).next().unwrap_or(line).trim();
            if line.is_empty() || line.ends_with(':') {
                continue;
            }
            if !mentions_register(line, ctx_reg) {
                continue;
            }
            let sanctioned = match arch {
                Arch::AArch64 => {
                    line.starts_with("stp x27, x28,")
                        || line.starts_with("ldp x27, x28,")
                        || line.starts_with("adrp x28, _rt_ctx")
                        || line.starts_with("add x28, x28, _rt_ctx")
                        || ((line.starts_with("ldr ") || line.starts_with("str "))
                            && line.contains("[x28, #")
                            && !line.starts_with("ldr x28"))
                        || line.starts_with("str x28, [sp")
                        || line.starts_with("ldr x28, [sp")
                        || (line.starts_with("add x") && line.contains(", x28, #"))
                }
                Arch::X86_64 => {
                    line == "push r14"
                        || line == "pop r14"
                        || line.starts_with("lea r14, [rip + _rt_ctx]")
                        // whole-register save/restore (fiber switch context block)
                        || line.starts_with("mov r14, QWORD PTR [")
                        || line.ends_with(", r14")
                        || ((line.starts_with("mov ") || line.starts_with("lea "))
                            && line.contains("[r14 +"))
                }
            };
            if !sanctioned {
                offenders.push(line.to_string());
            }
        }
        offenders
    }

    /// A NON-ZERO concat-offset reset must emit a real store on both targets.
    ///
    /// AArch64 has no store-immediate form, so the value has to travel through
    /// a register: the first version emitted `str #65536, [x28, #0]`, which the
    /// assembler rejects — and the only caller that passes a non-zero value is
    /// the cdylib/staticlib boundary, so every `--rt-ctx` library build on
    /// AArch64 failed to assemble while every executable test stayed green.
    #[test]
    fn concat_off_store_imm_materializes_a_non_zero_value() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            for ctx_register in [false, true] {
                let mut emitter = Emitter::new(Target::new(platform, arch));
                emitter.ctx_register = ctx_register;
                emit_concat_off_store_imm(&mut emitter, 65_536);
                let asm = emitter.output();
                // No operand may be a bare immediate in a store position.
                assert!(
                    !asm.contains("str #"),
                    "{arch:?} (ctx={ctx_register}) emitted a store of a bare immediate:\n{asm}"
                );
                match arch {
                    // AArch64 materializes 65536 as movz/movk (0x1 << 16) and
                    // stores through the register it built.
                    Arch::AArch64 => assert!(
                        asm.contains("str x10, "),
                        "{arch:?} (ctx={ctx_register}) must store through the materialized register:\n{asm}"
                    ),
                    Arch::X86_64 => assert!(
                        asm.contains("65536"),
                        "{arch:?} (ctx={ctx_register}) lost the stored value:\n{asm}"
                    ),
                }
            }
        }

        // Zero still stores through the zero register: no scratch clobbered.
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emitter.ctx_register = true;
        emit_concat_off_store_imm(&mut emitter, 0);
        let asm = emitter.output();
        assert!(asm.contains("str xzr, [x28, #0]"), "{asm}");
        assert!(!asm.contains("x10"), "a zero reset must clobber nothing:\n{asm}");
    }

    /// The arena accessors touch the DESTINATION register and nothing else, in
    /// either mode.
    ///
    /// They replaced call sites that materialized the symbol through the
    /// destination itself, so a helper borrowing the usual x9 scratch instead
    /// clobbers a working register the site still needs. That is not theory: the
    /// first version routed the capacity load through `emit_load_symbol_to_reg`,
    /// which borrows x9, and every allocation in the suite died with "heap
    /// memory exhausted" — the limit check compared against a corrupted x9.
    #[test]
    fn heap_arena_accessors_clobber_only_their_destination() {
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            for ctx_register in [false, true] {
                let mut emitter = Emitter::new(Target::new(platform, arch));
                emitter.ctx_register = ctx_register;
                let dest = if arch == Arch::AArch64 { "x13" } else { "r13" };
                emit_heap_base_address(&mut emitter, dest);
                emit_heap_max_load(&mut emitter, dest);
                let asm = emitter.output();
                for borrowed in ["x9", "x10", "r10", "r11", "rax"] {
                    assert!(
                        !mentions_register(&asm.replace('\n', " "), borrowed),
                        "{arch:?} (ctx={ctx_register}) arena accessors borrowed {borrowed}; \
                         the sites they replaced expect only {dest} to change:\n{asm}"
                    );
                }
            }
        }
    }

    /// In a ctx build, exactly ONE site may name the heap arena symbols: the
    /// init that installs the main context's arena. Everything else reads the
    /// base and the capacity out of the context.
    ///
    /// This is the arena family's tripwire, and it has to be a COUNT rather than
    /// the concat family's "omit the symbol from the data section" trick: the
    /// main context's arena still IS `_heap_buf`, so the symbol cannot go away.
    /// A helper that keeps materializing it would otherwise bump the right
    /// per-context offset into the wrong context's memory — the state made
    /// per-context while the memory stayed shared, which is precisely the bug
    /// this field exists to remove.
    #[test]
    fn ctx_runtime_names_the_heap_arena_symbol_once() {
        use crate::codegen_support::driver_support::generate_runtime_with_features;
        // Count the MATERIALIZATION, not every mention: AArch64 spells one
        // address as an `adrp`/`add` pair, so counting lines would report two
        // for a single site.
        for (platform, arch, pattern, materializer) in [
            (Platform::MacOS, Arch::AArch64, "adrp x10, _heap_buf", "adrp "),
            (
                Platform::Linux,
                Arch::X86_64,
                "lea r10, [rip + _heap_buf]",
                "lea ",
            ),
        ] {
            let asm = generate_runtime_with_features(
                8 * 1024 * 1024,
                Target::new(platform, arch),
                RuntimeFeatures {
                    ctx_register: true,
                    ..RuntimeFeatures::all()
                },
            );
            let materializations = asm
                .lines()
                .map(str::trim)
                .filter(|line| line.contains("_heap_buf") && line.starts_with(materializer))
                .count();
            assert_eq!(
                materializations, 1,
                "{arch:?} ctx runtime must name _heap_buf exactly once (in __rt_ctx_init), \
                 found {materializations}; every other heap site reads the base from the \
                 context, which is what gives a second context its own memory"
            );
            assert!(
                asm.contains(pattern),
                "{arch:?} ctx runtime must install the default arena in __rt_ctx_init"
            );
            // This used to compare against a legacy runtime that named `_heap_buf`
            // everywhere, as a control proving the two modes really differed. There is
            // no legacy runtime now, and the assertion that carries the weight was
            // always the one above: ONE materialization, in the init. A second would
            // mean some helper found the arena without going through the context, which
            // is exactly what stops a second context from owning its own memory.
        }
    }

    /// Scratch audit for the reserved ctx register on AArch64 (spike review,
    /// B3): x28 carries the per-context state pointer, so no hand-written
    /// helper may borrow it. Zero tolerance — this arm of the runtime is
    /// fully migrated.
    #[test]
    fn aarch64_ctx_runtime_never_scratches_the_ctx_register() {
        let offenders = ctx_register_scratch_offenders(Platform::MacOS, Arch::AArch64, "x28");
        assert!(
            offenders.is_empty(),
            "AArch64 ctx runtime uses x28 outside sanctioned shapes ({} offenders):\n{}",
            offenders.len(),
            offenders.join("\n"),
        );
    }

    /// The same audit on x86_64, also zero tolerance.
    ///
    /// It started as a shrinking baseline of 81: reading the instruction stream
    /// instead of the comment text exposed every helper family that still
    /// borrowed r14 — strtotime's weekday table, the fiber/generator API, the
    /// hash and array walkers, the user-filter brigade, the getX family. Each
    /// one overwrote the context pointer mid-helper, and several then called
    /// straight into compiled PHP (a usort comparator, an array_reduce
    /// callback, a Fiber body).
    ///
    /// Where a helper had a free non-allocated register the value moved there
    /// (r15, or a caller-saved one in a leaf); where it did not, it moved to
    /// rbx with the caller's value preserved, or to a frame slot. Two of them
    /// needed no register at all — a memory-operand `cmp` replaced the pair.
    #[test]
    fn x86_64_ctx_runtime_never_scratches_the_ctx_register() {
        let offenders = ctx_register_scratch_offenders(Platform::Linux, Arch::X86_64, "r14");
        assert!(
            offenders.is_empty(),
            "x86_64 ctx runtime uses r14 outside sanctioned shapes ({} offenders).\n\
             A helper borrowed r14 — the reserved ctx register. Use a free non-allocated \
             register, rbx (preserving the caller's value), or a frame slot.\n{}",
            offenders.len(),
            offenders.join("\n"),
        );
    }

    /// True when `line` names `reg` as a register operand rather than as a
    /// substring of another token (`x28` inside a symbol like `_probe_x280`).
    /// Sub-register writes clobber the full register, so `r14d`/`r14w` count.
    fn mentions_register(line: &str, reg: &str) -> bool {
        let bytes = line.as_bytes();
        let mut from = 0usize;
        while let Some(found) = line[from..].find(reg) {
            let start = from + found;
            let end = start + reg.len();
            let before_ok = start == 0
                || (!bytes[start - 1].is_ascii_alphanumeric() && bytes[start - 1] != b'_');
            let after_ok = match bytes.get(end).copied() {
                None => true,
                Some(byte) => !byte.is_ascii_digit() && byte != b'_',
            };
            if before_ok && after_ok {
                return true;
            }
            from = end;
        }
        false
    }

    /// What one emitted instruction does to `rbx`.
    #[derive(PartialEq)]
    enum RbxUse {
        /// Stores rbx to the STACK — `push rbx`, or a spill to `[rbp …]`/`[rsp …]`.
        Save,
        /// Loads rbx back FROM the stack.
        Restore,
        /// Any other write to rbx.
        Scratch,
        /// Reads rbx, or does not mention it.
        None,
    }

    /// Classifies one instruction's use of `rbx`.
    ///
    /// The distinction that matters is the STACK: only a store of rbx to the
    /// frame counts as a save, and only a load from the frame counts as the
    /// matching restore. An earlier version accepted any `…], rbx` as a save and
    /// any `rbx, QWORD PTR [` as a restore, so a helper that merely stored rbx's
    /// value into a data structure looked like it had saved it, and one that
    /// loaded ordinary data INTO rbx looked like it had restored it — the two
    /// shapes that hide a real clobber.
    fn classify_rbx(instr: &str) -> RbxUse {
        if !mentions_register(instr, "rbx") && !mentions_register(instr, "ebx") {
            // bl/bh are byte views of the same register.
            if !mentions_register(instr, "bl") && !mentions_register(instr, "bx") {
                return RbxUse::None;
            }
        }
        if instr == "push rbx" {
            return RbxUse::Save;
        }
        if instr == "pop rbx" {
            return RbxUse::Restore;
        }
        let stack_operand = instr.contains("[rbp") || instr.contains("[rsp");
        // `mov QWORD PTR [rbp - 8], rbx` — a spill.
        if stack_operand && instr.ends_with(", rbx") {
            return RbxUse::Save;
        }
        // `mov rbx, QWORD PTR [rbp - 8]` — the matching reload.
        if stack_operand && instr.starts_with("mov rbx, ") {
            return RbxUse::Restore;
        }
        // Destination-first: anything else whose first operand is rbx writes it.
        let writes = instr
            .split_once(' ')
            .map(|(mnemonic, rest)| {
                let dest = rest.split(',').next().unwrap_or("").trim();
                let reads_only = matches!(mnemonic, "cmp" | "test" | "push");
                !reads_only && matches!(dest, "rbx" | "ebx" | "bx" | "bl" | "bh")
            })
            .unwrap_or(false);
        if writes {
            RbxUse::Scratch
        } else {
            RbxUse::None
        }
    }

    /// The rbx callee-saved contract (spike review round 2, NB1): rbx is the
    /// ONLY callee-saved register the x86_64 linear-scan allocator assigns to
    /// cross-call values, and the EIR prologue preserves the caller's rbx once
    /// per FUNCTION entry — a runtime helper that scratches rbx and returns
    /// corrupts the live cross-call value of the calling PHP frame. Every
    /// emitted x86_64 helper that writes rbx must therefore balance it with
    /// push/pop (or spill/restore) pairs, one pop on EVERY return path.
    ///
    /// The audit walks every `__rt_*` label in the fully generated runtime and
    /// verifies the balance per helper body; a stray `ret` on a rbx-scratching
    /// helper fails here instead of miscompiling user code in the field.
    ///
    /// It runs on the ALL-features runtime: a feature-gated family (spl, zval
    /// packing, json, vsprintf, http) is exactly where an unbalanced helper
    /// hides from a base-feature scan, and those families are the ones the
    /// `r14` → `rbx` scratch migration touched.
    #[test]
    fn x86_64_runtime_helpers_that_scratch_rbx_preserve_it() {
        use crate::codegen_support::driver_support::generate_runtime_with_features;
        let asm = generate_runtime_with_features(
            8 * 1024 * 1024,
            Target::new(Platform::Linux, Arch::X86_64),
            RuntimeFeatures::all(),
        );
        let offenders = rbx_preservation_offenders(&asm);
        assert!(
            offenders.is_empty(),
            "x86_64 runtime helpers must preserve the allocator's rbx register:\n{}",
            offenders.join("\n")
        );
    }

    /// Per-`__rt_*` helper, reports the ones that write rbx without giving the
    /// caller's value back on every return path.
    ///
    /// Shared with the negative control below: an audit whose control exercises
    /// a private copy of the logic proves nothing about the audit that runs.
    fn rbx_preservation_offenders(asm: &str) -> Vec<String> {
        let mut offenders: Vec<String> = Vec::new();
        let mut current: Option<String> = None;
        let mut scratches_rbx = false;
        let mut push_depth: i32 = 0;
        let mut pop_depth: i32 = 0;
        let mut previous_was_globl = false;
        for line in asm.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(".globl") {
                previous_was_globl = true;
                continue;
            }
            if trimmed.starts_with("__rt_") && previous_was_globl {
                if let Some(name) = current.take() {
                    if scratches_rbx && (push_depth == 0 || pop_depth < push_depth) {
                        offenders.push(format!(
                            "{name}: writes rbx with {push_depth} push(es) vs {pop_depth} pop(s)"
                        ));
                    }
                }
                current = Some(trimmed.split(':').next().unwrap_or(trimmed).to_string());
                scratches_rbx = false;
                push_depth = 0;
                pop_depth = 0;
                continue;
            }
            if trimmed.starts_with("ret") || trimmed.starts_with("jmp __rt_") {
                if let Some(name) = current.clone() {
                    if scratches_rbx && (push_depth == 0 || pop_depth < push_depth) {
                        offenders.push(format!(
                            "{name}: writes rbx with {push_depth} push(es) vs {pop_depth} pop(s)"
                        ));
                    }
                    // A tail-jump hands the balance duty to the target helper.
                    current = None;
                    scratches_rbx = false;
                    push_depth = 0;
                    pop_depth = 0;
                }
            }
            previous_was_globl = false;
            match classify_rbx(trimmed) {
                RbxUse::Save => push_depth += 1,
                RbxUse::Restore => pop_depth += 1,
                RbxUse::Scratch => scratches_rbx = true,
                RbxUse::None => {}
            }
        }
        if let Some(name) = current.take() {
            if scratches_rbx && (push_depth == 0 || pop_depth < push_depth) {
                offenders.push(format!(
                    "{name}: writes rbx with {push_depth} push(es) vs {pop_depth} pop(s)"
                ));
            }
        }
        offenders
    }


    /// Negative control for the rbx audit, running the SAME scanner the real
    /// audit runs. Each case is a shape the audit has to get right, and the two
    /// marked below are the ones the first version got wrong: it accepted any
    /// store whose source was rbx as a save, and any load whose destination was
    /// rbx as a restore, so a helper that merely passed rbx's value to a data
    /// structure looked protected, and one that loaded ordinary data into rbx
    /// looked like it had restored it.
    #[test]
    fn rbx_audit_flags_unprotected_helpers_and_accepts_protected_ones() {
        let helper = |body: &str| {
            format!(".globl __rt_probe\n__rt_probe:\n{body}    ret\n")
        };

        // Plain unprotected scratch.
        assert!(
            !rbx_preservation_offenders(&helper("    mov rbx, 5\n")).is_empty(),
            "an unprotected rbx write must be flagged"
        );
        // Protected by push/pop.
        assert!(
            rbx_preservation_offenders(&helper("    push rbx\n    mov rbx, 5\n    pop rbx\n"))
                .is_empty(),
            "a push/pop pair protects the caller's rbx"
        );
        // Protected by a frame spill.
        assert!(
            rbx_preservation_offenders(&helper(
                "    mov QWORD PTR [rbp - 8], rbx\n    mov rbx, 5\n    mov rbx, QWORD PTR [rbp - 8]\n"
            ))
            .is_empty(),
            "a frame spill and its reload protect the caller's rbx"
        );
        // MISSED BY THE FIRST VERSION: storing rbx's value into a data
        // structure is not a save, so the write that follows is unprotected.
        assert!(
            !rbx_preservation_offenders(&helper("    mov QWORD PTR [r10], rbx\n    mov rbx, 5\n"))
                .is_empty(),
            "a store THROUGH a pointer is not a save of rbx"
        );
        // MISSED BY THE FIRST VERSION: loading ordinary data into rbx is a
        // clobber, not a restore.
        assert!(
            !rbx_preservation_offenders(&helper("    mov rbx, QWORD PTR [r10 + 8]\n")).is_empty(),
            "loading data into rbx clobbers the caller's value"
        );
        // Reading rbx without writing it is not a clobber.
        assert!(
            rbx_preservation_offenders(&helper("    cmp rbx, 4\n    mov r10, rbx\n")).is_empty(),
            "reading rbx leaves the caller's value intact"
        );
        // A sub-register write clobbers the whole register.
        assert!(
            !rbx_preservation_offenders(&helper("    xor ebx, ebx\n")).is_empty(),
            "a 32-bit write to ebx zeroes the whole of rbx"
        );
    }

    /// Dangling-x6 audit (spike review round 2, NB3): the legacy AArch64 arm of
    /// `emit_concat_off_store` borrows x6 to hold the `_concat_off` symbol
    /// address. x6 is an ABI argument register, so the borrow is only sound if
    /// every x6 write in the emitted legacy runtime either (a) is the adrp that
    /// materializes `_concat_off` for the store helper, or (b) belongs to a
    /// helper whose contract already owns x6 (itoa/strtoupper-style string
    /// producers that load the buffer address through x6 immediately before
    /// use). The audit walks each label-delimited block of the FULL legacy
    /// runtime text and fails on any `str/mov ..., [x6]` whose x6 was not
    /// written in the same block by an address materialization — the exact
    /// "orphaned store" class the naive concat migration produced (wild writes
    /// into argument registers caught only by luck of coverage).
    #[test]
    fn legacy_aarch64_runtime_has_no_dangling_x6_stores() {
        use crate::codegen_support::driver_support::generate_runtime_with_features;
        let asm = generate_runtime_with_features(
            8 * 1024 * 1024,
            Target::new(Platform::MacOS, Arch::AArch64),
            RuntimeFeatures::none(),
        );
        let mut offenders: Vec<String> = Vec::new();
        let mut current_label = String::from("<prelude>");
        let mut x6_addressed_in_block = false;
        let mut block_start = 0usize;
        let mut previous_was_globl = false;
        for (index, line) in asm.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with(".globl") {
                previous_was_globl = true;
                continue;
            }
            // Only GLOBAL helper boundaries reset the borrow scope: internal
            // loop labels share the helper's arm of x6 materialization.
            if trimmed.ends_with(':') && previous_was_globl {
                current_label = trimmed.trim_end_matches(':').to_string();
                x6_addressed_in_block = false;
                block_start = index;
                previous_was_globl = false;
                continue;
            }
            previous_was_globl = false;
            // Any write that establishes x6 (address materialization or a
            // value load) re-arms the borrow for the rest of the helper.
            let writes_x6 = trimmed.starts_with("adrp x6,")
                || trimmed.starts_with("add x6,")
                || trimmed.starts_with("ldr x6,")
                || trimmed.starts_with("ldrb w6,")
                || trimmed.starts_with("mov x6,")
                || trimmed.starts_with("mov w6,")
                || trimmed.starts_with("ldp x6,");
            if writes_x6 {
                x6_addressed_in_block = true;
                continue;
            }
            // A store/indirect access through x6 without a same-block
            // materialization is a dangling borrow.
            let uses_x6_as_base = (trimmed.starts_with("str ")
                || trimmed.starts_with("ldr ")
                || trimmed.starts_with("strb ")
                || trimmed.starts_with("ldrb "))
                && trimmed.contains("[x6]")
                && !trimmed.starts_with("ldr x6");
            if uses_x6_as_base && !x6_addressed_in_block {
                offenders.push(format!(
                    "{current_label} (line ~{index}, block from ~{block_start}): [x6] access without a same-block adrp materialization"
                ));
            }
        }
        assert!(
            offenders.is_empty(),
            "legacy runtime contains dangling x6 borrows (the orphaned-store class):\n{}",
            offenders.join("\n")
        );
    }

    /// Kitchen-sink ctx gate (spike review round 2, D-test-2): the ctx-mode
    /// runtime emitted with EVERY feature on must contain ZERO references to
    /// the legacy heap and concat state symbols. Together with the link
    /// tripwire (ctx builds omit those symbols from the data section), this
    /// pins that no feature-gated helper family — regex, fibers, generators,
    /// eval, phar, descriptor invoker, web — silently keeps a legacy
    /// symbol-addressed path that only fails on the shard that happens to
    /// enable that feature.
    #[test]
    fn ctx_runtime_kitchen_sink_references_no_legacy_state_symbols() {
        use crate::codegen_support::driver_support::generate_runtime_with_features_mode;
        for (platform, arch) in [
            (Platform::MacOS, Arch::AArch64),
            (Platform::Linux, Arch::X86_64),
        ] {
            let asm = generate_runtime_with_features_mode(
                8 * 1024 * 1024,
                Target::new(platform, arch),
                {
                    let mut features = RuntimeFeatures::all();
                    features.ctx_register = true;
                    features
                },
                false,
                false,
            );
            for legacy in [
                "_heap_off",
                "_heap_free_list",
                "_heap_small_bins",
                "_concat_off",
                "_concat_buf",
            ] {
                assert!(
                    !asm.contains(&format!("adrp x9, {legacy}"))
                        && !asm.contains(&format!("rip + {legacy}")),
                    "{arch:?} ctx runtime (all features) still references {legacy}"
                );
                assert!(
                    !asm.contains(&format!(".comm {legacy}")),
                    "{arch:?} ctx runtime (all features) must not declare {legacy}"
                );
            }
        }
    }
}
