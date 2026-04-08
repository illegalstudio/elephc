use super::{emit::Emitter, platform::Arch};
use crate::types::PhpType;

const MAX_INT_ARG_REGS: usize = 8;
const MAX_FLOAT_ARG_REGS: usize = 8;
const CALLER_STACK_START_OFFSET: usize = 32;
const STACK_ARG_SENTINEL: usize = usize::MAX;

fn int_result_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x0",
        Arch::X86_64 => "rax",
    }
}

fn float_result_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "d0",
        Arch::X86_64 => "xmm0",
    }
}

fn string_result_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rax", "rdx"),
    }
}

fn frame_pointer_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x29",
        Arch::X86_64 => "rbp",
    }
}

fn symbol_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r11",
    }
}

fn secondary_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x10",
        Arch::X86_64 => "r10",
    }
}

fn tertiary_scratch_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x11",
        Arch::X86_64 => "rcx",
    }
}

fn is_float_register(reg: &str) -> bool {
    reg.starts_with('d') || reg.starts_with("xmm")
}

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
    emitter.comment("prologue");
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_adjust_sp(emitter, frame_size, true);
            let footer_offset = frame_size - 16;
            if footer_offset <= 504 {
                emitter.instruction(&format!("stp x29, x30, [sp, #{}]", footer_offset)); // save frame pointer and return address in the fixed frame footer
            } else {
                emit_sp_address(emitter, "x9", footer_offset);
                emitter.instruction("stp x29, x30, [x9]");                              // save frame pointer and return address through the computed footer pointer
            }
            if footer_offset == 0 {
                emitter.instruction("mov x29, sp");                                   // use the current stack pointer directly when the frame footer starts at sp
            } else if footer_offset <= 4095 {
                emitter.instruction(&format!("add x29, sp, #{}", footer_offset));      // point the frame pointer at the nearby fixed frame footer
            } else {
                emit_sp_address(emitter, "x29", footer_offset);
            }
        }
        Arch::X86_64 => {
            let local_bytes = frame_size.saturating_sub(16);
            emitter.instruction("push rbp");                                            // save the caller frame pointer on the stack
            emitter.instruction("mov rbp, rsp");                                        // establish the current stack pointer as the new frame base
            if local_bytes > 0 {
                emitter.instruction(&format!("sub rsp, {}", local_bytes));              // reserve aligned stack space for local slots below rbp
            }
        }
    }
}

pub fn emit_frame_restore(emitter: &mut Emitter, frame_size: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            let footer_offset = frame_size - 16;
            if footer_offset <= 504 {
                emitter.instruction(&format!("ldp x29, x30, [sp, #{}]", footer_offset)); // restore frame pointer and return address from the fixed frame footer
            } else {
                emit_sp_address(emitter, "x9", footer_offset);
                emitter.instruction("ldp x29, x30, [x9]");                              // restore frame pointer and return address through the computed footer pointer
            }
            emit_adjust_sp(emitter, frame_size, false);
        }
        Arch::X86_64 => {
            let local_bytes = frame_size.saturating_sub(16);
            if local_bytes > 0 {
                emitter.instruction(&format!("add rsp, {}", local_bytes));              // release the aligned local-slot area below rbp
            }
            emitter.instruction("pop rbp");                                             // restore the caller frame pointer from the stack
        }
    }
}

pub fn emit_return(emitter: &mut Emitter) {
    emitter.instruction("ret");                                                 // return to the caller using the platform return instruction
}

pub fn emit_cleanup_callback_prologue(emitter: &mut Emitter, frame_base_reg: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #16");                                     // reserve spill space for the callback's saved frame state
            emitter.instruction("stp x29, x30, [sp, #0]");                              // save the callback caller's frame pointer and return address
            emitter.instruction(&format!("mov x29, {}", frame_base_reg));               // treat the unwound frame base as the temporary frame pointer during cleanup
        }
        Arch::X86_64 => {
            emitter.instruction("push rbp");                                            // preserve the callback caller frame pointer before rebasing cleanup
            emitter.instruction(&format!("mov rbp, {}", frame_base_reg));               // treat the unwound frame base as the temporary cleanup frame pointer
        }
    }
}

