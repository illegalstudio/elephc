use super::emit::Emitter;
use crate::types::PhpType;

const MAX_INT_ARG_REGS: usize = 8;
const MAX_FLOAT_ARG_REGS: usize = 8;
const CALLER_STACK_START_OFFSET: usize = 32;
const STACK_ARG_SENTINEL: usize = usize::MAX;

#[derive(Debug, Clone, Copy)]
pub struct IncomingArgCursor {
    int_reg_idx: usize,
    float_reg_idx: usize,
    caller_stack_offset: usize,
    int_stack_only: bool,
    float_stack_only: bool,
}

impl IncomingArgCursor {
    pub fn new(initial_int_reg_idx: usize) -> Self {
        Self {
            int_reg_idx: initial_int_reg_idx,
            float_reg_idx: 0,
            caller_stack_offset: CALLER_STACK_START_OFFSET,
            int_stack_only: initial_int_reg_idx >= MAX_INT_ARG_REGS,
            float_stack_only: false,
        }
    }
}

impl Default for IncomingArgCursor {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutgoingArgAssignment {
    pub ty: PhpType,
    pub start_reg: usize,
    pub is_float: bool,
}

impl OutgoingArgAssignment {
    fn in_register(&self) -> bool {
        self.start_reg != STACK_ARG_SENTINEL
    }
}

pub fn emit_frame_prologue(emitter: &mut Emitter, frame_size: usize) {
    emitter.target.ensure_aarch64_backend("function frame setup");
    emitter.comment("prologue");
    emitter.instruction(&format!("sub sp, sp, #{}", frame_size));               // allocate stack space for locals and saved frame state
    if frame_size - 16 <= 504 {
        emitter.instruction(&format!("stp x29, x30, [sp, #{}]", frame_size - 16)); // save frame pointer and return address in the fixed frame footer
    } else {
        emitter.instruction(&format!("add x9, sp, #{}", frame_size - 16));      // compute the address of the saved frame footer for a large frame
        emitter.instruction("stp x29, x30, [x9]");                              // save frame pointer and return address through the computed footer pointer
    }
    emitter.instruction(&format!("add x29, sp, #{}", frame_size - 16));         // establish the new frame pointer for local addressing
}

pub fn emit_frame_restore(emitter: &mut Emitter, frame_size: usize) {
    emitter.target.ensure_aarch64_backend("function frame teardown");
    if frame_size - 16 <= 504 {
        emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", frame_size - 16)); // restore frame pointer and return address from the fixed frame footer
    } else {
        emitter.instruction(&format!("add x9, sp, #{}", frame_size - 16));      // recompute the saved frame footer address for a large frame
        emitter.instruction("ldp x29, x30, [x9]");                              // restore frame pointer and return address through the computed footer pointer
    }
    emitter.instruction(&format!("add sp, sp, #{}", frame_size));               // release the current stack frame and restore the caller stack pointer
}

pub fn emit_return(emitter: &mut Emitter) {
    emitter.target.ensure_aarch64_backend("function return");
    emitter.instruction("ret");                                                 // return to the caller using the restored link register
}

pub fn emit_cleanup_callback_prologue(emitter: &mut Emitter, frame_base_reg: &str) {
    emitter.target.ensure_aarch64_backend("cleanup callback frame setup");
    emitter.instruction("sub sp, sp, #16");                                     // reserve spill space for the callback's saved frame state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save the callback caller's frame pointer and return address
    emitter.instruction(&format!("mov x29, {}", frame_base_reg));               // treat the unwound frame base as the temporary frame pointer during cleanup
}

pub fn emit_cleanup_callback_epilogue(emitter: &mut Emitter) {
    emitter.target.ensure_aarch64_backend("cleanup callback frame teardown");
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore the callback caller's frame pointer and return address
    emitter.instruction("add sp, sp, #16");                                     // release the callback spill space
    emit_return(emitter);
}

/// Emit a store of `reg` at `[x29, #-offset]`, handling large offsets.
/// Uses x9 as scratch register for offsets > 255.
///
/// For offsets <= 255: single `stur` instruction (9-bit signed immediate).
/// For offsets 256-4095: `sub x9, x29, #offset` then `str reg, [x9]` (12-bit unsigned immediate).
pub fn store_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    store_at_offset_scratch(emitter, reg, offset, "x9");
}

