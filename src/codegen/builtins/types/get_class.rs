//! Purpose:
//! Emits `get_class()` and `get_parent_class()` through runtime object class-id lookup.
//! Materializes no-argument scope lookups statically and object arguments dynamically.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`
//!
//! Key details:
//! - Arguments are still evaluated for side effects before class-name results are loaded.
//! - Object arguments use dense class-name metadata so caught base-type variables keep their concrete class.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits `get_class()` or `get_parent_class()`.
///
/// `get_class()` with no arguments returns the current class name from `ctx.current_class`.
/// With an object argument, evaluates the argument and reads the runtime class id from the
/// object header, preserving concrete subclasses even when the static type is a parent or interface.
/// Non-object arguments currently return an empty string after evaluation.
///
/// `get_parent_class()` with no arguments resolves the current class parent through `ctx.classes`.
/// With an object argument, it resolves the runtime object's parent id through emitted metadata.
///
/// The resolved class name is emitted as a string literal into the data section, and its
/// address/length are published via ABI string-result registers (`x1`/`x2` on ARM64).
///
/// # Arguments
/// * `name` — `"get_class"` or `"get_parent_class"`
/// * `args` — call arguments (empty for no-arg variant, one argument otherwise)
/// * `emitter` — code emitter
/// * `ctx` — codegen context (provides `current_class` and `classes` map)
/// * `data` — data section for string literal emission
///
/// # Returns
/// `Some(PhpType::Str)` — the result type is always a string
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}() — class-name lookup", name));

    let resolved_class = if args.is_empty() {
        ctx.current_class.clone().unwrap_or_default()
    } else {
        let arg_ty = emit_expr(&args[0], emitter, ctx, data);
        match arg_ty {
            PhpType::Object(_) => {
                emit_dynamic_object_class_name(name, emitter, ctx);
                return Some(PhpType::Str);
            }
            _ => String::new(),
        }
    };

    let final_name = match name {
        "get_class" => resolved_class,
        "get_parent_class" => parent_of(&resolved_class, ctx),
        _ => String::new(),
    };

    let bytes = final_name.as_bytes();
    let (label, len) = data.add_string(bytes);
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    abi::emit_symbol_address(emitter, ptr_reg, &label);                                 // expose the resolved class name in the string-pointer result register
    abi::emit_load_int_immediate(emitter, len_reg, len as i64);                         // publish the resolved class name length in the paired length result register
    Some(PhpType::Str)
}

/// Emits dynamic class-name lookup for an object pointer in the integer result register.
///
/// `get_class()` indexes `_class_name_entries` by the object's runtime class id.
/// `get_parent_class()` first maps the runtime class id through `_class_parent_ids`.
/// Invalid or parentless class ids return the shared zero-length `_class_name_missing` string.
fn emit_dynamic_object_class_name(name: &str, emitter: &mut Emitter, ctx: &mut Context) {
    let empty_label = ctx.next_label("get_class_empty");
    let done_label = ctx.next_label("get_class_done");
    match emitter.target.arch {
        Arch::AArch64 => emit_dynamic_object_class_name_arm64(name, &empty_label, &done_label, emitter),
        Arch::X86_64 => emit_dynamic_object_class_name_x86_64(name, &empty_label, &done_label, emitter),
    }
}

