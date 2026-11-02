//! Purpose:
//! Lowers `++`/`--` of one scalar `buffer<T>` element as a typed element read, a typed add or
//! subtract, and a typed element store, without staging the value in boxed `Mixed` cells.
//!
//! Called from:
//! - `crate::ir_lower::stmt::array_write_core` for statement updates of local buffers.
//! - `crate::ir_lower::stmt::property_array_writes` for statement updates of buffer properties.
//! - `crate::ir_lower::expr::assignments::lower_assignment_expr()` for expression updates.
//!
//! Key details:
//! - The parser writes a statement `$b[$k]++;` as `$b[$k] = (new = ++(old = $b[$k]))` over
//!   generated locals, and an expression `$b[$k]++` as `old = $b[$k]; new = old + 1;
//!   $b[$k] = new`. Lowered generically, the working local is `mixed` (an int `++` may promote
//!   to float), so a statement boxes the int seed, runs `StrIncDec` and copies the result with
//!   `MixedClone`, and an expression boxes `new` through `ICheckedAdd`.
//! - A scalar buffer element is always an int or a float, so PHP's string and null increment
//!   rules cannot apply. An int element (any integer width) steps with `ICheckedAddToInt` or
//!   `ICheckedSubToInt` and the cast immediate: in range it is a plain add, and an overflow
//!   promotes to float and converts back with PHP's float-to-int rule, which is the conversion
//!   the `BufferSet` of an int buffer applied to the promoted float before. A float element
//!   (`f64` or `f32`) adds or subtracts `1.0`, and the store rounds an `f32` as any write does.
//! - Only the arithmetic staging changes. The element read, the key, the receiver and the
//!   `BufferSet` keep the ordinary write paths, so evaluation order, bounds and stale-handle
//!   fatals and the width conversion of the store are unchanged. A statement reads the element
//!   where its value would be lowered, after the write's receiver; an expression reads and steps
//!   it where its prelude did, before the write loads its handle or receiver. The generated
//!   locals are not written: a statement never reads them, and an expression returns the typed
//!   values.
//! - A prefix expression on an int element still yields a `mixed`, because its value is PHP's
//!   sum, which is a float past `PHP_INT_MAX`. Only that expression value is boxed, after the
//!   store, which does not read it.

use super::*;

/// Scalar class of a `buffer<T>` element that `++`/`--` steps in registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScalarBufferElement {
    /// An integer element of any storage width, read and stepped as a PHP `int`.
    Int,
    /// An `f64` or `f32` element, read and stepped as a PHP `float`.
    Float,
}

/// Returns the scalar class of a `buffer<T>` type's elements, or `None` for any other type.
fn scalar_buffer_element(ty: &PhpType) -> Option<ScalarBufferElement> {
    let PhpType::Buffer(element) = ty.codegen_repr() else {
        return None;
    };
    match element.storage_value_type().codegen_repr() {
        PhpType::Int => Some(ScalarBufferElement::Int),
        PhpType::Float => Some(ScalarBufferElement::Float),
        _ => None,
    }
}

/// One `++`/`--` of a scalar buffer element: the element read and the step direction.
#[derive(Clone, Copy)]
pub(super) struct BufferIncDec<'a> {
    /// The element read the update observes, `$b[$k]` or `$o->p[$k]` with the write's key.
    read: &'a Expr,
    /// The scalar class of the buffer's elements.
    element: ScalarBufferElement,
    /// `true` for `++`, `false` for `--`.
    increment: bool,
}

/// The typed values of one buffer element update.
#[derive(Clone, Copy)]
pub(super) struct BufferIncDecValues {
    /// The element value the update read, which a postfix expression yields.
    pub(super) old: LoweredValue,
    /// The stepped value the update stores, in the element's scalar representation.
    pub(super) stored: LoweredValue,
}

impl BufferIncDec<'_> {
    /// Reads the element once and steps it by one in its scalar representation.
    pub(super) fn lower_values(
        &self,
        ctx: &mut LoweringContext<'_, '_>,
        span: Span,
    ) -> BufferIncDecValues {
        let read = lower_expr(ctx, self.read);
        let old = coerce_to_element(ctx, read, self.element, Some(self.read.span));
        let stored = lower_step(ctx, old, self.element, self.increment, span);
        BufferIncDecValues { old, stored }
    }
}

