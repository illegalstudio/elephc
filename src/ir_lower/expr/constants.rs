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
//! - `constant("NAME")` of a deprecated constant (`DATE_RFC7231`) raises `E_DEPRECATED`
//!   through `emit_constant_deprecation`, which scoped constant reads share.

use crate::ir::{CoreBuiltinOp, Immediate, Op, Ownership, Terminator};
use crate::ir_lower::context::{value_ir_type, LoweredValue, LoweringContext};
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;
use crate::builtins::semantics::BuiltinLoweringContext;

/// Emits php-src's `E_DEPRECATED` for a `MT_RAND_PHP` constant-expression read.
///
/// `report_span` supplies the file and line (a class-constant access reports the access
/// site); `declaration_span` identifies the declaration whose value is evaluated, so a
/// lazily-evaluated class constant notices only once no matter how many sites read it.
pub(crate) fn emit_mt_rand_php_deprecation(
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

/// Records `define("NAME", value)` and prepares its declaration value for runtime storage.
pub(super) fn register_static_define_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
) -> Option<Vec<Expr>> {
    if php_symbol_key(name.as_str().trim_start_matches('\\')) != "define" || args.len() != 2 {
        return None;
    }
    let ExprKind::StringLiteral(constant_name) = &args[0].kind else {
        return None;
    };
    let value = crate::types::constant_binding::bind_value(args[1].clone(), &PhpType::Mixed);
    ctx.register_constant(
        constant_name.clone(),
        value.clone(),
        constant_expr_type(&args[1].kind),
    );
    Some(vec![args[0].clone(), value])
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
    // php-src reports a deprecated constant from the fetch, so `constant()` reports it too;
    // `lower_const_ref` raises it at this call's span.
    Some(lower_const_ref(ctx, &referenced, expr))
}

thread_local! {
    /// Nesting depth of lowerings that re-evaluate an already evaluated constant initializer,
    /// during which deprecated constant reads stay silent.
    static SUPPRESSED_CONSTANT_DEPRECATIONS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Runs `f` with deprecated constant reads silenced, restoring the previous depth on exit.
pub(crate) fn with_constant_deprecations_suppressed<T>(f: impl FnOnce() -> T) -> T {
    /// Restores the suppression depth when the scope exits, including on unwind.
    struct Reset;
    impl Drop for Reset {
        /// Leaves one suppression level.
        fn drop(&mut self) {
            SUPPRESSED_CONSTANT_DEPRECATIONS.with(|depth| depth.set(depth.get() - 1));
        }
    }
    SUPPRESSED_CONSTANT_DEPRECATIONS.with(|depth| depth.set(depth.get() + 1));
    let _reset = Reset;
    f()
}

/// Reports whether deprecated constant reads are silenced at this point of the lowering.
pub(crate) fn constant_deprecations_suppressed() -> bool {
    SUPPRESSED_CONSTANT_DEPRECATIONS.with(|depth| depth.get() > 0)
}

/// Lowers the name resolver's inline `__elephc_builtin_deprecated(...)` marker to `true`
/// while a stored constant value is being re-evaluated.
///
/// `define("FOO", DATE_RFC7231)` stores its argument after the name resolver rewrote the read
/// into `__elephc_builtin_deprecated(message) ? value : value`. The `define()` call reports
/// once; a later `FOO` read re-lowers the stored expression and must stay silent, as php-src
/// only stored the computed value. The marker's message is a literal, so skipping the call
/// loses no side effect.
pub(super) fn lower_silenced_constant_deprecation_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    expr: &Expr,
) -> Option<LoweredValue> {
    if SUPPRESSED_CONSTANT_DEPRECATIONS.with(|depth| depth.get() == 0)
        || php_symbol_key(name.as_str().trim_start_matches('\\')) != "__elephc_builtin_deprecated"
    {
        return None;
    }
    Some(super::emit_bool_literal(ctx, true, Some(expr.span)))
}