/// Emit a store of `reg` at `[x29, #-offset]` using a custom scratch register.
///
/// For offsets <= 255: single `stur` instruction.
/// For offsets 256-4095: `sub scratch, x29, #offset` then `str reg, [scratch]`.
pub fn store_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    if offset <= 255 {
        emitter.instruction(&format!("stur {}, [x29, #-{}]", reg, offset));     // store via unscaled immediate offset
    } else {
        emitter.instruction(&format!("sub {}, x29, #{}", scratch, offset));     // compute stack address for large offset
        emitter.instruction(&format!("str {}, [{}]", reg, scratch));            // store via computed address
    }
}

/// Emit a load into `reg` from `[x29, #-offset]`, handling large offsets.
/// Uses x9 as scratch register for offsets > 255.
///
/// For offsets <= 255: single `ldur` instruction.
/// For offsets 256-4095: `sub x9, x29, #offset` then `ldr reg, [x9]`.
pub fn load_at_offset(emitter: &mut Emitter, reg: &str, offset: usize) {
    load_at_offset_scratch(emitter, reg, offset, "x9");
}

/// Emit a load into `reg` from `[x29, #-offset]` using a custom scratch register.
///
/// For offsets <= 255: single `ldur` instruction.
/// For offsets 256-4095: `sub scratch, x29, #offset` then `ldr reg, [scratch]`.
pub fn load_at_offset_scratch(emitter: &mut Emitter, reg: &str, offset: usize, scratch: &str) {
    if offset <= 255 {
        emitter.instruction(&format!("ldur {}, [x29, #-{}]", reg, offset));     // load via unscaled immediate offset
    } else {
        emitter.instruction(&format!("sub {}, x29, #{}", scratch, offset));     // compute stack address for large offset
        emitter.instruction(&format!("ldr {}, [{}]", reg, scratch));            // load via computed address
    }
}

pub fn load_from_caller_stack(emitter: &mut Emitter, reg: &str, offset: usize) {
    emitter.target.ensure_aarch64_backend("incoming caller-stack argument loads");
    if offset <= 4095 {
        emitter.instruction(&format!("ldr {}, [x29, #{}]", reg, offset));       // load a spilled incoming argument from the caller stack
    } else {
        emitter.instruction("mov x9, x29");                                     // seed a scratch pointer from the current frame base
        let mut remaining = offset;
        while remaining > 0 {
            let chunk = remaining.min(4080);
            emitter.instruction(&format!("add x9, x9, #{}", chunk));            // advance the scratch pointer toward the distant caller-stack slot
            remaining -= chunk;
        }
        emitter.instruction(&format!("ldr {}, [x9]", reg));                     // load the spilled incoming argument through the computed caller-stack pointer
    }
}

