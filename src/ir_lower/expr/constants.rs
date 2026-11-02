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

use crate::ir::{Immediate, Op, Ownership};
use crate::ir_lower::context::{value_ir_type, LoweredValue, LoweringContext};
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Returns whether the program has the run-time constant table and this body may route to it.
///
/// The table's own `defined()` / `constant()` calls are the ordinary ones: routing them back
/// would recurse.
fn dynamic_constant_table(ctx: &LoweringContext<'_, '_>) -> bool {
    let table = crate::dynamic_define_prelude::DYNAMIC_CONSTANTS_NAME;
    (ctx.functions.contains_key(table) || ctx.functions.contains_key(&php_symbol_key(table)))
        && !ctx.builder.function().name.eq_ignore_ascii_case(table)
}

/// Calls the run-time constant table with one operation.
fn call_dynamic_constant_table(
    ctx: &mut LoweringContext<'_, '_>,
    op: i64,
    name: Expr,
    value: Option<Expr>,
    expr: &Expr,
) -> LoweredValue {
    let args = vec![
        Expr::new(ExprKind::IntLiteral(op), expr.span),
        name,
        value.unwrap_or_else(|| Expr::new(ExprKind::Null, expr.span)),
    ];
    super::function_calls::lower_function_call(
        ctx,
        &Name::unqualified(crate::dynamic_define_prelude::DYNAMIC_CONSTANTS_NAME),
        &args,
        expr,
    )
}

