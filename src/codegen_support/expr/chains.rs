//! Purpose:
//! Lowers chained property, method, and array accesses that share a left-to-right receiver path.
//! Maintains intermediate receiver values while walking nested PHP access expressions.
//!
//! Called from:
//! - `crate::codegen_support::expr::emit_expr()`
//!
//! Key details:
//! - Each chain step must preserve nullability, ownership, and side-effect order for subsequent steps.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::functions;
use crate::codegen_support::platform::Arch;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::{arrays, calls, objects};

/// Represents a flattened left-to-right chain of property, method, array, and callable segments.
/// The `base` is the leftmost receiver; segments are ordered from innermost to outermost access.
struct Chain<'a> {
    /// The leftmost expression in the chain (e.g., `$obj` in `$obj->foo()->bar`).
    base: &'a Expr,
    /// Ordered list of access segments from innermost to outermost.
    segments: Vec<Segment<'a>>,
}

/// A single step in a postfix access chain (property, method, array, or callable).
enum Segment<'a> {
    /// Ordinary (`->property`) or nullsafe (`?->property`) property access.
    Property {
        /// The full expression for this segment.
        expr: &'a Expr,
        /// The receiver expression on the left side of the access.
        receiver: &'a Expr,
        /// The property name as a string.
        property: &'a str,
        /// Whether this is a nullsafe (`?.`) access.
        nullsafe: bool,
    },
    /// Ordinary (`->method()`) or nullsafe (`?->method()`) method call.
    Method {
        /// The full expression for this segment.
        expr: &'a Expr,
        /// The receiver expression on the left side of the call.
        receiver: &'a Expr,
        /// The method name as a string.
        method: &'a str,
        /// The call arguments.
        args: &'a [Expr],
        /// Whether this is a nullsafe (`?.`) call.
        nullsafe: bool,
    },
    /// Array element access (`$arr[$idx]`).
    Array {
        /// The full expression for this segment.
        expr: &'a Expr,
        /// The array expression on the left side of the access.
        array: &'a Expr,
        /// The index expression.
        index: &'a Expr,
    },
    /// Expression-callable invocation (`$fn($args)`).
    ExprCall {
        /// The full expression for this segment.
        expr: &'a Expr,
        /// The callee expression (must resolve to a callable).
        callee: &'a Expr,
        /// The call arguments.
        args: &'a [Expr],
    },
}

/// Emits a postfix chain that contains at least one nullsafe (`?.`) property or method access.
pub(super) fn emit_nullsafe_postfix_chain(
    expr: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    let chain = flatten_postfix_chain(expr)?;
    if !chain.segments.iter().any(Segment::is_nullsafe_member) {
        return None;
    }

    emitter.comment("nullsafe postfix chain");
    let null_label = ctx.next_label("nullsafe_chain_null");
    let done_label = ctx.next_label("nullsafe_chain_done");
    let mut current_ty = super::emit_expr(chain.base, emitter, ctx, data);
    let mut chain_can_short_circuit = false;
    let mut normal_path_terminated = false;

    for segment in chain.segments {
        if normal_path_terminated {
            break;
        }
        match segment {
            Segment::Property {
                expr,
                receiver,
                property,
                nullsafe,
            } => {
                let outcome = emit_property_segment(
                    expr,
                    receiver,
                    property,
                    nullsafe,
                    &current_ty,
                    &null_label,
                    emitter,
                    ctx,
                    data,
                );
                current_ty = outcome.ty;
                chain_can_short_circuit |= outcome.can_short_circuit;
                normal_path_terminated |= outcome.terminated_normal_path;
            }
            Segment::Method {
                expr,
                receiver,
                method,
                args,
                nullsafe,
            } => {
                let outcome = emit_method_segment(
                    expr,
                    receiver,
                    method,
                    args,
                    nullsafe,
                    &current_ty,
                    &null_label,
                    emitter,
                    ctx,
                    data,
                );
                current_ty = outcome.ty;
                chain_can_short_circuit |= outcome.can_short_circuit;
                normal_path_terminated |= outcome.terminated_normal_path;
            }
            Segment::Array { expr, array, index } => {
                let _ = (expr, array);
                current_ty = arrays::emit_array_access_with_loaded_base(
                    &current_ty,
                    index,
                    emitter,
                    ctx,
                    data,
                    true,
                );
            }
            Segment::ExprCall { expr, callee, args } => {
                let _ = expr;
                current_ty =
                    calls::emit_loaded_expr_call(callee, args, &current_ty, emitter, ctx, data);
            }
        }
    }

    if chain_can_short_circuit {
        objects::box_nullable_result(&current_ty, emitter);
        abi::emit_jump(emitter, &done_label);
        emitter.label(&null_label);
        objects::emit_boxed_null(emitter);
        emitter.label(&done_label);
        Some(PhpType::Mixed)
    } else {
        Some(current_ty)
    }
}