/// Recognizes a statement `++`/`--` that a loaded local `buffer` applies in registers.
pub(super) fn statement_update<'a>(
    ctx: &LoweringContext<'_, '_>,
    buffer: LoweredValue,
    value: &'a Expr,
    increment: bool,
) -> Option<BufferIncDec<'a>> {
    if buffer.ir_type != IrType::Heap(crate::ir::IrHeapKind::Buffer) {
        return None;
    }
    let element = scalar_buffer_element(&ctx.builder.value_php_type(buffer.value))?;
    Some(BufferIncDec { read: statement_read(value)?, element, increment })
}

/// Recognizes a statement `++`/`--` of a lowered receiver's scalar buffer property.
///
/// The property type comes from the same lookup that selects the property write's `BufferSet`.
pub(super) fn property_statement_update<'a>(
    ctx: &LoweringContext<'_, '_>,
    receiver: LoweredValue,
    property: &str,
    value: &'a Expr,
    increment: bool,
) -> Option<BufferIncDec<'a>> {
    let element = object_property_type(ctx, receiver.value, property)
        .as_ref()
        .and_then(scalar_buffer_element)?;
    Some(BufferIncDec { read: statement_read(value)?, element, increment })
}

/// Returns the element read of a parser statement `++`/`--` value.
///
/// `assignment_value` builds `new = ++old` (or `--old`) with the single prelude statement
/// `old = <element read>`, and `desugared_element_update` has matched that read to the write.
/// The two-statement form with a working copy belongs to nested expression updates, which read
/// their generated locals afterwards, so it is declined.
fn statement_read(value: &Expr) -> Option<&Expr> {
    let ExprKind::Assignment { prelude, result_target: None, .. } = &value.kind else {
        return None;
    };
    let [Stmt { kind: StmtKind::Assign { value: read, .. }, .. }] = prelude.as_slice() else {
        return None;
    };
    Some(read)
}

/// Stores a statement `++`/`--` into a loaded local buffer, as `lower_array_assign` stores a value.
///
/// The update's element read runs where the written value would, in `lower_write_key_and_value`'s
/// order with the key, and the stepped value goes through the ordinary `BufferSet`.
pub(super) fn lower_local_update(
    ctx: &mut LoweringContext<'_, '_>,
    buffer: LoweredValue,
    index: &Expr,
    update: &BufferIncDec<'_>,
    span: Span,
) {
    let (index_value, stored, index_pins) =
        lower_write_key_then(ctx, index, |ctx| update.lower_values(ctx, span).stored);
    store_local_element(ctx, buffer, index, index_value, stored, index_pins, span);
}

/// Stores an already stepped value into a loaded local buffer, keying it as any buffer write.
fn lower_local_store(
    ctx: &mut LoweringContext<'_, '_>,
    buffer: LoweredValue,
    index: &Expr,
    stored: LoweredValue,
    span: Span,
) {
    let (index_value, stored, index_pins) = lower_write_key_then(ctx, index, |_| stored);
    store_local_element(ctx, buffer, index, index_value, stored, index_pins, span);
}

/// Converts the key and the stepped value as the `BufferSet` path of an array write does.
fn store_local_element(
    ctx: &mut LoweringContext<'_, '_>,
    buffer: LoweredValue,
    index: &Expr,
    index_value: LoweredValue,
    stored: LoweredValue,
    index_pins: Vec<crate::ir_lower::expr::PinnedInFlightOwner>,
    span: Span,
) {
    crate::ir_lower::expr::unpin_in_flight_owners(ctx, index_pins, index.span);
    let index_value = coerce_to_int_at_span(ctx, index_value, Some(index.span));
    let buffer_ty = ctx.builder.value_php_type(buffer.value);
    let stored = coerce_buffer_set_value(ctx, &buffer_ty, stored, Some(span));
    lower_concrete_array_set_values(ctx, buffer, index_value, stored, Vec::new(), span, true);
}

/// Converts an element read to its scalar class; a typed `BufferGet` result passes unchanged.
fn coerce_to_element(
    ctx: &mut LoweringContext<'_, '_>,
    value: LoweredValue,
    element: ScalarBufferElement,
    span: Option<Span>,
) -> LoweredValue {
    let coerced = match element {
        ScalarBufferElement::Int => coerce_to_int(ctx, value, span),
        ScalarBufferElement::Float => coerce_to_float(ctx, value, span),
    };
    if coerced.value != value.value && ctx.value_is_owning_temporary(value) {
        crate::ir_lower::ownership::release_if_owned(ctx, value, span);
    }
    coerced
}