pub fn emit_store_incoming_param(
    emitter: &mut Emitter,
    name: &str,
    ty: &PhpType,
    offset: usize,
    is_ref: bool,
    cursor: &mut IncomingArgCursor,
) {
    emitter.target.ensure_aarch64_backend("incoming parameter spill");
    let ty = ty.codegen_repr();

    if is_ref {
        if !cursor.int_stack_only && cursor.int_reg_idx < MAX_INT_ARG_REGS {
            emitter.comment(&format!("param &${} from x{} (ref)", name, cursor.int_reg_idx));
            store_at_offset(emitter, &format!("x{}", cursor.int_reg_idx), offset); // save the by-reference address from the integer argument register
            cursor.int_reg_idx += 1;
        } else {
            emitter.comment(&format!(
                "param &${} from caller stack +{}",
                name,
                cursor.caller_stack_offset
            ));
            load_from_caller_stack(emitter, "x10", cursor.caller_stack_offset);
            store_at_offset(emitter, "x10", offset);                            // save the spilled by-reference address into the local param slot
            cursor.caller_stack_offset += 16;
            cursor.int_stack_only = true;
        }
        return;
    }

    match ty {
        PhpType::Bool | PhpType::Int => {
            if !cursor.int_stack_only && cursor.int_reg_idx < MAX_INT_ARG_REGS {
                emitter.comment(&format!("param ${} from x{}", name, cursor.int_reg_idx));
                store_at_offset(emitter, &format!("x{}", cursor.int_reg_idx), offset); // save the scalar parameter from the integer argument register
                cursor.int_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, "x10", cursor.caller_stack_offset);
                store_at_offset(emitter, "x10", offset);                        // save the spilled scalar parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
        PhpType::Float => {
            if !cursor.float_stack_only && cursor.float_reg_idx < MAX_FLOAT_ARG_REGS {
                emitter.comment(&format!("param ${} from d{}", name, cursor.float_reg_idx));
                store_at_offset(emitter, &format!("d{}", cursor.float_reg_idx), offset); // save the float parameter from the floating-point argument register
                cursor.float_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, "d15", cursor.caller_stack_offset);
                store_at_offset(emitter, "d15", offset);                        // save the spilled float parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.float_stack_only = true;
            }
        }
        PhpType::Str => {
            if !cursor.int_stack_only && cursor.int_reg_idx + 1 < MAX_INT_ARG_REGS {
                emitter.comment(&format!(
                    "param ${} from x{},x{}",
                    name,
                    cursor.int_reg_idx,
                    cursor.int_reg_idx + 1
                ));
                store_at_offset(emitter, &format!("x{}", cursor.int_reg_idx), offset); // save the string pointer from the integer-register pair
                store_at_offset(emitter, &format!("x{}", cursor.int_reg_idx + 1), offset - 8); // save the string length from the integer-register pair
                cursor.int_reg_idx += 2;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, "x10", cursor.caller_stack_offset);
                load_from_caller_stack(emitter, "x11", cursor.caller_stack_offset + 8);
                store_at_offset(emitter, "x10", offset);                        // save the spilled string pointer into the local param slot
                store_at_offset(emitter, "x11", offset - 8);                    // save the spilled string length into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
        PhpType::Void => {}
        PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            if !cursor.int_stack_only && cursor.int_reg_idx < MAX_INT_ARG_REGS {
                emitter.comment(&format!("param ${} from x{}", name, cursor.int_reg_idx));
                store_at_offset(emitter, &format!("x{}", cursor.int_reg_idx), offset); // save the pointer-like parameter from the integer argument register
                cursor.int_reg_idx += 1;
            } else {
                emitter.comment(&format!(
                    "param ${} from caller stack +{}",
                    name,
                    cursor.caller_stack_offset
                ));
                load_from_caller_stack(emitter, "x10", cursor.caller_stack_offset);
                store_at_offset(emitter, "x10", offset);                        // save the spilled pointer-like parameter into the local param slot
                cursor.caller_stack_offset += 16;
                cursor.int_stack_only = true;
            }
        }
    }
}

pub fn emit_preserve_return_value(emitter: &mut Emitter, return_ty: &PhpType, return_offset: usize) {
    emitter.target.ensure_aarch64_backend("return-value spill");
    match return_ty.codegen_repr() {
        PhpType::Float => {
            store_at_offset(emitter, "d0", return_offset);                      // preserve the float return value in the hidden frame slot
        }
        PhpType::Str => {
            store_at_offset(emitter, "x1", return_offset);                      // preserve the string return pointer in the hidden frame slot
            store_at_offset(emitter, "x2", return_offset - 8);                  // preserve the string return length in the hidden frame slot
        }
        _ => {
            store_at_offset(emitter, "x0", return_offset);                      // preserve the scalar or pointer-like return value in the hidden frame slot
        }
    }
}

pub fn emit_restore_return_value(emitter: &mut Emitter, return_ty: &PhpType, return_offset: usize) {
    emitter.target.ensure_aarch64_backend("return-value reload");
    match return_ty.codegen_repr() {
        PhpType::Float => {
            load_at_offset(emitter, "d0", return_offset);                       // restore the preserved float return value from the hidden frame slot
        }
        PhpType::Str => {
            load_at_offset(emitter, "x1", return_offset);                       // restore the preserved string return pointer from the hidden frame slot
            load_at_offset(emitter, "x2", return_offset - 8);                   // restore the preserved string return length from the hidden frame slot
        }
        _ => {
            load_at_offset(emitter, "x0", return_offset);                       // restore the preserved scalar or pointer-like return value from the hidden frame slot
        }
    }
}

