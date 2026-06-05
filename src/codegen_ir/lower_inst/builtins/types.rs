//! Purpose:
//! Lowers PHP type/reflection builtins for the EIR backend.
//! Handles class-name lookup against static metadata and runtime object class ids.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Dynamic object lookups use the same dense `_class_name_*` runtime tables
//!   emitted for the legacy backend, preserving concrete subclasses.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, store_if_result};

/// Lowers `get_class()` and `get_parent_class()` through static or dynamic class metadata.
pub(super) fn lower_class_name_lookup(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count_between(inst, name, 0, 1)?;
    if inst.operands.is_empty() {
        emit_no_arg_class_name_lookup(ctx, name);
        return store_if_result(ctx, inst);
    }

    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            emit_dynamic_object_class_name(ctx, name);
        }
        PhpType::Str if name == "get_parent_class" => {
            let class_name = const_string_operand(ctx, value)?;
            let parent = parent_of(ctx, &class_name);
            emit_string_result(ctx, parent.as_bytes());
        }
        other => {
            ctx.load_value_to_result(value)?;
            if matches!(other, PhpType::Mixed | PhpType::Union(_)) {
                return Err(CodegenIrError::unsupported(format!(
                    "{} for PHP type {:?}",
                    name,
                    other
                )));
            }
            emit_string_result(ctx, b"");
        }
    }
    store_if_result(ctx, inst)
}

/// Emits a static no-argument class-name result for the current method scope.
fn emit_no_arg_class_name_lookup(ctx: &mut FunctionContext<'_>, name: &str) {
    let class_name = current_method_class(ctx).unwrap_or_default();
    let result = if name == "get_parent_class" {
        parent_of(ctx, class_name)
    } else {
        class_name.to_string()
    };
    emit_string_result(ctx, result.as_bytes());
}

/// Emits dynamic class-name lookup for an object pointer already loaded in the result register.
fn emit_dynamic_object_class_name(ctx: &mut FunctionContext<'_>, name: &str) {
    let empty_label = ctx.next_label("get_class_empty");
    let done_label = ctx.next_label("get_class_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_dynamic_object_class_name_aarch64(ctx, name, &empty_label, &done_label),
        Arch::X86_64 => emit_dynamic_object_class_name_x86_64(ctx, name, &empty_label, &done_label),
    }
}

/// Emits AArch64 runtime object class-name lookup for `get_class()` and `get_parent_class()`.
fn emit_dynamic_object_class_name_aarch64(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    empty_label: &str,
    done_label: &str,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    ctx.emitter.instruction(&format!("cbz x0, {}", empty_label));               // null object pointers produce an empty class name
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the object's concrete runtime class id
    abi::emit_symbol_address(ctx.emitter, "x10", "_class_name_count");
    ctx.emitter.instruction("ldr x10, [x10]");                                  // load the number of dense class-name lookup rows
    if name == "get_parent_class" {
        ctx.emitter.instruction("cmp x9, x10");                                 // validate the object class id before reading its parent id
        ctx.emitter.instruction(&format!("b.hs {}", empty_label));              // reject unknown object class ids as parentless
        abi::emit_symbol_address(ctx.emitter, "x11", "_class_parent_ids");
        ctx.emitter.instruction("lsl x12, x9, #3");                             // scale the class id to a parent-id table byte offset
        ctx.emitter.instruction("ldr x9, [x11, x12]");                          // replace the class id with its parent class id
        ctx.emitter.instruction("mov x13, #-1");                                // materialize the parentless class sentinel
        ctx.emitter.instruction("cmp x9, x13");                                 // check whether the runtime class has no parent
        ctx.emitter.instruction(&format!("b.eq {}", empty_label));              // parentless runtime classes produce an empty string
    }
    ctx.emitter.instruction("cmp x9, x10");                                     // validate the class id before indexing class-name metadata
    ctx.emitter.instruction(&format!("b.hs {}", empty_label));                  // invalid class ids produce an empty class name
    abi::emit_symbol_address(ctx.emitter, "x11", "_class_name_entries");
    ctx.emitter.instruction("lsl x12, x9, #4");                                 // scale the class id by the 16-byte class-name row size
    ctx.emitter.instruction("add x11, x11, x12");                               // point at the selected class-name metadata row
    ctx.emitter.instruction(&format!("ldr {}, [x11]", ptr_reg));                // load the concrete class-name string pointer
    ctx.emitter.instruction(&format!("ldr {}, [x11, #8]", len_reg));            // load the concrete class-name string length
    ctx.emitter.instruction(&format!("b {}", done_label));                      // skip the empty-string fallback after a successful lookup

    ctx.emitter.label(empty_label);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, "_class_name_missing");
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);

    ctx.emitter.label(done_label);
}