/// Emits ARM64 runtime object class-name lookup for `get_class()` and `get_parent_class()`.
fn emit_dynamic_object_class_name_arm64(
    name: &str,
    empty_label: &str,
    done_label: &str,
    emitter: &mut Emitter,
) {
    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);
    emitter.instruction(&format!("cbz x0, {}", empty_label));                   // null object pointers produce an empty class name
    emitter.instruction("ldr x9, [x0]");                                        // load the object's concrete runtime class id
    abi::emit_symbol_address(emitter, "x10", "_class_name_count");
    emitter.instruction("ldr x10, [x10]");                                      // x10 = number of dense class-name lookup rows
    if name == "get_parent_class" {
        emitter.instruction("cmp x9, x10");                                     // validate the object class id before reading its parent id
        emitter.instruction(&format!("b.hs {}", empty_label));                  // unknown object class ids have no reportable parent class
        abi::emit_symbol_address(emitter, "x11", "_class_parent_ids");
        emitter.instruction("lsl x12, x9, #3");                                 // scale the class id to a parent-id table byte offset
        emitter.instruction("ldr x9, [x11, x12]");                              // replace the object class id with its parent class id
        emitter.instruction("mov x13, #-1");                                    // x13 = parentless class sentinel
        emitter.instruction("cmp x9, x13");                                     // check whether the runtime class has no parent
        emitter.instruction(&format!("b.eq {}", empty_label));                  // parentless runtime classes produce an empty string
    }
    emitter.instruction("cmp x9, x10");                                         // validate the class id before indexing class-name metadata
    emitter.instruction(&format!("b.hs {}", empty_label));                      // invalid class ids produce an empty class name
    abi::emit_symbol_address(emitter, "x11", "_class_name_entries");
    emitter.instruction("lsl x12, x9, #4");                                     // scale the class id by the 16-byte class-name row size
    emitter.instruction("add x11, x11, x12");                                   // x11 = selected class-name metadata row
    emitter.instruction(&format!("ldr {}, [x11]", ptr_reg));                    // load the concrete class-name string pointer
    emitter.instruction(&format!("ldr {}, [x11, #8]", len_reg));                // load the concrete class-name string length
    emitter.instruction(&format!("b {}", done_label));                          // skip the empty-string fallback after a successful lookup

    emitter.label(empty_label);
    abi::emit_symbol_address(emitter, ptr_reg, "_class_name_missing");
    abi::emit_load_int_immediate(emitter, len_reg, 0);

    emitter.label(done_label);
}

/// Emits x86_64 runtime object class-name lookup for `get_class()` and `get_parent_class()`.
fn emit_dynamic_object_class_name_x86_64(
    name: &str,
    empty_label: &str,
    done_label: &str,
    emitter: &mut Emitter,
) {
    emitter.instruction("test rax, rax");                                       // null object pointers produce an empty class name
    emitter.instruction(&format!("je {}", empty_label));                        // branch to the empty-string fallback for null object pointers
    emitter.instruction("mov r8, QWORD PTR [rax]");                             // load the object's concrete runtime class id
    abi::emit_load_symbol_to_reg(emitter, "r9", "_class_name_count", 0);        // r9 = number of dense class-name lookup rows
    if name == "get_parent_class" {
        emitter.instruction("cmp r8, r9");                                      // validate the object class id before reading its parent id
        emitter.instruction(&format!("jae {}", empty_label));                   // unknown object class ids have no reportable parent class
        abi::emit_symbol_address(emitter, "r10", "_class_parent_ids");          // materialize the runtime parent-id table base pointer
        emitter.instruction("mov r8, QWORD PTR [r10 + r8 * 8]");                // replace the object class id with its parent class id
        emitter.instruction("cmp r8, -1");                                      // check whether the runtime class has no parent
        emitter.instruction(&format!("je {}", empty_label));                    // parentless runtime classes produce an empty string
    }
    emitter.instruction("cmp r8, r9");                                          // validate the class id before indexing class-name metadata
    emitter.instruction(&format!("jae {}", empty_label));                       // invalid class ids produce an empty class name
    abi::emit_symbol_address(emitter, "r10", "_class_name_entries");            // materialize the class-name metadata table base pointer
    emitter.instruction("shl r8, 4");                                           // scale the class id by the 16-byte class-name row size
    emitter.instruction("mov rax, QWORD PTR [r10 + r8]");                       // load the concrete class-name string pointer
    emitter.instruction("mov rdx, QWORD PTR [r10 + r8 + 8]");                   // load the concrete class-name string length
    emitter.instruction(&format!("jmp {}", done_label));                        // skip the empty-string fallback after a successful lookup

    emitter.label(empty_label);
    abi::emit_symbol_address(emitter, "rax", "_class_name_missing");            // return the shared empty class-name string pointer
    emitter.instruction("xor edx, edx");                                        // return zero bytes for the empty class name

    emitter.label(done_label);
}

/// Returns the parent class name for `class_name`, consulting `ctx.classes`.
///
/// Returns an empty string if `class_name` is empty or the class has no parent entry.
///
/// # Arguments
/// * `class_name` — fully or partially qualified class name
/// * `ctx` — codegen context providing the class metadata map
///
/// # Returns
/// Parent class name as a `String`, or empty string if unavailable
fn parent_of(class_name: &str, ctx: &Context) -> String {
    if class_name.is_empty() {
        return String::new();
    }
    ctx.classes
        .get(class_name.trim_start_matches('\\'))
        .and_then(|info| info.parent.clone())
        .unwrap_or_default()
}
