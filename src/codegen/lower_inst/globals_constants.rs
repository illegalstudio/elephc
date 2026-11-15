//! Purpose:
//! Lowers globals, extern globals, constants, Mixed boxing, and invoker ref markers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Lowers a global storage load into the result register and SSA destination slot.
pub(super) fn lower_load_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let symbol = ir_global_symbol(name);
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("load_global missing result value"))?;
    let ty = ctx.value_php_type(result)?;
    ctx.data
        .add_comm(symbol.clone(), ty.codegen_repr().stack_size().max(8));
    abi::emit_load_symbol_to_result(ctx.emitter, &symbol, &ty);
    store_if_result(ctx, inst)
}

/// Lowers a named program-global unset: clears its cell and releases the previous owner.
pub(super) fn lower_unset_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let symbol = ir_global_symbol(&name);
    let done = ctx.next_label("unset_global_done");
    // Ordinary globals hold a boxed Mixed cell. Under `--web`, request superglobals hold a
    // Hash pointer instead, so their release must use the hash helper rather than reading
    // the hash header as a Mixed box.
    let storage_ty = if ctx.module.web && crate::superglobals::is_superglobal(&name) {
        crate::superglobals::superglobal_type().codegen_repr()
    } else {
        PhpType::Mixed
    };
    let release = abi::refcount_release_helper(&storage_ty);
    ctx.data.add_comm(symbol.clone(), 8);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // The zero store resolves the symbol through x9, so the retired owner must live in
            // a different register or it is overwritten by the address before the decref.
            abi::emit_load_symbol_to_reg(ctx.emitter, "x10", &symbol, 0);
            abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 0);
            ctx.emitter.instruction(&format!("cbz x10, {done}"));               // an unset global has no owner to release
            if let Some(release) = release {
                ctx.emitter.instruction("mov x0, x10");                         // pass the retired owner to its typed release helper
                abi::emit_call_label(ctx.emitter, release);
            }
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "rax", &symbol, 0);
            abi::emit_store_zero_to_symbol(ctx.emitter, &symbol, 0);
            ctx.emitter.instruction(&format!("test rax, rax"));                 // an unset global has no owner to release
            ctx.emitter.instruction(&format!("jz {done}"));                     // skip the decrement for an empty cell
            if let Some(release) = release {
                abi::emit_call_label(ctx.emitter, release);
            }
        }
    }
    ctx.emitter.label(&done);
    Ok(())
}

/// Lowers a global storage store from one SSA operand.
pub(super) fn lower_store_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let symbol = ir_global_symbol(&name);
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    let source_ty = ty.codegen_repr();
    if ctx.module.web && crate::superglobals::is_superglobal(&name) {
        return lower_typed_superglobal_store(ctx, &symbol, value, &source_ty);
    }
    if source_ty != PhpType::Mixed {
        // The declared type for a `?ptr<T>`, so address 0 boxes as PHP null.
        let box_ty = ctx.mixed_box_type(value)?;
        if ctx.value_can_transfer_ownership_to_consumer(value)?
            || global_store_owns_persisted_string(ctx, value, &source_ty)
        {
            emit_box_current_owned_value_as_mixed(ctx.emitter, &box_ty);
        } else {
            emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
        }
    }
    let store_ty = PhpType::Mixed;
    if source_ty == PhpType::Mixed && store_ty.codegen_repr() == PhpType::Mixed {
        retain_mixed_value_for_global_store(ctx, value)?;
    }
    ctx.data
        .add_comm(symbol.clone(), store_ty.codegen_repr().stack_size().max(8));
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &store_ty, true);
    Ok(())
}

/// Retains a Mixed cell the global store publishes when the source keeps its own reference.
///
/// A boxed local or parameter owns one reference that its scope releases later, while the
/// global slot becomes a second owner of the same cell. Publishing the pointer without an
/// increment left the global dangling once that scope ended; under heap debugging the freed
/// cell is poisoned and a later read of the global answers null. A value this store consumes
/// outright (`value_can_transfer_ownership_to_consumer`) needs no extra reference.
fn retain_mixed_value_for_global_store(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    if !ctx.value_can_transfer_ownership_to_consumer(value)? {
        abi::emit_call_label(ctx.emitter, "__rt_incref");
    }
    Ok(())
}

