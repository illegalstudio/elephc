//! Purpose:
//! Walks EIR basic blocks in function order and delegates instruction/terminator lowering.
//! Owns function setup for the initial Phase 04 backend path.
//!
//! Called from:
//! - `crate::codegen_ir::generate_user_asm_from_ir()`.
//!
//! Key details:
//! - This first backend increment supports straight-line main blocks and reports
//!   explicit unsupported-feature errors for control flow not lowered yet.
//! - The main prologue initializes supported static-property storage before
//!   user blocks run.

use crate::codegen::abi;
use crate::codegen::context::DeferredFiberWrapper;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit_fiber_wrapper;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::Emit;
use crate::codegen::UNINITIALIZED_TYPED_PROPERTY_SENTINEL;
use crate::ir::{BasicBlock, Function, InstId, Module};
use crate::names::{
    enum_case_symbol, function_epilogue_symbol, function_symbol, method_symbol, php_symbol_key,
    static_method_symbol, static_property_symbol,
};
use crate::parser::ast::ExprKind;
use crate::types::{EnumCaseInfo, EnumCaseValue, PhpType};

use super::context::FunctionContext;
use super::fibers;
use super::frame;
use super::function_variants;
use super::literal_defaults::{
    emit_array_literal_default_to_result, emit_assoc_array_literal_default_to_result,
    emit_boxed_bool_literal_to_result, emit_boxed_float_literal_to_result,
    emit_boxed_int_literal_to_result, emit_boxed_null_literal_to_result,
    emit_boxed_string_literal_default_to_result, emit_empty_assoc_array_literal_to_result,
    emit_string_literal_default_to_result, emit_tagged_null_literal_to_result,
    literal_default_value, LiteralDefaultValue,
};
use super::lower_inst;
use super::lower_term;
use super::{CodegenIrError, Result};

const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Emits all supported EIR functions and then the process-entry main function.
///
/// `web` restructures the entry point: the top-level body is emitted as the
/// C-callable `_elephc_web_handler` and the real entry becomes a stub that calls
/// `elephc_web_run`. When false the normal exit-based main is emitted unchanged.
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_module(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    gc_stats: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    emit: Emit,
    regalloc_linear: bool,
    web: bool,
) -> Result<()> {
    function_variants::emit_dispatchers(module, emitter, data);
    for function in module.functions.iter().filter(|function| !is_main(function)) {
        emit_user_function(module, function, emitter, data, regalloc_linear)?;
    }
    for method in &module.class_methods {
        emit_class_method(module, method, emitter, data, regalloc_linear)?;
    }
    for closure in &module.closures {
        emit_user_function(module, closure, emitter, data, regalloc_linear)?;
    }
    emit_eir_fiber_wrappers(module, emitter);
    if matches!(emit, Emit::Cdylib) {
        return Ok(());
    }
    let main = module
        .functions
        .iter()
        .find(|function| is_main(function))
        .ok_or_else(|| CodegenIrError::invalid_module("EIR module has no main function"))?;
    emit_main_function(
        module,
        main,
        emitter,
        data,
        gc_stats,
        heap_debug,
        requires_elephc_tls,
        regalloc_linear,
        web,
    )?;
    // Generate the per-request reset routine only for `--web`, and only after the
    // handler body is emitted so every function static local (including any in the
    // main body) has been recorded into `data`. The handler prologue's
    // `bl __rt_web_reset` forward-references the label emitted here.
    if web {
        super::web::emit_web_reset(emitter, module, data);
    }
    Ok(())
}

/// Emits the static EIR Fiber wrappers needed for closure callbacks.
fn emit_eir_fiber_wrappers(module: &Module, emitter: &mut Emitter) {
    for wrapper in required_eir_fiber_wrappers(module) {
        let wrapper = DeferredFiberWrapper {
            label: wrapper.label,
            sig: wrapper.sig,
            visible_param_count: wrapper.visible_param_count,
            hidden_arg_types: wrapper.hidden_arg_types,
            retain_hidden_args_for_closure_call: false,
            use_descriptor_invoker: wrapper.use_descriptor_invoker,
        };
        emit_fiber_wrapper(emitter, &wrapper);
    }
}

/// Collects unique Fiber wrappers needed by this module.
fn required_eir_fiber_wrappers(module: &Module) -> Vec<fibers::FiberWrapper> {
    let mut wrappers = Vec::new();
    for function in all_module_functions(module) {
        for inst in &function.instructions {
            let Some(wrapper) = fibers::wrapper_for_fiber_new(module, function, inst) else {
                continue;
            };
            if wrappers
                .iter()
                .any(|existing: &fibers::FiberWrapper| existing.label == wrapper.label)
            {
                continue;
            }
            wrappers.push(wrapper);
        }
    }
    wrappers
}

