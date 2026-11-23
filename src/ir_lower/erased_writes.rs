//! Purpose:
//! Outlines the receiver-dependent part of a property write whose receiver has lost its class
//! (`mixed`, a union, an interface or a bare `object`) into one shared synthetic function per
//! (form, property name, visibility-equivalent scope, source profile).
//!
//! Called from:
//! - `crate::ir_lower::stmt::property_array_writes`, once the receiver, the key and the value of
//!   `$o->p[$k] = $v`, `$o->p[$k] op= $v` and `$o->p[] = $v` are evaluated.
//! - `crate::ir_lower::program` materializes the requested bodies after the source bodies.
//!
//! Key details:
//! - The inline lowering of such a write is a ladder over every class that declares the name or
//!   a `__set`/`__get`/`ArrayAccess` protocol, plus a native XML arm, repeated at every site:
//!   Symfony's dumped container writes `$container->privates['id'] = ...` hundreds of times, each
//!   in its own service class, and lowering reached 40 GB. The helper holds that same ladder
//!   ONCE. Its body is the ordinary statement over three `mixed` parameters, lowered by the
//!   ordinary path, so typed-slot guards, readonly and asymmetric-visibility refusals, `__set`
//!   reentrancy, COW and ownership are the code the inline sites used.
//! - The site keeps everything that is evaluated in source order: the receiver, the key and the
//!   value, and the receiver's delayed null check. A bare-variable key is read again at call time,
//!   as the inline path reads it late. The site lends borrowed boxes of its operands, releases them
//!   after the call and keeps its own roots, so an unwinding helper leaks nothing.
//! - Scopes share a helper when they see the property alike: `named_helper_scope_groups_in`, the
//!   grouping the codegen named property helpers use (a name no class declares non-public, nor
//!   readonly or with an asymmetric `set`, is read alike from every scope; related scopes of a
//!   protected or private declaration share one helper).
//! - Locations. The body carries compiler-built spans, so an instruction in it publishes nothing:
//!   a warning or an `Error` it raises names the file and line the CALL site published, which is
//!   the PHP statement. The function has no source identity, so no backtrace frame is published.
//! - Left inline: an expression `++`/`--` of an element (its old and new values are published into
//!   caller locals), compound or `??=` updates of a plain property, nested dimensions
//!   (`$o->p[a][b] = $v`), static properties, `unset()`, values held in buffer, pointer or packed
//!   storage, a body with a runtime closure scope, a name written inside its own property hook,
//!   and the helper bodies themselves.
//! - `ELEPHC_NO_ERASED_WRITE_HELPERS=1` keeps the inline ladders, for size comparisons.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::codegen::NamedHelperScopes;
use crate::fast_hash::FastMap;
use crate::ir::{Immediate, Module, Op};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

/// Prefix of every outlined write's function name.
const PREFIX: &str = crate::names::ERASED_WRITE_HELPER_PREFIX;
/// Receiver parameter of a helper.
const OBJECT: &str = "o";
/// Key parameter of a helper.
const KEY: &str = "k";
/// Key parameter of a helper whose write half already diagnosed the key.
pub(super) const DIAGNOSED_KEY: &str = "dk";
/// Value parameter of a helper.
const VALUE: &str = "v";

/// What a helper writes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Form {
    /// `$o->p[$k] = $v`, with the key already diagnosed by a read half when `diagnosed`.
    Index { diagnosed: bool },
    /// `$o->p[$k] op= $v`: the element is read after the value and written back.
    Compound(String),
    /// `$o->p[] = $v`.
    Append,
    /// `$o->p = $v`.
    Assign,
    /// A statement `$o->p[$k]++` (`--` when false): the element is read, stepped and written back.
    IncDec { increment: bool },
}

impl Form {
    /// Whether the write names an element key.
    fn has_key(&self) -> bool { !matches!(self, Form::Append | Form::Assign) }
    /// Whether the write carries a value operand.
    fn has_value(&self) -> bool { !matches!(self, Form::IncDec { .. }) }
}