/// Recognizes an owned string whose sole payload owner must move into the global box.
fn global_store_owns_persisted_string(ctx: &FunctionContext<'_>, value: ValueId, ty: &PhpType) -> bool {
    if ty != &PhpType::Str { return false; }
    let Some(value_ref) = ctx.function.value(value) else { return false; };
    let ValueDef::Instruction { inst, .. } = value_ref.def else { return false; };
    let Some(producer) = ctx.function.instruction(inst) else { return false; };
    (matches!(producer.op, Op::Acquire | Op::StrPersist)
        || (producer.op == Op::RuntimeCall
            && producer.immediate == Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ExecutablePath))))
        && !ctx.function.instructions.iter().any(|inst| {
            inst.op == Op::Release && inst.operands.first().copied() == Some(value)
        })
}

/// Length of PHP's undefined-global warning prefix emitted by dynamic `$GLOBALS` reads.
const UNDEFINED_GLOBAL_PREFIX_LEN: usize = "Warning: Undefined global variable $".len();

/// Lowers the bare `$GLOBALS` materialization into the runtime hash builder.
pub(super) fn lower_load_globals_array(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    abi::emit_call_label(ctx.emitter, "__rt_globals_materialize");
    store_if_result(ctx, inst)
}

/// Lowers a `$GLOBALS[$name]` read through the program's runtime global-name table.
///
/// The lookup returns the address of the global's Mixed storage word, or zero when the name
/// was never a program global. A miss boxes PHP null and, for ordinary reads, emits PHP's
/// undefined-global warning; silent `isset()`/`??` probes set the immediate Bool to false.
pub(super) fn lower_load_global_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let name = expect_operand(inst, 0)?;
    let warn_on_missing = matches!(inst.immediate, Some(Immediate::Bool(true)));
    let miss = ctx.next_label("dyn_global_load_miss");
    let done = ctx.next_label("dyn_global_load_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(name, "x1", "x2")?;
            abi::emit_store_to_sp(ctx.emitter, "x1", 0);
            abi::emit_store_to_sp(ctx.emitter, "x2", 8);
            let hash_load = ctx.next_label("dyn_global_load_hash");
            abi::emit_call_label(ctx.emitter, "__rt_global_lookup_dynamic");
            ctx.emitter.instruction(&format!("cbz x0, {miss}"));                // no matching global: use the undefined-name path
            ctx.emitter.instruction(&format!("cmp x1, #1"));                    // did the match resolve to a typed Hash slot?
            ctx.emitter.instruction(&format!("b.eq {hash_load}"));              // yes: box a copy of the superglobal hash
            ctx.emitter.instruction("ldr x0, [x0]");                            // load the global's boxed Mixed pointer
            ctx.emitter.instruction(&format!("cbz x0, {miss}"));                // an unset global uses the same miss path
            abi::emit_call_label(ctx.emitter, "__rt_incref");                   // the read result owns one reference of its own
            ctx.emitter.instruction(&format!("b {done}"));                      // the found value is ready
            ctx.emitter.label(&hash_load);
            ctx.emitter.instruction("ldr x1, [x0]");                            // load the typed superglobal's Hash pointer
            ctx.emitter.instruction(&format!("cbz x1, {miss}"));                // an unset superglobal uses the same miss path
            ctx.emitter.instruction("mov x2, #0");                              // heap payloads use one word
            ctx.emitter.instruction("mov x0, #5");                              // runtime tag 5 = associative array
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");         // retain the hash and box it for the read
            ctx.emitter.instruction(&format!("b {done}"));                      // the boxed superglobal is ready
        }
        Arch::X86_64 => {
            let hash_load = ctx.next_label("dyn_global_load_hash");
            ctx.load_string_value_to_regs(name, "rdi", "rsi")?;
            abi::emit_store_to_sp(ctx.emitter, "rdi", 0);
            abi::emit_store_to_sp(ctx.emitter, "rsi", 8);
            abi::emit_call_label(ctx.emitter, "__rt_global_lookup_dynamic");
            ctx.emitter.instruction("test rax, rax");                           // no matching global: use the undefined-name path
            ctx.emitter.instruction(&format!("jz {miss}"));                     // no matching global: use the undefined-name path
            ctx.emitter.instruction("cmp rdx, 1");                              // did the match resolve to a typed Hash slot?
            ctx.emitter.instruction(&format!("je {hash_load}"));                // yes: box a copy of the superglobal hash
            ctx.emitter.instruction("mov rax, QWORD PTR [rax]");                // load the global's boxed Mixed pointer
            ctx.emitter.instruction("test rax, rax");                           // an unset global uses the same miss path
            ctx.emitter.instruction(&format!("jz {miss}"));                     // an unset global uses the same miss path
            abi::emit_call_label(ctx.emitter, "__rt_incref");                   // the read result owns one reference of its own
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the found value is ready
            ctx.emitter.label(&hash_load);
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax]");                // load the typed superglobal's Hash pointer
            ctx.emitter.instruction("test rdi, rdi");                           // an unset superglobal uses the same miss path
            ctx.emitter.instruction(&format!("jz {miss}"));                     // an unset superglobal uses the same miss path
            ctx.emitter.instruction("xor esi, esi");                            // heap payloads use one word
            ctx.emitter.instruction("mov eax, 5");                              // runtime tag 5 = associative array
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");         // retain the hash and box it for the read
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the boxed superglobal is ready
        }
    }
    ctx.emitter.label(&miss);
    if warn_on_missing {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                abi::emit_symbol_address(ctx.emitter, "x1", "_diag_undefined_global_prefix");
                abi::emit_load_int_immediate(ctx.emitter, "x2", UNDEFINED_GLOBAL_PREFIX_LEN as i64);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 0);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 8);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
                abi::emit_symbol_address(ctx.emitter, "x1", "_diag_undefined_array_key_suffix");
                abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            }
            Arch::X86_64 => {
                abi::emit_symbol_address(ctx.emitter, "rdi", "_diag_undefined_global_prefix");
                abi::emit_load_int_immediate(ctx.emitter, "rsi", UNDEFINED_GLOBAL_PREFIX_LEN as i64);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 0);
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 8);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
                abi::emit_symbol_address(ctx.emitter, "rdi", "_diag_undefined_array_key_suffix");
                abi::emit_load_int_immediate(ctx.emitter, "rsi", 1);
                abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
            }
        }
    }
    crate::codegen::lower_inst::objects::emit_boxed_null(ctx);
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    store_if_result(ctx, inst)
}