/// Iterates every function-like body owned by the EIR module.
fn all_module_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Emits a non-main EIR function as a direct-call target.
fn emit_user_function(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    regalloc_linear: bool,
) -> Result<()> {
    if function.flags.is_generator {
        let entry_label = user_function_entry_symbol(function);
        return emit_generator_function(
            module,
            function,
            &entry_label,
            emitter,
            data,
            regalloc_linear,
        );
    }
    let layout = frame::layout_for_function(function, emitter.target, regalloc_linear);
    let epilogue_label = user_function_epilogue_symbol(function);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    let entry_label = user_function_entry_symbol(function);
    frame::emit_function_prologue_with_label(&mut ctx, &entry_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    Ok(())
}

/// Returns the assembly entry label for a user or synthetic EIR function.
fn user_function_entry_symbol(function: &Function) -> String {
    if is_property_init_thunk(function) {
        return function.name.clone();
    }
    function_symbol(&function.name)
}

/// Returns the epilogue label paired with `user_function_entry_symbol()`.
fn user_function_epilogue_symbol(function: &Function) -> String {
    if is_property_init_thunk(function) {
        return format!("{}_epilogue", function.name);
    }
    function_epilogue_symbol(&function.name)
}

/// Returns true for synthetic property-default init thunks referenced by runtime metadata.
fn is_property_init_thunk(function: &Function) -> bool {
    function.name.starts_with("_class_propinit_")
}

/// Emits a class method using the legacy runtime metadata symbol shape.
fn emit_class_method(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    regalloc_linear: bool,
) -> Result<()> {
    let entry_label = class_method_entry_symbol(function)?;
    if function.flags.is_generator {
        return emit_generator_function(
            module,
            function,
            &entry_label,
            emitter,
            data,
            regalloc_linear,
        );
    }
    let layout = frame::layout_for_function(function, emitter.target, regalloc_linear);
    let epilogue_label = format!("{}_epilogue", entry_label);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue_with_label(&mut ctx, &entry_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    Ok(())
}

/// Emits a generator as a stackful coroutine: a constructor at the public entry
/// label, the Mixed-returning body via the normal backend, and an arg-unboxing
/// callback that the fiber entry trampoline runs on the coroutine stack.
///
/// The constructor (`entry_label`) allocates a Fiber-shaped Generator object via
/// `__rt_fiber_construct`, stamping the Generator class id and the callback as
/// the coroutine entry wrapper, then returns the object. The body
/// (`<entry_label>__genbody`) is the generator's EIR function, which returns the
/// value passed to `return` (read back by `getReturn`). The callback
/// (`<entry_label>__gencb`) runs the body and parks its return value in the
/// generator's `gen_return_value` slot.
fn emit_generator_function(
    module: &Module,
    function: &Function,
    entry_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
    regalloc_linear: bool,
) -> Result<()> {
    let body_label = format!("{}__genbody", entry_label);
    let callback_label = format!("{}__gencb", entry_label);
    let param_types = generator_param_types(function);
    // Parameters (and closure captures, which arrive as call arguments) are boxed into the
    // coroutine's fixed `start_args` slots; there are only `FIBER_START_ARGS_MAX` of them.
    // Reject overflow with a clear diagnostic instead of silently writing past the slots into
    // adjacent fiber fields.
    let max_params = crate::codegen::runtime::FIBER_START_ARGS_MAX as usize;
    if param_types.len() > max_params {
        return Err(CodegenIrError::unsupported(format!(
            "generator '{}' has {} parameters including closure captures; \
             generators support at most {} because parameters are stored in the \
             coroutine's start-argument slots",
            function.name,
            param_types.len(),
            max_params
        )));
    }
    emit_generator_constructor(emitter, entry_label, &callback_label, &param_types);
    emit_generator_body(module, function, &body_label, emitter, data, regalloc_linear)?;
    emit_generator_callback(emitter, &callback_label, &body_label, &param_types);
    Ok(())
}

/// Returns the codegen representations of every value a generator's caller
/// forwards into the coroutine in argument registers: the visible parameters
/// plus any closure-capture parameters (closure captures are lowered as direct
/// call arguments — `call $capture data[symbol]` — so they arrive in registers
/// exactly like ordinary parameters). The constructor boxes each into the
/// generator's `start_args` slots and the callback unboxes them back into the
/// body's parameter registers.
fn generator_param_types(function: &Function) -> Vec<PhpType> {
    function
        .params
        .iter()
        .map(|param| param.php_type.codegen_repr())
        .collect()
}

/// ABI placement category for a generator parameter as it crosses the
/// box/unbox boundary between the constructor and the coroutine body.
#[derive(Clone, Copy, PartialEq)]
enum GenParamKind {
    /// One integer/pointer register (int, bool, object, array, callable, ...).
    IntLike,
    /// One floating-point register.
    Float,
    /// Two integer registers: string pointer followed by length.
    Str,
    /// An already-boxed Mixed cell pointer forwarded as-is.
    Mixed,
}

/// Classifies a parameter's codegen representation into a `GenParamKind`.
fn gen_param_kind(ty: &PhpType) -> GenParamKind {
    match ty {
        PhpType::Float => GenParamKind::Float,
        PhpType::Str => GenParamKind::Str,
        PhpType::Mixed | PhpType::Union(_) => GenParamKind::Mixed,
        _ => GenParamKind::IntLike,
    }
}

/// Frame-pointer-relative byte offset of the slot caching the saved
/// generator-object register. Frame slots use negative offsets from the frame
/// pointer (`[x29,#-off]` / `[rbp-off]`); offset 0 aliases the saved frame
/// pointer on x86_64, so the first usable slot starts at 16.
const GEN_SAVE_OFF: usize = 16;

/// Returns the frame-pointer-relative byte offset of the scratch slot for
/// generator parameter `idx`. Each parameter reserves a contiguous 16-byte cell
/// `[slot - 8, slot + 8)`: the primary value (pointer/scalar/float) sits at `slot`
/// and the string length / tagged tag at `slot - 8`, matching the secondary-word
/// convention of `emit_store_incoming_param` and the outgoing call helpers.
fn gen_param_slot(idx: usize) -> usize {
    32 + idx * 16
}

/// Computes the 16-byte-aligned frame size for the generator constructor/callback
/// scratch frame holding `n` parameter slots (plus the saved generator-object
/// slot and the 16-byte frame footer).
fn gen_arg_frame_size(n: usize) -> usize {
    let bytes = n * 16 + 48;
    (bytes + 15) & !15
}

/// Emits the generator constructor at `entry_label`.
///
/// Allocates the Generator coroutine object (reusing `__rt_fiber_construct` with
/// the Generator class id and `callback_label` as the coroutine entry wrapper),
/// boxes each caller-visible parameter into the generator's `start_args` slots
/// (so the values survive until the body lazily starts, owned by the generator),
/// then returns the object.
fn emit_generator_constructor(
    emitter: &mut Emitter,
    entry_label: &str,
    callback_label: &str,
    param_types: &[PhpType],
) {
    use crate::codegen::runtime::{FIBER_START_ARGS_OFFSET, FIBER_START_ARG_COUNT_OFFSET};
    let target = emitter.target;
    let n = param_types.len();
    let frame_size = gen_arg_frame_size(n);
    let gen_reg = abi::int_result_reg(emitter); // x0 / rax — also the construct result
    let gen_cache = match target.arch {
        Arch::AArch64 => "x19",
        Arch::X86_64 => "r12",
    };

    emitter.blank();
    emitter.comment(&format!("--- generator constructor {} ---", entry_label));
    emitter.label_global(entry_label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, gen_cache, GEN_SAVE_OFF); // preserve the caller's callee-saved generator-object cache register

    // -- spill the incoming parameters into frame slots before __rt_fiber_construct clobbers them --
    // `emit_store_incoming_param` consumes the argument registers in ABI order and falls back to the
    // caller stack once the integer/float register budget is exhausted (e.g. a 7th integer parameter
    // on x86_64 SysV), so stack-passed parameters survive instead of being dropped. The register reads
    // must happen before the construct call; caller-stack reads are frame-pointer-relative and remain
    // valid across it.
    let mut cursor = abi::IncomingArgCursor::for_target(target, 0);
    for (idx, ty) in param_types.iter().enumerate() {
        abi::emit_store_incoming_param(emitter, &format!("arg{idx}"), ty, gen_param_slot(idx), false, &mut cursor);
    }

    // -- allocate the Generator coroutine object --
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x0, #0");                                  // no callable descriptor — the wrapper runs the compiled body
            abi::emit_load_symbol_to_reg(emitter, "x1", "_generator_class_id", 0); // x1 = runtime class id of Generator
            abi::emit_symbol_address(emitter, "x2", callback_label);            // x2 = generator coroutine entry wrapper
            emitter.instruction("bl __rt_fiber_construct");                     // x0 = freshly allocated Generator coroutine object
            emitter.instruction("mov x19, x0");                                 // cache the generator object across the per-parameter boxing calls
        }
        Arch::X86_64 => {
            emitter.instruction("xor edi, edi");                                // no callable descriptor — the wrapper runs the compiled body
            abi::emit_load_symbol_to_reg(emitter, "rsi", "_generator_class_id", 0); // rsi = runtime class id of Generator
            abi::emit_symbol_address(emitter, "rdx", callback_label);          // rdx = generator coroutine entry wrapper
            emitter.instruction("call __rt_fiber_construct");                   // rax = freshly allocated Generator coroutine object
            emitter.instruction("mov r12, rax");                                // cache the generator object across the per-parameter boxing calls
        }
    }

    // -- box each spilled parameter into an owned Mixed cell stored in start_args --
    for (idx, ty) in param_types.iter().enumerate() {
        let slot = gen_param_slot(idx);
        let store_off = FIBER_START_ARGS_OFFSET as usize + idx * 8;
        match gen_param_kind(ty) {
            GenParamKind::Mixed => {
                abi::load_at_offset(emitter, gen_reg, slot);
                match target.arch {
                    Arch::AArch64 => emitter.instruction("bl __rt_incref"),     // own a reference to the forwarded Mixed cell
                    Arch::X86_64 => emitter.instruction("call __rt_incref"),    // own a reference to the forwarded Mixed cell
                }
            }
            GenParamKind::Float => {
                let freg = match target.arch {
                    Arch::AArch64 => "d0",
                    Arch::X86_64 => "xmm0",
                };
                abi::load_at_offset(emitter, freg, slot);
                crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Float);
            }
            GenParamKind::Str => match target.arch {
                Arch::AArch64 => {
                    abi::load_at_offset(emitter, "x1", slot);
                    abi::load_at_offset(emitter, "x2", slot - 8);
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Str);
                }
                Arch::X86_64 => {
                    abi::load_at_offset(emitter, "rax", slot);
                    abi::load_at_offset(emitter, "rdx", slot - 8);
                    crate::codegen::emit_box_current_value_as_mixed(emitter, &PhpType::Str);
                }
            },
            GenParamKind::IntLike => {
                abi::load_at_offset(emitter, gen_reg, slot);
                crate::codegen::emit_box_current_value_as_mixed(emitter, ty);
            }
        }
        match target.arch {
            Arch::AArch64 => emitter
                .instruction(&format!("str {}, [x19, #{}]", gen_reg, store_off)), // store the owned Mixed cell into the generator start_args slot
            Arch::X86_64 => emitter.instruction(&format!(
                "mov QWORD PTR [r12 + {}], {}",
                store_off, gen_reg
            )), // store the owned Mixed cell into the generator start_args slot
        }
    }

    // -- publish the forwarded argument count and return the Generator object --
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x9, #{}", n));                    // number of boxed start arguments forwarded to the body
            emitter.instruction(&format!("str x9, [x19, #{}]", FIBER_START_ARG_COUNT_OFFSET)); // publish the start argument count
            emitter.instruction("mov x0, x19");                                 // return the Generator object to the caller
        }
        Arch::X86_64 => {
            emitter.instruction(&format!(
                "mov QWORD PTR [r12 + {}], {}",
                FIBER_START_ARG_COUNT_OFFSET, n
            )); // publish the start argument count
            emitter.instruction("mov rax, r12");                                // return the Generator object to the caller
        }
    }
    abi::load_at_offset(emitter, gen_cache, GEN_SAVE_OFF); // restore the caller's callee-saved generator-object cache register
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}

