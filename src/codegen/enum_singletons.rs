//! Purpose:
//! Emits LAZY enum case singletons: the per-case materializer functions that
//! allocate a case object on its FIRST evaluation and publish it to the case's
//! global slot, plus the guarded load sequence every case-read site uses.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()`, which emits the materializers
//!   once per program alongside the user functions.
//! - `crate::codegen::lower_inst::scoped_constants` (`E::A`),
//!   `crate::codegen::lower_inst::enums` (`cases()` / `from()` / `tryFrom()`),
//!   `crate::codegen::lower_inst::objects::reflection` and
//!   `crate::codegen::eval_class_constant_helpers`, which all read a case slot.
//! - `crate::codegen::web::emit_web_reset()`, which clears the slots per request.
//!
//! Key details:
//! - WHY LAZY. PHP creates an enum case object on FIRST ACCESS and caches it, so a
//!   case that is never evaluated allocates nothing and consumes no object handle.
//!   elephc used to materialize every case of every referenced enum in `main`'s
//!   prologue, which burnt one handle per case before user code ran and shifted
//!   every subsequent `var_dump()` `#N` / `spl_object_id()` by that count. The slot
//!   is the same `.comm` word as before; only the moment it is filled moved.
//! - IDENTITY IS THE SLOT, NOT THE ALLOCATION. Every read goes through the same
//!   global slot, and a materializer writes it exactly once (it returns early when
//!   the slot is already non-null). So `E::A === E::A` compares one pointer against
//!   itself, `in_array($e, E::cases(), true)` finds the same pointer, and a case
//!   used as a `match` subject or an array key keeps working. A scheme that
//!   allocated per access would print plausible handles and break all of that.
//! - PURE VS BACKED ENUMS DIFFER, AND PHP'S DIFFERENCE IS REPRODUCED HERE.
//!   Verified against PHP 8.5.6: a PURE enum materializes only the case you touch,
//!   while a BACKED enum materializes EVERY case, in declaration order, on the
//!   first touch of ANY case (php-src builds the whole backing table at once).
//!   `enum E { case A; case B; case C; } $e = E::B;` leaves two cases unborn;
//!   `enum E: int { … } $e = E::B;` creates all three, handing A handle 1. So pure
//!   enums get one materializer PER CASE and backed enums get ONE materializer for
//!   the whole enum, which every case of that enum calls.
//! - THE MATERIALIZER PRESERVES EVERY CALLER-SAVED INTEGER REGISTER. Case reads
//!   happen in the middle of expression lowering, where the surrounding lowering
//!   may hold values in scratch registers. Saving/restoring x0-x17 (rax/rcx/rdx/
//!   rsi/rdi/r8-r11 on x86_64) makes the helper droppable at ANY read site without
//!   re-auditing liveness, and it costs nothing measurable because the body runs at
//!   most once per case per process. The float registers are left alone: the body's
//!   only call is `__rt_heap_alloc`, which every ordinary `new` already calls from
//!   arbitrary expression positions, so the helper's residual clobber set is a
//!   subset of what the backend already tolerates.
//! - OWNERSHIP LIVES AT THE READ SITES, AND ONE OF THEM WAS ALREADY BROKEN. This
//!   module only allocates; the singleton is created with the refcount
//!   `__rt_heap_alloc` hands out and is owned by its global slot for the life of the
//!   process (or, under `--web`, of the request). Handing a case OUT is the read
//!   site's job: `cases()` and `from()`/`tryFrom()` already increfed what they
//!   return (issue #349), but a bare `E::A` read did not — so a case passed into a
//!   typed parameter or returned from a typed function was released once by the
//!   consumer and FREED while its slot still pointed at it. That predates lazy
//!   materialization and is provable on the eager build: `spl_object_id(D::A)` after
//!   such a call reports handle `0`, the "this block never acquired a handle" value.
//!   Eager creation hid the consequence because no other case reused the freed
//!   block; lazily, the next case materialized after the free lands on exactly that
//!   block and two slots alias one object, turning a latent use-after-free into
//!   `E::A === E::B`. `scoped_constants::lower_scoped_constant_get` now increfs like
//!   the other read sites. Over-retaining is the safe direction for a
//!   process-lifetime singleton: it can only guarantee what should already hold.
//! - PER-PROCESS UNDER `--web`, BY CONSTRUCTION. The slots are `.comm` (BSS), so a
//!   prefork worker gets a private copy the moment it writes one; the parent never
//!   runs user code, so no slot is populated before the fork. Requests are served
//!   serially inside a worker, so no two writers race a slot and no atomics are
//!   needed. `emit_enum_slot_resets` clears every slot in `__rt_web_reset`, which
//!   keeps the pre-existing per-request lifecycle exactly: today the handler
//!   prologue re-runs the eager initializers and overwrites the slots each request,
//!   so cases were already per-request objects. Clearing (rather than carrying the
//!   pointer across requests) is the conservative choice: per-request cleanup can
//!   release a value that reached a top-level local, so a slot reused across
//!   requests could outlive its object.