/// Lowers `$GLOBALS[$name] = $value` into the runtime-selected global storage word.
///
/// Known program globals are replaced in place, publishing the new box before retiring the
/// previous payload so a destructor observes the assigned value. A name absent from the static
/// table creates a runtime-global cell, so a computed write can introduce a name that exists
/// only at run time.
pub(super) fn lower_store_global_dynamic(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    let name = expect_operand(inst, 0)?;
    if matches!(inst.immediate, Some(Immediate::Bool(true))) {
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.load_string_value_to_regs(name, "x0", "x1")?;
                abi::emit_call_label(ctx.emitter, "__rt_global_unset_dynamic");
            }
            Arch::X86_64 => {
                ctx.load_string_value_to_regs(name, "rdi", "rsi")?;
                abi::emit_call_label(ctx.emitter, "__rt_global_unset_dynamic");
            }
        }
        return Ok(());
    }
    let value = expect_operand(inst, 1)?;
    let miss = ctx.next_label("dyn_global_store_miss");
    let done = ctx.next_label("dyn_global_store_done");
    abi::emit_reserve_temporary_stack(ctx.emitter, 32);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_string_value_to_regs(name, "x1", "x2")?;
            ctx.emitter.instruction("mov x0, x1");                              // create helper expects the name pointer in x0
            ctx.emitter.instruction("mov x1, x2");                              // and the name length in x1
            abi::emit_call_label(ctx.emitter, "__rt_global_lookup_or_create_dynamic");
            abi::emit_store_to_sp(ctx.emitter, "x0", 0);
            abi::emit_store_to_sp(ctx.emitter, "x1", 16);                       // 0 = Mixed cell, 1 = typed web-superglobal Hash slot
        }
        Arch::X86_64 => {
            ctx.load_string_value_to_regs(name, "rdi", "rsi")?;
            abi::emit_call_label(ctx.emitter, "__rt_global_lookup_or_create_dynamic");
            abi::emit_store_to_sp(ctx.emitter, "rax", 0);
            abi::emit_store_to_sp(ctx.emitter, "rdx", 16);                      // 0 = Mixed cell, 1 = typed web-superglobal Hash slot
        }
    }
    let ty = ctx.load_value_to_result(value)?;
    let source_ty = ty.codegen_repr();
    // Branch on the resolved storage kind while the raw value is still in its result registers: the
    // typed-superglobal path publishes a hash and must see the value, not the Mixed box below.
    let hash_store = ctx.next_label("dyn_global_store_hash");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 16);
            ctx.emitter.instruction("cmp x9, #1");                              // is the resolved storage a typed Hash slot?
            ctx.emitter.instruction(&format!("b.eq {hash_store}"));             // yes: publish a hash payload, not a Mixed cell
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 16);
            ctx.emitter.instruction("cmp r10, 1");                              // is the resolved storage a typed Hash slot?
            ctx.emitter.instruction(&format!("je {hash_store}"));               // yes: publish a hash payload, not a Mixed cell
        }
    }
    if source_ty == PhpType::Mixed {
        // The dynamic store publishes the caller's cell into program-global storage. The
        // caller still owns its reference and releases it when its scope ends, so the new
        // global owner must be counted separately.
        retain_mixed_value_for_global_store(ctx, value)?;
    } else {
        let box_ty = ctx.mixed_box_type(value)?;
        if ctx.value_can_transfer_ownership_to_consumer(value)?
            || global_store_owns_persisted_string(ctx, value, &source_ty)
        {
            emit_box_current_owned_value_as_mixed(ctx.emitter, &box_ty);
        } else {
            emit_box_current_value_as_mixed(ctx.emitter, &box_ty);
        }
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_store_to_sp(ctx.emitter, "x0", 8);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            ctx.emitter.instruction(&format!("cbz x9, {miss}"));                // does the lookup return a writable global cell?
            ctx.emitter.instruction("ldr x10, [x9]");                           // capture the previous Mixed owner
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x11", 8);
            ctx.emitter.instruction("str x11, [x9]");                           // publish the assigned value before releasing the old one
            ctx.emitter.instruction(&format!("cbz x10, {done}"));               // nothing to release when the global was unset
            ctx.emitter.instruction("mov x0, x10");                             // pass the retired Mixed cell to the decrement helper
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction(&format!("b {done}"));                      // the assignment is complete
        }
        Arch::X86_64 => {
            abi::emit_store_to_sp(ctx.emitter, "rax", 8);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            ctx.emitter.instruction(&format!("test r10, r10"));                 // does the lookup return a writable global cell?
            ctx.emitter.instruction(&format!("jz {miss}"));                     // no matching global: evaluate then discard the value
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // capture the previous Mixed owner
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
            ctx.emitter.instruction("mov QWORD PTR [r10], rax");                // publish the assigned value before releasing the old one
            ctx.emitter.instruction("test r11, r11");                           // nothing to release when the global was unset
            ctx.emitter.instruction(&format!("jz {done}"));                     // nothing to release when the global was unset
            ctx.emitter.instruction("mov rax, r11");                            // pass the retired Mixed cell in the x86_64 decref argument register
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the assignment is complete
        }
    }
    ctx.emitter.label(&hash_store);
    lower_hash_superglobal_store(ctx, value, &source_ty, &done)?;

    ctx.emitter.label(&miss);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
            ctx.emitter.instruction(&format!("cbz x0, {done}"));                // unknown names are evaluated then discarded
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
            ctx.emitter.instruction(&format!("test rax, rax"));                 // nothing to release when the value was never owned
            ctx.emitter.instruction(&format!("jz {done}"));                     // nothing to release when the value was never owned
            abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
        }
    }
    ctx.emitter.label(&done);
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    Ok(())
}