/// Emits the generator body as a normal EIR function at `body_label`.
///
/// The body is the generator's EIR function, lowered with a Mixed return type so
/// `return $x` produces the value read back by `getReturn`. It runs on the
/// coroutine stack and reaches `__rt_gen_suspend` at each `yield`.
fn emit_generator_body(
    module: &Module,
    function: &Function,
    body_label: &str,
    emitter: &mut Emitter,
    data: &mut DataSection,
    regalloc_linear: bool,
) -> Result<()> {
    let layout = frame::layout_for_function(function, emitter.target, regalloc_linear);
    let epilogue_label = format!("{}_epilogue", body_label);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        false,
        false,
        false,
        Some(epilogue_label),
    );
    frame::emit_function_prologue_with_label(&mut ctx, body_label)?;
    emit_blocks(&mut ctx)?;
    frame::emit_function_epilogue(&mut ctx);
    Ok(())
}

/// Emits the generator coroutine entry wrapper at `callback_label`.
///
/// The fiber entry trampoline calls this wrapper on the coroutine stack with the
/// Generator object in the first argument register. The wrapper unboxes the
/// generator's `start_args` cells back into the body's parameter registers, runs
/// the body, stores the body's boxed return value into the generator's
/// `gen_return_value` slot, then returns null so the fiber transfer value does
/// not alias the parked return value.
fn emit_generator_callback(
    emitter: &mut Emitter,
    callback_label: &str,
    body_label: &str,
    param_types: &[PhpType],
) {
    use crate::codegen::runtime::generators::coro::GEN_RETURN_VALUE_OFFSET;
    use crate::codegen::runtime::FIBER_START_ARGS_OFFSET;
    let target = emitter.target;
    let n = param_types.len();
    let frame_size = gen_arg_frame_size(n);
    let gen_cache = match target.arch {
        Arch::AArch64 => "x19",
        Arch::X86_64 => "r12",
    };
    let result_reg = abi::int_result_reg(emitter); // x0 / rax
    let assignments = abi::build_outgoing_arg_assignments_for_target(target, param_types, 0);

    emitter.blank();
    emitter.comment(&format!("--- generator entry wrapper {} ---", callback_label));
    emitter.label_global(callback_label);
    abi::emit_frame_prologue(emitter, frame_size);
    abi::store_at_offset(emitter, gen_cache, GEN_SAVE_OFF); // preserve the caller's callee-saved generator-object cache register
    match target.arch {
        Arch::AArch64 => emitter.instruction("mov x19, x0"),                    // x19 = Generator object passed by __rt_fiber_entry
        Arch::X86_64 => emitter.instruction("mov r12, rdi"),                    // r12 = Generator object passed by __rt_fiber_entry
    };

    // -- unbox each start_args cell into its scratch frame slot --
    for (idx, ty) in param_types.iter().enumerate() {
        let slot = gen_param_slot(idx);
        let load_off = FIBER_START_ARGS_OFFSET as usize + idx * 8;
        match target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("ldr x0, [x19, #{}]", load_off));  // load the boxed Mixed start argument
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("mov rax, QWORD PTR [r12 + {}]", load_off)); // load the boxed Mixed start argument
            }
        }
        if gen_param_kind(ty) == GenParamKind::Mixed {
            abi::store_at_offset(emitter, result_reg, slot); // Mixed parameters keep their boxed cell pointer
            continue;
        }
        match target.arch {
            Arch::AArch64 => emitter.instruction("bl __rt_mixed_unbox"),        // x1 = primary payload, x2 = string length
            Arch::X86_64 => emitter.instruction("call __rt_mixed_unbox"),       // rdi = primary payload, rdx = string length
        }
        match (gen_param_kind(ty), target.arch) {
            (GenParamKind::Float, Arch::AArch64) => {
                emitter.instruction("fmov d0, x1");                             // reinterpret the unboxed float payload bits
                abi::store_at_offset(emitter, "d0", slot);
            }
            (GenParamKind::Float, Arch::X86_64) => {
                emitter.instruction("movq xmm0, rdi");                          // reinterpret the unboxed float payload bits
                abi::store_at_offset(emitter, "xmm0", slot);
            }
            (GenParamKind::Str, Arch::AArch64) => {
                abi::store_at_offset(emitter, "x1", slot);
                abi::store_at_offset(emitter, "x2", slot - 8);
            }
            (GenParamKind::Str, Arch::X86_64) => {
                abi::store_at_offset(emitter, "rdi", slot);
                abi::store_at_offset(emitter, "rdx", slot - 8);
            }
            (_, Arch::AArch64) => abi::store_at_offset(emitter, "x1", slot),
            (_, Arch::X86_64) => abi::store_at_offset(emitter, "rdi", slot),
        }
    }

    // -- forward the unboxed parameters to the body through the normal call ABI --
    // Push each parameter onto the temporary call stack in source order, then let
    // `materialize_outgoing_args` place them into argument registers and the caller-stack
    // overflow area exactly as an ordinary call would. This routes parameters beyond the
    // ABI register budget (e.g. a 7th integer parameter on x86_64 SysV) through the stack
    // instead of dropping them.
    for (idx, ty) in param_types.iter().enumerate() {
        let slot = gen_param_slot(idx);
        match gen_param_kind(ty) {
            GenParamKind::Float => {
                let freg = match target.arch {
                    Arch::AArch64 => "d0",
                    Arch::X86_64 => "xmm0",
                };
                abi::load_at_offset(emitter, freg, slot);
                abi::emit_push_float_reg(emitter, freg);                        // stage the float parameter on the temporary call stack
            }
            GenParamKind::Str => {
                let (lo, hi) = match target.arch {
                    Arch::AArch64 => ("x9", "x10"),
                    Arch::X86_64 => ("r10", "r11"),
                };
                abi::load_at_offset(emitter, lo, slot);
                abi::load_at_offset(emitter, hi, slot - 8);
                abi::emit_push_reg_pair(emitter, lo, hi);                       // stage the string pointer/length pair on the temporary call stack
            }
            GenParamKind::IntLike | GenParamKind::Mixed => {
                let reg = match target.arch {
                    Arch::AArch64 => "x9",
                    Arch::X86_64 => "r10",
                };
                abi::load_at_offset(emitter, reg, slot);
                abi::emit_push_reg(emitter, reg);                               // stage the scalar/pointer parameter on the temporary call stack
            }
        }
    }
    let overflow_bytes = abi::materialize_outgoing_args(emitter, &assignments);
    // ARM64 keeps a 16-byte nested-call save slot below the outgoing stack arguments;
    // x86_64 needs no such pad because the call instruction itself pushes the return address.
    let call_pad = if overflow_bytes > 0 && target.arch == Arch::AArch64 {
        16
    } else {
        0
    };
    abi::emit_reserve_temporary_stack(emitter, call_pad);

    // -- run the body, park its return value, and yield a null fiber transfer value --
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("bl {}", body_label));                 // run the generator body to completion; x0 = boxed return value
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("call {}", body_label));               // run the generator body to completion; rax = boxed return value
        }
    }
    abi::emit_release_temporary_stack(emitter, call_pad); // drop the ARM64 nested-call alignment pad
    abi::emit_release_temporary_stack(emitter, overflow_bytes); // drop any stack-passed parameters after the body returns
    match target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("str x0, [x19, #{}]", GEN_RETURN_VALUE_OFFSET)); // park the body return value for getReturn()
            emitter.instruction("mov x0, #0");                                  // hand the fiber transfer value a null so it does not alias the return
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov QWORD PTR [r12 + {}], rax", GEN_RETURN_VALUE_OFFSET)); // park the body return value for getReturn()
            emitter.instruction("xor eax, eax");                                // hand the fiber transfer value a null so it does not alias the return
        }
    }
    abi::load_at_offset(emitter, gen_cache, GEN_SAVE_OFF); // restore the caller's callee-saved generator-object cache register
    abi::emit_frame_restore(emitter, frame_size);
    abi::emit_return(emitter);
}


