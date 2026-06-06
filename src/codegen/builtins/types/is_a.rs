//! Purpose:
//! Emits `is_a()` and `is_subclass_of()` class-relation checks.
//! Folds to a constant for a statically-known object receiver, and falls back to a runtime
//! class-id check for a boxed `Mixed`/`Union` receiver (e.g. `Foo|false` from PDO, or an
//! untyped parameter) against a literal target class name.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`
//!
//! Key details:
//! - All class-name comparisons are PHP-style case-insensitive via `php_symbol_key`.
//! - The runtime path unboxes the receiver and compares its header class id against the
//!   compile-time set of class ids that satisfy the relation (target + subclasses + implementers).
//! - A non-object receiver returns `false` (not a fatal); `$allow_string` (treating a string
//!   receiver as a class name) and non-literal target class names are not yet supported and
//!   fall back to the statically-determined result.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::platform::Arch;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{ClassInfo, PhpType};

/// Emits `is_a()` / `is_subclass_of()`.
///
/// For a statically-known `Object` receiver the relation is folded to a constant boolean. For a
/// boxed `Mixed`/`Union` receiver with a string-literal target, a runtime check unboxes the value
/// and tests its class id against the compile-time set of class ids satisfying the relation; a
/// non-object value yields `false`. All other shapes fall back to the folded (often `false`)
/// result. `is_subclass_of()` excludes an exact self match. Returns `PhpType::Bool`.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}()", name));
    let exclude_self = name == "is_subclass_of";

    // The target class name must be a string literal for the fold and the runtime id-set; a
    // runtime (non-literal) class name is not yet supported and falls back to false.
    let target_literal = match &args[1].kind {
        ExprKind::StringLiteral(s) => Some(s.trim_start_matches('\\').to_string()),
        _ => None,
    };

    let arg_ty = emit_expr(&args[0], emitter, ctx, data);

    // Runtime path: a boxed Mixed/Union receiver with a literal target.
    if matches!(arg_ty, PhpType::Mixed | PhpType::Union(_)) {
        if let Some(target) = &target_literal {
            let ids = matching_class_ids(ctx, target, exclude_self);
            // Preserve the boxed receiver across the remaining args' side effects.
            abi::emit_push_reg(emitter, abi::int_result_reg(emitter));
            for arg in args.iter().skip(1) {
                emit_expr(arg, emitter, ctx, data);
            }
            abi::emit_pop_reg(emitter, abi::int_result_reg(emitter));
            emit_runtime_class_is_a(emitter, ctx, &ids);
            return Some(PhpType::Bool);
        }
    }

    // Fold path: evaluate the remaining args for side effects, then load the static result.
    for arg in args.iter().skip(1) {
        emit_expr(arg, emitter, ctx, data);
    }
    let result = match (&arg_ty, &target_literal) {
        (PhpType::Object(obj_class), Some(target)) => {
            class_is_a(ctx, obj_class.trim_start_matches('\\'), target, exclude_self)
        }
        _ => false,
    };
    abi::emit_load_int_immediate(
        emitter,
        abi::int_result_reg(emitter),
        if result { 1 } else { 0 },
    );
    Some(PhpType::Bool)
}