use crate::codegen::abi;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::Module;
use crate::names::{enum_case_symbol, join_php_symbol};
use crate::types::{ClassInfo, EnumCaseInfo, EnumCaseValue, EnumInfo, PhpType};

use super::context::FunctionContext;

/// Frame size for a materializer: just the x29/x30 footer (`push rbp` on x86_64),
/// which is all the body needs on top of its explicit register saves.
const MATERIALIZER_FRAME_SIZE: usize = 16;

/// Byte size of the AArch64 caller-saved integer save area (x0-x17, nine pairs).
const AARCH64_SAVE_AREA: usize = 144;

/// The AArch64 caller-saved integer registers a materializer preserves, in pair order.
const AARCH64_SAVED_PAIRS: [(&str, &str); 9] = [
    ("x0", "x1"),
    ("x2", "x3"),
    ("x4", "x5"),
    ("x6", "x7"),
    ("x8", "x9"),
    ("x10", "x11"),
    ("x12", "x13"),
    ("x14", "x15"),
    ("x16", "x17"),
];

/// The x86_64 caller-saved integer registers a materializer preserves, in push order.
const X86_64_SAVED: [&str; 9] = [
    "rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11",
];

/// Returns the materializer symbol for one enum case of a PURE enum.
///
/// Format: `_enum_init_<enum>_<case>`. Uses the same injective `join_php_symbol()` encoding as
/// `enum_case_symbol()` so the two symbols always agree on how a name was escaped.
fn enum_case_init_symbol(enum_name: &str, case_name: &str) -> String {
    join_php_symbol("_enum_init", &[enum_name, case_name])
}

/// Returns the whole-enum materializer symbol used by BACKED enums.
///
/// Format: `_enum_init_all_<enum>_<first_case>` — derived from the first case so it needs no
/// separate mangling helper and cannot collide with a per-case symbol.
fn enum_init_all_symbol(enum_name: &str, first_case: &str) -> String {
    join_php_symbol("_enum_init_all", &[enum_name, first_case])
}

/// Returns the materializer to call before reading `enum_name::case_name`, or
/// `None` when the enum has no runtime class shape (nothing can be materialized).
///
/// A backed enum answers with its whole-enum materializer for EVERY case, which is
/// what reproduces PHP's "first touch of any case creates them all".
fn materializer_symbol(module: &Module, enum_name: &str, case_name: &str) -> Option<String> {
    let enum_info = module.enum_infos.get(enum_name)?;
    if !enum_info.cases.iter().any(|case| case.name == case_name) {
        return None;
    }
    module.class_infos.get(enum_name)?;
    Some(materializer_symbol_for(enum_info, enum_name, case_name))
}

/// Returns the materializer symbol for a case of an enum already known to exist.
fn materializer_symbol_for(enum_info: &EnumInfo, enum_name: &str, case_name: &str) -> String {
    if enum_info.backing_type.is_some() {
        let first = enum_info
            .cases
            .first()
            .map(|case| case.name.as_str())
            .unwrap_or(case_name);
        enum_init_all_symbol(enum_name, first)
    } else {
        enum_case_init_symbol(enum_name, case_name)
    }
}