pub fn emit_cleanup_callback_epilogue(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore the callback caller's frame pointer and return address
            emitter.instruction("add sp, sp, #16");                                     // release the callback spill space
        }
        Arch::X86_64 => {
            emitter.instruction("pop rbp");                                             // restore the callback caller frame pointer after cleanup work
        }
    }
    emit_return(emitter);
}

pub fn emit_frame_slot_address(emitter: &mut Emitter, dest: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, x29", dest));                     // copy the frame pointer when the requested slot is the frame base itself
            } else if offset <= 4095 {
                emitter.instruction(&format!("sub {}, x29, #{}", dest, offset));        // compute the local-slot address directly from the frame pointer
            } else {
                emitter.instruction(&format!("mov {}, x29", dest));                     // seed the destination register from the frame pointer for a far local-slot address
                let mut remaining = offset;
                while remaining > 0 {
                    let chunk = remaining.min(4095);
                    emitter.instruction(&format!("sub {}, {}, #{}", dest, dest, chunk)); // walk the destination register down toward the distant local-slot address
                    remaining -= chunk;
                }
            }
        }
        Arch::X86_64 => {
            if offset == 0 {
                emitter.instruction(&format!("mov {}, {}", dest, frame_pointer_reg(emitter))); // copy rbp when the requested slot is the frame base itself
            } else {
                emitter.instruction(&format!("lea {}, [{} - {}]", dest, frame_pointer_reg(emitter), offset)); // materialize the local-slot address relative to rbp
            }
        }
    }
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
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 255 {
                emitter.instruction(&format!("stur {}, [x29, #-{}]", reg, offset));     // store via unscaled immediate offset
            } else {
                emit_frame_slot_address(emitter, scratch, offset);
                emitter.instruction(&format!("str {}, [{}]", reg, scratch));            // store via computed address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} - {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd QWORD PTR {}, {}", slot, reg));     // store the floating-point payload into the local frame slot
            } else {
                emitter.instruction(&format!("mov QWORD PTR {}, {}", slot, reg));       // store the integer or pointer payload into the local frame slot
            }
        }
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
    match emitter.target.arch {
        Arch::AArch64 => {
            if offset <= 255 {
                emitter.instruction(&format!("ldur {}, [x29, #-{}]", reg, offset));     // load via unscaled immediate offset
            } else {
                emit_frame_slot_address(emitter, scratch, offset);
                emitter.instruction(&format!("ldr {}, [{}]", reg, scratch));            // load via computed address
            }
        }
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} - {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot));     // load the floating-point payload from the local frame slot
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot));       // load the integer or pointer payload from the local frame slot
            }
        }
    }
}

pub fn load_from_caller_stack(emitter: &mut Emitter, reg: &str, offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
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
        Arch::X86_64 => {
            let slot = if offset == 0 {
                format!("[{}]", frame_pointer_reg(emitter))
            } else {
                format!("[{} + {}]", frame_pointer_reg(emitter), offset)
            };
            if is_float_register(reg) {
                emitter.instruction(&format!("movsd {}, QWORD PTR {}", reg, slot));     // load a spilled floating-point argument from the caller stack area
            } else {
                emitter.instruction(&format!("mov {}, QWORD PTR {}", reg, slot));       // load a spilled integer or pointer argument from the caller stack area
            }
        }
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
    match return_ty.codegen_repr() {
        PhpType::Float => {
            store_at_offset(emitter, float_result_reg(emitter), return_offset);         // preserve the float return value in the hidden frame slot
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            store_at_offset(emitter, ptr_reg, return_offset);                           // preserve the string return pointer in the hidden frame slot
            store_at_offset(emitter, len_reg, return_offset - 8);                       // preserve the string return length in the hidden frame slot
        }
        _ => {
            store_at_offset(emitter, int_result_reg(emitter), return_offset);           // preserve the scalar or pointer-like return value in the hidden frame slot
        }
    }
}