/// Publishes one array value into a web superglobal's typed Hash symbol.
///
/// The literal `$GLOBALS['_SERVER'] = $value` path reaches `StoreGlobal` directly, without the
/// dynamic table's kind marker. A Mixed value has to be unpacked exactly like the computed path;
/// otherwise the box pointer lands in a slot every static `$_SERVER` read treats as a Hash.
fn lower_typed_superglobal_store(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    value: ValueId,
    source_ty: &PhpType,
) -> Result<()> {
    use crate::codegen::lower_inst::exceptions::emit_type_error;

    let owns_value = ctx.value_can_transfer_ownership_to_consumer(value)?;
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let publish = ctx.next_label("typed_sg_publish");
            let type_error = ctx.next_label("typed_sg_type_error");
            let after = ctx.next_label("typed_sg_after");
            match source_ty {
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve the source box for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");      // x0 = tag, x1 = payload
                    let keep = ctx.next_label("typed_sg_keep");
                    let convert = ctx.next_label("typed_sg_convert");
                    ctx.emitter.instruction("cmp x0, #5");                      // tag 5 = associative hash
                    ctx.emitter.instruction(&format!("b.eq {keep}"));
                    ctx.emitter.instruction("cmp x0, #4");                      // tag 4 = indexed array
                    ctx.emitter.instruction(&format!("b.eq {convert}"));
                    ctx.emitter.instruction(&format!("b {type_error}"));
                    ctx.emitter.label(&keep);
                    ctx.emitter.instruction("mov x0, x1");                      // retain the hash for the global slot
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                    ctx.emitter.label(&convert);
                    ctx.emitter.instruction("mov x0, x1");                      // build an independent hash from the indexed array
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                PhpType::AssocArray { .. } => {
                    if !owns_value {
                        abi::emit_call_label(ctx.emitter, "__rt_incref");       // borrowed source: the slot needs its own hash reference
                    }
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve the indexed source for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");    // x0 = fresh owned hash
                    ctx.emitter.instruction("mov x2, x0");
                    if owns_value {
                        abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
                    }
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                _ => {
                    if source_ty == &PhpType::Str {
                        abi::emit_store_to_sp(ctx.emitter, "x1", 8);            // string results carry the owned payload in the length-adjacent register
                    } else {
                        abi::emit_store_to_sp(ctx.emitter, "x0", 8);            // preserve a non-array owner for the TypeError cleanup
                    }
                    ctx.emitter.instruction(&format!("b {type_error}"));
                }
            }
            ctx.emitter.label(&publish);
            if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) && owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");         // the slot owns an independent hash reference
            }
            abi::emit_load_symbol_to_reg(ctx.emitter, "x10", symbol, 0);
            abi::emit_store_reg_to_symbol(ctx.emitter, "x2", symbol, 0);
            let done = ctx.next_label("typed_sg_done");
            ctx.emitter.instruction(&format!("cbz x10, {done}"));               // nothing to release when the superglobal was unset
            ctx.emitter.instruction("mov x0, x10");
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.label(&done);
            ctx.emitter.instruction(&format!("b {after}"));
            ctx.emitter.label(&type_error);
            if owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                if source_ty == &PhpType::Str {
                    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
                } else if let Some(release) = abi::refcount_release_helper(source_ty) {
                    abi::emit_call_label(ctx.emitter, release);
                }
            }
            emit_type_error(ctx, "Cannot use a non-array value as a superglobal");
            ctx.emitter.label(&after);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
        Arch::X86_64 => {
            let publish = ctx.next_label("typed_sg_publish");
            let type_error = ctx.next_label("typed_sg_type_error");
            let after = ctx.next_label("typed_sg_after");
            match source_ty {
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve the source box for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");      // rax = tag, rdi = payload
                    let keep = ctx.next_label("typed_sg_keep");
                    let convert = ctx.next_label("typed_sg_convert");
                    ctx.emitter.instruction("cmp rax, 5");                      // tag 5 = associative hash
                    ctx.emitter.instruction(&format!("je {keep}"));
                    ctx.emitter.instruction("cmp rax, 4");                      // tag 4 = indexed array
                    ctx.emitter.instruction(&format!("je {convert}"));
                    ctx.emitter.instruction(&format!("jmp {type_error}"));
                    ctx.emitter.label(&keep);
                    ctx.emitter.instruction("mov rax, rdi");                    // retain the hash for the global slot
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                    ctx.emitter.label(&convert);
                    ctx.emitter.instruction("mov rax, rdi");                    // build an independent hash from the indexed array
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                PhpType::AssocArray { .. } => {
                    if !owns_value {
                        abi::emit_call_label(ctx.emitter, "__rt_incref");       // borrowed source: the slot needs its own hash reference
                    }
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve the indexed source for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");    // rax = fresh owned hash
                    ctx.emitter.instruction("mov rcx, rax");
                    if owns_value {
                        abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
                    }
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                _ => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve a non-array owner for the TypeError cleanup
                    ctx.emitter.instruction(&format!("jmp {type_error}"));
                }
            }
            ctx.emitter.label(&publish);
            if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) && owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");         // the slot owns an independent hash reference
            }
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", symbol, 0);
            abi::emit_store_reg_to_symbol(ctx.emitter, "rcx", symbol, 0);
            let done = ctx.next_label("typed_sg_done");
            ctx.emitter.instruction("test r11, r11");
            ctx.emitter.instruction(&format!("jz {done}"));
            ctx.emitter.instruction("mov rax, r11");
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.label(&done);
            ctx.emitter.instruction(&format!("jmp {after}"));
            ctx.emitter.label(&type_error);
            if owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                if source_ty == &PhpType::Str {
                    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
                } else if let Some(release) = abi::refcount_release_helper(source_ty) {
                    abi::emit_call_label(ctx.emitter, release);
                }
            }
            emit_type_error(ctx, "Cannot use a non-array value as a superglobal");
            ctx.emitter.label(&after);
            abi::emit_release_temporary_stack(ctx.emitter, 16);
        }
    }
    Ok(())
}