/// Emits the guarded lazy read of one enum case singleton into the integer result
/// register, at a site that owns a `FunctionContext` (and therefore a label generator).
///
/// Shape: load the slot, take the already-materialized fast path when it is
/// non-null, otherwise call the materializer and re-load the now-published slot.
/// The fast path is two instructions, so a hot `E::A` read costs a load and a
/// branch over the cold call.
pub(super) fn emit_lazy_case_load(ctx: &mut FunctionContext<'_>, enum_name: &str, case_name: &str) {
    let slot = enum_case_symbol(enum_name, case_name);
    let Some(init) = materializer_symbol(ctx.module, enum_name, case_name) else {
        // No runtime class shape for this enum: keep the historical bare load, which
        // reads the zero-initialized slot rather than inventing an object.
        abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot, 0);
        return;
    };
    let done = ctx.next_label("enum_case_ready");
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &done);
    abi::emit_call_label(ctx.emitter, &init);
    abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &slot, 0);
    ctx.emitter.label(&done);
}

/// Emits the lazy read of one enum case singleton into the integer result register
/// at a site that has no label generator (the eval and Reflection constant bridges).
///
/// Calls the materializer unconditionally instead of branching over it. The
/// materializer is idempotent and preserves every caller-saved integer register, so
/// the only cost is one call on a path that is already a bridge call.
pub(super) fn emit_lazy_case_load_unguarded(
    emitter: &mut Emitter,
    module: &Module,
    enum_name: &str,
    case_name: &str,
) {
    if let Some(init) = materializer_symbol(module, enum_name, case_name) {
        abi::emit_call_label(emitter, &init);
    }
    let slot = enum_case_symbol(enum_name, case_name);
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), &slot, 0);
}

/// Emits the materializer call that `from()`/`tryFrom()` needs before its scan.
///
/// PHP materializes every case of a backed enum when `from()`/`tryFrom()` runs —
/// including when the argument matches nothing, verified against 8.5.6:
/// `S::tryFrom('nope')` on a three-case enum still leaves the next object at `#4`.
/// The unrolled scan compares against compile-time literals and only touches a case
/// slot on a match, so without this call a failed lookup would materialize nothing.
pub(super) fn emit_materialize_all_cases(ctx: &mut FunctionContext<'_>, enum_name: &str) {
    let Some(enum_info) = ctx.module.enum_infos.get(enum_name) else {
        return;
    };
    let Some(first) = enum_info.cases.first() else {
        return;
    };
    let first_name = first.name.clone();
    if materializer_symbol(ctx.module, enum_name, &first_name).is_none() {
        return;
    }
    let init = {
        let enum_info = &ctx.module.enum_infos[enum_name];
        materializer_symbol_for(enum_info, enum_name, &first_name)
    };
    abi::emit_call_label(ctx.emitter, &init);
}

/// Emits every enum case materializer the program can call, once per module.
///
/// Materializers are emitted for EVERY declared enum that has a runtime class
/// shape, not just the ones a reachability scan proves are read. That filter used
/// to matter because an eagerly initialized enum burnt handles; a materializer that
/// is never called costs only its own instructions, and emitting all of them means
/// a read site can never reference a missing symbol.
pub(super) fn emit_enum_case_materializers(
    emitter: &mut Emitter,
    module: &Module,
    data: &mut DataSection,
) {
    let mut sorted_enums = module.enum_infos.iter().collect::<Vec<_>>();
    sorted_enums.sort_by_key(|(name, _)| name.as_str());
    for (enum_name, enum_info) in sorted_enums {
        let Some(class_info) = module.class_infos.get(enum_name) else {
            continue;
        };
        if enum_info.cases.is_empty() {
            continue;
        }
        if enum_info.backing_type.is_some() {
            emit_backed_enum_materializer(emitter, data, enum_name, enum_info, class_info);
        } else {
            for case in &enum_info.cases {
                emit_pure_case_materializer(emitter, data, enum_name, enum_info, class_info, case);
            }
        }
    }
}

/// Emits the whole-enum materializer for a BACKED enum: one function that creates
/// every case, in declaration order, guarded so it runs at most once.
///
/// The guard reads the FIRST case's slot, which is sound because this function is
/// the only writer of any slot of this enum and it writes them all together.
fn emit_backed_enum_materializer(
    emitter: &mut Emitter,
    data: &mut DataSection,
    enum_name: &str,
    enum_info: &EnumInfo,
    class_info: &ClassInfo,
) {
    let first = &enum_info.cases[0];
    let symbol = enum_init_all_symbol(enum_name, &first.name);
    let done = format!("{}_done", symbol);
    let guard_slot = enum_case_symbol(enum_name, &first.name);
    let name_offset = enum_name_property_offset(class_info);

    emit_materializer_prologue(emitter, &symbol, &format!("backed enum {}", enum_name));
    emit_guard_on_slot(emitter, &guard_slot, &done);
    for case in &enum_info.cases {
        emit_one_case(
            emitter,
            data,
            enum_name,
            class_info,
            name_offset,
            case,
        );
    }
    emit_materializer_epilogue(emitter, &done);
}