pub fn emit_restore_return_value(emitter: &mut Emitter, return_ty: &PhpType, return_offset: usize) {
    match return_ty.codegen_repr() {
        PhpType::Float => {
            load_at_offset(emitter, float_result_reg(emitter), return_offset);          // restore the preserved float return value from the hidden frame slot
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            load_at_offset(emitter, ptr_reg, return_offset);                            // restore the preserved string return pointer from the hidden frame slot
            load_at_offset(emitter, len_reg, return_offset - 8);                        // restore the preserved string return length from the hidden frame slot
        }
        _ => {
            load_at_offset(emitter, int_result_reg(emitter), return_offset);            // restore the preserved scalar or pointer-like return value from the hidden frame slot
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
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store int/bool to stack
        }
        PhpType::Float => {
            store_at_offset(emitter, float_result_reg(emitter), offset);                // store float to stack
        }
        PhpType::Str => {
            // Persist string to heap so it survives concat_buf resets
            emitter.instruction("bl __rt_str_persist");                         // copy string to heap, x1=heap_ptr, x2=len
            // Strings use 16 bytes: pointer at offset, length at offset-8
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            store_at_offset(emitter, ptr_reg, offset);                                  // store string pointer
            store_at_offset(emitter, len_reg, offset - 8);                              // store string length
        }
        PhpType::Void => {
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store null sentinel
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
            store_at_offset(emitter, int_result_reg(emitter), offset);                  // store array/callable/object/pointer value
        }
    }
}

pub fn emit_store_local_slot_to_symbol(
    emitter: &mut Emitter,
    symbol: &str,
    ty: &PhpType,
    offset: usize,
) {
    let symbol_reg = symbol_scratch_reg(emitter);
    let local_reg = secondary_scratch_reg(emitter);
    let local_hi_reg = tertiary_scratch_reg(emitter);
    match ty.codegen_repr() {
        PhpType::Float => {
            load_at_offset_scratch(emitter, float_result_reg(emitter), offset, local_reg); // load the local float value from its frame slot
            emit_store_reg_to_symbol(emitter, float_result_reg(emitter), symbol, 0);        // store the local float value into symbol storage
        }
        PhpType::Str => {
            load_at_offset_scratch(emitter, local_reg, offset, symbol_reg);            // load the local string pointer from its frame slot
            load_at_offset_scratch(emitter, local_hi_reg, offset - 8, symbol_reg);     // load the local string length from its paired frame slot
            emit_store_reg_to_symbol(emitter, local_reg, symbol, 0);                    // store the local string pointer into symbol storage
            emit_store_reg_to_symbol(emitter, local_hi_reg, symbol, 8);                 // store the local string length into symbol storage
        }
        PhpType::Void => {}
        _ => {
            load_at_offset_scratch(emitter, local_reg, offset, symbol_reg);            // load the local scalar or pointer-like value from its frame slot
            emit_store_reg_to_symbol(emitter, local_reg, symbol, 0);                    // store the local scalar or pointer-like value into symbol storage
        }
    }
}

pub fn emit_load_symbol_to_local_slot(
    emitter: &mut Emitter,
    symbol: &str,
    ty: &PhpType,
    offset: usize,
) {
    let local_reg = secondary_scratch_reg(emitter);
    let local_hi_reg = tertiary_scratch_reg(emitter);
    match ty.codegen_repr() {
        PhpType::Float => {
            emit_load_symbol_to_reg(emitter, float_result_reg(emitter), symbol, 0);     // load the float value from symbol storage
            store_at_offset_scratch(emitter, float_result_reg(emitter), offset, local_reg); // write the loaded float value into the local frame slot
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emit_load_symbol_to_reg(emitter, ptr_reg, symbol, 0);                       // load the string pointer from symbol storage
            emit_load_symbol_to_reg(emitter, len_reg, symbol, 8);                       // load the string length from symbol storage
            store_at_offset_scratch(emitter, ptr_reg, offset, local_reg);               // write the loaded string pointer into the local frame slot
            store_at_offset_scratch(emitter, len_reg, offset - 8, local_hi_reg);        // write the loaded string length into the paired local frame slot
        }
        PhpType::Void => {}
        _ => {
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);       // load the scalar or pointer-like value from symbol storage
            store_at_offset_scratch(emitter, int_result_reg(emitter), offset, local_reg); // write the loaded scalar or pointer-like value into the local frame slot
        }
    }
}