pub fn build_outgoing_arg_assignments(
    arg_types: &[PhpType],
    initial_int_reg_idx: usize,
) -> Vec<OutgoingArgAssignment> {
    let mut assignments = Vec::new();
    let mut int_reg_idx = initial_int_reg_idx;
    let mut float_reg_idx = 0usize;
    let mut int_stack_only = initial_int_reg_idx >= MAX_INT_ARG_REGS;
    let mut float_stack_only = false;

    for ty in arg_types {
        if ty.is_float_reg() {
            if !float_stack_only && float_reg_idx < MAX_FLOAT_ARG_REGS {
                assignments.push(OutgoingArgAssignment {
                    ty: ty.clone(),
                    start_reg: float_reg_idx,
                    is_float: true,
                });
                float_reg_idx += 1;
            } else {
                assignments.push(OutgoingArgAssignment {
                    ty: ty.clone(),
                    start_reg: STACK_ARG_SENTINEL,
                    is_float: true,
                });
                float_stack_only = true;
            }
        } else {
            let reg_count = ty.register_count();
            if !int_stack_only && int_reg_idx + reg_count <= MAX_INT_ARG_REGS {
                assignments.push(OutgoingArgAssignment {
                    ty: ty.clone(),
                    start_reg: int_reg_idx,
                    is_float: false,
                });
                int_reg_idx += reg_count;
            } else {
                assignments.push(OutgoingArgAssignment {
                    ty: ty.clone(),
                    start_reg: STACK_ARG_SENTINEL,
                    is_float: false,
                });
                int_stack_only = true;
            }
        }
    }

    assignments
}

fn arg_slot_size(ty: &PhpType) -> usize {
    match ty {
        PhpType::Void => 0,
        _ => 16,
    }
}

fn emit_adjust_sp(emitter: &mut Emitter, amount: usize, subtract: bool) {
    let mut remaining = amount;
    while remaining > 0 {
        let chunk = remaining.min(4080);
        if subtract {
            emitter.instruction(&format!("sub sp, sp, #{}", chunk));            // reserve stack space for spilled outgoing call arguments
        } else {
            emitter.instruction(&format!("add sp, sp, #{}", chunk));            // release temporary outgoing call-argument stack space
        }
        remaining -= chunk;
    }
}

fn emit_sp_address(emitter: &mut Emitter, scratch: &str, offset: usize) {
    emitter.instruction(&format!("mov {}, sp", scratch));                       // seed a scratch pointer from the current stack pointer
    let mut remaining = offset;
    while remaining > 0 {
        let chunk = remaining.min(4080);
        emitter.instruction(&format!("add {}, {}, #{}", scratch, scratch, chunk)); // advance the scratch pointer toward the desired stack slot
        remaining -= chunk;
    }
}

fn emit_load_from_sp(emitter: &mut Emitter, reg: &str, offset: usize) {
    if offset == 0 {
        emitter.instruction(&format!("ldr {}, [sp]", reg));                     // load directly from the top of the temporary argument stack
    } else if offset <= 4095 {
        emitter.instruction(&format!("ldr {}, [sp, #{}]", reg, offset));        // load from a nearby temporary argument slot with an immediate offset
    } else {
        emit_sp_address(emitter, "x9", offset);
        emitter.instruction(&format!("ldr {}, [x9]", reg));                     // load from a distant temporary argument slot through a scratch address
    }
}

fn emit_store_to_sp(emitter: &mut Emitter, reg: &str, offset: usize) {
    if offset == 0 {
        emitter.instruction(&format!("str {}, [sp]", reg));                     // store directly to the top of the outgoing stack-argument area
    } else if offset <= 4095 {
        emitter.instruction(&format!("str {}, [sp, #{}]", reg, offset));        // store to a nearby outgoing stack-argument slot with an immediate offset
    } else {
        emit_sp_address(emitter, "x9", offset);
        emitter.instruction(&format!("str {}, [x9]", reg));                     // store to a distant outgoing stack-argument slot through a scratch address
    }
}