/// Outcome of emitting a single chain segment, used to track type, short-circuiting, and control flow.
struct SegmentOutcome {
    /// The PHP type produced by this segment.
    ty: PhpType,
    /// Whether this segment can short-circuit (null-propagate) the chain.
    can_short_circuit: bool,
    /// Whether the normal (non-null) path is guaranteed to terminate (e.g., always returns null).
    terminated_normal_path: bool,
}

impl SegmentOutcome {
    /// Creates a normal (non-terminating) outcome with the given type and short-circuit flag.
    fn normal(ty: PhpType, can_short_circuit: bool) -> Self {
        Self {
            ty,
            can_short_circuit,
            terminated_normal_path: false,
        }
    }

    /// Creates an outcome indicating the normal path always yields null.
    /// The chain will short-circuit and `terminated_normal_path` is set to true.
    fn always_null() -> Self {
        Self {
            ty: PhpType::Void,
            can_short_circuit: true,
            terminated_normal_path: true,
        }
    }
}

impl Segment<'_> {
    /// Returns true if this segment is a nullsafe property or method access.
    fn is_nullsafe_member(&self) -> bool {
        matches!(
            self,
            Segment::Property {
                nullsafe: true,
                ..
            } | Segment::Method {
                nullsafe: true,
                ..
            }
        )
    }
}

/// Flattens a postfix expression tree into a `Chain` by walking from the outermost access
/// inward, collecting property, method, array, and callable segments. Returns `None` if the
/// expression contains no postfix accesses. Segments are stored in reverse order (innermost
/// first) and reversed after the walk so they are ordered from base to outermost.
fn flatten_postfix_chain(expr: &Expr) -> Option<Chain<'_>> {
    let mut base = expr;
    let mut segments = Vec::new();

    loop {
        match &base.kind {
            ExprKind::PropertyAccess { object, property } => {
                segments.push(Segment::Property {
                    expr: base,
                    receiver: object,
                    property,
                    nullsafe: false,
                });
                base = object;
            }
            ExprKind::NullsafePropertyAccess { object, property } => {
                segments.push(Segment::Property {
                    expr: base,
                    receiver: object,
                    property,
                    nullsafe: true,
                });
                base = object;
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                segments.push(Segment::Method {
                    expr: base,
                    receiver: object,
                    method,
                    args,
                    nullsafe: false,
                });
                base = object;
            }
            ExprKind::NullsafeMethodCall {
                object,
                method,
                args,
            } => {
                segments.push(Segment::Method {
                    expr: base,
                    receiver: object,
                    method,
                    args,
                    nullsafe: true,
                });
                base = object;
            }
            ExprKind::ArrayAccess { array, index } => {
                segments.push(Segment::Array {
                    expr: base,
                    array,
                    index,
                });
                base = array;
            }
            ExprKind::ExprCall { callee, args } => {
                segments.push(Segment::ExprCall {
                    expr: base,
                    callee,
                    args,
                });
                base = callee;
            }
            _ => break,
        }
    }

    if segments.is_empty() {
        return None;
    }

    segments.reverse();
    Some(Chain { base, segments })
}