/// Emits x86_64 runtime object class-name lookup for `get_class()` and `get_parent_class()`.
fn emit_dynamic_object_class_name_x86_64(
    ctx: &mut FunctionContext<'_>,
    name: &str,
    empty_label: &str,
    done_label: &str,
) {
    ctx.emitter.instruction("test rax, rax");                                   // test whether the object pointer is null
    ctx.emitter.instruction(&format!("je {}", empty_label));                    // null object pointers produce an empty class name
    ctx.emitter.instruction("mov r8, QWORD PTR [rax]");                         // load the object's concrete runtime class id
    ctx.emitter.instruction("mov r9, QWORD PTR [rip + _class_name_count]");     // load the number of dense class-name lookup rows
    if name == "get_parent_class" {
        ctx.emitter.instruction("cmp r8, r9");                                  // validate the object class id before reading its parent id
        ctx.emitter.instruction(&format!("jae {}", empty_label));               // reject unknown object class ids as parentless
        ctx.emitter.instruction("lea r10, [rip + _class_parent_ids]");          // materialize the runtime parent-id table base pointer
        ctx.emitter.instruction("mov r8, QWORD PTR [r10 + r8 * 8]");            // replace the class id with its parent class id
        ctx.emitter.instruction("cmp r8, -1");                                  // check whether the runtime class has no parent
        ctx.emitter.instruction(&format!("je {}", empty_label));                // parentless runtime classes produce an empty string
    }
    ctx.emitter.instruction("cmp r8, r9");                                      // validate the class id before indexing class-name metadata
    ctx.emitter.instruction(&format!("jae {}", empty_label));                   // invalid class ids produce an empty class name
    ctx.emitter.instruction("lea r10, [rip + _class_name_entries]");            // materialize the class-name metadata table base pointer
    ctx.emitter.instruction("shl r8, 4");                                       // scale the class id by the 16-byte class-name row size
    ctx.emitter.instruction("mov rax, QWORD PTR [r10 + r8]");                   // load the concrete class-name string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [r10 + r8 + 8]");               // load the concrete class-name string length
    ctx.emitter.instruction(&format!("jmp {}", done_label));                    // skip the empty-string fallback after a successful lookup

    ctx.emitter.label(empty_label);
    ctx.emitter.instruction("lea rax, [rip + _class_name_missing]");            // return the shared empty class-name string pointer
    ctx.emitter.instruction("xor edx, edx");                                    // return zero bytes for the empty class name

    ctx.emitter.label(done_label);
}

/// Emits `bytes` as the current string result register pair.
fn emit_string_result(ctx: &mut FunctionContext<'_>, bytes: &[u8]) {
    let (label, len) = ctx.data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Returns the lexical class name encoded in an EIR method function name.
fn current_method_class<'a>(ctx: &'a FunctionContext<'_>) -> Option<&'a str> {
    ctx.function
        .name
        .rsplit_once("::")
        .map(|(class_name, _)| class_name)
}

/// Returns the parent class name for a known class, or an empty string when unavailable.
fn parent_of(ctx: &FunctionContext<'_>, class_name: &str) -> String {
    if class_name.is_empty() {
        return String::new();
    }
    ctx.module
        .class_infos
        .get(class_name.trim_start_matches('\\'))
        .and_then(|info| info.parent.clone())
        .unwrap_or_default()
}

/// Returns a string literal value defined by a `ConstStr` operand.
fn const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "get_parent_class with non-literal class name",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Err(CodegenIrError::unsupported(
            "get_parent_class with non-literal class name",
        ));
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "get_parent_class string literal has no data id",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}