/// Returns the runtime metadata entry label for an EIR class-method function.
fn class_method_entry_symbol(function: &Function) -> Result<String> {
    let Some((class_name, method_name)) = function.name.rsplit_once("::") else {
        return Err(CodegenIrError::invalid_module(format!(
            "class method function '{}' has no class receiver",
            function.name
        )));
    };
    let method_key = php_symbol_key(method_name);
    if function.flags.is_static {
        Ok(static_method_symbol(class_name, &method_key))
    } else {
        Ok(method_symbol(class_name, &method_key))
    }
}

/// Emits the EIR main function as the process entry point.
///
/// When `web` is false this is the normal exit-based process entry. When `web`
/// is true the top-level body is emitted as the C-callable `_elephc_web_handler`
/// (a `ret`-based function that runs the body and returns without exiting), and
/// a separate process-entry stub is emitted that calls `elephc_web_run` with
/// argc/argv and the handler address, then exits with the bridge return value.
#[allow(clippy::too_many_arguments)]
fn emit_main_function(
    module: &Module,
    function: &Function,
    emitter: &mut Emitter,
    data: &mut DataSection,
    gc_stats: bool,
    heap_debug: bool,
    requires_elephc_tls: bool,
    regalloc_linear: bool,
    web: bool,
) -> Result<()> {
    let layout = frame::layout_for_function(function, emitter.target, regalloc_linear);
    let mut ctx = FunctionContext::new(
        module,
        function,
        emitter,
        data,
        layout,
        true,
        gc_stats,
        heap_debug,
        None,
    );
    if web {
        ctx.web = true;
        frame::emit_web_handler_prologue(&mut ctx);
    } else {
        frame::emit_main_prologue(&mut ctx);
    }
    if requires_elephc_tls {
        crate::codegen::builtins::publish_tls_function_pointers(ctx.emitter);
    }
    emit_enum_singleton_initializers(&mut ctx);
    emit_static_property_initializers(&mut ctx)?;
    emit_blocks(&mut ctx)?;
    if !ctx.epilogue_emitted {
        if web {
            frame::emit_web_handler_epilogue(&mut ctx);
        } else {
            frame::emit_main_epilogue(&mut ctx);
        }
    }
    if web {
        frame::emit_web_entry_stub(&mut ctx);
    }
    Ok(())
}