/// Routes `define()` with a computed name, and the lookups that may have to see its constants,
/// to the run-time constant table when the program has one.
///
/// `defined('LIT')` and `constant()` with a computed name ask the table only after the compiled
/// constants (the table falls back to the ordinary lookups); a literal name the compiler knows is
/// left to `lower_static_defined_call`.
pub(super) fn lower_dynamic_constant_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    if args.iter().any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. } | ExprKind::Spread(_)))
        || !dynamic_constant_table(ctx)
    {
        return None;
    }
    let key = php_symbol_key(name.as_str().trim_start_matches('\\'));
    let literal = |arg: &Expr| match &arg.kind {
        ExprKind::StringLiteral(value) => Some(value.clone()),
        _ => None,
    };
    match (key.rsplit('\\').next().unwrap_or(""), args) {
        ("define", [constant, value]) if literal(constant).is_none() => Some(
            call_dynamic_constant_table(
                ctx,
                crate::dynamic_define_prelude::OP_DEFINE,
                constant.clone(),
                Some(value.clone()),
                expr,
            ),
        ),
        ("defined", [constant])
            if literal(constant).is_none_or(|known| ctx.constant_value(&known).is_none()) =>
        {
            Some(call_dynamic_constant_table(
                ctx,
                crate::dynamic_define_prelude::OP_DEFINED,
                constant.clone(),
                None,
                expr,
            ))
        }
        ("constant", [constant]) if literal(constant).is_none() => Some(call_dynamic_constant_table(
            ctx,
            crate::dynamic_define_prelude::OP_FETCH,
            constant.clone(),
            None,
            expr,
        )),
        _ => None,
    }
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
    let base = ctx.constants.base();
    let bare = constant_name.trim_start_matches('\\');
    if !base.contains_key(constant_name)
        && !base.contains_key(bare)
        && crate::ir_lower::context::is_runtime_define_name(bare)
    {
        // Only a builtin or an unconditional top-level declaration exists before its code runs.
        // Anything else is a `define()` somewhere -- in a branch, a function, an included file
        // -- and whether it has run is a runtime fact: folding it answered `false` on the second
        // pass through `if (!defined('X')) define('X', ...)`, which then warned and redefined.
        // The runtime `defined()` reads the define's own flag (`lower_defined`).
        return None;
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
/// `mixed`. A literal `Foo::BAR` lowers as the class-constant expression it names, which the
/// checker (`crate::builtins::system::constant`) typed the same way; any other `::` name keeps the
/// runtime call, which reads the class-constant table; a name nothing defines takes
/// `lower_const_ref`'s throwing runtime lookup.
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
    if let Some(access) =
        crate::builtins::literal_class_constant_expr(constant_name, expr.span)
    {
        // `constant('Foo::BAR')` is the `Foo::BAR` expression the checker already typed.
        return Some(super::lower_expr(ctx, &access));
    }
    if constant_name.contains("::") {
        // A class constant or enum case: the runtime lookup reads the class-constant table.
        return None;
    }
    let referenced = Name::unqualified(constant_name.trim_start_matches('\\'));
    // Lowered as the bare constant read it is, so a name a `define()` creates at run time (in a
    // function: php-parser's compatibility tokens) is loaded at the defined value's type: the call
    // expression typed the global's raw int as a `mixed` box.
    let reference = Expr::new(ExprKind::ConstRef(referenced.clone()), expr.span);
    Some(lower_const_ref(ctx, &referenced, &reference))
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
    // php's fallback for an unqualified name in a namespace: the global constant.
    if let Some(global) = name.function_fallback() {
        if let Some((value, php_type)) = ctx.constant_value(global) {
            return lower_constant_value(ctx, value, php_type, expr);
        }
    }
    // Neither form is known at compile time. The runtime lookup uses the GLOBAL form of an
    // unqualified name: `define()` takes a string and so creates global constants, which is how
    // a runtime-loaded polyfill supplies `IDNA_DEFAULT` to namespaced callers.
    // php's `Undefined constant` names the form the source wrote (`App\BARE`), so the throwing
    // lookup below keeps it.
    let written_name = name;
    let name = &name
        .function_fallback()
        .map(Name::unqualified)
        .unwrap_or_else(|| name.clone());
    if let Some(php_type) = crate::types::predefined_constants::target_restricted_type(name.as_str())
    {
        return lower_target_undefined_constant(ctx, name, php_type, expr);
    }
    // A name the compiler cannot resolve may come from a `define()` with a computed name: the
    // run-time table answers it, or falls back to the ordinary lookup (and its `Error`).
    if dynamic_constant_table(ctx) && !ctx.has_eval_barrier() && !ctx.eval_executed() {
        let bare = name.as_str().trim_start_matches('\\').to_string();
        return call_dynamic_constant_table(
            ctx,
            crate::dynamic_define_prelude::OP_FETCH,
            Expr::new(ExprKind::StringLiteral(bare), expr.span),
            None,
            expr,
        );
    }
    // Nothing in the closed world defines the name. When the program loads no PHP at run time,
    // php's read throws `Error: Undefined constant "NAME"`, which the runtime lookup reproduces
    // (and answers, should a dynamic `define()` add it). A file loaded at run time may define it,
    // so that case takes the runtime fetch below instead.
    if !ctx.has_eval_barrier()
        && !ctx.eval_executed()
        && !ctx.can_resolve_runtime_dynamic_functions()
        && crate::types::predefined_constants::is_undefined_constant_read(name.as_str())
        && !crate::ir_lower::context::is_runtime_define_name(name.as_str().trim_start_matches('\\'))
    {
        return lower_target_undefined_constant(ctx, written_name, PhpType::Mixed, expr);
    }
    if ctx.has_eval_barrier() || ctx.eval_executed() || ctx.can_resolve_runtime_dynamic_functions()
    {
        // Barrier-free AOT evals can still define constants dynamically, and so can a file the
        // program includes at run time; the fetch needs the eval context, so make sure its slot
        // exists.
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

/// Lowers a read of a constant php does not define: a catalogued one restricted to other targets
/// (`PHP_WINDOWS_VERSION_BUILD` off Windows), which prescan gave no value, or a name nothing
/// defines (typed `mixed`). The read goes to the runtime `constant()` lookup, which misses and
/// throws php's `Error: Undefined constant "NAME"`.
///
/// The checker still types the name from the catalog, so a guarded platform branch compiles;
/// the result is coerced to that type for the code that follows, which the throw never reaches.
fn lower_target_undefined_constant(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    php_type: PhpType,
    expr: &Expr,
) -> LoweredValue {
    let bare = name.as_str().trim_start_matches('\\');
    let data = ctx.intern_string(bare);
    let name_value = emit_typed_constant(
        ctx,
        Op::ConstStr,
        Some(Immediate::Data(data)),
        PhpType::Str,
        expr,
    );
    let lookup = crate::ir::RuntimeFnId::Constant;
    let value = ctx.emit_value(
        Op::RuntimeCall,
        vec![name_value.value],
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(lookup))),
        PhpType::Mixed,
        lookup.effects(),
        Some(expr.span),
    );
    match php_type {
        PhpType::Int => super::coerce_to_int(ctx, value, expr),
        PhpType::Float => super::coerce_to_float(ctx, value, expr),
        PhpType::Str => super::coerce_to_string(ctx, value, expr),
        _ => value,
    }
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
