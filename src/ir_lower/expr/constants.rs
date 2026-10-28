//! Purpose:
//! Lowers PHP global constant references and static constant-introspection calls
//! encountered while converting AST expressions into EIR.
//!
//! Called from:
//! - `crate::ir_lower::expr::lower_expr()` and the function-call lowering path.
//!
//! Key details:
//! - `define("NAME", value)` updates the per-function lowering context in source
//!   order so later `ConstRef` expressions can keep precise PHP metadata.
//! - Literal `defined("NAME")` folds using global constants plus scope-aware
//!   `Class::CONST` existence from class/interface/enum metadata.

use crate::ir::{CoreBuiltinOp, Immediate, Op, Ownership, Terminator};
use crate::span::Span;
use crate::ir_lower::context::{value_ir_type, LoweredValue, LoweringContext};
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits php-src's `E_DEPRECATED` for a `MT_RAND_PHP` constant-expression read.
///
/// `report_span` supplies the file and line (a class-constant access reports the access
/// site); `declaration_span` identifies the declaration whose value is evaluated, so a
/// lazily-evaluated class constant notices only once no matter how many sites read it.
pub(super) fn emit_mt_rand_php_deprecation(
    ctx: &mut LoweringContext<'_, '_>,
    report_span: Span,
    declaration_span: Span,
) {
    // The flag is keyed by the DECLARATION span, not the reported one: every access site of a
    // class constant shares one notice even though each reports its own line.
    let first = ctx.emit_value(
        Op::CoreBuiltin,
        Vec::new(),
        Some(Immediate::I64(CoreBuiltinOp::DeprecatedOnceBegin.as_i64())),
        PhpType::Bool,
        CoreBuiltinOp::DeprecatedOnceBegin.effects(),
        Some(declaration_span),
    );
    let deprecate = ctx.builder.create_named_block("mt_rand_php_deprecation", Vec::new());
    let done = ctx
        .builder
        .create_named_block("mt_rand_php_deprecation_done", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: first.value,
        then_target: deprecate,
        then_args: Vec::new(),
        else_target: done,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(deprecate);
    let message_data = ctx.intern_string(
        "Constant MT_RAND_PHP is deprecated since 8.3, as it uses a biased non-standard variant of Mt19937",
    );
    let message = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(message_data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(report_span),
    );
    let level = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(8_192)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(report_span),
    );
    let file_text = crate::source::canonical_span_source_path(report_span)
        .or_else(|| ctx.source_path().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string());
    let file_data = ctx.intern_string(&file_text);
    let file = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(file_data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(report_span),
    );
    let line = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(i64::from(report_span.line))),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(report_span),
    );
    ctx.emit_void(
        Op::CoreBuiltin,
        vec![message.value, level.value, file.value, line.value],
        Some(Immediate::I64(CoreBuiltinOp::BuiltinDeprecated.as_i64())),
        CoreBuiltinOp::BuiltinDeprecated.effects(),
        Some(report_span),
    );
    ctx.builder.terminate(Terminator::Br {
        target: done,
        args: Vec::new(),
    });
    ctx.builder.position_at_end(done);
}

/// Records `define("NAME", value)` constants for later source-order references.
pub(super) fn register_static_define_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
) {
    if php_symbol_key(name.as_str().trim_start_matches('\\')) != "define" || args.len() != 2 {
        return;
    }
    let ExprKind::StringLiteral(constant_name) = &args[0].kind else {
        return;
    };
    ctx.register_constant(
        constant_name.clone(),
        args[1].kind.clone(),
        constant_expr_type(&args[1].kind),
    );
}

