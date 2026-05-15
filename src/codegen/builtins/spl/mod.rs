//! Purpose:
//! Emits SPL autoload and object-introspection builtins.
//! Provides runtime stubs for AOT-resolved autoload behavior plus simple object ids/hashes.
//!
//! Called from:
//! - `crate::codegen::builtins::emit_builtin_call()`
//!
//! Key details:
//! - Conforming autoload registrations are consumed before codegen; remaining calls keep PHP-visible defaults.
//! - `spl_classes()` is a static snapshot of compiler-shipped SPL/core class-like names.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

const EXTS_PTR_SYMBOL: &str = "_spl_autoload_exts_ptr";
const EXTS_LEN_SYMBOL: &str = "_spl_autoload_exts_len";

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "spl_autoload_register" | "spl_autoload_unregister" => {
            Some(emit_const_bool(name, args, true, emitter, ctx, data))
        }
        "spl_autoload_functions" => Some(emit_functions_array(name, args, emitter, ctx, data)),
        "spl_autoload_extensions" => Some(emit_extensions(name, args, emitter, ctx, data)),
        "spl_autoload_call" | "spl_autoload" => Some(emit_void(name, args, emitter, ctx, data)),
        "spl_object_id" => Some(emit_object_id(args, emitter, ctx, data)),
        "spl_object_hash" => Some(emit_object_hash(args, emitter, ctx, data)),
        "spl_classes" => Some(emit_classes(emitter, data)),
        _ => None,
    }
}

/// Return the object's heap pointer as an integer — unique per object,
/// stable per process. Matches PHP's contract for `spl_object_id`
/// (PHP's IDs start at 1 and increment, ours are pointer-sized; both
/// satisfy "two distinct objects → distinct ids" within a process).
fn emit_object_id(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("spl_object_id() — return heap pointer as int");
    emit_expr(&args[0], emitter, ctx, data);
    PhpType::Int
}

/// Return the object's heap pointer formatted as a string. PHP returns
/// a 32-character hex string; we return the pointer as a decimal string
/// via `__rt_itoa`. Both forms are unique-per-object and stable
/// per-process — only the textual format differs.
fn emit_object_hash(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment("spl_object_hash() — pointer formatted as decimal string");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_itoa");                                         // convert the heap pointer integer into the standard decimal string output
    PhpType::Str
}

/// Materialise the SPL class/interface registry as an indexed string
/// array. Names mirror what we ship today (10 SPL/core interfaces +
/// `Throwable` and `Exception` plus the 13 SPL exception subclasses);
/// upcoming phases (data structures, iterator decorators, file iterators)
/// will extend this list as their classes land.
fn emit_classes(emitter: &mut Emitter, data: &mut DataSection) -> PhpType {
    let names = SPL_CLASS_NAMES;
    emitter.comment("spl_classes() — AOT snapshot of shipped SPL types");
    let cap = names.len().max(1);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", cap));                  // request capacity for one entry per shipped SPL type
            emitter.instruction("mov x1, #16");                                 // request 16-byte string slots so the array stores ptr+len pairs
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", cap));                  // request capacity for one entry per shipped SPL type
            emitter.instruction("mov rsi, 16");                                 // request 16-byte string slots so the array stores ptr+len pairs
        }
    }
    abi::emit_call_label(emitter, "__rt_array_new");                                    // allocate the SPL-classes registry view through the shared array constructor
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // park the array pointer between push calls
            for n in names {
                let (label, len) = data.add_string(n.as_bytes());
                emitter.instruction("ldr x0, [sp]");                            // reload the array pointer for this push call
                abi::emit_symbol_address(emitter, "x1", &label);                        // load the address of this SPL type's name
                emitter.instruction(&format!("mov x2, #{}", len));              // load the length of this SPL type's name
                emitter.instruction("bl __rt_array_push_str");                  // append the name; may grow the storage
                emitter.instruction("str x0, [sp]");                            // refresh the saved array pointer if the storage grew
            }
            emitter.instruction("ldr x0, [sp], #16");                           // restore the final array pointer as the builtin result
        }
        Arch::X86_64 => {
            emitter.instruction("push rax");                                    // park the array pointer between push calls
            emitter.instruction("sub rsp, 8");                                  // keep stack 16-byte aligned for the call sequence
            for n in names {
                let (label, len) = data.add_string(n.as_bytes());
                emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");            // reload the array pointer for this push call
                abi::emit_symbol_address(emitter, "rsi", &label);                       // load the address of this SPL type's name
                emitter.instruction(&format!("mov rdx, {}", len));              // load the length of this SPL type's name
                emitter.instruction("call __rt_array_push_str");                // append the name; may grow the storage
                emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // refresh the saved array pointer if the storage grew
            }
            emitter.instruction("add rsp, 8");                                  // pop the alignment padding
            emitter.instruction("pop rax");                                     // restore the final array pointer as the builtin result
        }
    }
    PhpType::Array(Box::new(PhpType::Str))
}

/// The static set of SPL/core type names shipped today. Stays in sync
/// with `inject_builtin_interfaces` and `inject_builtin_spl_exceptions`.
const SPL_CLASS_NAMES: &[&str] = &[
    "ArrayAccess",
    "BadFunctionCallException",
    "BadMethodCallException",
    "Countable",
    "DomainException",
    "Exception",
    "InvalidArgumentException",
    "Iterator",
    "IteratorAggregate",
    "JsonSerializable",
    "LengthException",
    "LogicException",
    "OuterIterator",
    "OutOfBoundsException",
    "OutOfRangeException",
    "OverflowException",
    "RangeException",
    "RecursiveIterator",
    "RuntimeException",
    "SeekableIterator",
    "SplObserver",
    "SplSubject",
    "Stringable",
    "Throwable",
    "Traversable",
    "UnderflowException",
    "UnexpectedValueException",
];