/// Emits the single-case materializer for one case of a PURE enum, guarded on its
/// own slot so repeated reads allocate exactly once.
fn emit_pure_case_materializer(
    emitter: &mut Emitter,
    data: &mut DataSection,
    enum_name: &str,
    _enum_info: &EnumInfo,
    class_info: &ClassInfo,
    case: &EnumCaseInfo,
) {
    let symbol = enum_case_init_symbol(enum_name, &case.name);
    let done = format!("{}_done", symbol);
    let slot = enum_case_symbol(enum_name, &case.name);
    let name_offset = enum_name_property_offset(class_info);

    emit_materializer_prologue(
        emitter,
        &symbol,
        &format!("enum case {}::{}", enum_name, case.name),
    );
    emit_guard_on_slot(emitter, &slot, &done);
    emit_one_case(emitter, data, enum_name, class_info, name_offset, case);
    emit_materializer_epilogue(emitter, &done);
}

/// Returns the byte offset of an enum's `name` property.
///
/// The class metadata is authoritative; the last-property fallback (`8 + (count-1)*16`)
/// only applies if the slot is somehow absent, matching what the eager initializer did.
fn enum_name_property_offset(class_info: &ClassInfo) -> usize {
    class_info
        .property_offsets
        .get("name")
        .copied()
        .unwrap_or_else(|| 8 + class_info.properties.len().saturating_sub(1) * 16)
}

/// Emits a materializer's label, frame and caller-saved integer register saves.
fn emit_materializer_prologue(emitter: &mut Emitter, symbol: &str, what: &str) {
    if emitter.target.arch == Arch::AArch64 {
        emitter.raw(".align 2");
    }
    emitter.blank();
    emitter.comment(&format!("--- lazy materializer: {} ---", what));
    emitter.label_global(symbol);
    abi::emit_frame_prologue(emitter, MATERIALIZER_FRAME_SIZE);
    match emitter.target.arch {
        Arch::AArch64 => {
            for (index, (lo, hi)) in AARCH64_SAVED_PAIRS.iter().enumerate() {
                if index == 0 {
                    emitter.instruction(&format!(
                        "stp {}, {}, [sp, #-{}]!",
                        lo, hi, AARCH64_SAVE_AREA
                    ));                                                         // open the caller-saved save area and store the first pair
                } else {
                    emitter.instruction(
                        &format!("stp {}, {}, [sp, #{}]", lo, hi, index * 16)
                    );                                                          // preserve one more caller-saved integer pair
                }
            }
        }
        Arch::X86_64 => {
            for reg in X86_64_SAVED {
                emitter.instruction(&format!("push {}", reg));                  // preserve one caller-saved integer register
            }
            emitter.instruction("sub rsp, 8");                                  // restore the SysV 16-byte call alignment the odd push count broke
        }
    }
}

/// Emits the "already materialized" early exit: when the guard slot is non-null the
/// cases exist, so the body is skipped entirely.
fn emit_guard_on_slot(emitter: &mut Emitter, slot: &str, done: &str) {
    emitter.comment("skip the whole body once the cases already exist");
    abi::emit_load_symbol_to_reg(emitter, abi::int_result_reg(emitter), slot, 0);
    abi::emit_branch_if_int_result_nonzero(emitter, done);
}

/// Emits a materializer's done label, register restores and return.
///
/// The body contains a nested `bl __rt_heap_alloc`, but this function owns a real
/// frame and reloads x29/x30 from it, so returning with `ret` cannot see a clobbered
/// link register.
fn emit_materializer_epilogue(emitter: &mut Emitter, done: &str) {
    emitter.label(done);
    match emitter.target.arch {
        Arch::AArch64 => {
            for (index, (lo, hi)) in AARCH64_SAVED_PAIRS.iter().enumerate().rev() {
                if index == 0 {
                    emitter.instruction(&format!(
                        "ldp {}, {}, [sp], #{}",
                        lo, hi, AARCH64_SAVE_AREA
                    ));                                                         // restore the first pair and close the caller-saved save area
                } else {
                    emitter.instruction(
                        &format!("ldp {}, {}, [sp, #{}]", lo, hi, index * 16)
                    );                                                          // restore one more caller-saved integer pair
                }
            }
        }
        Arch::X86_64 => {
            emitter.instruction("add rsp, 8");                                  // release the SysV call-alignment pad
            for reg in X86_64_SAVED.iter().rev() {
                emitter.instruction(&format!("pop {}", reg));                   // restore one caller-saved integer register
            }
        }
    }
    abi::emit_frame_restore(emitter, MATERIALIZER_FRAME_SIZE);
    abi::emit_return(emitter);
}