/// Returns true when a function is the process entry function.
fn is_main(function: &Function) -> bool {
    function.flags.is_main || function.name == "main"
}

/// Emits global singleton objects for enum cases used by EIR user code.
fn emit_enum_singleton_initializers(ctx: &mut FunctionContext<'_>) {
    let allowed_class_names = super::runtime_referenced_class_names(ctx.module);
    let mut sorted_enums = ctx.module.enum_infos.iter().collect::<Vec<_>>();
    sorted_enums.sort_by_key(|(name, _)| name.as_str());
    for (enum_name, enum_info) in sorted_enums {
        if !allowed_class_names.contains(enum_name) {
            continue;
        }
        let Some(class_info) = ctx.module.class_infos.get(enum_name) else {
            continue;
        };
        // The `name` property slot is authoritative in the class metadata; fall back to the last
        // property slot (`8 + (count - 1) * 16`) only if it is somehow absent.
        let name_offset = class_info
            .property_offsets
            .get("name")
            .copied()
            .unwrap_or_else(|| 8 + class_info.properties.len().saturating_sub(1) * 16);
        for case in &enum_info.cases {
            emit_enum_singleton_initializer(
                ctx,
                enum_name,
                class_info.class_id,
                class_info.properties.len(),
                name_offset,
                case,
            );
        }
    }
}