/// Publishes one array value into a typed web-superglobal Hash slot.
///
/// The dynamic global table marks a web superglobal with kind 1, because its storage word is a
/// raw `Heap(Array)` pointer instead of the boxed Mixed cell every ordinary global uses. A
/// computed write has to unpack the Mixed value, accept either array representation, take the
/// slot's own reference, and publish it with the same release-before-retire ordering as a typed
/// property store. A non-array value is rejected with a catchable TypeError; PHP would re-type
/// the superglobal, but elephc's request prelude and static accesses require hash storage.
fn lower_hash_superglobal_store(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    source_ty: &PhpType,
    done: &str,
) -> Result<()> {
    use crate::codegen::lower_inst::exceptions::emit_type_error;

    let owns_value = ctx.value_can_transfer_ownership_to_consumer(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let publish = ctx.next_label("dyn_hash_publish");
            let type_error = ctx.next_label("dyn_hash_type_error");
            match source_ty {
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve the source box for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");      // x0 = tag, x1 = payload
                    let keep = ctx.next_label("dyn_hash_keep");
                    let convert = ctx.next_label("dyn_hash_convert");
                    ctx.emitter.instruction("cmp x0, #5");                      // tag 5 = associative hash
                    ctx.emitter.instruction(&format!("b.eq {keep}"));
                    ctx.emitter.instruction("cmp x0, #4");                      // tag 4 = indexed array
                    ctx.emitter.instruction(&format!("b.eq {convert}"));
                    ctx.emitter.instruction(&format!("b {type_error}"));
                    ctx.emitter.label(&keep);
                    ctx.emitter.instruction("mov x0, x1");                      // retain the hash for the global slot
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                    ctx.emitter.label(&convert);
                    ctx.emitter.instruction("mov x0, x1");                      // build an independent hash from the indexed array
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                PhpType::AssocArray { .. } => {
                    if !owns_value {
                        abi::emit_call_label(ctx.emitter, "__rt_incref");       // borrowed source: the slot needs its own hash reference
                    }
                    ctx.emitter.instruction("mov x2, x0");
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve the indexed source for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");    // x0 = fresh owned hash
                    ctx.emitter.instruction("mov x2, x0");
                    if owns_value {
                        abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
                    }
                    ctx.emitter.instruction(&format!("b {publish}"));
                }
                _ => {
                    abi::emit_store_to_sp(ctx.emitter, "x0", 8);                // preserve a non-array owner for the TypeError cleanup
                    ctx.emitter.instruction(&format!("b {type_error}"));
                }
            }
            ctx.emitter.label(&publish);
            if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) && owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");         // the slot owns an independent hash reference
            }
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x9", 0);
            ctx.emitter.instruction(&format!("cbz x9, {done}"));
            ctx.emitter.instruction("ldr x10, [x9]");                           // capture the previous hash owner
            ctx.emitter.instruction("str x2, [x9]");                            // publish the assigned hash before retiring the old one
            ctx.emitter.instruction(&format!("cbz x10, {done}"));               // nothing to release when the superglobal was unset
            ctx.emitter.instruction("mov x0, x10");
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.instruction(&format!("b {done}"));
            ctx.emitter.label(&type_error);
            if owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 8);
                if source_ty == &PhpType::Str {
                    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
                } else if let Some(release) = abi::refcount_release_helper(source_ty) {
                    abi::emit_call_label(ctx.emitter, release);
                }
            }
            emit_type_error(ctx, "Cannot use a non-array value as a superglobal");
            ctx.emitter.instruction(&format!("b {done}"));                      // unreachable: the TypeError unwind leaves the helper
        }
        Arch::X86_64 => {
            let publish = ctx.next_label("dyn_hash_publish");
            let type_error = ctx.next_label("dyn_hash_type_error");
            match source_ty {
                PhpType::Mixed | PhpType::Union(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve the source box for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");      // rax = tag, rdi = payload
                    let keep = ctx.next_label("dyn_hash_keep");
                    let convert = ctx.next_label("dyn_hash_convert");
                    ctx.emitter.instruction("cmp rax, 5");                      // tag 5 = associative hash
                    ctx.emitter.instruction(&format!("je {keep}"));
                    ctx.emitter.instruction("cmp rax, 4");                      // tag 4 = indexed array
                    ctx.emitter.instruction(&format!("je {convert}"));
                    ctx.emitter.instruction(&format!("jmp {type_error}"));
                    ctx.emitter.label(&keep);
                    ctx.emitter.instruction("mov rax, rdi");                    // retain the hash for the global slot
                    abi::emit_call_label(ctx.emitter, "__rt_incref");
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                    ctx.emitter.label(&convert);
                    ctx.emitter.instruction("mov rax, rdi");                    // build an independent hash from the indexed array
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                PhpType::AssocArray { .. } => {
                    if !owns_value {
                        abi::emit_call_label(ctx.emitter, "__rt_incref");       // borrowed source: the slot needs its own hash reference
                    }
                    ctx.emitter.instruction("mov rcx, rax");
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                PhpType::Array(_) => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve the indexed source for owner cleanup
                    abi::emit_call_label(ctx.emitter, "__rt_array_to_hash");    // rax = fresh owned hash
                    ctx.emitter.instruction("mov rcx, rax");
                    if owns_value {
                        abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                        abi::emit_call_label(ctx.emitter, "__rt_decref_array");
                    }
                    ctx.emitter.instruction(&format!("jmp {publish}"));
                }
                _ => {
                    abi::emit_store_to_sp(ctx.emitter, "rax", 8);               // preserve a non-array owner for the TypeError cleanup
                    ctx.emitter.instruction(&format!("jmp {type_error}"));
                }
            }
            ctx.emitter.label(&publish);
            if matches!(source_ty, PhpType::Mixed | PhpType::Union(_)) && owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");         // the slot owns an independent hash reference
            }
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r10", 0);
            ctx.emitter.instruction(&format!("test r10, r10"));
            ctx.emitter.instruction(&format!("jz {done}"));
            ctx.emitter.instruction("mov r11, QWORD PTR [r10]");                // capture the previous hash owner
            ctx.emitter.instruction("mov QWORD PTR [r10], rcx");                // publish the assigned hash before retiring the old one
            ctx.emitter.instruction(&format!("test r11, r11"));
            ctx.emitter.instruction(&format!("jz {done}"));
            ctx.emitter.instruction("mov rax, r11");
            abi::emit_call_label(ctx.emitter, "__rt_decref_hash");
            ctx.emitter.instruction(&format!("jmp {done}"));
            ctx.emitter.label(&type_error);
            if owns_value {
                abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", 8);
                if source_ty == &PhpType::Str {
                    abi::emit_call_label(ctx.emitter, "__rt_heap_free_safe");
                } else if let Some(release) = abi::refcount_release_helper(source_ty) {
                    abi::emit_call_label(ctx.emitter, release);
                }
            }
            emit_type_error(ctx, "Cannot use a non-array value as a superglobal");
            ctx.emitter.instruction(&format!("jmp {done}"));                    // unreachable: the TypeError unwind leaves the helper
        }
    }
    Ok(())
}