/// Emits PHP's `E_DEPRECATED` for a deprecated constant read that reaches lowering.
///
/// A read written inside a declared initializer (class or interface constant value, property
/// default, parameter default) deprecates on its first evaluation only: php-src evaluates such
/// an initializer once and caches it, while elephc lowers it again at every use. A per-site
/// test-and-set flag (`Op::IncludeOnceGuard` on a label derived from the read's span) keeps
/// the notice to the first evaluation. Any other read deprecates every time it runs.
pub(crate) fn emit_constant_read_deprecation(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: Span,
) {
    emit_read_deprecation(ctx, message, span, span);
}

/// Emits the deprecation of a deprecated GLOBAL constant read (`DATE_RFC7231`, `MT_RAND_PHP`)
/// that reaches lowering. Inside an initializer evaluation the frame decides the line and
/// the once flag; otherwise it behaves like [`emit_constant_read_deprecation`].
pub(crate) fn emit_global_constant_read_deprecation(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: Span,
) {
    if let Some(frame) = crate::ir_lower::initializer_evaluation::current_frame() {
        crate::ir_lower::initializer_evaluation::emit_global_read(ctx, &frame, message, span);
        return;
    }
    emit_read_deprecation(ctx, message, span, span);
}

/// Shared body of the read emitters: `span` keys the once flag of a read inside a declared
/// initializer, `line_span` is reported.
pub(crate) fn emit_read_deprecation(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: Span,
    line_span: Span,
) {
    // A read without a source node is one the compiler synthesized, such as a builtin's
    // `SUNFUNCS_RET_STRING` default filling an omitted argument; php-src's internal functions
    // never fetch those constants, so nothing is reported.
    if SUPPRESSED_CONSTANT_DEPRECATIONS.with(|depth| depth.get() > 0) || !span.identifies_a_node() {
        return;
    }
    if !crate::ir_lower::initializer_reads::is_declared_initializer_span(ctx, span) {
        emit_constant_deprecation(ctx, message, line_span);
        return;
    }
    let mut line_span = line_span;
    if let Some((root, scoped)) = crate::ir_lower::initializer_reads::parameter_default_containing(ctx, span) {
        line_span = crate::ir_lower::initializer_reads::parameter_default_line(&root, scoped, span);
        // `RECV_INIT` caches only a value that is not refcounted, so a default building an
        // array or a string is evaluated, and reports, on every omitted call.
        if crate::ir_lower::initializer_reads::parameter_default_is_refcounted(ctx, ctx.current_class.as_deref(), &root) {
            emit_constant_deprecation(ctx, message, line_span);
            return;
        }
    }
    let label = format!(
        "_constant_deprecation_once_{}_{}_{}",
        span.source_id(),
        span.line,
        span.col
    );
    let message = message.to_string();
    crate::ir_lower::initializer_evaluation::emit_until_success(ctx, &label, span, |ctx| {
        emit_constant_deprecation(ctx, &message, line_span)
    });
}

/// Emits PHP's `E_DEPRECATED` for a deprecated constant read at `span`.
///
/// This is the EIR `__elephc_builtin_deprecated(message)` lowers to: the message, level 8192,
/// and the read's file and line go through the ordinary error-handler and reporting path. The
/// name resolver covers reads it can name statically; lowering covers reads whose declaring
/// class is only known from the class hierarchy (`self::`, `parent::`, `static::`, a user
/// subclass) and `constant()`.
pub(crate) fn emit_constant_deprecation(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: crate::span::Span,
) {
    let line = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(i64::from(span.line))),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    );
    emit_constant_deprecation_with_line(ctx, message, span, line, false);
}