/// Adds or subtracts one from a scalar element value in its own representation.
///
/// The int step carries the cast immediate, so an overflow converts PHP's promoted float back
/// with the PHP float-to-int rule instead of stopping the program.
fn lower_step(
    ctx: &mut LoweringContext<'_, '_>,
    old: LoweredValue,
    element: ScalarBufferElement,
    increment: bool,
    span: Span,
) -> LoweredValue {
    match element {
        ScalarBufferElement::Int => {
            let one = lower_int_one(ctx, span);
            let op = if increment { Op::ICheckedAddToInt } else { Op::ICheckedSubToInt };
            ctx.emit_value(
                op,
                vec![old.value, one.value],
                Some(Immediate::Bool(true)),
                PhpType::Int,
                op.default_effects(),
                Some(span),
            )
        }
        ScalarBufferElement::Float => {
            let one = ctx.emit_value(
                Op::ConstF64,
                Vec::new(),
                Some(Immediate::F64(1.0)),
                PhpType::Float,
                Op::ConstF64.default_effects(),
                Some(span),
            );
            let op = if increment { Op::FAdd } else { Op::FSub };
            ctx.emit_value(
                op,
                vec![old.value, one.value],
                None,
                PhpType::Float,
                op.default_effects(),
                Some(span),
            )
        }
    }
}

/// Emits the integer constant one an int element step adds or subtracts.
fn lower_int_one(ctx: &mut LoweringContext<'_, '_>, span: Span) -> LoweredValue {
    ctx.emit_value(
        Op::ConstI64,
        Vec::new(),
        Some(Immediate::I64(1)),
        PhpType::Int,
        Op::ConstI64.default_effects(),
        Some(span),
    )
}

/// The receiver of an expression `++`/`--` of a one-dimensional element.
enum ExpressionRoot<'a> {
    /// `$name[$k]`.
    Local(&'a str),
    /// `$this->property[$k]` or `$name->property[$k]`, where `object` is the receiver expression.
    Property { object: &'a Expr, property: &'a str },
}

/// A parser-desugared expression `++`/`--` of one element that may live in a scalar buffer.
pub(crate) struct BufferExpressionUpdate<'a> {
    /// The receiver whose type decides whether the typed path applies.
    root: ExpressionRoot<'a>,
    /// The stabilized key shared by the read and the write.
    index: &'a Expr,
    /// The element read the prelude binds to the old value.
    read: &'a Expr,
    /// The assignment's value, which the typed path does not lower.
    value: &'a Expr,
    /// The prelude position of the element read, after every stabilizing statement.
    read_position: usize,
    /// `true` for `++`, `false` for `--`.
    increment: bool,
    /// Whether the expression yields the old value (`$b[$k]++`) or the new one (`++$b[$k]`).
    postfix: bool,
}

