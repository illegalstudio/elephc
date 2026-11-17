//! Purpose:
//! Raises the deprecations php-src reports when a reflection getter evaluates a stored
//! constant expression: `ReflectionClassConstant::getValue()`, `ReflectionClass::getConstant()`
//! and `getConstants()`, and `ReflectionProperty::getDefaultValue()`.
//!
//! Called from:
//! - `crate::ir_lower::reflection` marks the synthetic reflection method being lowered.
//! - `crate::ir_lower::function` emits the marked prologue before that method's body.
//!
//! Key details:
//! - The reflectors carry values folded at construction, so the notices come from a prologue
//!   that dispatches on the reflected class and member names at run time. They report at the
//!   PHP line that called the getter (`DiagnosticCallerLine`), as php-src reports a notice
//!   raised inside an internal method.
//! - `getValue()`, `getConstant()`, and `getConstants()` are first evaluations of class
//!   constants, behind the same once flags as an ordinary read. `getDefaultValue()` evaluates
//!   a copy every call: a static default always, an instance default only until the class is
//!   initialized (php-src then answers from the evaluated default table).

use std::cell::Cell;

use crate::ir::{Immediate, Op, Ownership, Terminator};
use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::ir_lower::initializer_evaluation::{InitializerFrame, LineSource};
use crate::ir_lower::initializer_reads as reads;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::PhpType;

/// A reflection getter whose synthetic body starts with a deprecation prologue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReflectionPrologue {
    /// `ReflectionClassConstant::getValue()` and the enum case reflectors' `getValue()`.
    ConstantValue,
    /// `ReflectionClass::getConstant()` and `getConstants()`, which evaluate every constant
    /// of the reflected class's table.
    ClassConstants,
    /// `ReflectionProperty::getDefaultValue()`.
    PropertyDefault,
}

impl ReflectionPrologue {
    /// Returns the prologue of builtin method `method_key` (lowercase) of `class_name`.
    pub(crate) fn for_method(class_name: &str, method_key: &str) -> Option<Self> {
        match (class_name, method_key) {
            ("ReflectionClassConstant" | "ReflectionEnumUnitCase" | "ReflectionEnumBackedCase", "getvalue") => {
                Some(Self::ConstantValue)
            }
            ("ReflectionClass" | "ReflectionObject" | "ReflectionEnum", "getconstant" | "getconstants") => {
                Some(Self::ClassConstants)
            }
            ("ReflectionProperty", "getdefaultvalue") => Some(Self::PropertyDefault),
            _ => None,
        }
    }
}

thread_local! {
    /// The prologue of the synthetic method whose body is about to be lowered.
    static PENDING: Cell<Option<ReflectionPrologue>> = const { Cell::new(None) };
}

/// Runs `lower` (the lowering of one synthetic reflection method) with `prologue` pending for
/// its body.
pub(crate) fn with_prologue<T>(prologue: Option<ReflectionPrologue>, lower: impl FnOnce() -> T) -> T {
    /// Clears a prologue the body never consumed.
    struct Clear;
    impl Drop for Clear {
        /// Drops the pending prologue when the method lowering ends.
        fn drop(&mut self) {
            PENDING.with(|pending| pending.set(None));
        }
    }
    PENDING.with(|pending| pending.set(prologue));
    let _clear = Clear;
    lower()
}

/// Emits the pending prologue, if any, at the start of the body being lowered.
pub(crate) fn emit_pending_prologue(ctx: &mut LoweringContext<'_, '_>) {
    let Some(prologue) = PENDING.with(|pending| pending.take()) else { return };
    let span = Span::dummy();
    match prologue {
        ReflectionPrologue::ConstantValue => emit_constant_value(ctx, span),
        ReflectionPrologue::ClassConstants => emit_class_constants(ctx, span),
        ReflectionPrologue::PropertyDefault => emit_property_default(ctx, span),
    }
}