/// Allocates one case object, fills its `value`/`name` properties and publishes it
/// to its global slot. This is the body the eager `main`-prologue initializer used
/// to run; only its trigger moved.
fn emit_one_case(
    emitter: &mut Emitter,
    data: &mut DataSection,
    enum_name: &str,
    class_info: &ClassInfo,
    name_offset: usize,
    case: &EnumCaseInfo,
) {
    emitter.comment(&format!(
        "materialize enum singleton {}::{}",
        enum_name, case.name
    ));
    emit_enum_object_allocation(emitter, class_info.class_id, class_info.properties.len());
    if let Some(case_value) = &case.value {
        emit_enum_backing_value(emitter, data, case_value);
    }
    emit_enum_name_property(emitter, data, &case.name, name_offset);
    let symbol = enum_case_symbol(enum_name, &case.name);
    abi::emit_store_reg_to_symbol(emitter, abi::int_result_reg(emitter), &symbol, 0);
}

/// Writes an enum case's `name` string (the case identifier) into its singleton name slot.
///
/// The name is interned as a static data-section string, so the pointer/length pair stored here
/// mirrors a string-backed enum's `value` slot and needs no refcount management.
fn emit_enum_name_property(
    emitter: &mut Emitter,
    data: &mut DataSection,
    case_name: &str,
    offset: usize,
) {
    let object_reg = abi::int_result_reg(emitter);
    let temp_reg = abi::temp_int_reg(emitter.target);
    let (label, len) = data.add_string(case_name.as_bytes());
    abi::emit_symbol_address(emitter, temp_reg, &label);
    abi::emit_store_to_address(emitter, temp_reg, object_reg, offset);
    abi::emit_load_int_immediate(emitter, temp_reg, len as i64);
    abi::emit_store_to_address(emitter, temp_reg, object_reg, offset + 8);
}

/// Allocates an object-shaped enum singleton and zeroes its property storage.
fn emit_enum_object_allocation(emitter: &mut Emitter, class_id: u64, property_count: usize) {
    let payload_size = 8 + property_count * 16;
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", payload_size));         // request enum singleton object payload storage
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction("mov x9, #4");                                  // heap kind 4 marks enum singletons as object instances
            emitter.instruction("str x9, [x0, #-8]");                           // stamp the heap header before the enum singleton payload
            emitter.instruction("bl __rt_object_handle_acquire");               // bind the new object to its PHP object handle
            emitter.instruction(&format!("mov x10, #{}", class_id));            // materialize the enum class id
            emitter.instruction("str x10, [x0]");                               // store the enum class id at payload offset zero
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rax, {}", payload_size));         // request enum singleton object payload storage
            abi::emit_call_label(emitter, "__rt_heap_alloc");
            emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(4)
            ));                                                                 // materialize the x86_64 object heap kind word
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // stamp the heap header before the enum singleton payload
            emitter.instruction("call __rt_object_handle_acquire");             // bind the new object to its PHP object handle
            emitter.instruction(&format!("mov r10, {}", class_id));             // materialize the enum class id
            emitter.instruction("mov QWORD PTR [rax], r10");                    // store the enum class id at payload offset zero
        }
    }
    let object_reg = abi::int_result_reg(emitter);
    for index in 0..property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(emitter, object_reg, offset);
        abi::emit_store_zero_to_address(emitter, object_reg, offset + 8);
    }
}