/// Emits one enum case singleton allocation and publishes it to its global slot.
fn emit_enum_singleton_initializer(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    class_id: u64,
    property_count: usize,
    name_offset: usize,
    case: &EnumCaseInfo,
) {
    ctx.emitter.comment(&format!("initialize enum singleton {}::{}", enum_name, case.name));
    emit_enum_object_allocation(ctx, class_id, property_count);
    if let Some(case_value) = &case.value {
        emit_enum_backing_value(ctx, case_value);
    }
    emit_enum_name_property(ctx, &case.name, name_offset);
    let symbol = enum_case_symbol(enum_name, &case.name);
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), &symbol, 0);
}

/// Writes an enum case's `name` string (the case identifier) into its singleton name slot.
///
/// The name is interned as a static data-section string, so the pointer/length pair stored here
/// mirrors a string-backed enum's `value` slot and needs no refcount management.
fn emit_enum_name_property(ctx: &mut FunctionContext<'_>, case_name: &str, offset: usize) {
    let object_reg = abi::int_result_reg(ctx.emitter);
    let temp_reg = abi::temp_int_reg(ctx.emitter.target);
    let (label, len) = ctx.data.add_string(case_name.as_bytes());
    abi::emit_symbol_address(ctx.emitter, temp_reg, &label);
    abi::emit_store_to_address(ctx.emitter, temp_reg, object_reg, offset);
    abi::emit_load_int_immediate(ctx.emitter, temp_reg, len as i64);
    abi::emit_store_to_address(ctx.emitter, temp_reg, object_reg, offset + 8);
}