/// Lowers `defined("NAME")` to a compile-time boolean when the name is literal.
///
/// Global names use prescanned `define()`/`const` metadata. `Class::CONST`
/// strings also succeed when the member is visible from the current lexical
/// class scope, including inherited and implemented-interface constants.
/// Relative receivers without a valid class or parent scope throw a catchable
/// PHP `Error`.
pub(super) fn lower_static_defined_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.as_str().trim_start_matches('\\')) != "defined" || args.len() != 1 {
        return None;
    }
    let constant_name = static_defined_name_arg(&args[0])?;
    if let Some(message) = crate::types::class_like_constant_scope_error(
        ctx.classes,
        constant_name,
        ctx.current_class.as_deref(),
    ) {
        return Some(crate::ir_lower::stmt::lower_throw_access_error_expr(
            ctx,
            &message,
            expr.span,
        ));
    }
    if let Some(member_name) = crate::types::defined_late_static_member(constant_name) {
        return Some(super::scoped_values::lower_late_static_defined(
            ctx,
            member_name,
            expr,
        ));
    }
    let exists = literal_constant_is_defined(ctx, constant_name);
    if !exists && (ctx.has_eval_barrier() || ctx.eval_executed()) {
        // Barrier-free AOT evals can still define constants dynamically; the
        // probe needs the eval context, so make sure its slot exists.
        ctx.declare_eval_context_local();
        let data = ctx.intern_global_name(constant_name);
        return Some(ctx.emit_value(
            Op::EvalConstantExists,
            Vec::new(),
            Some(Immediate::Data(data)),
            PhpType::Bool,
            Op::EvalConstantExists.default_effects(),
            Some(expr.span),
        ));
    }
    Some(emit_typed_constant(
        ctx,
        Op::ConstBool,
        Some(Immediate::Bool(exists)),
        PhpType::Bool,
        expr,
    ))
}