/// Emits the runtime class-relation check for a boxed receiver currently in the integer result
/// register: unboxes it, returns `0` for a non-object, otherwise compares the header class id
/// against each id in `ids` (the compile-time set satisfying the relation) and returns `1`/`0`.
fn emit_runtime_class_is_a(emitter: &mut Emitter, ctx: &mut Context, ids: &[u64]) {
    let no_match = ctx.next_label("is_a_no");
    let matched = ctx.next_label("is_a_yes");
    let done = ctx.next_label("is_a_done");
    abi::emit_call_label(emitter, "__rt_mixed_unbox"); // unbox: tag in x0/rax, value_lo (obj ptr) in x1/rdi
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #6");                                  // runtime tag 6 == object?
            emitter.instruction(&format!("b.ne {}", no_match));                 // a non-object receiver is never is-a a class
            emitter.instruction("ldr x9, [x1]");                                // load the receiver's class id from the object header
            for id in ids {
                abi::emit_load_int_immediate(emitter, "x10", *id as i64); // a class id that satisfies the relation
                emitter.instruction("cmp x9, x10");                             // compare the receiver class id against the candidate
                emitter.instruction(&format!("b.eq {}", matched));              // matched: the relation holds
            }
            emitter.label(&no_match);
            emitter.instruction("mov x0, #0");                                  // relation does not hold
            emitter.instruction(&format!("b {}", done));                        // skip the matched path
            emitter.label(&matched);
            emitter.instruction("mov x0, #1");                                  // relation holds
            emitter.label(&done);
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 6");                                  // runtime tag 6 == object?
            emitter.instruction(&format!("jne {}", no_match));                  // a non-object receiver is never is-a a class
            emitter.instruction("mov r9, QWORD PTR [rdi]");                     // load the receiver's class id from the object header
            for id in ids {
                abi::emit_load_int_immediate(emitter, "r10", *id as i64); // a class id that satisfies the relation
                emitter.instruction("cmp r9, r10");                             // compare the receiver class id against the candidate
                emitter.instruction(&format!("je {}", matched));                // matched: the relation holds
            }
            emitter.label(&no_match);
            emitter.instruction("xor rax, rax");                                // relation does not hold
            emitter.instruction(&format!("jmp {}", done));                      // skip the matched path
            emitter.label(&matched);
            emitter.instruction("mov rax, 1");                                  // relation holds
            emitter.label(&done);
        }
    }
}

/// Returns the class ids of every known class that satisfies `is_a(_, target)` (i.e. is the target
/// class, a subclass, or an implementer), excluding the target's own id when `exclude_self` is set
/// (the `is_subclass_of` semantics). Ids are sorted for deterministic emission.
fn matching_class_ids(ctx: &Context, target: &str, exclude_self: bool) -> Vec<u64> {
    let mut ids: Vec<u64> = ctx
        .classes
        .iter()
        .filter(|(class_name, _)| {
            class_is_a(ctx, class_name.trim_start_matches('\\'), target, exclude_self)
        })
        .map(|(_, info)| info.class_id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Returns `true` if class `obj_class` satisfies the relation to `target`: it is the same class
/// (unless `exclude_self`), a subclass via the parent chain, or implements `target` as an interface.
/// All comparisons are PHP-style case-insensitive.
fn class_is_a(ctx: &Context, obj_class: &str, target: &str, exclude_self: bool) -> bool {
    let obj_class = obj_class.trim_start_matches('\\');
    let target_key = php_symbol_key(target.trim_start_matches('\\'));

    if !exclude_self && php_symbol_key(obj_class) == target_key {
        return true;
    }

    // Walk the parent chain.
    let mut current = obj_class.to_string();
    while let Some(info) = lookup_class(ctx, &current) {
        if let Some(parent) = &info.parent {
            let parent_clean = parent.trim_start_matches('\\');
            if php_symbol_key(parent_clean) == target_key {
                return true;
            }
            current = parent_clean.to_string();
        } else {
            break;
        }
    }

    // Walk implemented (and transitively-inherited) interfaces.
    if let Some(info) = lookup_class(ctx, obj_class) {
        for iface in &info.interfaces {
            if php_symbol_key(iface.trim_start_matches('\\')) == target_key {
                return true;
            }
        }
    }

    false
}

/// Looks up a class by name in `ctx.classes` using PHP-style case-insensitive lookup.
/// Tries an exact match first (with leading backslash stripped), then falls back to a
/// linear search via `php_symbol_key`. Returns the `ClassInfo` if found.
fn lookup_class<'a>(ctx: &'a Context, name: &str) -> Option<&'a ClassInfo> {
    let clean = name.trim_start_matches('\\');
    if let Some(info) = ctx.classes.get(clean) {
        return Some(info);
    }
    let key = php_symbol_key(clean);
    ctx.classes
        .iter()
        .find(|(candidate, _)| php_symbol_key(candidate.trim_start_matches('\\')) == key)
        .map(|(_, info)| info)
}