/// `getValue()`: the first evaluation of the reflected constant, held by the reflector's
/// declaring class (`$this->class`).
fn emit_constant_value(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    let mut holders: Vec<(String, Vec<String>)> = Vec::new();
    let mut class_names: Vec<&String> = ctx.classes.keys().collect();
    class_names.sort();
    for class_name in class_names {
        let class = &ctx.classes[class_name];
        let names = constants_reaching(ctx, class_name, class.constants.keys());
        if !names.is_empty() {
            holders.push((class_name.clone(), names));
        }
    }
    let mut interface_names: Vec<&String> = ctx.interfaces.keys().collect();
    interface_names.sort();
    for interface_name in interface_names {
        let interface = &ctx.interfaces[interface_name];
        let own = interface.constants.keys().filter(|name| {
            interface.constant_declaring_interfaces.get(*name).is_none_or(|owner| owner == interface_name)
        });
        let names = constants_reaching(ctx, interface_name, own);
        if !names.is_empty() {
            holders.push((interface_name.clone(), names));
        }
    }
    if holders.is_empty() {
        return;
    }
    let class_value = this_property(ctx, "class", span);
    let options: Vec<String> = holders.iter().map(|(holder, _)| holder.clone()).collect();
    crate::ir_lower::property_default_errors::dispatch_string(ctx, class_value, span, &options, true, &|ctx, holder| {
        let Some((_, names)) = holders.iter().find(|(candidate, _)| candidate == holder) else { return };
        let name_value = this_property(ctx, "name", span);
        crate::ir_lower::property_default_errors::dispatch_string(ctx, name_value, span, names, false, &|ctx, name| {
            reads::emit_class_constant_evaluation(ctx, holder, name, LineSource::Caller);
        });
        release(ctx, name_value, span);
    });
    release(ctx, class_value, span);
}

/// Returns the names among `names` of constants held by `holder` whose initializer reads a
/// deprecated constant, sorted.
fn constants_reaching<'a>(
    ctx: &LoweringContext<'_, '_>,
    holder: &str,
    names: impl Iterator<Item = &'a String>,
) -> Vec<String> {
    let mut reaching: Vec<String> = names
        .filter(|name| {
            ctx.scoped_constant_value(holder, name)
                .is_some_and(|value| reads::reaches_deprecated_read_in(ctx, Some(holder), &value))
        })
        .cloned()
        .collect();
    reaching.sort();
    reaching
}

/// `getConstant()` / `getConstants()`: the first evaluation of every constant in the
/// reflected class's table, in table order.
fn emit_class_constants(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    let mut options: Vec<String> = ctx
        .classes
        .keys()
        .chain(ctx.interfaces.keys())
        .filter(|class_like| reads::constant_table_reports(ctx, class_like))
        .cloned()
        .collect();
    if options.is_empty() {
        return;
    }
    options.sort();
    let class_value = this_property(ctx, "__name", span);
    crate::ir_lower::property_default_errors::dispatch_string(ctx, class_value, span, &options, true, &|ctx, class_like| {
        reads::emit_constant_table_evaluation(ctx, class_like, LineSource::Caller);
    });
    release(ctx, class_value, span);
}

/// `ReflectionProperty::getDefaultValue()`: a fresh evaluation of the declared default, every
/// call for a static property and until the declaring class is initialized for an instance
/// property.
fn emit_property_default(ctx: &mut LoweringContext<'_, '_>, span: Span) {
    let mut declarers: Vec<(String, Vec<(String, bool, Expr)>)> = Vec::new();
    let mut class_names: Vec<&String> = ctx.classes.keys().collect();
    class_names.sort();
    for class_name in class_names {
        let defaults = declared_property_defaults(ctx, class_name);
        if !defaults.is_empty() {
            declarers.push((class_name.clone(), defaults));
        }
    }
    if declarers.is_empty() {
        return;
    }
    let options: Vec<String> = declarers.iter().map(|(class, _)| class.clone()).collect();
    let class_value = this_property(ctx, "class", span);
    crate::ir_lower::property_default_errors::dispatch_string(ctx, class_value, span, &options, true, &|ctx, class| {
        let Some((_, defaults)) = declarers.iter().find(|(candidate, _)| candidate == class) else { return };
        let names: Vec<String> = defaults.iter().map(|(name, _, _)| name.clone()).collect();
        let name_value = this_property(ctx, "name", span);
        crate::ir_lower::property_default_errors::dispatch_string(ctx, name_value, span, &names, false, &|ctx, name| {
            let Some((_, is_static, default)) = defaults.iter().find(|(candidate, _, _)| candidate == name) else {
                return;
            };
            let lone = reads::lone_global_read(ctx, Some(class), default);
            let frame = InitializerFrame::top(true, LineSource::Caller, lone, None);
            if *is_static {
                reads::emit_initializer_reads(ctx, default, Some(class), &frame);
                return;
            }
            let Some(label) = crate::ir_lower::property_default_errors::initialization_label(ctx, class) else {
                return;
            };
            emit_unless_flag_set(ctx, &label, span, |ctx| {
                reads::emit_initializer_reads(ctx, default, Some(class), &frame);
            });
        });
        release(ctx, name_value, span);
    });
    release(ctx, class_value, span);
}