/// Returns the literal name from positional or PHP-named `defined()` syntax.
fn static_defined_name_arg(arg: &Expr) -> Option<&str> {
    match &arg.kind {
        ExprKind::StringLiteral(value) => Some(value.as_str()),
        ExprKind::NamedArg { name, value } if name == "constant_name" => match &value.kind {
            ExprKind::StringLiteral(value) => Some(value.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// Returns true when a literal `defined()` name exists as a global or class-like constant.
fn literal_constant_is_defined(ctx: &LoweringContext<'_, '_>, constant_name: &str) -> bool {
    ctx.constant_value(constant_name).is_some()
        || crate::types::class_like_constant_is_defined(
            ctx.classes,
            ctx.interfaces,
            ctx.enums,
            constant_name,
            ctx.current_class.as_deref(),
        )
}

/// Lowers `constant("NAME")` to exactly the EIR a bare `NAME` reference produces.
///
/// PHP's `constant()` performs a GLOBAL constant lookup: the name is already fully qualified,
/// so no namespace/`use` resolution applies and a leading `\` is stripped. Reusing
/// [`lower_const_ref`] means the call inherits the constant's real type and value — including
/// the prescanned `define()` metadata recorded earlier in source order — instead of an opaque
/// `mixed`. The checker (`crate::builtins::system::constant`) has already refused a dynamic
/// name, a `Foo::BAR` class constant, and an unknown constant, so this hook only ever sees a
/// resolvable global name.
pub(super) fn lower_static_constant_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if php_symbol_key(name.as_str().trim_start_matches('\\')) != "constant" || args.len() != 1 {
        return None;
    }
    let constant_name = static_constant_name_arg(&args[0])?;
    let referenced = Name::unqualified(constant_name.trim_start_matches('\\'));
    Some(lower_const_ref(ctx, &referenced, expr))
}

/// Returns the literal constant name from `constant()`'s single argument.
///
/// Accepts the positional form and the `name:` named-argument form; every other shape (a
/// runtime string, a spread) yields `None` so the caller falls back to the registry path,
/// which the checker has already rejected.
fn static_constant_name_arg(arg: &Expr) -> Option<&str> {
    match &arg.kind {
        ExprKind::StringLiteral(value) => Some(value.as_str()),
        ExprKind::NamedArg { name, value } if name == "name" => match &value.kind {
            ExprKind::StringLiteral(value) => Some(value.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// Lowers a constant reference through prescanned metadata or global storage fallback.
pub(super) fn lower_const_ref(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    expr: &Expr,
) -> LoweredValue {
    if elephc_builtin_contract::canonical_constant_name(name.as_str()) == "MT_RAND_PHP" {
        // The resolver rewrites an ordinary read into a call, so a surviving `ConstRef` is a
        // constant-expression read: php-src evaluates those lazily at runtime, once per
        // declaration site. Emit that deprecation here instead of at compile time.
        let report_span = ctx.const_deprecation_report_span.unwrap_or(expr.span);
        emit_mt_rand_php_deprecation(ctx, report_span, expr.span);
    }
    if elephc_builtin_contract::canonical_constant_name(name.as_str()) == "PHP_BINARY" {
        return ctx.emit_value(
            Op::RuntimeCall,
            Vec::new(),
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ExecutablePath)),
            PhpType::Str,
            crate::ir::Effects::READS_PROCESS | crate::ir::Effects::ALLOC_HEAP,
            Some(expr.span),
        );
    }
    if let Some((value, php_type)) = ctx.constant_value(name.as_str()) {
        return lower_constant_value(ctx, value, php_type, expr);
    }
    if ctx.has_eval_barrier() || ctx.eval_executed() {
        // Barrier-free AOT evals can still define constants dynamically; the
        // fetch needs the eval context, so make sure its slot exists.
        ctx.declare_eval_context_local();
        let data = ctx.intern_global_name(name.as_str());
        return ctx.emit_value(
            Op::EvalConstantFetch,
            Vec::new(),
            Some(Immediate::Data(data)),
            PhpType::Mixed,
            Op::EvalConstantFetch.default_effects(),
            Some(expr.span),
        );
    }
    let data = ctx.intern_global_name(name.as_str());
    ctx.emit_value(
        Op::LoadGlobal,
        Vec::new(),
        Some(Immediate::GlobalName(data)),
        super::fallback_expr_type(expr),
        Op::LoadGlobal.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a prescanned constant value using its checker-visible PHP type.
fn lower_constant_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: ExprKind,
    php_type: PhpType,
    expr: &Expr,
) -> LoweredValue {
    match value {
        ExprKind::IntLiteral(value) => emit_typed_constant(
            ctx,
            Op::ConstI64,
            Some(Immediate::I64(value)),
            php_type,
            expr,
        ),
        ExprKind::FloatLiteral(value) => emit_typed_constant(
            ctx,
            Op::ConstF64,
            Some(Immediate::F64(value)),
            php_type,
            expr,
        ),
        ExprKind::StringLiteral(value) => {
            let data = ctx.intern_string(&value);
            emit_typed_constant(ctx, Op::ConstStr, Some(Immediate::Data(data)), php_type, expr)
        }
        ExprKind::BoolLiteral(value) => emit_typed_constant(
            ctx,
            Op::ConstBool,
            Some(Immediate::Bool(value)),
            php_type,
            expr,
        ),
        ExprKind::Null => emit_typed_constant(ctx, Op::ConstNull, None, php_type, expr),
        other => {
            let synthetic = Expr::new(other, expr.span);
            super::lower_expr(ctx, &synthetic)
        }
    }
}

/// Emits a literal constant opcode with caller-supplied PHP metadata.
fn emit_typed_constant(
    ctx: &mut LoweringContext<'_, '_>,
    op: Op,
    immediate: Option<Immediate>,
    php_type: PhpType,
    expr: &Expr,
) -> LoweredValue {
    let ir_type = value_ir_type(&php_type);
    let value = ctx
        .builder
        .emit_with_effects(
            op,
            Vec::new(),
            immediate,
            ir_type,
            php_type.clone(),
            Ownership::for_php_type(&php_type),
            op.default_effects(),
            Some(expr.span),
        )
        .expect("constant opcode produces a value");
    LoweredValue { value, ir_type }
}

/// Returns the PHP type used for a compile-time constant expression.
fn constant_expr_type(kind: &ExprKind) -> PhpType {
    match kind {
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::BoolLiteral(false) => PhpType::False,
        ExprKind::BoolLiteral(true) => PhpType::Bool,
        ExprKind::Null => PhpType::Void,
        _ => PhpType::Int,
    }
}