/// How a key or value operand crosses into a helper: scalars and strings keep their type, which
/// keeps the typed write paths of the inline lowering; anything else is boxed as `mixed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Repr { Int, Float, Bool, Str, Mixed }

impl Repr {
    /// The representation an operand is handed over in.
    fn of(ctx: &LoweringContext<'_, '_>, value: LoweredValue) -> Self {
        match ctx.builder.value_php_type(value.value).codegen_repr() {
            PhpType::Int => Repr::Int,
            PhpType::Float => Repr::Float,
            PhpType::Bool => Repr::Bool,
            PhpType::Str => Repr::Str,
            _ => Repr::Mixed,
        }
    }

    /// The letter naming this representation in a helper's function name.
    fn tag(self) -> &'static str {
        match self { Repr::Int => "i", Repr::Float => "f", Repr::Bool => "b", Repr::Str => "s", Repr::Mixed => "m" }
    }

    /// The parameter type.
    fn php_type(self) -> PhpType {
        match self {
            Repr::Int => PhpType::Int, Repr::Float => PhpType::Float, Repr::Bool => PhpType::Bool,
            Repr::Str => PhpType::Str, Repr::Mixed => PhpType::Mixed,
        }
    }

    /// The declared type of the parameter.
    fn type_expr(self) -> crate::parser::ast::TypeExpr {
        use crate::parser::ast::TypeExpr;
        match self {
            Repr::Int => TypeExpr::Int, Repr::Float => TypeExpr::Float, Repr::Bool => TypeExpr::Bool,
            Repr::Str => TypeExpr::Str, Repr::Mixed => crate::synthetic_class::t_mixed(),
        }
    }
}

/// One helper to materialize.
#[derive(Clone)]
struct Request {
    form: Form,
    key: Option<Repr>,
    value: Option<Repr>,
    op: Option<BinOp>,
    property: String,
    scope: Option<String>,
    /// Whether the caller's class id travels as a trailing operand (a program that can run eval).
    runtime_scope: bool,
    mode: crate::source::SourceMode,
    strict_types: bool,
}

thread_local! {
    /// Helpers requested by the bodies lowered so far, by function name.
    static REQUESTS: RefCell<BTreeMap<String, Request>> = const { RefCell::new(BTreeMap::new()) };
    /// Scope groups of the property names written so far, by name.
    static GROUPS: RefCell<FastMap<String, Rc<NamedHelperScopes>>> = RefCell::new(FastMap::default());
}

/// Forgets the requests and scope groups of the previous program.
pub(super) fn reset() {
    REQUESTS.with(|requests| requests.borrow_mut().clear());
    GROUPS.with(|groups| groups.borrow_mut().clear());
}

/// Environment switch that keeps the inline ladders.
fn disabled() -> bool {
    std::env::var_os("ELEPHC_NO_ERASED_WRITE_HELPERS").is_some()
}

/// Whether the key of a helper statement is the marker of an already diagnosed key.
pub(super) fn key_is_diagnosed_marker(index: &Expr) -> bool {
    matches!(&index.kind, ExprKind::Variable(name) if name == DIAGNOSED_KEY)
}

/// Whether `expr` names the key or value parameter of the helper being lowered.
///
/// The helper's caller holds those operands for the whole call, and nothing the write runs can
/// rebind a parameter local, so lowering such an operand needs no copy and no unwind record.
pub(super) fn lends_parameter(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> bool {
    inside_helper(ctx)
        && matches!(&expr.kind, ExprKind::Variable(name)
            if name == KEY || name == DIAGNOSED_KEY || name == VALUE)
}

/// Whether the body being lowered is one of the helpers.
fn inside_helper(ctx: &LoweringContext<'_, '_>) -> bool {
    ctx.builder.function().name.starts_with(PREFIX)
}

/// Whether a receiver of this type has lost its class.
fn erased(ctx: &LoweringContext<'_, '_>, ty: &PhpType) -> bool {
    match ty.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) => true,
        PhpType::Object(class) => class.is_empty() || ctx.interfaces.contains_key(&class),
        _ => false,
    }
}