fn emit_args_for_side_effects(
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) {
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
    }
}

fn emit_const_bool(
    name: &str,
    args: &[Expr],
    value: bool,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("{}() — AOT stub", name));
    emit_args_for_side_effects(args, emitter, ctx, data);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), value as i64); // signal success: register/unregister always reports the call as accepted
    PhpType::Bool
}

fn emit_void(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    emitter.comment(&format!("{}() — AOT stub", name));
    emit_args_for_side_effects(args, emitter, ctx, data);
    PhpType::Void
}

fn emit_functions_array(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let rule_count = crate::codegen::autoload_rule_count();
    emitter.comment(&format!(
        "{}() — AOT registry view ({} rule{})",
        name,
        rule_count,
        if rule_count == 1 { "" } else { "s" }
    ));
    emit_args_for_side_effects(args, emitter, ctx, data);
    let cap = rule_count.max(1);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x0, #{}", cap));                  // request enough capacity to hold one entry per registered autoload rule
            emitter.instruction("mov x1, #8");                                  // request 8-byte int slots — the introspection array stores rule indexes
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdi, {}", cap));                  // request enough capacity to hold one entry per registered autoload rule
            emitter.instruction("mov rsi, 8");                                  // request 8-byte int slots — the introspection array stores rule indexes
        }
    }
    abi::emit_call_label(emitter, "__rt_array_new");                                    // allocate the indexed registry view through the shared array constructor

    if rule_count > 0 {
        emit_functions_array_fill(rule_count, emitter);
    }

    PhpType::Array(Box::new(PhpType::Int))
}

/// After `__rt_array_new` returns the empty array in `x0`/`rax`, push
/// `rule_count` integer placeholders (rule indexes 0..N-1) so `count()`
/// and `foreach` see one entry per registered rule.
fn emit_functions_array_fill(rule_count: usize, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x0, [sp, #-16]!");                         // park the indexed-array pointer while we push placeholder entries
            for i in 0..rule_count {
                emitter.instruction("ldr x0, [sp]");                            // reload the array pointer for each push call
                emitter.instruction(&format!("mov x1, #{}", i));                // load the rule-index placeholder for this slot
                emitter.instruction("bl __rt_array_push_int");                  // append the placeholder index, may grow the storage
                emitter.instruction("str x0, [sp]");                            // refresh the saved array pointer in case __rt_array_push_int grew it
            }
            emitter.instruction("ldr x0, [sp], #16");                           // restore the final array pointer as the builtin result
        }
        Arch::X86_64 => {
            emitter.instruction("push rax");                                    // park the indexed-array pointer while we push placeholder entries
            emitter.instruction("sub rsp, 8");                                  // keep the stack 16-byte aligned for the call sequence
            for i in 0..rule_count {
                emitter.instruction("mov rdi, QWORD PTR [rsp + 8]");            // reload the array pointer for each push call
                emitter.instruction(&format!("mov rsi, {}", i));                // load the rule-index placeholder for this slot
                emitter.instruction("call __rt_array_push_int");                // append the placeholder index, may grow the storage
                emitter.instruction("mov QWORD PTR [rsp + 8], rax");            // refresh the saved array pointer in case __rt_array_push_int grew it
            }
            emitter.instruction("add rsp, 8");                                  // pop the alignment padding before restoring the array pointer
            emitter.instruction("pop rax");                                     // restore the final array pointer as the builtin result
        }
    }
}

/// Read or read+write the runtime-mutable `_spl_autoload_exts_*` globals.
///
/// Read (no arg, or arg is the `null` literal): load (ptr, len) into the
/// string result registers.
///
/// Write (string-typed arg): evaluate the new value, save it, load the
/// previous (ptr, len) into the result registers, and overwrite the
/// globals with the new value. Returns the previous value as PHP does.
fn emit_extensions(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    let writes_new = args
        .first()
        .is_some_and(|arg| !matches!(arg.kind, ExprKind::Null));

    let (ptr_reg, len_reg) = abi::string_result_regs(emitter);

    if writes_new {
        emitter.comment(&format!("{}() — store new extensions, return previous", name));
        let arg = &args[0];
        emit_expr(arg, emitter, ctx, data);
        // -- save the new (ptr, len) we just evaluated --
        abi::emit_push_reg_pair(emitter, ptr_reg, len_reg);
        // -- load previous (ptr, len) into the string result regs --
        abi::emit_load_symbol_to_reg(emitter, ptr_reg, EXTS_PTR_SYMBOL, 0);
        abi::emit_load_symbol_to_reg(emitter, len_reg, EXTS_LEN_SYMBOL, 0);
        // -- pop the saved new value into scratch regs and write to globals --
        let new_ptr = abi::secondary_scratch_reg(emitter);
        let new_len = abi::tertiary_scratch_reg(emitter);
        abi::emit_pop_reg_pair(emitter, new_ptr, new_len);
        abi::emit_store_reg_to_symbol(emitter, new_ptr, EXTS_PTR_SYMBOL, 0);
        abi::emit_store_reg_to_symbol(emitter, new_len, EXTS_LEN_SYMBOL, 0);
    } else {
        emitter.comment(&format!("{}() — read current extensions", name));
        // -- evaluate any null arg for parity (no observable effect) --
        emit_args_for_side_effects(args, emitter, ctx, data);
        abi::emit_load_symbol_to_reg(emitter, ptr_reg, EXTS_PTR_SYMBOL, 0);
        abi::emit_load_symbol_to_reg(emitter, len_reg, EXTS_LEN_SYMBOL, 0);
    }

    PhpType::Str
}