fn emit_copy_stack_arg_slot(emitter: &mut Emitter, ty: &PhpType, src_offset: usize, dst_offset: usize) {
    match ty {
        PhpType::Float => {
            emit_load_from_sp(emitter, "d15", src_offset);
            emit_store_to_sp(emitter, "d15", dst_offset);
        }
        PhpType::Str => {
            emit_load_from_sp(emitter, "x10", src_offset);
            emit_load_from_sp(emitter, "x11", src_offset + 8);
            emit_store_to_sp(emitter, "x10", dst_offset);
            emit_store_to_sp(emitter, "x11", dst_offset + 8);
        }
        PhpType::Void => {}
        _ => {
            emit_load_from_sp(emitter, "x10", src_offset);
            emit_store_to_sp(emitter, "x10", dst_offset);
        }
    }
}

pub fn materialize_outgoing_args(
    emitter: &mut Emitter,
    assignments: &[OutgoingArgAssignment],
) -> usize {
    let slot_sizes: Vec<usize> = assignments
        .iter()
        .map(|assignment| arg_slot_size(&assignment.ty))
        .collect();
    let total_temp_bytes: usize = slot_sizes.iter().sum();
    let mut temp_offsets = vec![0usize; assignments.len()];
    let mut running_offset = 0usize;
    for i in (0..assignments.len()).rev() {
        temp_offsets[i] = running_offset;
        running_offset += slot_sizes[i];
    }

    let overflow_indices: Vec<usize> = assignments
        .iter()
        .enumerate()
        .filter_map(|(idx, assignment)| (!assignment.in_register()).then_some(idx))
        .collect();
    let overflow_bytes: usize = overflow_indices.iter().map(|idx| slot_sizes[*idx]).sum();

    if overflow_bytes > 0 {
        emit_adjust_sp(emitter, overflow_bytes, true);
    }

    let base_shift = overflow_bytes;
    for (i, assignment) in assignments.iter().enumerate() {
        if !assignment.in_register() {
            continue;
        }
        let src_offset = base_shift + temp_offsets[i];
        match &assignment.ty {
            PhpType::Bool
            | PhpType::Int
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Buffer(_)
            | PhpType::Callable
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Pointer(_) => {
                emit_load_from_sp(emitter, &format!("x{}", assignment.start_reg), src_offset);
            }
            PhpType::Float => {
                emit_load_from_sp(emitter, &format!("d{}", assignment.start_reg), src_offset);
            }
            PhpType::Str => {
                emit_load_from_sp(emitter, &format!("x{}", assignment.start_reg), src_offset);
                emit_load_from_sp(emitter, &format!("x{}", assignment.start_reg + 1), src_offset + 8);
            }
            PhpType::Void => {}
        }
    }

    if overflow_bytes > 0 {
        let mut dst_offset = total_temp_bytes;
        for idx in &overflow_indices {
            let src_offset = overflow_bytes + temp_offsets[*idx];
            emit_copy_stack_arg_slot(emitter, &assignments[*idx].ty, src_offset, dst_offset);
            dst_offset += slot_sizes[*idx];
        }
    }

    if total_temp_bytes > 0 {
        emit_adjust_sp(emitter, total_temp_bytes, false);
    }

    overflow_bytes
}

/// Store the current result registers to a local variable on the stack.
///
/// ARM64 register conventions for each PHP type:
///   Int/Bool: value in x0 (64-bit general register)
///   Float:    value in d0 (64-bit FP register)
///   Str:      pointer in x1, length in x2 (two 8-byte slots)
///   Null:     sentinel value in x0
///   Array:    heap pointer in x0
pub fn emit_store(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            store_at_offset(emitter, "x0", offset);                             // store int/bool to stack
        }
        PhpType::Float => {
            store_at_offset(emitter, "d0", offset);                             // store float to stack
        }
        PhpType::Str => {
            // Persist string to heap so it survives concat_buf resets
            emitter.instruction("bl __rt_str_persist");                         // copy string to heap, x1=heap_ptr, x2=len
            // Strings use 16 bytes: pointer at offset, length at offset-8
            store_at_offset(emitter, "x1", offset);                             // store string pointer
            store_at_offset(emitter, "x2", offset - 8);                         // store string length
        }
        PhpType::Void => {
            store_at_offset(emitter, "x0", offset);                             // store null sentinel
        }
        PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            store_at_offset(emitter, "x0", offset);                             // store array/callable/object/pointer value
        }
    }
}