/// Whether a stored operand can be handed over as a `mixed` box.
fn boxable(ctx: &LoweringContext<'_, '_>, value: LoweredValue) -> bool {
    !matches!(
        ctx.builder.value_php_type(value.value).codegen_repr(),
        PhpType::Buffer(_) | PhpType::Pointer(_) | PhpType::Packed(_)
    )
}

/// The scope a helper for `property` is built in, `None` for the module-wide helper.
fn helper_scope(ctx: &LoweringContext<'_, '_>, property: &str) -> Option<String> {
    let groups = GROUPS.with(|groups| {
        if let Some(found) = groups.borrow().get(property) {
            return Rc::clone(found);
        }
        let built = Rc::new(crate::codegen::named_helper_scope_groups_in(
            ctx.classes,
            false,
            property,
            true,
        ));
        groups.borrow_mut().insert(property.to_string(), Rc::clone(&built));
        built
    });
    let lexical = ctx.current_class.as_deref().map(|class| class.trim_start_matches('\\'));
    groups.scope_of(lexical, ctx.classes)
}

/// Spells a property name so it can sit inside a function name. Function names are matched
/// without regard to case and property names are not, so an upper-case letter is spelled in hex.
fn escape(property: &str) -> String {
    property
        .chars()
        .map(|c| if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' { c.to_string() } else { format!("x{:x}_", c as u32) })
        .collect()
}

/// Routes `$receiver->property[$key] = $value` (or the append, with no `key`) through the shared
/// helper. Returns `false`, having emitted nothing, when this site keeps the inline ladder.
///
/// A bare-variable key is read by the caller at call time (`with_late_property_key`), as the
/// inline path reads it late.
pub(super) fn eligible(
    ctx: &LoweringContext<'_, '_>, receiver: LoweredValue, form: &Form,
    property: &str, key: Option<LoweredValue>, value: Option<LoweredValue>,
) -> bool {
    !(disabled()
        || inside_helper(ctx)
        || property.starts_with("__elephc_")
        || ctx.has_local_slot(crate::names::CLOSURE_SCOPE_LOCAL)
        || ctx.in_own_property_accessor(property)
        || !erased(ctx, &ctx.builder.value_php_type(receiver.value))
        || value.is_some() != form.has_value()
        || value.is_some_and(|value| !boxable(ctx, value))
        || key.is_some_and(|key| !boxable(ctx, key))
        || key.is_some() != form.has_key())
}

/// Emits the call of an [`eligible`] write.
pub(super) fn call(
    ctx: &mut LoweringContext<'_, '_>, receiver: LoweredValue, form: Form, op: Option<&BinOp>,
    property: &str, key: Option<LoweredValue>, value: Option<LoweredValue>, span: Span,
) {
    let scope = helper_scope(ctx, property);
    // With eval linked the helper is still shared by scope group: the eval bridge needs the real
    // scope, which travels as one more operand and is resolved by the property scope gate.
    let runtime_scope = ctx.program_loads_code_at_run_time();
    let key_repr = key.map(|key| Repr::of(ctx, key));
    let value_repr = value.map(|value| Repr::of(ctx, value));
    let mode = crate::source::current_parse_mode();
    let strict_types = ctx.php_strict_types;
    let name = format!(
        "{PREFIX}{}{}{}{}@{}@{}@{:?}@{}",
        match &form {
            Form::Index { diagnosed: false } => "set".to_string(),
            Form::Index { diagnosed: true } => "setd".to_string(),
            Form::Compound(op) => format!("op{op}"),
            Form::Append => "push".to_string(),
            Form::Assign => "assign".to_string(),
            Form::IncDec { increment } => if *increment { "inc" } else { "dec" }.to_string(),
        },
        key_repr.map_or("", Repr::tag),
        value_repr.map_or("", Repr::tag),
        if runtime_scope { "r" } else { "" },
        scope.as_deref().unwrap_or(""),
        u8::from(strict_types),
        mode,
        escape(property),
    );
    REQUESTS.with(|requests| {
        requests.borrow_mut().entry(name.clone()).or_insert_with(|| Request {
            form: form.clone(),
            key: key_repr,
            value: value_repr,
            op: op.cloned(),
            property: property.to_string(),
            scope,
            runtime_scope,
            mode,
            strict_types,
        });
    });
    let callee = ctx.intern_function_name(&name);
    let mut boxes = Vec::new();
    let mut operands = Vec::new();
    let mut lend = |ctx: &mut LoweringContext<'_, '_>, operand: LoweredValue, repr: Repr| {
        let ty = ctx.builder.value_php_type(operand.value);
        // Boxing an owning temporary would consume it; its roots stay with the caller.
        let view = if crate::ir::Ownership::php_type_needs_lifetime_tracking(&ty) {
            let view = ctx.emit_value(Op::Borrow, vec![operand.value], None, ty,
                Op::Borrow.default_effects(), Some(span));
            ctx.builder.set_value_ownership(view.value, crate::ir::Ownership::Borrowed);
            view
        } else { operand };
        // A scalar or string the helper is typed for is lent as it is.
        if repr != Repr::Mixed {
            operands.push(view.value);
            return;
        }
        // A box the caller already holds is lent as it is: the helper only borrows its operands.
        if view.ir_type == crate::ir::IrType::Heap(crate::ir::IrHeapKind::Mixed) {
            operands.push(view.value);
            return;
        }
        let boxed = ctx.box_value_as_mixed(view, PhpType::Mixed, Some(span));
        boxes.push(boxed);
        operands.push(boxed.value);
    };
    lend(ctx, receiver, Repr::Mixed);
    if let (Some(key), Some(repr)) = (key, key_repr) { lend(ctx, key, repr); }
    if let (Some(value), Some(repr)) = (value, value_repr) { lend(ctx, value, repr); }
    if runtime_scope {
        // The caller's class id plus one; zero names the helper's own lexical scope.
        let id = ctx.current_class.as_deref().map(|class| class.trim_start_matches('\\'))
            .and_then(|class| ctx.classes.get(class)).map_or(0, |info| info.class_id as i64 + 1);
        let id = ctx.emit_value(Op::ConstI64, vec![], Some(Immediate::I64(id)), PhpType::Int,
            Op::ConstI64.default_effects(), Some(span));
        operands.push(id.value);
    }
    let owned: Vec<_> = boxes.iter().map(|boxed| boxed.value).collect();
    let pins = super::expr::pin_in_flight_owners(ctx, &owned, span);
    ctx.emit_void(Op::Call, operands, Some(Immediate::Data(callee)), Op::Call.default_effects(), Some(span));
    super::expr::unpin_in_flight_owners(ctx, pins, span);
    for boxed in boxes.into_iter().rev() {
        super::expr::release_binary_operand_temporary(ctx, boxed, span);
    }
}

/// The signature every helper has: `(mixed $o, mixed $k, mixed $v): void`, without `$k` for an append.
fn signature(request: &Request) -> FunctionSig {
    let form = &request.form;
    let mut params = vec![(OBJECT, Repr::Mixed)];
    if let Some(repr) = request.key {
        params.push((if *form == (Form::Index { diagnosed: true }) { DIAGNOSED_KEY } else { KEY }, repr));
    }
    if let Some(repr) = request.value { params.push((VALUE, repr)); }
    if request.runtime_scope { params.push((crate::names::ERASED_WRITE_SCOPE_LOCAL, Repr::Int)); }
    let count = params.len();
    FunctionSig {
        param_type_exprs: params.iter().map(|(_, repr)| Some(repr.type_expr())).collect(),
        params: params.into_iter().map(|(name, repr)| (name.to_string(), repr.php_type())).collect(),
        param_attributes: vec![Vec::new(); count],
        defaults: vec![None; count],
        return_type: PhpType::Void,
        declared_return: true,
        by_ref_return: false,
        ref_params: vec![false; count],
        declared_params: vec![true; count],
        variadic: None,
        deprecation: None,
        is_generator: false,
    }
}

/// The one statement a helper holds.
fn statement(request: &Request) -> Stmt {
    let span = Span::dummy();
    let variable = |name: &str| Expr::new(ExprKind::Variable(name.to_string()), span);
    let object = || Box::new(variable(OBJECT));
    let kind = match &request.form {
        Form::Assign => StmtKind::PropertyAssign {
            object: object(), property: request.property.clone(), value: variable(VALUE),
        },
        Form::Append => StmtKind::PropertyArrayPush {
            object: object(), property: request.property.clone(), value: variable(VALUE),
        },
        Form::Index { diagnosed } => StmtKind::PropertyArrayAssign {
            object: object(), property: request.property.clone(),
            index: variable(if *diagnosed { DIAGNOSED_KEY } else { KEY }), value: variable(VALUE),
        },
        Form::IncDec { increment } => {
            // The parser's statement kernel: the element is read into `w`, which is stepped in place.
            let element = Expr::new(ExprKind::ArrayAccess {
                array: Box::new(Expr::new(ExprKind::PropertyAccess {
                    object: object(), property: request.property.clone(),
                }, span)),
                index: Box::new(variable(KEY)),
            }, span);
            let working = crate::names::generated_local_name("__elephc_erased_incdec_work");
            let step = if *increment { ExprKind::PreIncrement(working.clone()) } else { ExprKind::PreDecrement(working.clone()) };
            let update = Expr::new(ExprKind::Assignment {
                target: Box::new(variable(&crate::names::generated_local_name("__elephc_erased_incdec_next"))),
                value: Box::new(Expr::new(step, span)),
                result_target: None,
                prelude: vec![Stmt::new(StmtKind::Assign { name: working, value: element }, span)],
                conditional_value_temp: None,
            }, span);
            StmtKind::PropertyArrayAssign {
                object: object(), property: request.property.clone(), index: variable(KEY), value: update,
            }
        }
        Form::Compound(_) => {
            let element = Expr::new(ExprKind::ArrayAccess {
                array: Box::new(Expr::new(ExprKind::PropertyAccess {
                    object: object(), property: request.property.clone(),
                }, span)),
                index: Box::new(variable(KEY)),
            }, span);
            let update = Expr::new(ExprKind::BinaryOp {
                left: Box::new(element),
                op: request.op.clone().expect("a compound helper carries its operator"),
                right: Box::new(variable(VALUE)),
            }, span);
            StmtKind::PropertyArrayAssign {
                object: object(), property: request.property.clone(), index: variable(KEY), value: update,
            }
        }
    };
    let mut statement = Stmt::new(kind, span);
    statement.source_mode = request.mode;
    statement.strict_types = request.strict_types;
    statement
}

/// Materializes the helpers the lowered bodies called.
pub(super) fn lower_requested(module: &mut Module) {
    let requests = REQUESTS.with(|requests| std::mem::take(&mut *requests.borrow_mut()));
    for (name, request) in requests {
        let signature = signature(&request);
        let mut function = super::function::lower_synthetic_body_in_scope(
            module, &name, &signature, request.scope.as_deref(), vec![statement(&request)],
        );
        function.add_local(Some(crate::names::ERASED_WRITE_SITE_LOCAL.to_string()), crate::ir::IrType::I64,
            PhpType::Int, crate::ir::LocalKind::PhpLocal);
        function.flags.is_internal = true;
        function.flags.is_synthetic = true;
        module.add_function(function);
    }
}
