//! Purpose:
//! Emits PHP `is_iterable` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen_support::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `is_iterable` builtin call.
///
/// dispatches based on the resolved type of `args[0]`:
/// - For `PhpType::Mixed` or `PhpType::Union`: unboxes the runtime value at runtime and
///   checks the payload tag (indexed array, assoc hash, or object implementing
///   Iterator/IteratorAggregate). Returns true or false via the `true_case`/`done` control flow.
/// - For `PhpType::Array`, `PhpType::AssocArray`, `PhpType::Iterable`, or a known object
///   implementing Iterator/IteratorAggregate: folds to a compile-time `1` or `0`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_iterable()");
    let ty = emit_expr(&args[0], emitter, ctx, data);

    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        // Mixed/Union values are boxed cells. Unwrap to the concrete runtime tag and
        // report true for arrays and objects implementing Iterator/IteratorAggregate.
        let true_case = ctx.next_label("builtin_is_iterable_true");
        let object_case = ctx.next_label("builtin_is_iterable_object");
        let done = ctx.next_label("builtin_is_iterable_done");

        abi::emit_call_label(emitter, "__rt_mixed_unbox");                      // resolve the boxed mixed payload tag for the iterable predicate
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #4");                              // runtime tag 4 = indexed array
                emitter.instruction(&format!("b.eq {}", true_case));            // indexed arrays satisfy is_iterable
                emitter.instruction("cmp x0, #5");                              // runtime tag 5 = associative hash
                emitter.instruction(&format!("b.eq {}", true_case));            // hash tables satisfy is_iterable
                emitter.instruction("cmp x0, #6");                              // runtime tag 6 = object
                emitter.instruction(&format!("b.eq {}", object_case));          // Traversable objects satisfy is_iterable
                emitter.instruction("mov x0, #0");                              // every other concrete payload reports false
                emitter.instruction(&format!("b {}", done));                    // skip the truthy assignment
            }
            Arch::X86_64 => {
                emitter.instruction("cmp rax, 4");                              // runtime tag 4 = indexed array
                emitter.instruction(&format!("je {}", true_case));              // indexed arrays satisfy is_iterable
                emitter.instruction("cmp rax, 5");                              // runtime tag 5 = associative hash
                emitter.instruction(&format!("je {}", true_case));              // hash tables satisfy is_iterable
                emitter.instruction("cmp rax, 6");                              // runtime tag 6 = object
                emitter.instruction(&format!("je {}", object_case));            // Traversable objects satisfy is_iterable
                emitter.instruction("mov rax, 0");                              // every other concrete payload reports false
                emitter.instruction(&format!("jmp {}", done));                  // skip the truthy assignment
            }
        }

        emitter.label(&object_case);
        emit_runtime_object_iterable_check(emitter, ctx, &true_case, &done);

        emitter.label(&true_case);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("mov x0, #1");                              // record the truthy is_iterable result on AArch64
            }
            Arch::X86_64 => {
                emitter.instruction("mov rax, 1");                              // record the truthy is_iterable result on x86_64
            }
        }
        emitter.label(&done);
        return Some(PhpType::Bool);
    }

    let val = matches!(
        ty,
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable
    ) || matches!(&ty, PhpType::Object(name) if object_type_implements_iterable(ctx, name));
    abi::emit_load_int_immediate(
        emitter,
        abi::int_result_reg(emitter),
        if val { 1 } else { 0 },
    );                                                                          // record the compile-time is_iterable predicate result
    Some(PhpType::Bool)
}