pub fn emit_symbol_address(emitter: &mut Emitter, dest: &str, symbol: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp(dest, &format!("{}", symbol));                                // load the page of the requested symbol storage
            emitter.add_lo12(dest, dest, &format!("{}", symbol));                      // resolve the exact address of the requested symbol storage
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("lea {}, [rip + {}]", dest, symbol));         // materialize the symbol address through a RIP-relative LEA
        }
    }
}

pub fn emit_load_symbol_to_reg(emitter: &mut Emitter, reg: &str, symbol: &str, byte_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_symbol_address(emitter, "x9", symbol);
            if byte_offset == 0 {
                emitter.instruction(&format!("ldr {}, [x9]", reg));                    // load the symbol payload directly from its base address
            } else {
                emitter.instruction(&format!("ldr {}, [x9, #{}]", reg, byte_offset));  // load the symbol payload from the requested byte offset
            }
        }
        Arch::X86_64 => {
            let scratch = symbol_scratch_reg(emitter);
            if byte_offset == 0 {
                if is_float_register(reg) {
                    emitter.instruction(&format!("movsd {}, QWORD PTR [rip + {}]", reg, symbol)); // load the floating-point symbol payload through RIP-relative addressing
                } else {
                    emitter.instruction(&format!("mov {}, QWORD PTR [rip + {}]", reg, symbol));   // load the integer or pointer symbol payload through RIP-relative addressing
                }
            } else {
                emit_symbol_address(emitter, scratch, symbol);
                if is_float_register(reg) {
                    emitter.instruction(&format!("movsd {}, QWORD PTR [{} + {}]", reg, scratch, byte_offset)); // load the floating-point symbol payload from a non-zero byte offset
                } else {
                    emitter.instruction(&format!("mov {}, QWORD PTR [{} + {}]", reg, scratch, byte_offset));   // load the integer or pointer symbol payload from a non-zero byte offset
                }
            }
        }
    }
}

pub fn emit_store_reg_to_symbol(emitter: &mut Emitter, reg: &str, symbol: &str, byte_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emit_symbol_address(emitter, "x9", symbol);
            if byte_offset == 0 {
                emitter.instruction(&format!("str {}, [x9]", reg));                    // store the register payload directly into the symbol base slot
            } else {
                emitter.instruction(&format!("str {}, [x9, #{}]", reg, byte_offset));  // store the register payload into the requested symbol byte offset
            }
        }
        Arch::X86_64 => {
            let scratch = symbol_scratch_reg(emitter);
            if byte_offset == 0 {
                if is_float_register(reg) {
                    emitter.instruction(&format!("movsd QWORD PTR [rip + {}], {}", symbol, reg)); // store the floating-point payload directly into RIP-relative symbol storage
                } else {
                    emitter.instruction(&format!("mov QWORD PTR [rip + {}], {}", symbol, reg));   // store the integer or pointer payload directly into RIP-relative symbol storage
                }
            } else {
                emit_symbol_address(emitter, scratch, symbol);
                if is_float_register(reg) {
                    emitter.instruction(&format!("movsd QWORD PTR [{} + {}], {}", scratch, byte_offset, reg)); // store the floating-point payload into a non-zero symbol byte offset
                } else {
                    emitter.instruction(&format!("mov QWORD PTR [{} + {}], {}", scratch, byte_offset, reg));   // store the integer or pointer payload into a non-zero symbol byte offset
                }
            }
        }
    }
}

pub fn emit_load_symbol_to_result(emitter: &mut Emitter, symbol: &str, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Float => {
            emit_load_symbol_to_reg(emitter, float_result_reg(emitter), symbol, 0);     // load the float payload from symbol storage into the result register
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emit_load_symbol_to_reg(emitter, ptr_reg, symbol, 0);                       // load the string pointer from symbol storage into the result register pair
            emit_load_symbol_to_reg(emitter, len_reg, symbol, 8);                       // load the string length from symbol storage into the result register pair
        }
        PhpType::Void => {}
        _ => {
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);       // load the scalar or pointer-like payload from symbol storage into the result register
        }
    }
}