/// Returns the properties `class_name` declares itself whose default reads a deprecated
/// constant: (name, is static, default), sorted by name.
fn declared_property_defaults(ctx: &LoweringContext<'_, '_>, class_name: &str) -> Vec<(String, bool, Expr)> {
    let class = &ctx.classes[class_name];
    let declares = |declaring: Option<&String>| declaring.is_none_or(|declaring| declaring.eq_ignore_ascii_case(class_name));
    let instance = class
        .properties
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| declares(class.property_declaring_classes.get(name)))
        .filter_map(|(index, (name, _))| {
            class.defaults.get(index).cloned().flatten().map(|default| (name.clone(), false, default))
        });
    let statics = class
        .static_properties
        .iter()
        .enumerate()
        .filter(|(_, (name, _))| declares(class.static_property_declaring_classes.get(name)))
        .filter_map(|(index, (name, _))| {
            class.static_defaults.get(index).cloned().flatten().map(|default| (name.clone(), true, default))
        });
    let mut defaults: Vec<(String, bool, Expr)> = instance
        .chain(statics)
        .filter(|(_, _, default)| reads::reaches_deprecated_read_in(ctx, Some(class_name), default))
        .collect();
    defaults.sort_by(|left, right| left.0.cmp(&right.0));
    defaults.dedup_by(|left, right| left.0 == right.0);
    defaults
}

/// Runs `body` only while the program-wide once flag `label` is still clear.
fn emit_unless_flag_set(
    ctx: &mut LoweringContext<'_, '_>,
    label: &str,
    span: Span,
    body: impl FnOnce(&mut LoweringContext<'_, '_>),
) {
    let data = ctx.intern_string(label);
    let set = ctx
        .builder
        .emit_with_effects(
            Op::IncludeOnceTest,
            Vec::new(),
            Some(Immediate::Data(data)),
            crate::ir::IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
            Op::IncludeOnceTest.default_effects(),
            Some(span),
        )
        .expect("a once-flag test produces a branch condition");
    let clear = ctx.builder.create_named_block("once.clear", Vec::new());
    let after = ctx.builder.create_named_block("once.after", Vec::new());
    ctx.builder.terminate(Terminator::CondBr {
        cond: set,
        then_target: after,
        then_args: Vec::new(),
        else_target: clear,
        else_args: Vec::new(),
    });
    ctx.builder.position_at_end(clear);
    body(ctx);
    ctx.builder.terminate(Terminator::Br { target: after, args: Vec::new() });
    ctx.builder.position_at_end(after);
}

/// Releases a prologue read once its dispatch no longer uses it.
fn release(ctx: &mut LoweringContext<'_, '_>, value: LoweredValue, span: Span) {
    if ctx.value_needs_release_after_use(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, Some(span));
    }
}

/// Lowers a read of `$this->property` inside the synthetic method.
fn this_property(ctx: &mut LoweringContext<'_, '_>, property: &str, span: Span) -> LoweredValue {
    crate::ir_lower::expr::lower_expr(
        ctx,
        &Expr::new(
            ExprKind::PropertyAccess { object: Box::new(Expr::new(ExprKind::This, span)), property: property.to_string() },
            span,
        ),
    )
}