/// Emits the runtime check for whether a boxed object payload implements Iterator or IteratorAggregate.
///
/// Saves the object pointer from `x1`/`rdi` onto the stack, then tests it against both interface IDs
/// via `__rt_exception_matches`. Jumps to `true_case` on either match, otherwise falls through to
/// load `0` and jump to `done`. Preserves stack balance on both paths.
fn emit_runtime_object_iterable_check(
    emitter: &mut Emitter,
    ctx: &mut Context,
    true_case: &str,
    done: &str,
) {
    let object_true = ctx.next_label("builtin_is_iterable_object_true");
    let Some(iterator_id) = ctx.interfaces.get("Iterator").map(|info| info.interface_id) else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        abi::emit_jump(emitter, done);                                          // no Iterator metadata means object payloads cannot satisfy is_iterable
        return;
    };
    let Some(aggregate_id) = ctx
        .interfaces
        .get("IteratorAggregate")
        .map(|info| info.interface_id)
    else {
        abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), 0);
        abi::emit_jump(emitter, done);                                          // no IteratorAggregate metadata means object payloads cannot satisfy is_iterable
        return;
    };

    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("str x1, [sp, #-16]!");                         // preserve the unboxed object pointer across interface checks
            emit_saved_object_interface_check(iterator_id, &object_true, emitter);
            emit_saved_object_interface_check(aggregate_id, &object_true, emitter);
            emitter.instruction("add sp, sp, #16");                             // discard the saved object pointer after failed interface checks
            emitter.instruction("mov x0, #0");                                  // non-Traversable objects do not satisfy is_iterable
            emitter.instruction(&format!("b {}", done));                        // skip the truthy assignment
            emitter.label(&object_true);
            emitter.instruction("add sp, sp, #16");                             // discard the saved object pointer before returning true
            emitter.instruction(&format!("b {}", true_case));                   // continue through the shared truthy result path
        }
        Arch::X86_64 => {
            abi::emit_push_reg(emitter, "rdi");                                  // preserve the unboxed object pointer across interface checks
            emit_saved_object_interface_check(iterator_id, &object_true, emitter);
            emit_saved_object_interface_check(aggregate_id, &object_true, emitter);
            abi::emit_pop_reg(emitter, "r10");                                   // discard the saved object pointer after failed interface checks
            emitter.instruction("xor eax, eax");                                // non-Traversable objects do not satisfy is_iterable
            emitter.instruction(&format!("jmp {}", done));                      // skip the truthy assignment
            emitter.label(&object_true);
            abi::emit_pop_reg(emitter, "r10");                                   // discard the saved object pointer before returning true
            emitter.instruction(&format!("jmp {}", true_case));                 // continue through the shared truthy result path
        }
    }
}

/// Emits a single interface-implements check for a previously saved object pointer.
///
/// Reloads the saved object from the stack and calls `__rt_exception_matches` with the given
/// `interface_id`. On success (non-zero result), jumps to `true_case`. This function does not
/// modify the stack pointer; the caller manages push/pop around the two checks.
fn emit_saved_object_interface_check(interface_id: u64, true_case: &str, emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("ldr x0, [sp]");                                // reload the object pointer as matcher argument 1
            abi::emit_load_int_immediate(emitter, "x1", interface_id as i64);
            abi::emit_load_int_immediate(emitter, "x2", 1);
            abi::emit_call_label(emitter, "__rt_exception_matches");            // test whether the object implements this Traversable interface
            emitter.instruction("cmp x0, #0");                                  // did the runtime interface matcher succeed?
            emitter.instruction(&format!("b.ne {}", true_case));                // matching Iterator/IteratorAggregate means is_iterable is true
        }
        Arch::X86_64 => {
            emitter.instruction("mov rdi, QWORD PTR [rsp]");                    // reload the object pointer as matcher argument 1
            abi::emit_load_int_immediate(emitter, "rsi", interface_id as i64);
            abi::emit_load_int_immediate(emitter, "rdx", 1);
            abi::emit_call_label(emitter, "__rt_exception_matches");            // test whether the object implements this Traversable interface
            emitter.instruction("test rax, rax");                               // did the runtime interface matcher succeed?
            emitter.instruction(&format!("jne {}", true_case));                 // matching Iterator/IteratorAggregate means is_iterable is true
        }
    }
}

/// Statically checks whether a named class or interface implements Iterator or IteratorAggregate.
///
/// For classes, checks the `interfaces` list directly. For interfaces, performs a DFS up the
/// parent hierarchy. Returns `false` if the type is unknown or implements neither interface.
fn object_type_implements_iterable(ctx: &Context, type_name: &str) -> bool {
    if ctx.classes.contains_key(type_name) {
        return ctx.classes.get(type_name).is_some_and(|class_info| {
            class_info
                .interfaces
                .iter()
                .any(|name| name == "Iterator" || name == "IteratorAggregate")
        });
    }
    if ctx.interfaces.contains_key(type_name) {
        return interface_extends_interface(ctx, type_name, "Iterator")
            || interface_extends_interface(ctx, type_name, "IteratorAggregate");
    }
    false
}

/// Returns `true` if `interface_name` is or transitively extends `ancestor_name`.
///
/// Uses an iterative DFS with a visited set to avoid cycles. The `interface_name == ancestor_name`
/// check handles the direct-match case before the search loop.
fn interface_extends_interface(ctx: &Context, interface_name: &str, ancestor_name: &str) -> bool {
    if interface_name == ancestor_name {
        return true;
    }
    let mut stack = vec![interface_name.to_string()];
    let mut seen = std::collections::HashSet::new();
    while let Some(current_name) = stack.pop() {
        if !seen.insert(current_name.clone()) {
            continue;
        }
        let Some(interface_info) = ctx.interfaces.get(&current_name) else {
            continue;
        };
        for parent_name in &interface_info.parents {
            if parent_name == ancestor_name {
                return true;
            }
            stack.push(parent_name.clone());
        }
    }
    false
}