/// Emits a single property-access segment, handling both ordinary and nullsafe (`?.`) forms.
/// Updates the current type, short-circuit flag, and normal-path termination flag in the
/// returned `SegmentOutcome`. May emit a jump to `null_label` for nullsafe chains when the
/// receiver is statically null or the access fails at runtime.
fn emit_property_segment(
    expr: &Expr,
    receiver: &Expr,
    property: &str,
    nullsafe: bool,
    current_ty: &PhpType,
    null_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> SegmentOutcome {
    let receiver_ty = functions::infer_contextual_type(receiver, ctx);
    if nullsafe {
        if receiver_is_static_null(current_ty, &receiver_ty) {
            abi::emit_jump(emitter, null_label);
            return SegmentOutcome::always_null();
        }
        let Some(class_name) = singular_receiver_class(&receiver_ty, current_ty) else {
            if matches!(current_ty.codegen_repr(), PhpType::Mixed) {
                let ty = objects::emit_mixed_property_access(property, emitter, ctx, data);
                return SegmentOutcome::normal(ty, false);
            }
            emitter.comment("WARNING: nullsafe property access on non-object");
            abi::emit_jump(emitter, null_label);
            return SegmentOutcome::always_null();
        };
        let can_short_circuit =
            emit_nullsafe_receiver_check(current_ty, null_label, emitter);
        let property_ty = objects::emit_loaded_object_property_access(
            &class_name,
            property,
            emitter,
            ctx,
            data,
        );
        return SegmentOutcome::normal(property_ty, can_short_circuit);
    }

    let Some(class_name) = singular_receiver_class(&receiver_ty, current_ty) else {
        if matches!(current_ty.codegen_repr(), PhpType::Mixed) {
            let ty = objects::emit_mixed_property_access(property, emitter, ctx, data);
            return SegmentOutcome::normal(ty, false);
        }
        emitter.comment("WARNING: property access on non-object");
        return SegmentOutcome::normal(functions::infer_contextual_type(expr, ctx), false);
    };
    if matches!(current_ty.codegen_repr(), PhpType::Mixed) {
        let ty = objects::emit_nullable_object_property_access(
            &class_name,
            property,
            emitter,
            ctx,
            data,
        );
        SegmentOutcome::normal(ty, false)
    } else {
        let ty = objects::emit_loaded_object_property_access(
            &class_name,
            property,
            emitter,
            ctx,
            data,
        );
        SegmentOutcome::normal(ty, false)
    }
}

/// Emits a single method-call segment, handling both ordinary and nullsafe (`?.`) forms.
/// On nullsafe calls with a statically-null receiver, jumps to `null_label`. For ordinary
/// calls on `Mixed` types, emits a fatal error if the receiver is null. Returns the method's
/// return type and short-circuit flag in `SegmentOutcome`.
fn emit_method_segment(
    expr: &Expr,
    receiver: &Expr,
    method: &str,
    args: &[Expr],
    nullsafe: bool,
    current_ty: &PhpType,
    null_label: &str,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> SegmentOutcome {
    let receiver_ty = functions::infer_contextual_type(receiver, ctx);
    if nullsafe {
        if receiver_is_static_null(current_ty, &receiver_ty) {
            abi::emit_jump(emitter, null_label);
            return SegmentOutcome::always_null();
        }
        let Some(class_name) = singular_receiver_class(&receiver_ty, current_ty) else {
            emitter.comment("WARNING: nullsafe method call on non-object");
            abi::emit_jump(emitter, null_label);
            return SegmentOutcome::always_null();
        };
        let can_short_circuit =
            emit_nullsafe_receiver_check(current_ty, null_label, emitter);
        let return_ty =
            emit_loaded_method_call(&class_name, method, args, emitter, ctx, data);
        return SegmentOutcome::normal(return_ty, can_short_circuit);
    }

    let Some(class_name) = singular_receiver_class(&receiver_ty, current_ty) else {
        emitter.comment("WARNING: method call on non-object");
        return SegmentOutcome::normal(functions::infer_contextual_type(expr, ctx), false);
    };
    if matches!(current_ty.codegen_repr(), PhpType::Mixed) {
        let message = format!(
            "Fatal error: Call to a member function {}() on null\n",
            method
        );
        objects::emit_unbox_mixed_object_or_fatal(message.as_bytes(), emitter, ctx, data);
    }
    let return_ty = emit_loaded_method_call(&class_name, method, args, emitter, ctx, data);
    SegmentOutcome::normal(return_ty, false)
}

/// Emits a method call where the receiver is already loaded on the ABI's result register.
/// Saves the receiver register before emitting arguments, then dispatches to the method.
/// Falls back to `__call` if the method is not defined, passing the original method name as
/// the first magic argument. Returns the method's return type.
fn emit_loaded_method_call(
    class_name: &str,
    method: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    abi::emit_push_reg(emitter, abi::int_result_reg(emitter));                 // save the loaded receiver below later method arguments
    let method_key = php_symbol_key(method);
    let mut dispatch_method = method_key.as_str();
    let mut magic_args = None;
    let sig = ctx.classes.get(class_name).and_then(|class_info| {
        if let Some(sig) = class_info.methods.get(&method_key) {
            return Some(sig.clone());
        }
        if let Some(sig) = class_info.methods.get("__call") {
            dispatch_method = "__call";
            let span = args
                .first()
                .map(|arg| arg.span)
                .unwrap_or_else(crate::span::Span::dummy);
            magic_args = Some(objects::magic_method_args(method, args, span));
            return Some(sig.clone());
        }
        None
    });
    let args_to_emit = magic_args.as_deref().unwrap_or(args);
    let emitted_args =
        objects::emit_pushed_method_args(args_to_emit, sig.as_ref(), emitter, ctx, data);
    objects::emit_method_call_with_saved_receiver_below_args(
        class_name,
        dispatch_method,
        &emitted_args.arg_types,
        emitted_args.source_temp_bytes,
        emitter,
        ctx,
    )
}

/// Emits a runtime null check for a nullsafe chain's receiver. Unboxes the receiver if it is
/// `Mixed`, compares it against the null tag, and jumps to `null_label` if it is null. Returns
/// `true` if a short-circuit jump was emitted; `false` if the type cannot be null at this point.
fn emit_nullsafe_receiver_check(
    current_ty: &PhpType,
    null_label: &str,
    emitter: &mut Emitter,
) -> bool {
    match current_ty.codegen_repr() {
        PhpType::Void => {
            abi::emit_jump(emitter, null_label);
            true
        }
        PhpType::Mixed => {
            abi::emit_call_label(emitter, "__rt_mixed_unbox");                  // unwrap nullable receiver before the nullsafe chain segment
            match emitter.target.arch {
                Arch::AArch64 => {
                    emitter.instruction("cmp x0, #8");                          // runtime tag 8 means the receiver is null
                    emitter.instruction(&format!("b.eq {}", null_label));       // short-circuit the remaining postfix chain on null
                    emitter.instruction("mov x0, x1");                          // move the unboxed object pointer into the normal result register
                }
                Arch::X86_64 => {
                    emitter.instruction("cmp rax, 8");                          // runtime tag 8 means the receiver is null
                    emitter.instruction(&format!("je {}", null_label));         // short-circuit the remaining postfix chain on null
                    emitter.instruction("mov rax, rdi");                        // move the unboxed object pointer into the normal result register
                }
            }
            true
        }
        _ => false,
    }
}

/// Returns true if either the current runtime type or the statically-inferred receiver type
/// is `Void` (PHP null). Used to determine if a nullsafe chain can short-circuit at compile
/// time without emitting a runtime null check.
fn receiver_is_static_null(current_ty: &PhpType, receiver_ty: &PhpType) -> bool {
    matches!(current_ty.codegen_repr(), PhpType::Void) || matches!(receiver_ty, PhpType::Void)
}

/// Attempts to resolve the receiver class name from either the statically-inferred receiver
/// type or the current runtime type. Tries `receiver_ty` first, then falls back to `current_ty`.
/// Returns the class name as an `Option<String>`, or `None` if the type cannot be resolved to a
/// single concrete class (e.g., `Mixed` or a union type).
fn singular_receiver_class(receiver_ty: &PhpType, current_ty: &PhpType) -> Option<String> {
    functions::singular_object_class(receiver_ty)
        .or_else(|| functions::singular_object_class(current_ty))
        .map(str::to_string)
}