/// Retain the current value in x0 if it is runtime-refcounted.
pub fn emit_incref_if_refcounted(emitter: &mut Emitter, ty: &PhpType) {
    if ty.is_refcounted() {
        emitter.instruction("str x0, [sp, #-16]!");                             // preserve heap pointer across incref helper call
        emitter.instruction("bl __rt_incref");                                  // retain shared heap value before creating a new owner
        emitter.instruction("ldr x0, [sp], #16");                               // restore original heap pointer after incref
    }
}

/// Release the current value in x0 if it is runtime-refcounted.
pub fn emit_decref_if_refcounted(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("bl __rt_decref_mixed");                        // release mixed cell reference
        }
        PhpType::Array(_) => {
            emitter.instruction("bl __rt_decref_array");                        // release indexed array reference
        }
        PhpType::AssocArray { .. } => {
            emitter.instruction("bl __rt_decref_hash");                         // release associative array reference
        }
        PhpType::Object(_) => {
            emitter.instruction("bl __rt_decref_object");                       // release object reference
        }
        _ => {}
    }
}

/// Load a local variable from the stack into result registers.
///
/// Restores the value into the same registers used by emit_store.
pub fn emit_load(emitter: &mut Emitter, ty: &PhpType, offset: usize) {
    match ty {
        PhpType::Bool | PhpType::Int => {
            load_at_offset(emitter, "x0", offset);                              // load int/bool from stack
        }
        PhpType::Float => {
            load_at_offset(emitter, "d0", offset);                              // load float from stack
        }
        PhpType::Str => {
            load_at_offset(emitter, "x1", offset);                              // load string pointer
            load_at_offset(emitter, "x2", offset - 8);                          // load string length
        }
        PhpType::Void => {
            load_at_offset(emitter, "x0", offset);                              // load null sentinel
        }
        PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            load_at_offset(emitter, "x0", offset);                              // load array/callable/object/pointer value
        }
    }
}