/// Writes a backed enum case value into the singleton's first property slot.
fn emit_enum_backing_value(
    emitter: &mut Emitter,
    data: &mut DataSection,
    case_value: &EnumCaseValue,
) {
    let object_reg = abi::int_result_reg(emitter);
    let temp_reg = abi::temp_int_reg(emitter.target);
    match case_value {
        EnumCaseValue::Int(value) => {
            abi::emit_load_int_immediate(emitter, temp_reg, *value);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, 8);
            abi::emit_store_zero_to_address(emitter, object_reg, 16);
        }
        EnumCaseValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (label, len) = data.add_string(&bytes);
            abi::emit_symbol_address(emitter, temp_reg, &label);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, 8);
            abi::emit_load_int_immediate(emitter, temp_reg, len as i64);
            abi::emit_store_to_address(emitter, temp_reg, object_reg, 16);
        }
    }
}

/// Emits the `--web` per-request clear of every enum case slot.
///
/// Restores the pre-lazy per-request lifecycle: the eager initializers used to
/// overwrite every slot with a fresh object at the top of each request, so cases
/// were already per-request objects. Clearing the slot means request N+1
/// re-materializes on demand instead of reusing request N's pointer, which a
/// per-request local cleanup may already have released.
pub(super) fn emit_enum_slot_resets(emitter: &mut Emitter, module: &Module) {
    let mut sorted_enums = module.enum_infos.iter().collect::<Vec<_>>();
    sorted_enums.sort_by_key(|(name, _)| name.as_str());
    for (enum_name, enum_info) in sorted_enums {
        for case in &enum_info.cases {
            let symbol = enum_case_symbol(enum_name, &case.name);
            emitter.comment(&format!(
                "clear lazy enum case slot {}::{}",
                enum_name, case.name
            ));
            abi::emit_store_zero_to_symbol(emitter, &symbol, 0);
        }
    }
}