/// Lowers a C extern global load into the EIR result slot.
pub(super) fn lower_extern_global_load(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let result = inst
        .result
        .ok_or_else(|| CodegenIrError::invalid_module("extern_global_load missing result value"))?;
    let ty = ctx.value_php_type(result)?;
    let symbol = ctx.emitter.target.extern_symbol(name);
    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Float => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Str => {
            abi::emit_load_extern_symbol_to_reg(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
            abi::emit_call_label(ctx.emitter, "__rt_cstr_to_str");
        }
        other => {
            ctx.emitter.comment(&format!(
                "WARNING: unsupported extern global load for ${} with PHP type {:?}",
                name, other
            ));
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers a C extern global store from one SSA operand.
pub(super) fn lower_extern_global_store(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?.to_string();
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?.codegen_repr();
    let symbol = ctx.emitter.target.extern_symbol(&name);
    match ty {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Callable => {
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Float => {
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::float_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_to_cstr");
            abi::emit_store_reg_to_extern_symbol(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                &symbol,
                0,
            );
        }
        other => {
            ctx.emitter.comment(&format!(
                "WARNING: unsupported extern global store for ${} with PHP type {:?}",
                name, other
            ));
        }
    }
    Ok(())
}

/// Lowers an integer constant into the canonical integer result register and slot.
pub(super) fn lower_const_i64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_i64(inst)?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a boolean constant into the canonical integer result register and slot.
pub(super) fn lower_const_bool(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = i64::from(expect_bool(inst)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a null constant to the selected one-word or tagged-scalar representation.
pub(super) fn lower_const_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if inst.result_php_type.codegen_repr() == PhpType::TaggedScalar {
        crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_result_reg(ctx.emitter),
            0x7fff_ffff_ffff_fffe,
        );
    }
    store_if_result(ctx, inst)
}

/// Lowers explicit Mixed boxing for scalar, string, object, and existing Mixed operands.
pub(super) fn lower_mixed_box(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(cell) = inst.result.and_then(|value| ctx.numeric_cell_offsets.get(&value).copied()) {
        return super::arithmetic::stack_cell::lower_box(ctx, inst, cell);
    }
    let value = expect_operand(inst, 0)?;
    let source_ty = ctx.load_value_to_result(value)?;
    let raw_source_ty = ctx.raw_value_php_type(value)?;
    let box_ty = mixed_box_payload_type(source_ty, raw_source_ty);
    box_loaded_value_as_mixed(ctx, value, &box_ty);
    store_if_result(ctx, inst)
}

/// Produces an owned Mixed cell whose storage is independent from the assignment source.
///
/// Lowering initially inserts `MixedClone` for a boxed result, but optimization may narrow its
/// producer to a scalar constant. The final SSA storage type is authoritative: clone an existing
/// Mixed cell, otherwise box the current scalar, string, heap payload, or tagged nullable value.
pub(super) fn lower_mixed_clone(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    // A proven numeric clone owns a frame cell; it copies the tag and payload there.
    if let Some(cell) = inst.result.and_then(|value| ctx.numeric_cell_offsets.get(&value).copied()) {
        return super::arithmetic::stack_cell::lower_clone(ctx, inst, cell);
    }
    let value = expect_operand(inst, 0)?;
    let source_ir_type = ctx.value_ir_type(value)?;
    if matches!(
        source_ir_type,
        crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed | crate::ir::IrHeapKind::Union)
    ) {
        load_value_to_first_int_arg(ctx, value)?;
        abi::emit_call_label(ctx.emitter, "__rt_mixed_clone");
    } else {
        let source_ty = ctx.load_value_to_result(value)?;
        let raw_source_ty = ctx.raw_value_php_type(value)?;
        let box_ty = mixed_box_payload_type(source_ty, raw_source_ty);
        box_loaded_value_as_mixed(ctx, value, &box_ty);
    }
    store_if_result(ctx, inst)
}

/// Boxes a loaded value while keeping borrowed string-offset bytes outside managed heap headers.
fn box_loaded_value_as_mixed(
    ctx: &mut FunctionContext<'_>,
    value: crate::ir::ValueId,
    box_ty: &PhpType,
) {
    let staged = if box_ty.codegen_repr() == PhpType::Str {
        let (pointer_reg, length_reg) = abi::string_result_regs(ctx.emitter);
        super::ownership::stage_string_byte_view(ctx, value, pointer_reg, length_reg)
    } else {
        false
    };
    emit_box_current_value_as_mixed(ctx.emitter, box_ty);
    super::ownership::release_string_byte_stage(ctx, staged);
}

/// Selects the runtime boxing strategy without losing erased array-layout metadata.
fn mixed_box_payload_type(source_ty: PhpType, raw_source_ty: PhpType) -> PhpType {
    if matches!(
        &source_ty,
        PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed
    ) {
        // `Array<Mixed>` is the raw ABI used for PHP arrays whose runtime storage may already
        // be associative. Reuse the iterable boxer so the owned Mixed cell records tag 4 or 5
        // from the payload's heap kind instead of stamping every value as indexed storage.
        PhpType::Iterable
    } else if matches!(raw_source_ty, PhpType::Resource(_))
        || crate::types::is_nullable_pointer_type(&raw_source_ty)
    {
        // A resource keeps its tag, and a `?ptr<T>` its null member (address 0 boxes as null).
        raw_source_ty
    } else {
        source_ty
    }
}

/// Lowers an invoker-only by-reference argument marker for descriptor calls.
pub(super) fn lower_invoker_ref_arg(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let source_ty = ctx.local_php_type(slot)?.codegen_repr();
    let ref_cell_reg = abi::secondary_scratch_reg(ctx.emitter);
    let marker_tag_reg = abi::tertiary_scratch_reg(ctx.emitter);
    let source_tag_reg = abi::symbol_scratch_reg(ctx.emitter);
    ctx.materialize_local_storage_address(slot, ref_cell_reg)?;
    abi::emit_load_int_immediate(
        ctx.emitter,
        marker_tag_reg,
        callable_invoker_args::INVOKER_ARG_REF_CELL_TAG,
    );
    abi::emit_load_int_immediate(
        ctx.emitter,
        source_tag_reg,
        if matches!(source_ty, PhpType::TaggedScalar) {
            callable_invoker_args::INVOKER_ARG_TAGGED_SCALAR_SOURCE
        } else { crate::codegen::runtime_value_tag(&source_ty) as i64 },
    );
    ctx.emitter.comment("cufa_invoker_ref_cell");
    emit_box_runtime_payload_as_mixed(ctx.emitter, marker_tag_reg, ref_cell_reg, source_tag_reg);
    store_if_result(ctx, inst)
}