/// Emit sys_write(stdout, ptr, len) to print a value.
///
/// Uses macOS syscall convention:
///   x0 = fd (1 = stdout), x1 = buffer pointer, x2 = buffer length
///   x16 = syscall number (4 = write), then svc #0x80 to invoke kernel.
///
/// For non-string types, converts to string first via runtime helpers.
pub fn emit_write_stdout(emitter: &mut Emitter, ty: &PhpType) {
    match ty {
        PhpType::Str => {
            // x1=ptr, x2=len already set by the expression evaluator
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
        }
        PhpType::Bool | PhpType::Int => {
            // Convert integer in x0 to decimal string, then write
            emitter.instruction("bl __rt_itoa");                                // x0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
        }
        PhpType::Float => {
            // Convert float in d0 to string via snprintf, then write
            emitter.instruction("bl __rt_ftoa");                                // d0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
        }
        PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_) => {
            // Convert pointer address in x0 to hex string, then write
            emitter.instruction("bl __rt_ptoa");                                // x0 → x1=ptr, x2=len
            emitter.instruction("mov x0, #1");                                  // fd = stdout
            emitter.syscall(4);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emitter.instruction("bl __rt_mixed_write_stdout");                  // inspect boxed mixed payload and print if scalar/string
        }
        PhpType::Void | PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Callable | PhpType::Object(_) => {} // null/array/callable/object: nothing to print
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::{Arch, Platform, Target};

    fn test_emitter() -> Emitter {
        Emitter::new(Target::new(Platform::MacOS, Arch::AArch64))
    }

    #[test]
    fn test_emit_frame_helpers_small_frame() {
        let mut emitter = test_emitter();
        emit_frame_prologue(&mut emitter, 64);
        emit_frame_restore(&mut emitter, 64);
        emit_return(&mut emitter);

        assert_eq!(
            emitter.output(),
            concat!(
                "    ; prologue\n",
                "    sub sp, sp, #64\n",
                "    stp x29, x30, [sp, #48]\n",
                "    add x29, sp, #48\n",
                "    ldp x29, x30, [sp, #48]\n",
                "    add sp, sp, #64\n",
                "    ret\n",
            )
        );
    }

    #[test]
    fn test_emit_store_incoming_param_uses_registers_then_caller_stack() {
        let mut emitter = test_emitter();
        let mut cursor = IncomingArgCursor::default();

        emit_store_incoming_param(&mut emitter, "a", &PhpType::Int, 8, false, &mut cursor);
        emit_store_incoming_param(&mut emitter, "b", &PhpType::Int, 16, false, &mut cursor);
        emit_store_incoming_param(&mut emitter, "c", &PhpType::Int, 24, false, &mut cursor);
        emit_store_incoming_param(&mut emitter, "d", &PhpType::Int, 32, false, &mut cursor);
        emit_store_incoming_param(&mut emitter, "e", &PhpType::Int, 40, false, &mut cursor);
        emit_store_incoming_param(&mut emitter, "f", &PhpType::Int, 48, false, &mut cursor);
        emit_store_incoming_param(&mut emitter, "g", &PhpType::Int, 56, false, &mut cursor);
        emit_store_incoming_param(&mut emitter, "h", &PhpType::Int, 64, false, &mut cursor);
        emit_store_incoming_param(&mut emitter, "i", &PhpType::Int, 72, false, &mut cursor);

        let out = emitter.output();
        assert!(out.contains("    ; param $h from x7\n"));
        assert!(out.contains("    ; param $i from caller stack +32\n"));
        assert!(out.contains("    ldr x10, [x29, #32]\n"));
    }

    #[test]
    fn test_emit_preserve_and_restore_return_value_for_strings() {
        let mut emitter = test_emitter();
        emit_preserve_return_value(&mut emitter, &PhpType::Str, 32);
        emit_restore_return_value(&mut emitter, &PhpType::Str, 32);

        assert_eq!(
            emitter.output(),
            concat!(
                "    stur x1, [x29, #-32]\n",
                "    stur x2, [x29, #-24]\n",
                "    ldur x1, [x29, #-32]\n",
                "    ldur x2, [x29, #-24]\n",
            )
        );
    }

    #[test]
    fn test_build_outgoing_arg_assignments_respects_register_limits() {
        let assignments = build_outgoing_arg_assignments(
            &[
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Float,
                PhpType::Float,
                PhpType::Float,
                PhpType::Float,
                PhpType::Float,
                PhpType::Float,
                PhpType::Float,
                PhpType::Float,
                PhpType::Float,
            ],
            0,
        );

        assert_eq!(assignments[0].start_reg, 0);
        assert_eq!(assignments[7].start_reg, 7);
        assert_eq!(assignments[8].start_reg, STACK_ARG_SENTINEL);
        assert!(assignments[9].is_float);
        assert_eq!(assignments[16].start_reg, 7);
        assert_eq!(assignments[17].start_reg, STACK_ARG_SENTINEL);
    }

    #[test]
    fn test_materialize_outgoing_args_keeps_overflow_on_stack() {
        let mut emitter = test_emitter();
        let assignments = build_outgoing_arg_assignments(
            &[
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
                PhpType::Int,
            ],
            0,
        );

        let overflow_bytes = materialize_outgoing_args(&mut emitter, &assignments);
        let out = emitter.output();

        assert_eq!(overflow_bytes, 16);
        assert!(out.contains("    sub sp, sp, #16\n"));
        assert!(out.contains("    ldr x0, [sp, #144]\n"));
        assert!(out.contains("    ldr x7, [sp, #32]\n"));
        assert!(out.contains("    str x10, [sp, #144]\n"));
        assert!(out.contains("    add sp, sp, #144\n"));
    }
}