/// Allocates an object-shaped enum singleton and zeroes its property storage.
fn emit_enum_object_allocation(ctx: &mut FunctionContext<'_>, class_id: u64, property_count: usize) {
    let payload_size = 8 + property_count * 16;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x0, #{}", payload_size));     // request enum singleton object payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #4");                              // heap kind 4 marks enum singletons as object instances
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the heap header before the enum singleton payload
            ctx.emitter.instruction(&format!("mov x10, #{}", class_id));        // materialize the enum class id
            ctx.emitter.instruction("str x10, [x0]");                           // store the enum class id at payload offset zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rax, {}", payload_size));     // request enum singleton object payload storage
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", (X86_64_HEAP_MAGIC_HI32 << 32) | 4)); // materialize the x86_64 object heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the heap header before the enum singleton payload
            ctx.emitter.instruction(&format!("mov r10, {}", class_id));         // materialize the enum class id
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the enum class id at payload offset zero
        }
    }
    let object_reg = abi::int_result_reg(ctx.emitter);
    for index in 0..property_count {
        let offset = 8 + index * 16;
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset);
        abi::emit_store_zero_to_address(ctx.emitter, object_reg, offset + 8);
    }
}

/// Writes a backed enum case value into the singleton's first property slot.
fn emit_enum_backing_value(ctx: &mut FunctionContext<'_>, case_value: &EnumCaseValue) {
    let object_reg = abi::int_result_reg(ctx.emitter);
    let temp_reg = abi::temp_int_reg(ctx.emitter.target);
    match case_value {
        EnumCaseValue::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, temp_reg, *value);
            abi::emit_store_to_address(ctx.emitter, temp_reg, object_reg, 8);
            abi::emit_store_zero_to_address(ctx.emitter, object_reg, 16);
        }
        EnumCaseValue::Str(value) => {
            let bytes = crate::string_bytes::literal_bytes(value);
            let (label, len) = ctx.data.add_string(&bytes);
            abi::emit_symbol_address(ctx.emitter, temp_reg, &label);
            abi::emit_store_to_address(ctx.emitter, temp_reg, object_reg, 8);
            abi::emit_load_int_immediate(ctx.emitter, temp_reg, len as i64);
            abi::emit_store_to_address(ctx.emitter, temp_reg, object_reg, 16);
        }
    }
}

/// Initializes static-property storage before user code runs.
fn emit_static_property_initializers(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let mut default_initializers = Vec::new();
    let mut uninitialized_static_properties = Vec::new();
    let mut class_names = super::runtime_referenced_class_names(ctx.module)
        .into_iter()
        .collect::<Vec<_>>();
    class_names.sort();
    for class_name in class_names {
        let Some(class_info) = ctx.module.class_infos.get(&class_name) else {
            continue;
        };
        for (index, (property, php_type)) in class_info.static_properties.iter().enumerate() {
            let declaring_class = class_info
                .static_property_declaring_classes
                .get(property)
                .map(String::as_str)
                .unwrap_or(class_name.as_str());
            if declaring_class != class_name {
                continue;
            }
            let default = class_info.static_defaults.get(index).and_then(Option::as_ref);
            if let Some(default_expr) = default {
                default_initializers.push((
                    class_name.clone(),
                    property.clone(),
                    php_type.clone(),
                    default_expr.kind.clone(),
                ));
            } else if class_info.declared_static_properties.contains(property) {
                uninitialized_static_properties.push((class_name.clone(), property.clone()));
            }
        }
    }
    for (class_name, property) in uninitialized_static_properties {
        emit_static_property_sentinel(ctx, &class_name, &property);
    }
    for (class_name, property, php_type, expr) in default_initializers {
        emit_static_property_default(ctx, &class_name, &property, &php_type, &expr)?;
    }
    Ok(())
}