impl<'a> BufferExpressionUpdate<'a> {
    /// Matches the one-dimensional local or property shape of `desugar_lvalue_incdec`.
    ///
    /// The prelude ends with `old = <target>; new = old ± 1`, the value is `new` and the result
    /// is `old` (postfix) or `new` (prefix), all generated by the parser with the expression's
    /// own span. Recognition only reads the AST; the buffer decision waits for the receiver's
    /// type at the element read.
    pub(crate) fn recognize(
        target: &'a Expr,
        value: &'a Expr,
        result_target: Option<&'a Expr>,
        prelude: &'a [Stmt],
        conditional_value_temp: Option<&str>,
        span: Span,
    ) -> Option<Self> {
        if conditional_value_temp.is_some() {
            return None;
        }
        let ExprKind::ArrayAccess { array, index } = &target.kind else {
            return None;
        };
        let root = match &array.kind {
            ExprKind::Variable(name) => ExpressionRoot::Local(name),
            ExprKind::PropertyAccess { object, property }
                if matches!(object.kind, ExprKind::This | ExprKind::Variable(_)) =>
            {
                ExpressionRoot::Property { object, property }
            }
            _ => return None,
        };
        let ExprKind::Variable(new_name) = &value.kind else {
            return None;
        };
        let ExprKind::Variable(result) = &result_target?.kind else {
            return None;
        };
        let read_position = prelude.len().checked_sub(2)?;
        let [old_stmt, new_stmt] = &prelude[read_position..] else {
            return None;
        };
        if old_stmt.span != span || new_stmt.span != span {
            return None;
        }
        let StmtKind::Assign { name: old_name, value: read } = &old_stmt.kind else {
            return None;
        };
        let StmtKind::Assign { name: stepped, value: step } = &new_stmt.kind else {
            return None;
        };
        let ExprKind::BinaryOp { left, op, right } = &step.kind else {
            return None;
        };
        let increment = match op {
            crate::parser::ast::BinOp::Add => true,
            crate::parser::ast::BinOp::Sub => false,
            _ => return None,
        };
        if read != target
            || stepped != new_name
            || !matches!(&left.kind, ExprKind::Variable(name) if name == old_name)
            || !matches!(right.kind, ExprKind::IntLiteral(1))
            || !crate::names::is_generated_local_name(old_name)
            || !crate::names::is_generated_local_name(new_name)
        {
            return None;
        }
        let postfix = if result == old_name {
            true
        } else if result == new_name {
            false
        } else {
            return None;
        };
        Some(Self { root, index, read, value, read_position, increment, postfix })
    }

    /// Returns the prelude position of the element read, where the typed path takes over.
    pub(crate) fn read_position(&self) -> usize {
        self.read_position
    }
}

/// Lowers a recognized expression update once its stabilizing prelude statements have run.
///
/// Returns `None`, having emitted nothing, when the receiver is not a scalar buffer; the caller
/// then lowers the element read, the step, the write and the result as written. The decision
/// reads the type `load_local` gives the receiver, which is what the write lowering sees.
///
/// The element read and the step run first, where the prelude ran them, and only then does the
/// ordinary write load its handle or receiver. A get hook or warning handler that runs during the
/// read and rebinds the receiver is therefore seen by the write exactly as before.
pub(crate) fn lower_buffer_expression_update(
    ctx: &mut LoweringContext<'_, '_>,
    update: &BufferExpressionUpdate<'_>,
    span: Span,
) -> Option<LoweredValue> {
    let element = match update.root {
        ExpressionRoot::Local(name) => scalar_buffer_element(&ctx.local_load_php_type(name)?)?,
        ExpressionRoot::Property { object, property } => {
            let receiver = match &object.kind {
                ExprKind::This => "this",
                ExprKind::Variable(name) => name.as_str(),
                _ => return None,
            };
            let receiver_ty = ctx.local_load_php_type(receiver)?;
            // A null receiver reads null with a warning whose handler may bind the object the
            // write then uses, and the expression yields that null; only a definite object reads
            // the typed element, so a nullable receiver keeps the generic update.
            if !matches!(receiver_ty.codegen_repr(), PhpType::Object(_)) {
                return None;
            }
            object_type_property_type(ctx, &receiver_ty, property)
                .as_ref()
                .and_then(scalar_buffer_element)?
        }
    };
    let values = BufferIncDec { read: update.read, element, increment: update.increment }
        .lower_values(ctx, span);
    match update.root {
        ExpressionRoot::Local(name) => {
            let buffer = load_array_local_for_write(ctx, name, span);
            lower_local_store(ctx, buffer, update.index, values.stored, span);
        }
        ExpressionRoot::Property { object, property } => lower_property_array_assign_with_update(
            ctx,
            object,
            property,
            update.index,
            update.value,
            span,
            true,
            PropertyElementUpdate::Stored(values.stored),
        ),
    }
    Some(match (update.postfix, element) {
        (true, _) => values.old,
        (false, ScalarBufferElement::Float) => values.stored,
        (false, ScalarBufferElement::Int) => {
            // PHP's prefix value is the sum itself, a float past the int range, so it is the
            // checked `mixed` the checker typed this expression as. The store above is typed.
            let one = lower_int_one(ctx, span);
            let op = if update.increment { Op::ICheckedAdd } else { Op::ICheckedSub };
            ctx.emit_value(
                op,
                vec![values.old.value, one.value],
                None,
                PhpType::Mixed,
                op.default_effects(),
                Some(span),
            )
        }
    })
}