pub fn emit_store_result_to_symbol(
    emitter: &mut Emitter,
    symbol: &str,
    ty: &PhpType,
    release_previous: bool,
) {
    let ty = ty.codegen_repr();
    if release_previous {
        if matches!(ty, PhpType::Str) {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("stp x1, x2, [sp, #-16]!");                    // preserve the incoming string result while releasing the previous symbol payload
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("push {}", ptr_reg));                  // preserve the incoming string pointer result while releasing the previous symbol payload
                    emitter.instruction(&format!("push {}", len_reg));                  // preserve the incoming string length result while releasing the previous symbol payload
                }
            }
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);
            emitter.instruction("bl __rt_heap_free_safe");                      // release the previous string allocation before overwriting the symbol slot
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldp x1, x2, [sp], #16");                      // restore the incoming string result after the release helper call
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("pop {}", len_reg));                   // restore the incoming string length result after the release helper call
                    emitter.instruction(&format!("pop {}", ptr_reg));                   // restore the incoming string pointer result after the release helper call
                }
            }
        } else if ty.is_refcounted() {
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("str x0, [sp, #-16]!");                        // preserve the incoming heap pointer while decreffing the previous symbol payload
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("push {}", int_result_reg(emitter)));  // preserve the incoming heap pointer while decreffing the previous symbol payload
                }
            }
            emit_load_symbol_to_reg(emitter, int_result_reg(emitter), symbol, 0);
            emit_decref_if_refcounted(emitter, &ty);
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("ldr x0, [sp], #16");                          // restore the incoming heap pointer after decreffing the previous payload
                }
                Arch::X86_64 => {
                    emitter.instruction(&format!("pop {}", int_result_reg(emitter)));   // restore the incoming heap pointer after decreffing the previous payload
                }
            }
        }
    }

    match ty {
        PhpType::Float => {
            emit_store_reg_to_symbol(emitter, float_result_reg(emitter), symbol, 0);    // store the float result into symbol storage
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            emit_store_reg_to_symbol(emitter, ptr_reg, symbol, 0);                      // store the string pointer result into symbol storage
            emit_store_reg_to_symbol(emitter, len_reg, symbol, 8);                      // store the string length result into symbol storage
        }
        PhpType::Void => {}
        _ => {
            emit_store_reg_to_symbol(emitter, int_result_reg(emitter), symbol, 0);      // store the scalar or pointer-like result into symbol storage
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
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load int/bool from stack
        }
        PhpType::Float => {
            load_at_offset(emitter, float_result_reg(emitter), offset);                 // load float from stack
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = string_result_regs(emitter);
            load_at_offset(emitter, ptr_reg, offset);                                   // load string pointer
            load_at_offset(emitter, len_reg, offset - 8);                               // load string length
        }
        PhpType::Void => {
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load null sentinel
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
            load_at_offset(emitter, int_result_reg(emitter), offset);                   // load array/callable/object/pointer value
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

    fn test_emitter_x86() -> Emitter {
        Emitter::new(Target::new(Platform::Linux, Arch::X86_64))
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
    fn test_emit_frame_slot_address_large_offset() {
        let mut emitter = test_emitter();
        emit_frame_slot_address(&mut emitter, "x0", 5000);

        assert_eq!(
            emitter.output(),
            concat!(
                "    mov x0, x29\n",
                "    sub x0, x0, #4095\n",
                "    sub x0, x0, #905\n",
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

    #[test]
    fn test_emit_symbol_address_uses_platform_relocations() {
        let mut emitter = test_emitter();
        emit_symbol_address(&mut emitter, "x9", "_demo_symbol");

        assert_eq!(
            emitter.output(),
            concat!(
                "    adrp x9, _demo_symbol@PAGE\n",
                "    add x9, x9, _demo_symbol@PAGEOFF\n",
            )
        );
    }

    #[test]
    fn test_emit_store_and_load_result_to_symbol_for_string() {
        let mut emitter = test_emitter();
        emit_store_result_to_symbol(&mut emitter, "_demo_symbol", &PhpType::Str, false);
        emit_load_symbol_to_result(&mut emitter, "_demo_symbol", &PhpType::Str);
        let out = emitter.output();

        assert!(out.contains("    str x1, [x9]\n"));
        assert!(out.contains("    str x2, [x9, #8]\n"));
        assert!(out.contains("    ldr x1, [x9]\n"));
        assert!(out.contains("    ldr x2, [x9, #8]\n"));
    }

    #[test]
    fn test_emit_store_local_slot_to_symbol_handles_large_string_slot() {
        let mut emitter = test_emitter();
        emit_store_local_slot_to_symbol(&mut emitter, "_static_demo_name", &PhpType::Str, 5000);
        let out = emitter.output();

        assert!(out.contains("    adrp x9, _static_demo_name@PAGE\n"));
        assert!(out.contains("    add x9, x9, _static_demo_name@PAGEOFF\n"));
        assert!(out.contains("    mov x8, x29\n"));
        assert!(out.contains("    sub x8, x8, #4095\n"));
        assert!(out.contains("    ldr x10, [x8]\n"));
        assert!(out.contains("    ldr x11, [x8]\n"));
        assert!(out.contains("    str x10, [x9]\n"));
        assert!(out.contains("    str x11, [x9, #8]\n"));
    }

    #[test]
    fn test_emit_load_symbol_to_local_slot_handles_large_string_slot() {
        let mut emitter = test_emitter();
        emit_load_symbol_to_local_slot(&mut emitter, "_static_demo_name", &PhpType::Str, 5000);
        let out = emitter.output();

        assert!(out.contains("    adrp x9, _static_demo_name@PAGE\n"));
        assert!(out.contains("    add x9, x9, _static_demo_name@PAGEOFF\n"));
        assert!(out.contains("    ldr x1, [x9]\n"));
        assert!(out.contains("    ldr x2, [x9, #8]\n"));
        assert!(out.contains("    mov x10, x29\n"));
        assert!(out.contains("    sub x10, x10, #4095\n"));
        assert!(out.contains("    str x1, [x10]\n"));
        assert!(out.contains("    str x2, [x10]\n"));
    }

    #[test]
    fn test_emit_frame_helpers_linux_x86_64() {
        let mut emitter = test_emitter_x86();
        emit_frame_prologue(&mut emitter, 48);
        emit_frame_restore(&mut emitter, 48);
        emit_return(&mut emitter);

        assert_eq!(
            emitter.output(),
            concat!(
                "    # prologue\n",
                "    push rbp\n",
                "    mov rbp, rsp\n",
                "    sub rsp, 32\n",
                "    add rsp, 32\n",
                "    pop rbp\n",
                "    ret\n",
            )
        );
    }

    #[test]
    fn test_emit_frame_slot_address_linux_x86_64() {
        let mut emitter = test_emitter_x86();
        emit_frame_slot_address(&mut emitter, "r10", 40);

        assert_eq!(emitter.output(), "    lea r10, [rbp - 40]\n");
    }

    #[test]
    fn test_emit_symbol_address_uses_rip_relative_on_linux_x86_64() {
        let mut emitter = test_emitter_x86();
        emit_symbol_address(&mut emitter, "r11", "_demo_symbol");

        assert_eq!(emitter.output(), "    lea r11, [rip + _demo_symbol]\n");
    }

    #[test]
    fn test_emit_store_and_load_result_to_symbol_for_string_linux_x86_64() {
        let mut emitter = test_emitter_x86();
        emit_store_result_to_symbol(&mut emitter, "_demo_symbol", &PhpType::Str, false);
        emit_load_symbol_to_result(&mut emitter, "_demo_symbol", &PhpType::Str);
        let out = emitter.output();

        assert!(out.contains("    mov QWORD PTR [rip + _demo_symbol], rax\n"));
        assert!(out.contains("    mov QWORD PTR [r11 + 8], rdx\n"));
        assert!(out.contains("    mov rax, QWORD PTR [rip + _demo_symbol]\n"));
        assert!(out.contains("    mov rdx, QWORD PTR [r11 + 8]\n"));
    }
}