/// Releases every materialized enum singleton at process shutdown.
///
/// Enum case slots keep the one process-lifetime reference that makes repeated case reads
/// canonical. Main has already released ordinary locals, statics, and globals when it reaches
/// this cleanup, so dropping that final slot reference is safe and lets heap-debug distinguish
/// genuine leaks from intentional singleton caching.
pub(super) fn emit_enum_slot_cleanup(ctx: &mut FunctionContext<'_>) {
    let mut slots = ctx
        .module
        .enum_infos
        .iter()
        .flat_map(|(enum_name, enum_info)| {
            enum_info.cases.iter().map(move |case| {
                (
                    enum_name.clone(),
                    case.name.clone(),
                    enum_case_symbol(enum_name, &case.name),
                )
            })
        })
        .collect::<Vec<_>>();
    slots.sort_by(|left, right| left.2.cmp(&right.2));

    for (enum_name, case_name, symbol) in slots {
        let done = ctx.next_label("enum_singleton_cleanup_done");
        let ty = PhpType::Object(enum_name);
        ctx.emitter.comment(&format!(
            "epilogue release enum singleton {} from {}",
            case_name, symbol
        ));
        abi::emit_load_symbol_to_reg(ctx.emitter, abi::int_result_reg(ctx.emitter), &symbol, 0);
        abi::emit_branch_if_int_result_zero(ctx.emitter, &done);
        abi::emit_decref_if_refcounted(ctx.emitter, &ty);
        abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 0);
        ctx.emitter.label(&done);
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// Locks the symbol derivation: both materializer names must be built from the
    /// same mangling as the slot they fill, so a case name needing escapes cannot
    /// produce a call to a label that was never emitted.
    #[test]
    fn materializer_symbols_track_the_slot_mangling() {
        let slot = enum_case_symbol("App\\Suit", "Hearts");
        assert_eq!(slot, "_enum_case___App_N_Suit___Hearts");
        assert_eq!(
            enum_case_init_symbol("App\\Suit", "Hearts"),
            "_enum_init___App_N_Suit___Hearts"
        );
        assert_eq!(
            enum_init_all_symbol("App\\Suit", "Hearts"),
            "_enum_init_all___App_N_Suit___Hearts"
        );
    }

    /// The AArch64 save area must stay 16-byte aligned and cover exactly the pairs
    /// the prologue writes, or the frame below it is misaligned.
    #[test]
    fn aarch64_save_area_matches_the_saved_pairs() {
        assert_eq!(AARCH64_SAVE_AREA, AARCH64_SAVED_PAIRS.len() * 16);
        assert_eq!(AARCH64_SAVE_AREA % 16, 0);
    }

    /// The x86_64 push count must be odd so the explicit 8-byte pad restores the
    /// SysV 16-byte alignment a call requires.
    #[test]
    fn x86_64_push_count_needs_the_alignment_pad() {
        assert_eq!(X86_64_SAVED.len() % 2, 1);
    }

    /// Emits a complete materializer body on a target and returns the assembly.
    ///
    /// Drives the same primitives `emit_pure_case_materializer` uses, so the
    /// arch-specific halves are exercised on BOTH targets without needing a full
    /// `Module` (whose `ClassInfo` is far too large to synthesize meaningfully).
    fn materializer_asm(target: Target) -> String {
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        emit_materializer_prologue(&mut emitter, "_enum_init_E_A", "enum case E::A");
        emit_guard_on_slot(&mut emitter, "_enum_case_E_A", "_enum_init_E_A_done");
        emit_enum_object_allocation(&mut emitter, 7, 2);
        emit_enum_backing_value(&mut emitter, &mut data, &EnumCaseValue::Str("a".into()));
        emit_enum_name_property(&mut emitter, &mut data, "A", 24);
        let result_reg = abi::int_result_reg(&emitter);
        abi::emit_store_reg_to_symbol(&mut emitter, result_reg, "_enum_case_E_A", 0);
        emit_materializer_epilogue(&mut emitter, "_enum_init_E_A_done");
        emitter.output()
    }

    /// Both targets must emit a materializer that is a real function: a labelled
    /// entry, the early-exit guard, the handle-acquiring allocation, a store to the
    /// case slot, and a return.
    #[test]
    fn emits_a_materializer_on_both_targets() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let asm = materializer_asm(target);
            assert!(asm.contains("_enum_init_E_A:"), "{target:?}");
            assert!(asm.contains("_enum_init_E_A_done:"), "{target:?}");
            assert!(asm.contains("__rt_heap_alloc"), "{target:?}");
            // The handle must be acquired inside the materializer, or a lazily
            // created case would render as `#0` instead of taking a real handle.
            assert!(asm.contains("__rt_object_handle_acquire"), "{target:?}");
            assert!(asm.contains("_enum_case_E_A"), "{target:?}");
        }
    }

    /// Every caller-saved integer register the materializer touches must be both
    /// saved and restored. A read site drops this call mid-expression, so a
    /// register saved but not restored (or vice versa) would corrupt whatever the
    /// surrounding lowering was holding — silently, and only on the FIRST read of
    /// each case.
    #[test]
    fn saves_and_restores_every_caller_saved_integer_register() {
        let arm = materializer_asm(Target::new(Platform::MacOS, Arch::AArch64));
        for (lo, hi) in AARCH64_SAVED_PAIRS {
            assert!(
                arm.contains(&format!("stp {}, {}, [sp", lo, hi)),
                "missing save of {lo}/{hi}"
            );
            assert!(
                arm.contains(&format!("ldp {}, {}, [sp", lo, hi)),
                "missing restore of {lo}/{hi}"
            );
        }

        let x86 = materializer_asm(Target::new(Platform::Linux, Arch::X86_64));
        for reg in X86_64_SAVED {
            assert!(x86.contains(&format!("push {}", reg)), "missing push of {reg}");
            assert!(x86.contains(&format!("pop {}", reg)), "missing pop of {reg}");
        }
        // The odd push count is corrected by an explicit pad, so `call` sees a
        // 16-byte-aligned stack; both halves must be present exactly once.
        assert_eq!(x86.matches("sub rsp, 8").count(), 1);
        assert_eq!(x86.matches("add rsp, 8").count(), 1);
    }

    /// The AArch64 restores must mirror the saves through the SAME save-area size,
    /// so the stack pointer lands exactly where the prologue left it.
    #[test]
    fn aarch64_save_area_is_opened_and_closed_once() {
        let arm = materializer_asm(Target::new(Platform::MacOS, Arch::AArch64));
        assert_eq!(
            arm.matches(&format!("[sp, #-{}]!", AARCH64_SAVE_AREA)).count(),
            1
        );
        assert_eq!(
            arm.matches(&format!("[sp], #{}", AARCH64_SAVE_AREA)).count(),
            1
        );
        // The body contains a nested `bl`, so the helper owns a real frame and
        // returns through `ret` after reloading x30 from it.
        assert!(arm.contains("bl __rt_heap_alloc"));
        assert!(arm.trim_end().ends_with("ret"));
    }
}