/// Marks one typed static property without a default as uninitialized.
fn emit_static_property_sentinel(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
) {
    ctx.emitter.comment(&format!(
        "mark static property {}::${} uninitialized",
        class_name, property
    ));
    let marker_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_reg,
        UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
    );
    let symbol = static_property_symbol(class_name, property);
    abi::emit_store_reg_to_symbol(ctx.emitter, marker_reg, &symbol, 8);
}

/// Writes a supported literal static-property default into its symbol storage.
fn emit_static_property_default(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    php_type: &PhpType,
    expr: &ExprKind,
) -> Result<()> {
    ensure_static_property_default_type_supported(class_name, property, php_type)?;
    let value = literal_default_value(
        &format!("static property {}::${}", class_name, property),
        php_type,
        expr,
        "static property initializer",
    )?;
    ctx.emitter.comment(&format!(
        "initialize static property {}::${}",
        class_name, property
    ));
    emit_static_property_default_value(ctx, class_name, property, php_type, &value)?;
    Ok(())
}

/// Verifies the EIR static-property initializer has a direct storage representation.
fn ensure_static_property_default_type_supported(
    class_name: &str,
    property: &str,
    php_type: &PhpType,
) -> Result<()> {
    match php_type {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Void
        | PhpType::Never
        | PhpType::Mixed
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Union(_) => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!(
            "static property initializer for {}::${} with PHP type {:?}",
            class_name, property, php_type
        ))),
    }
}

/// Emits the target-specific literal load and symbol store for one static-property default.
fn emit_static_property_default_value(
    ctx: &mut FunctionContext<'_>,
    class_name: &str,
    property: &str,
    php_type: &PhpType,
    value: &LiteralDefaultValue,
) -> Result<()> {
    match value {
        LiteralDefaultValue::Int(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, *value);
        }
        LiteralDefaultValue::Bool(value) => {
            let int_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, int_reg, i64::from(*value));
        }
        LiteralDefaultValue::Float(value) => {
            let label = ctx.data.add_float(*value);
            let float_reg = abi::float_result_reg(ctx.emitter);
            abi::emit_load_symbol_to_reg(ctx.emitter, float_reg, &label, 0);
        }
        LiteralDefaultValue::Str(value) => {
            emit_string_literal_default_to_result(ctx, value);
        }
        LiteralDefaultValue::Null => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        LiteralDefaultValue::NullSentinel => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
        LiteralDefaultValue::TaggedNull => {
            emit_tagged_null_literal_to_result(ctx);
        }
        LiteralDefaultValue::BoxedNull => {
            emit_boxed_null_literal_to_result(ctx);
        }
        LiteralDefaultValue::BoxedStr(value) => {
            emit_boxed_string_literal_default_to_result(ctx, value);
        }
        LiteralDefaultValue::BoxedInt(value) => {
            emit_boxed_int_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::BoxedBool(value) => {
            emit_boxed_bool_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::BoxedFloat(value) => {
            emit_boxed_float_literal_to_result(ctx, *value);
        }
        LiteralDefaultValue::Array {
            elem_type,
            elements,
        } => {
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
        }
        LiteralDefaultValue::AssocArray {
            value_type,
            entries,
        } => {
            emit_assoc_array_literal_default_to_result(ctx, value_type, entries)?;
        }
        LiteralDefaultValue::EmptyAssocArray { value_type } => {
            emit_empty_assoc_array_literal_to_result(ctx, value_type);
        }
    }
    let symbol = static_property_symbol(class_name, property);
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, php_type, false);
    if !matches!(php_type.codegen_repr(), PhpType::Str | PhpType::TaggedScalar) {
        abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 8);
    }
    Ok(())
}

/// Emits every block in table order.
fn emit_blocks(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let blocks = ctx.function.blocks.clone();
    for block in blocks {
        emit_block(ctx, &block)?;
    }
    Ok(())
}

/// Emits one EIR basic block.
fn emit_block(ctx: &mut FunctionContext<'_>, block: &BasicBlock) -> Result<()> {
    ctx.emitter.label(&ctx.block_label(&block.name, block.id.as_raw()));
    for inst_id in &block.instructions {
        emit_instruction_source_marker(ctx, *inst_id)?;
        lower_inst::lower_instruction(ctx, *inst_id)?;
    }
    let terminator = block
        .terminator
        .as_ref()
        .ok_or_else(|| CodegenIrError::invalid_module(format!("block '{}' has no terminator", block.name)))?;
    lower_term::lower_terminator(ctx, terminator)
}

/// Emits the source-map marker for an EIR instruction when it carries a real PHP span.
fn emit_instruction_source_marker(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {
    let Some(inst) = ctx.function.instruction(inst_id) else {
        return Err(CodegenIrError::missing_entry("instruction", inst_id.as_raw()));
    };
    let Some(span) = inst.span else {
        return Ok(());
    };
    if span.line > 0 {
        ctx.emitter
            .comment(&format!("@src line={} col={}", span.line, span.col));
    }
    Ok(())
}