/// [`emit_constant_deprecation`] with the reported line already lowered as an `int` value.
/// `caller_file` reports the file the current call was made from, for a line that is the
/// caller's (`DiagnosticCallerLine`).
pub(crate) fn emit_constant_deprecation_with_line(
    ctx: &mut LoweringContext<'_, '_>,
    message: &str,
    span: crate::span::Span,
    line: LoweredValue,
    caller_file: bool,
) {
    let message_data = ctx.intern_string(message);
    let message = ctx.emit_value(
        Op::ConstStr,
        Vec::new(),
        Some(Immediate::Data(message_data)),
        PhpType::Str,
        Op::ConstStr.default_effects(),
        Some(span),
    );
    let level = ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(8_192)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    );
    let file = if caller_file {
        let operation = crate::ir::CoreBuiltinOp::DiagnosticCallerFile;
        ctx.emit_value(Op::CoreBuiltin, Vec::new(), Some(Immediate::I64(operation.as_i64())), PhpType::Str, operation.effects(), Some(span))
    } else {
        // The reporting site's own file: a class constant evaluated through an included
        // file's fetch reports that file, not the one whose code is being lowered.
        let file = crate::source::canonical_span_source_path(span)
            .or_else(|| ctx.source_path().map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string());
        let file_data = ctx.intern_string(&file);
        ctx.emit_value(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(file_data)),
            PhpType::Str,
            Op::ConstStr.default_effects(),
            Some(span),
        )
    };
    let operation = crate::ir::CoreBuiltinOp::BuiltinDeprecated;
    ctx.emit_value(
        Op::CoreBuiltin,
        vec![message.value, level.value, file.value, line.value],
        Some(Immediate::I64(operation.as_i64())),
        PhpType::Bool,
        operation.effects(),
        Some(span),
    );
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
    // Ordinary reads of a deprecated constant were rewritten into an inline deprecation by the
    // name resolver; a read that gets here sits in a constant expression or `constant()`.
    if let Some(message) = crate::ir_lower::initializer_reads::global_read_deprecation(name.as_str()) {
        emit_global_constant_read_deprecation(ctx, message, expr.span);
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
    let eval_possible = ctx.has_eval_barrier()
        || ctx.eval_executed()
        || crate::ir_lower::program_eval::contains_eval();
    if eval_possible {
        // Barrier-free AOT evals can still define constants dynamically; the
        // fetch needs the eval context, so make sure its slot exists. A program
        // that calls `eval()` anywhere also reaches here for constants written in
        // a declared initializer, which is lowered in its own function and never
        // crosses the eval call itself.
        ctx.declare_eval_context_local();
        let data = ctx.intern_global_name(name.as_str());
        if crate::ir_lower::program_eval::has_native_define(name.as_str()) {
            // A compiled `define()` for this name exists somewhere, but finding it is not
            // proof it ran: the eval registry may be the only store that holds the value.
            // Read the native slot through the normal global load and let codegen consult
            // the `define()` flag at run time before choosing between the two stores.
            let native = ctx.emit_value(
                Op::LoadGlobal,
                Vec::new(),
                Some(Immediate::GlobalName(data)),
                super::fallback_expr_type(expr, ctx.semantic_profile()),
                Op::LoadGlobal.default_effects(),
                Some(expr.span),
            );
            return ctx.emit_value(
                Op::EvalConstantFetchOrGlobal,
                vec![native.value],
                Some(Immediate::Data(data)),
                PhpType::Mixed,
                Op::EvalConstantFetchOrGlobal.default_effects(),
                Some(expr.span),
            );
        }
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
        super::fallback_expr_type(expr, ctx.semantic_profile),
        Op::LoadGlobal.default_effects(),
        Some(expr.span),
    )
}

/// Lowers a prescanned constant value using its checker-visible PHP type.
fn lower_constant_value(
    ctx: &mut LoweringContext<'_, '_>,
    value: Expr,
    php_type: PhpType,
    expr: &Expr,
) -> LoweredValue {
    match value.kind {
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
        _ => {
            let synthetic = value;
            // This re-evaluates a user constant's stored initializer at a read. php-src
            // evaluated it once, when the `const` statement or `define()` ran, and reported any
            // deprecated constant inside it then; the read itself reports nothing, including
            // the name resolver's inline marker a `define()` argument carries.
            with_constant_deprecations_suppressed(|| super::lower_expr(ctx, &synthetic))
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
