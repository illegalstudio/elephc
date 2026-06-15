//! Purpose:
//! Interprets EvalIR against a materialized caller scope.
//! The interpreter is generic over runtime value operations so it can execute
//! by manipulating opaque elephc runtime-cell handles.
//!
//! Called from:
//! - Future `crate::__elephc_eval_execute()` implementation.
//! - `cargo test -p elephc-eval` for scope/value-flow validation.
//!
//! Key details:
//! - This module does not own PHP values. Constants and operations are delegated
//!   to `RuntimeValueOps`, which will be backed by elephc runtime hooks.

use crate::context::{ElephcEvalContext, NativeFunction};
use crate::errors::EvalStatus;
use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalCallArg, EvalConst, EvalExpr, EvalFunction, EvalMagicConst,
    EvalProgram, EvalStmt, EvalSwitchCase, EvalUnaryOp,
};
use crate::parser::parse_fragment;
use crate::scope::{ElephcEvalScope, ScopeCellOwnership};
use crate::value::RuntimeCellHandle;

/// Internal statement-control result used to propagate eval returns and loops.
enum EvalControl {
    None,
    Return(RuntimeCellHandle),
    Break,
    Continue,
}

/// One already evaluated function-like call argument.
struct EvaluatedCallArg {
    name: Option<String>,
    value: RuntimeCellHandle,
}

/// Runtime value hooks required by the EvalIR interpreter.
pub trait RuntimeValueOps {
    /// Creates a runtime indexed-array cell with room for at least `capacity` elements.
    fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime associative-array cell with room for at least `capacity` elements.
    fn assoc_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reads one element from a runtime array-like Mixed cell using an index expression.
    fn array_get(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Checks whether a normalized PHP array key exists without conflating null values with misses.
    fn array_key_exists(
        &mut self,
        key: RuntimeCellHandle,
        array: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the foreach-visible key at a zero-based iteration position.
    fn array_iter_key(
        &mut self,
        array: RuntimeCellHandle,
        position: usize,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Writes one element to a runtime array-like Mixed cell and returns the target cell.
    fn array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reads a named property from a runtime object held in a boxed Mixed cell.
    fn property_get(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Writes a named property on a runtime object held in a boxed Mixed cell.
    fn property_set(
        &mut self,
        object: RuntimeCellHandle,
        property: &str,
        value: RuntimeCellHandle,
    ) -> Result<(), EvalStatus>;

    /// Calls a named method on a runtime object held in a boxed Mixed cell.
    fn method_call(
        &mut self,
        object: RuntimeCellHandle,
        method: &str,
        args: Vec<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the visible element count for an array-like runtime cell.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus>;

    /// Returns whether a runtime cell can be indexed like an array by eval writes.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Returns whether a runtime cell holds PHP null.
    fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

    /// Returns the concrete boxed Mixed runtime tag after unwrapping nested Mixed cells.
    fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus>;

    /// Releases one owned runtime cell that is no longer held by the eval scope.
    fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Creates a runtime null cell.
    fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime bool cell.
    fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime int cell.
    fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime float cell.
    fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Creates a runtime string cell.
    fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP integer cell.
    fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP float cell.
    fn cast_float(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP string cell.
    fn cast_string(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Casts one runtime cell to a boxed PHP boolean cell.
    fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `abs()` for one runtime cell while preserving integer/float result typing.
    fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `ceil()` for one runtime cell after PHP numeric conversion.
    fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `floor()` for one runtime cell after PHP numeric conversion.
    fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes PHP `sqrt()` for one runtime cell after PHP numeric conversion.
    fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Reverses a string value using PHP `strrev()` byte-string semantics.
    fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Divides two runtime cells using PHP `fdiv()` semantics.
    fn fdiv(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes the floating-point remainder using PHP `fmod()` semantics.
    fn fmod(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Adds two runtime cells using PHP addition semantics.
    fn add(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Subtracts two runtime cells using PHP numeric semantics.
    fn sub(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Multiplies two runtime cells using PHP numeric semantics.
    fn mul(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Divides two runtime cells using PHP numeric semantics.
    fn div(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Computes modulo for two runtime cells using PHP integer modulo semantics.
    fn modulo(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Raises one runtime cell to another using PHP exponentiation semantics.
    fn pow(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Rounds one runtime cell using PHP `round()` semantics and optional precision.
    fn round(
        &mut self,
        value: RuntimeCellHandle,
        precision: Option<RuntimeCellHandle>,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Applies an integer bitwise or shift operation to two runtime cells.
    fn bitwise(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Applies integer bitwise NOT to one runtime cell.
    fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Concatenates two runtime cells using PHP string conversion semantics.
    fn concat(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Compares two runtime cells and returns a boxed PHP boolean cell.
    fn compare(
        &mut self,
        op: EvalBinOp,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Compares two runtime cells and returns a boxed PHP spaceship integer.
    fn spaceship(
        &mut self,
        left: RuntimeCellHandle,
        right: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Emits one runtime cell to stdout using PHP echo semantics.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Casts one runtime cell to a PHP string and copies its bytes for parsing.
    fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus>;

    /// Converts one runtime cell to PHP boolean truthiness.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;
}

const EVAL_TAG_INT: u64 = 0;
const EVAL_TAG_STRING: u64 = 1;
const EVAL_TAG_FLOAT: u64 = 2;
const EVAL_TAG_BOOL: u64 = 3;
const EVAL_TAG_ARRAY: u64 = 4;
const EVAL_TAG_ASSOC: u64 = 5;
const EVAL_TAG_OBJECT: u64 = 6;
const EVAL_TAG_NULL: u64 = 8;
const EVAL_TAG_RESOURCE: u64 = 9;

/// Executes an EvalIR program and returns the eval result cell.
pub fn execute_program(
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut context = ElephcEvalContext::new();
    execute_program_with_context(&mut context, program, scope, values)
}

/// Executes an EvalIR program with a persistent eval context for dynamic declarations.
pub fn execute_program_with_context(
    context: &mut ElephcEvalContext,
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_statements(program.statements(), context, scope, values)? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Executes a zero-argument function declared in the shared eval context.
pub fn execute_context_function_zero_args(
    context: &mut ElephcEvalContext,
    name: &str,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    execute_context_function(context, name, Vec::new(), values)
}

/// Executes a function declared in the shared eval context with prepared argument cells.
pub fn execute_context_function(
    context: &mut ElephcEvalContext,
    name: &str,
    args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    context
        .function(name)
        .cloned()
        .map_or(Err(EvalStatus::UnsupportedConstruct), |function| {
            eval_dynamic_function_with_values(&function, args, context, values)
        })
}

/// Executes statements in source order and propagates the first eval `return`.
fn execute_statements(
    statements: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    for stmt in statements {
        match execute_stmt(stmt, context, scope, values)? {
            EvalControl::None => {}
            control => return Ok(control),
        }
    }
    Ok(EvalControl::None)
}

/// Executes one statement and returns `Some` only for eval `return`.
fn execute_stmt(
    stmt: &EvalStmt,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match stmt {
        EvalStmt::ArrayAppendVar { name, value } => {
            let mut ownership = ScopeCellOwnership::Owned;
            let array = if let Some(existing) =
                scope.entry(name).filter(|entry| entry.flags().is_visible())
            {
                if values.is_array_like(existing.cell())? {
                    let tag = values.type_tag(existing.cell())?;
                    if !matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    ownership = existing.flags().ownership;
                    existing.cell()
                } else {
                    values.array_new(1)?
                }
            } else {
                values.array_new(1)?
            };
            let index = eval_array_append_key(array, values)?;
            let value = eval_expr(value, context, scope, values)?;
            let array = values.array_set(array, index, value)?;
            if let Some(replaced) = scope.set(name.clone(), array, ownership) {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::ArraySetVar { name, index, value } => {
            let mut ownership = ScopeCellOwnership::Owned;
            let array = if let Some(existing) =
                scope.entry(name).filter(|entry| entry.flags().is_visible())
            {
                if values.is_array_like(existing.cell())? {
                    ownership = existing.flags().ownership;
                    existing.cell()
                } else {
                    values.array_new(1)?
                }
            } else {
                values.array_new(1)?
            };
            let index = eval_expr(index, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            let array = values.array_set(array, index, value)?;
            if let Some(replaced) = scope.set(name.clone(), array, ownership) {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Break => Ok(EvalControl::Break),
        EvalStmt::Continue => Ok(EvalControl::Continue),
        EvalStmt::DoWhile { body, condition } => {
            execute_do_while_stmt(body, condition, context, scope, values)
        }
        EvalStmt::Echo(expr) => {
            let value = eval_expr(expr, context, scope, values)?;
            values.echo(value)?;
            Ok(EvalControl::None)
        }
        EvalStmt::For {
            init,
            condition,
            update,
            body,
        } => execute_for_stmt(
            init,
            condition.as_ref(),
            update,
            body,
            context,
            scope,
            values,
        ),
        EvalStmt::Foreach {
            array,
            key_name,
            value_name,
            body,
        } => execute_foreach_stmt(
            array,
            key_name.as_deref(),
            value_name,
            body,
            context,
            scope,
            values,
        ),
        EvalStmt::FunctionDecl { name, params, body } => {
            let key = name.to_ascii_lowercase();
            context
                .define_function(
                    key,
                    EvalFunction::new(name.clone(), params.clone(), body.clone()),
                )
                .map_err(|_| EvalStatus::RuntimeFatal)?;
            Ok(EvalControl::None)
        }
        EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                execute_statements(then_branch, context, scope, values)
            } else {
                execute_statements(else_branch, context, scope, values)
            }
        }
        EvalStmt::Return(Some(expr)) => Ok(EvalControl::Return(eval_expr(
            expr, context, scope, values,
        )?)),
        EvalStmt::Return(None) => Ok(EvalControl::Return(values.null()?)),
        EvalStmt::PropertySet {
            object,
            property,
            value,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let value = eval_expr(value, context, scope, values)?;
            values.property_set(object, property, value)?;
            Ok(EvalControl::None)
        }
        EvalStmt::StoreVar { name, value } => {
            let value = eval_expr(value, context, scope, values)?;
            if let Some(replaced) = scope.set(name.clone(), value, ScopeCellOwnership::Owned) {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Switch { expr, cases } => {
            execute_switch_stmt(expr, cases, context, scope, values)
        }
        EvalStmt::UnsetVar { name } => {
            if let Some(replaced) = scope.unset(name.clone()) {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::While { condition, body } => {
            while {
                let condition = eval_expr(condition, context, scope, values)?;
                values.truthy(condition)?
            } {
                match execute_statements(body, context, scope, values)? {
                    EvalControl::None | EvalControl::Continue => {}
                    EvalControl::Break => break,
                    EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
                }
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Expr(expr) => {
            let _ = eval_expr(expr, context, scope, values)?;
            Ok(EvalControl::None)
        }
    }
}

/// Executes a PHP switch with loose case matching, default fallback, and fallthrough.
fn execute_switch_stmt(
    expr: &EvalExpr,
    cases: &[EvalSwitchCase],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let subject = eval_expr(expr, context, scope, values)?;
    let mut default_index = None;
    let mut matched_index = None;
    for (index, case) in cases.iter().enumerate() {
        let Some(condition) = &case.condition else {
            if default_index.is_none() {
                default_index = Some(index);
            }
            continue;
        };
        let condition = eval_expr(condition, context, scope, values)?;
        let matches = values.compare(EvalBinOp::LooseEq, subject, condition)?;
        if values.truthy(matches)? {
            matched_index = Some(index);
            break;
        }
    }
    let Some(start_index) = matched_index.or(default_index) else {
        return Ok(EvalControl::None);
    };
    for case in &cases[start_index..] {
        match execute_statements(&case.body, context, scope, values)? {
            EvalControl::None => {}
            EvalControl::Break | EvalControl::Continue => break,
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `do/while` loop, evaluating the condition after every body run.
fn execute_do_while_stmt(
    body: &[EvalStmt],
    condition: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    loop {
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        let condition = eval_expr(condition, context, scope, values)?;
        if !values.truthy(condition)? {
            break;
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `for` loop while preserving update-on-continue semantics.
fn execute_for_stmt(
    init: &[EvalStmt],
    condition: Option<&EvalExpr>,
    update: &[EvalStmt],
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    match execute_statements(init, context, scope, values)? {
        EvalControl::None | EvalControl::Continue => {}
        EvalControl::Break => return Ok(EvalControl::None),
        EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
    }
    loop {
        if let Some(condition) = condition {
            let condition = eval_expr(condition, context, scope, values)?;
            if !values.truthy(condition)? {
                break;
            }
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
        match execute_statements(update, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Executes a PHP `foreach` loop over eval array values.
fn execute_foreach_stmt(
    array: &EvalExpr,
    key_name: Option<&str>,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let array = eval_expr(array, context, scope, values)?;
    let len = values.array_len(array)?;
    for index in 0..len {
        let key = values.array_iter_key(array, index)?;
        let value = values.array_get(array, key)?;
        if let Some(key_name) = key_name {
            if let Some(replaced) = scope.set(key_name.to_string(), key, ScopeCellOwnership::Owned)
            {
                values.release(replaced)?;
            }
        } else {
            values.release(key)?;
        }
        if let Some(replaced) = scope.set(value_name.to_string(), value, ScopeCellOwnership::Owned)
        {
            values.release(replaced)?;
        }
        match execute_statements(body, context, scope, values)? {
            EvalControl::None | EvalControl::Continue => {}
            EvalControl::Break => break,
            EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
        }
    }
    Ok(EvalControl::None)
}

/// Returns PHP's next automatic integer key for `$array[]` append writes.
fn eval_array_append_key(
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut next_key = None;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        if values.type_tag(key)? != EVAL_TAG_INT {
            continue;
        }
        let one = values.int(1)?;
        let candidate = values.add(key, one)?;
        let replace = if let Some(current) = next_key {
            let is_greater = values.compare(EvalBinOp::Gt, candidate, current)?;
            values.truthy(is_greater)?
        } else {
            true
        };
        if replace {
            next_key = Some(candidate);
        }
    }
    next_key.map_or_else(|| values.int(0), Ok)
}

/// Evaluates one expression to an opaque runtime-cell handle.
fn eval_expr(
    expr: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match expr {
        EvalExpr::Array(elements) => {
            if elements
                .iter()
                .any(|element| matches!(element, EvalArrayElement::KeyValue { .. }))
            {
                eval_assoc_array(elements, context, scope, values)
            } else {
                eval_indexed_array(elements, context, scope, values)
            }
        }
        EvalExpr::ArrayGet { array, index } => {
            let array = eval_expr(array, context, scope, values)?;
            let index = eval_expr(index, context, scope, values)?;
            values.array_get(array, index)
        }
        EvalExpr::Call { name, args } => eval_call(name, args, context, scope, values),
        EvalExpr::Const(value) => eval_const(value, values),
        EvalExpr::LoadVar(name) => scope.visible_cell(name).map_or_else(|| values.null(), Ok),
        EvalExpr::Magic(magic) => eval_magic_const(magic, context, values),
        EvalExpr::MethodCall {
            object,
            method,
            args,
        } => {
            let object = eval_expr(object, context, scope, values)?;
            let evaluated_args = eval_method_call_arg_values(args, context, scope, values)?;
            values.method_call(object, method, evaluated_args)
        }
        EvalExpr::NullCoalesce { value, default } => {
            let value = eval_expr(value, context, scope, values)?;
            if values.is_null(value)? {
                eval_expr(default, context, scope, values)
            } else {
                Ok(value)
            }
        }
        EvalExpr::PropertyGet { object, property } => {
            let object = eval_expr(object, context, scope, values)?;
            values.property_get(object, property)
        }
        EvalExpr::Print(inner) => {
            let value = eval_expr(inner, context, scope, values)?;
            values.echo(value)?;
            values.int(1)
        }
        EvalExpr::Ternary {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, context, scope, values)?;
            if values.truthy(condition)? {
                if let Some(then_branch) = then_branch {
                    eval_expr(then_branch, context, scope, values)
                } else {
                    Ok(condition)
                }
            } else {
                eval_expr(else_branch, context, scope, values)
            }
        }
        EvalExpr::Unary { op, expr } => {
            let value = eval_expr(expr, context, scope, values)?;
            match op {
                EvalUnaryOp::Plus => {
                    let zero = values.int(0)?;
                    values.add(zero, value)
                }
                EvalUnaryOp::Negate => {
                    let zero = values.int(0)?;
                    values.sub(zero, value)
                }
                EvalUnaryOp::LogicalNot => {
                    let truthy = values.truthy(value)?;
                    values.bool_value(!truthy)
                }
                EvalUnaryOp::BitNot => values.bit_not(value),
            }
        }
        EvalExpr::Binary { op, left, right } => {
            if *op == EvalBinOp::LogicalAnd {
                let left = eval_expr(left, context, scope, values)?;
                if !values.truthy(left)? {
                    return values.bool_value(false);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            if *op == EvalBinOp::LogicalOr {
                let left = eval_expr(left, context, scope, values)?;
                if values.truthy(left)? {
                    return values.bool_value(true);
                }
                let right = eval_expr(right, context, scope, values)?;
                let truthy = values.truthy(right)?;
                return values.bool_value(truthy);
            }
            let left = eval_expr(left, context, scope, values)?;
            let right = eval_expr(right, context, scope, values)?;
            match op {
                EvalBinOp::Add => values.add(left, right),
                EvalBinOp::Sub => values.sub(left, right),
                EvalBinOp::Mul => values.mul(left, right),
                EvalBinOp::Div => values.div(left, right),
                EvalBinOp::Mod => values.modulo(left, right),
                EvalBinOp::Pow => values.pow(left, right),
                EvalBinOp::BitAnd
                | EvalBinOp::BitOr
                | EvalBinOp::BitXor
                | EvalBinOp::ShiftLeft
                | EvalBinOp::ShiftRight => values.bitwise(*op, left, right),
                EvalBinOp::Concat => values.concat(left, right),
                EvalBinOp::LogicalXor => {
                    let left_truthy = values.truthy(left)?;
                    let right_truthy = values.truthy(right)?;
                    values.bool_value(left_truthy ^ right_truthy)
                }
                EvalBinOp::LooseEq
                | EvalBinOp::LooseNotEq
                | EvalBinOp::StrictEq
                | EvalBinOp::StrictNotEq
                | EvalBinOp::Lt
                | EvalBinOp::LtEq
                | EvalBinOp::Gt
                | EvalBinOp::GtEq => values.compare(*op, left, right),
                EvalBinOp::Spaceship => values.spaceship(left, right),
                EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr => {
                    Err(EvalStatus::UnsupportedConstruct)
                }
            }
        }
    }
}

/// Returns cloned positional argument expressions, rejecting named arguments.
fn positional_call_arg_exprs(args: &[EvalCallArg]) -> Result<Vec<EvalExpr>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(args.iter().map(|arg| arg.value().clone()).collect())
}

/// Evaluates a positional-only call argument list in source order.
fn eval_positional_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if args
        .iter()
        .any(|arg| arg.name().is_some() || arg.is_spread())
    {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg.value(), context, scope, values)?);
    }
    Ok(evaluated_args)
}

/// Evaluates method-call arguments, allowing numeric spread but not named args.
fn eval_method_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    if evaluated_args.iter().any(|arg| arg.name.is_some()) {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(evaluated_args.into_iter().map(|arg| arg.value).collect())
}

/// Evaluates supported function-like calls from a runtime eval fragment.
fn eval_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if eval_expr_language_construct_name(name) {
        let args = positional_call_arg_exprs(args)?;
        return eval_positional_expr_call(name, &args, context, scope, values);
    }
    if eval_php_visible_builtin_exists(name) {
        if eval_call_args_are_plain_positional(args) {
            let args = positional_call_arg_exprs(args)?;
            return eval_positional_expr_call(name, &args, context, scope, values);
        }
        return eval_builtin_call(name, args, context, scope, values);
    }

    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function(&function, args, context, scope, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function(function, args, context, scope, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Returns true for language constructs that need unevaluated argument expressions.
fn eval_expr_language_construct_name(name: &str) -> bool {
    matches!(name, "empty" | "eval" | "isset")
}

/// Returns true when every source argument is plain positional.
fn eval_call_args_are_plain_positional(args: &[EvalCallArg]) -> bool {
    args.iter()
        .all(|arg| arg.name().is_none() && !arg.is_spread())
}

/// Evaluates builtins and language constructs after positional-only argument validation.
fn eval_positional_expr_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "abs" => eval_builtin_abs(args, context, scope, values),
        "array_keys" | "array_values" => {
            eval_builtin_array_projection(name, args, context, scope, values)
        }
        "array_key_exists" => eval_builtin_array_key_exists(args, context, scope, values),
        "array_product" | "array_sum" => {
            eval_builtin_array_aggregate(name, args, context, scope, values)
        }
        "array_search" | "in_array" => {
            eval_builtin_array_search(name, args, context, scope, values)
        }
        "ceil" => eval_builtin_ceil(args, context, scope, values),
        "call_user_func" => eval_builtin_call_user_func(args, context, scope, values),
        "call_user_func_array" => eval_builtin_call_user_func_array(args, context, scope, values),
        "chop" => eval_builtin_trim_like(name, args, context, scope, values),
        "boolval" | "floatval" | "intval" | "strval" => {
            eval_builtin_cast(name, args, context, scope, values)
        }
        "count" => eval_builtin_count(args, context, scope, values),
        "empty" => eval_builtin_empty(args, context, scope, values),
        "eval" => eval_nested_eval(args, context, scope, values),
        "fdiv" | "fmod" => eval_builtin_float_binary(name, args, context, scope, values),
        "floor" => eval_builtin_floor(args, context, scope, values),
        "function_exists" | "is_callable" => {
            eval_builtin_function_probe(args, context, scope, values)
        }
        "gettype" => eval_builtin_gettype(args, context, scope, values),
        "hash_equals" => eval_builtin_hash_equals(args, context, scope, values),
        "is_array" | "is_bool" | "is_double" | "is_float" | "is_int" | "is_integer" | "is_long"
        | "is_null" | "is_numeric" | "is_real" | "is_resource" | "is_string" => {
            eval_builtin_type_predicate(name, args, context, scope, values)
        }
        "ltrim" | "rtrim" => eval_builtin_trim_like(name, args, context, scope, values),
        "max" | "min" => eval_builtin_min_max(name, args, context, scope, values),
        "ord" => eval_builtin_ord(args, context, scope, values),
        "pi" => eval_builtin_pi(args, values),
        "pow" => eval_builtin_pow(args, context, scope, values),
        "round" => eval_builtin_round(args, context, scope, values),
        "isset" => eval_builtin_isset(args, context, scope, values),
        "sqrt" => eval_builtin_sqrt(args, context, scope, values),
        "strrev" => eval_builtin_strrev(args, context, scope, values),
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            eval_builtin_string_search(name, args, context, scope, values)
        }
        "strcmp" | "strcasecmp" => eval_builtin_string_compare(name, args, context, scope, values),
        "strlen" => eval_builtin_strlen(args, context, scope, values),
        "strpos" | "strrpos" => eval_builtin_string_position(name, args, context, scope, values),
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            eval_builtin_string_case(name, args, context, scope, values)
        }
        "trim" => eval_builtin_trim_like(name, args, context, scope, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates string-name function probes against eval and supported builtin tables.
fn eval_builtin_function_probe(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [name] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let name = eval_expr(name, context, scope, values)?;
    let name = values.string_bytes(name)?;
    let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
    let name = name.trim_start_matches('\\').to_ascii_lowercase();
    values.bool_value(eval_function_probe_exists(context, &name))
}

/// Evaluates PHP's `isset(...)` language construct over eval-visible values.
fn eval_builtin_isset(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return values.bool_value(false);
    }
    for arg in args {
        if !eval_isset_arg(arg, context, scope, values)? {
            return values.bool_value(false);
        }
    }
    values.bool_value(true)
}

/// Evaluates PHP's `empty(...)` language construct over eval-visible values.
fn eval_builtin_empty(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [arg] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let empty = eval_empty_arg(arg, context, scope, values)?;
    values.bool_value(empty)
}

/// Evaluates one `empty` operand without warning or failing on missing variables.
fn eval_empty_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = scope.visible_cell(name) else {
            return Ok(true);
        };
        return Ok(!values.truthy(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.truthy(value)?)
}

/// Evaluates one `isset` operand without allocating a null cell for missing variables.
fn eval_isset_arg(
    arg: &EvalExpr,
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    if let EvalExpr::LoadVar(name) = arg {
        let Some(value) = scope.visible_cell(name) else {
            return Ok(false);
        };
        return Ok(!values.is_null(value)?);
    }
    let value = eval_expr(arg, context, scope, values)?;
    Ok(!values.is_null(value)?)
}

/// Returns true when a PHP function name is visible to eval builtin probes.
fn eval_function_probe_exists(context: &ElephcEvalContext, name: &str) -> bool {
    !name.contains("::") && (context.has_function(name) || eval_php_visible_builtin_exists(name))
}

/// Returns true for PHP-visible builtin names implemented by the eval interpreter.
fn eval_php_visible_builtin_exists(name: &str) -> bool {
    matches!(
        name,
        "abs"
            | "array_key_exists"
            | "array_keys"
            | "array_product"
            | "array_search"
            | "array_sum"
            | "array_values"
            | "ceil"
            | "call_user_func"
            | "call_user_func_array"
            | "boolval"
            | "chop"
            | "count"
            | "fdiv"
            | "floor"
            | "floatval"
            | "fmod"
            | "function_exists"
            | "gettype"
            | "hash_equals"
            | "in_array"
            | "intval"
            | "ltrim"
            | "is_callable"
            | "is_array"
            | "is_bool"
            | "is_double"
            | "is_float"
            | "is_int"
            | "is_integer"
            | "is_long"
            | "is_null"
            | "is_numeric"
            | "is_real"
            | "is_resource"
            | "is_string"
            | "lcfirst"
            | "max"
            | "min"
            | "ord"
            | "pi"
            | "pow"
            | "rtrim"
            | "round"
            | "sqrt"
            | "strcasecmp"
            | "str_contains"
            | "str_ends_with"
            | "str_starts_with"
            | "strcmp"
            | "strlen"
            | "strpos"
            | "strrpos"
            | "strrev"
            | "strtolower"
            | "strtoupper"
            | "strval"
            | "trim"
            | "ucfirst"
    )
}

/// Evaluates a direct PHP-visible builtin call with named or spread arguments.
fn eval_builtin_call(
    name: &str,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, scope, values)?;
    let evaluated_args = bind_evaluated_builtin_args(name, evaluated_args)?;
    let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? else {
        return Err(EvalStatus::UnsupportedConstruct);
    };
    Ok(result)
}

/// Binds evaluated builtin arguments to PHP parameter order when names are used.
fn bind_evaluated_builtin_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        return Ok(evaluated_args.into_iter().map(|arg| arg.value).collect());
    }

    let params = eval_builtin_param_names(name).ok_or(EvalStatus::RuntimeFatal)?;
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_builtin_named_arg(params, &mut bound_args, &name, arg.value)?;
        } else {
            bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
        }
    }

    collect_contiguous_bound_args(bound_args)
}

/// Binds one named builtin-call value to the matching PHP parameter slot.
fn bind_builtin_named_arg(
    params: &[&str],
    bound_args: &mut [Option<RuntimeCellHandle>],
    name: &str,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let Some(param_index) = params.iter().position(|param| *param == name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[param_index] = Some(value);
    Ok(())
}

/// Collects ordered bound arguments, rejecting gaps where defaults would be needed.
fn collect_contiguous_bound_args(
    bound_args: Vec<Option<RuntimeCellHandle>>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let Some(last_index) = bound_args.iter().rposition(Option::is_some) else {
        return Ok(Vec::new());
    };
    bound_args
        .into_iter()
        .take(last_index + 1)
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Returns PHP parameter names for builtin calls implemented by eval.
fn eval_builtin_param_names(name: &str) -> Option<&'static [&'static str]> {
    match name {
        "abs" | "ceil" | "floor" | "sqrt" => Some(&["num"]),
        "array_keys" | "array_product" | "array_sum" | "array_values" => Some(&["array"]),
        "array_key_exists" => Some(&["key", "array"]),
        "array_search" | "in_array" => Some(&["needle", "haystack", "strict"]),
        "boolval" | "floatval" | "gettype" | "intval" | "is_array" | "is_bool" | "is_double"
        | "is_float" | "is_int" | "is_integer" | "is_long" | "is_null" | "is_numeric"
        | "is_real" | "is_resource" | "is_string" | "is_callable" | "strval" => Some(&["value"]),
        "call_user_func" => Some(&["callback"]),
        "call_user_func_array" => Some(&["callback", "args"]),
        "chop" | "ltrim" | "rtrim" | "trim" => Some(&["string", "characters"]),
        "count" => Some(&["value", "mode"]),
        "fdiv" | "fmod" => Some(&["num1", "num2"]),
        "function_exists" => Some(&["function"]),
        "hash_equals" => Some(&["known_string", "user_string"]),
        "max" | "min" => Some(&["value"]),
        "ord" => Some(&["character"]),
        "pi" => Some(&[]),
        "pow" => Some(&["num", "exponent"]),
        "round" => Some(&["num", "precision"]),
        "strcasecmp" | "strcmp" => Some(&["string1", "string2"]),
        "str_contains" | "str_ends_with" | "str_starts_with" => Some(&["haystack", "needle"]),
        "strpos" | "strrpos" => Some(&["haystack", "needle", "offset"]),
        "lcfirst" | "strlen" | "strrev" | "strtolower" | "strtoupper" | "ucfirst" => {
            Some(&["string"])
        }
        _ => None,
    }
}

/// Evaluates `call_user_func($name, ...$args)` inside a runtime eval fragment.
fn eval_builtin_call_user_func(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_call_user_func_with_values(evaluated_args, context, values)
}

/// Evaluates `call_user_func_array($name, $args)` inside a runtime eval fragment.
fn eval_builtin_call_user_func_array(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [callback, arg_array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = eval_expr(callback, context, scope, values)?;
    let arg_array = eval_expr(arg_array, context, scope, values)?;
    eval_call_user_func_array_with_values(callback, arg_array, context, values)
}

/// Dispatches `call_user_func_array` after callback and array arguments are evaluated.
fn eval_call_user_func_array_with_values(
    callback: RuntimeCellHandle,
    arg_array: RuntimeCellHandle,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let callback = values.string_bytes(callback)?;
    let callback = String::from_utf8(callback).map_err(|_| EvalStatus::RuntimeFatal)?;
    let callback = callback.trim_start_matches('\\').to_ascii_lowercase();
    if callback.contains("::") {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    if !values.is_array_like(arg_array)? {
        return Err(EvalStatus::RuntimeFatal);
    }
    let evaluated_args = eval_array_call_arg_values(arg_array, values)?;
    eval_callable_with_call_array_args(&callback, evaluated_args, context, values)
}

/// Dispatches `call_user_func` after its callback and arguments are already evaluated.
fn eval_call_user_func_with_values(
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((callback, callback_args)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let callback = values.string_bytes(*callback)?;
    let callback = String::from_utf8(callback).map_err(|_| EvalStatus::RuntimeFatal)?;
    let callback = callback.trim_start_matches('\\').to_ascii_lowercase();
    if callback.contains("::") {
        return Err(EvalStatus::UnsupportedConstruct);
    }
    eval_callable_with_values(&callback, callback_args.to_vec(), context, values)
}

/// Invokes a PHP-visible callable name with source-order positional values.
fn eval_callable_with_values(
    name: &str,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if let Some(result) = eval_builtin_with_values(name, &evaluated_args, context, values)? {
        return Ok(result);
    }
    if let Some(function) = context.function(name).cloned() {
        return eval_dynamic_function_with_values(&function, evaluated_args, context, values);
    }
    if let Some(function) = context.native_function(name) {
        return eval_native_function_with_values(function, evaluated_args, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Invokes a callable with arguments that may carry `call_user_func_array` names.
fn eval_callable_with_call_array_args(
    name: &str,
    evaluated_args: Vec<EvaluatedCallArg>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.iter().all(|arg| arg.name.is_none()) {
        let evaluated_args = evaluated_args.into_iter().map(|arg| arg.value).collect();
        return eval_callable_with_values(name, evaluated_args, context, values);
    }
    if eval_php_visible_builtin_exists(name) {
        return Err(EvalStatus::RuntimeFatal);
    }
    if let Some(function) = context.function(name).cloned() {
        let evaluated_args = bind_evaluated_function_args(function.params(), evaluated_args)?;
        return eval_dynamic_function_with_values(&function, evaluated_args, context, values);
    }
    if let Some(function) = context.native_function(name) {
        if function.param_names().len() != function.param_count() {
            return Err(EvalStatus::RuntimeFatal);
        }
        let evaluated_args = bind_evaluated_function_args(function.param_names(), evaluated_args)?;
        return eval_native_function_with_values(function, evaluated_args, values);
    }
    Err(EvalStatus::UnsupportedConstruct)
}

/// Evaluates PHP-visible builtins when they are invoked through a dynamic callable name.
fn eval_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "abs" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.abs(*value)?
        }
        "array_product" | "array_sum" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_aggregate_result(name, *array, values)?
        }
        "array_keys" | "array_values" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_projection_result(name, *array, values)?
        }
        "array_key_exists" => {
            let [key, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.array_key_exists(*key, *array)?
        }
        "array_search" | "in_array" => {
            let [needle, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_search_result(name, *needle, *array, values)?
        }
        "ceil" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.ceil(*value)?
        }
        "floor" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.floor(*value)?
        }
        "fdiv" | "fmod" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_binary_result(name, *left, *right, values)?
        }
        "pi" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.float(std::f64::consts::PI)?
        }
        "pow" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.pow(*left, *right)?
        }
        "round" => match evaluated_args {
            [value] => values.round(*value, None)?,
            [value, precision] => values.round(*value, Some(*precision))?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "sqrt" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.sqrt(*value)?
        }
        "strrev" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.strrev(*value)?
        }
        "call_user_func" => {
            return eval_call_user_func_with_values(evaluated_args.to_vec(), context, values)
                .map(Some);
        }
        "call_user_func_array" => {
            let [callback, arg_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            return eval_call_user_func_array_with_values(*callback, *arg_array, context, values)
                .map(Some);
        }
        "boolval" | "floatval" | "intval" | "strval" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_cast_result(name, *value, values)?
        }
        "count" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let len = values.array_len(*value)?;
            let len = i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(len)?
        }
        "ord" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ord_result(*value, values)?
        }
        "max" | "min" => eval_min_max_result(name, evaluated_args, values)?,
        "trim" | "ltrim" | "rtrim" | "chop" => match evaluated_args {
            [value] => eval_trim_like_result(name, *value, None, values)?,
            [value, mask] => eval_trim_like_result(name, *value, Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "function_exists" | "is_callable" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let name = values.string_bytes(*name)?;
            let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
            let name = name.trim_start_matches('\\').to_ascii_lowercase();
            values.bool_value(eval_function_probe_exists(context, &name))?
        }
        "gettype" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gettype_result(*value, values)?
        }
        "hash_equals" => {
            let [known, user] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_equals_result(*known, *user, values)?
        }
        "is_array" | "is_bool" | "is_double" | "is_float" | "is_int" | "is_integer" | "is_long"
        | "is_null" | "is_numeric" | "is_real" | "is_resource" | "is_string" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_type_predicate_result(name, *value, values)?
        }
        "strlen" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let bytes = values.string_bytes(*value)?;
            let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(len)?
        }
        "strpos" | "strrpos" => {
            let [haystack, needle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_position_result(name, *haystack, *needle, values)?
        }
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            let [haystack, needle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_search_result(name, *haystack, *needle, values)?
        }
        "strcmp" | "strcasecmp" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_compare_result(name, *left, *right, values)?
        }
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_case_result(name, *value, values)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// Evaluates PHP's `abs(...)` over one eval expression.
fn eval_builtin_abs(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.abs(value)
}

/// Evaluates PHP array aggregate builtins over one eval array expression.
fn eval_builtin_array_aggregate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_aggregate_result(name, array, values)
}

/// Computes `array_sum()` or `array_product()` through eval's numeric value hooks.
fn eval_array_aggregate_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = match name {
        "array_sum" => values.int(0)?,
        "array_product" => values.int(1)?,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        result = match name {
            "array_sum" => values.add(result, value)?,
            "array_product" => values.mul(result, value)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
    }
    Ok(result)
}

/// Evaluates PHP array projection builtins over one eval array expression.
fn eval_builtin_array_projection(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let array = eval_expr(array, context, scope, values)?;
    eval_array_projection_result(name, array, values)
}

/// Builds the indexed result array for `array_keys()` or `array_values()`.
fn eval_array_projection_result(
    name: &str,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    let mut result = values.array_new(len)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = match name {
            "array_keys" => key,
            "array_values" => values.array_get(array, key)?,
            _ => return Err(EvalStatus::UnsupportedConstruct),
        };
        let index = values.int(position as i64)?;
        result = values.array_set(result, index, value)?;
    }
    Ok(result)
}

/// Evaluates PHP `array_key_exists()` over a key and array expression.
fn eval_builtin_array_key_exists(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [key, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let key = eval_expr(key, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    values.array_key_exists(key, array)
}

/// Evaluates PHP array search builtins over a needle and haystack expression.
fn eval_builtin_array_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [needle, array] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let needle = eval_expr(needle, context, scope, values)?;
    let array = eval_expr(array, context, scope, values)?;
    eval_array_search_result(name, needle, array, values)
}

/// Searches an eval array with PHP's default loose comparison semantics.
fn eval_array_search_result(
    name: &str,
    needle: RuntimeCellHandle,
    array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        let equal = values.compare(EvalBinOp::LooseEq, needle, value)?;
        if values.truthy(equal)? {
            return match name {
                "in_array" => values.bool_value(true),
                "array_search" => Ok(key),
                _ => Err(EvalStatus::UnsupportedConstruct),
            };
        }
    }
    match name {
        "in_array" => values.bool_value(false),
        "array_search" => values.bool_value(false),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `ceil(...)` over one eval expression.
fn eval_builtin_ceil(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.ceil(value)
}

/// Evaluates PHP's `floor(...)` over one eval expression.
fn eval_builtin_floor(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.floor(value)
}

/// Evaluates PHP's zero-argument `pi()` builtin.
fn eval_builtin_pi(
    args: &[EvalExpr],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if !args.is_empty() {
        return Err(EvalStatus::RuntimeFatal);
    }
    values.float(std::f64::consts::PI)
}

/// Evaluates PHP's `pow(...)` over two eval expressions.
fn eval_builtin_pow(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    values.pow(left, right)
}

/// Evaluates PHP's `round(...)` over one value and an optional precision expression.
fn eval_builtin_round(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            values.round(value, None)
        }
        [value, precision] => {
            let value = eval_expr(value, context, scope, values)?;
            let precision = eval_expr(precision, context, scope, values)?;
            values.round(value, Some(precision))
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Evaluates PHP's `sqrt(...)` over one eval expression.
fn eval_builtin_sqrt(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.sqrt(value)
}

/// Evaluates PHP's `strrev(...)` over one eval expression.
fn eval_builtin_strrev(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    values.strrev(value)
}

/// Evaluates PHP floating-point binary math builtins over two eval expressions.
fn eval_builtin_float_binary(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_float_binary_result(name, left, right, values)
}

/// Dispatches an evaluated pair through the matching PHP float math hook.
fn eval_float_binary_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "fdiv" => values.fdiv(left, right),
        "fmod" => values.fmod(left, right),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP numeric `min(...)` and `max(...)` over eval expressions.
fn eval_builtin_min_max(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() < 2 {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_min_max_result(name, &evaluated_args, values)
}

/// Selects the smallest or largest evaluated cell using runtime comparison hooks.
fn eval_min_max_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some((&first, rest)) = evaluated_args.split_first() else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let op = match name {
        "min" => EvalBinOp::Lt,
        "max" => EvalBinOp::Gt,
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    let mut selected = first;
    for candidate in rest {
        let better = values.compare(op, *candidate, selected)?;
        if values.truthy(better)? {
            selected = *candidate;
        }
    }
    Ok(selected)
}

/// Evaluates PHP scalar cast builtins over one eval expression.
fn eval_builtin_cast(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_cast_result(name, value, values)
}

/// Dispatches an already evaluated value through the matching PHP cast hook.
fn eval_cast_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "intval" => values.cast_int(value),
        "floatval" => values.cast_float(value),
        "strval" => values.cast_string(value),
        "boolval" => values.cast_bool(value),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates PHP's `gettype(...)` over one eval expression.
fn eval_builtin_gettype(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_gettype_result(value, values)
}

/// Converts one boxed runtime tag into PHP's `gettype()` spelling.
fn eval_gettype_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    values.string(eval_gettype_name(tag))
}

/// Returns the PHP-visible type name for a concrete eval runtime tag.
fn eval_gettype_name(tag: u64) -> &'static str {
    match tag {
        EVAL_TAG_INT => "integer",
        EVAL_TAG_FLOAT => "double",
        EVAL_TAG_STRING => "string",
        EVAL_TAG_BOOL => "boolean",
        EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => "array",
        EVAL_TAG_OBJECT => "object",
        EVAL_TAG_RESOURCE => "resource",
        EVAL_TAG_NULL => "NULL",
        _ => "NULL",
    }
}

/// Evaluates PHP scalar/container type predicate builtins over one eval expression.
fn eval_builtin_type_predicate(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_type_predicate_result(name, value, values)
}

/// Converts a concrete runtime tag into a PHP `is_*` predicate result.
fn eval_type_predicate_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let tag = values.type_tag(value)?;
    let result = match name {
        "is_int" | "is_integer" | "is_long" => tag == EVAL_TAG_INT,
        "is_float" | "is_double" | "is_real" => tag == EVAL_TAG_FLOAT,
        "is_string" => tag == EVAL_TAG_STRING,
        "is_bool" => tag == EVAL_TAG_BOOL,
        "is_null" => tag == EVAL_TAG_NULL,
        "is_array" => matches!(tag, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC),
        "is_resource" => tag == EVAL_TAG_RESOURCE,
        "is_numeric" => {
            tag == EVAL_TAG_INT
                || tag == EVAL_TAG_FLOAT
                || (tag == EVAL_TAG_STRING && eval_is_numeric_string(&values.string_bytes(value)?))
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(result)
}

/// Matches the static backend's legacy ASCII numeric-string scan.
fn eval_is_numeric_string(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let mut index = 0;
    let mut consumed_digits = 0;
    if bytes[index] == b'-' {
        index += 1;
        if index >= bytes.len() {
            return false;
        }
    }

    while index < bytes.len() {
        if bytes[index] == b'.' {
            index += 1;
            break;
        }
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            return false;
        }
        consumed_digits += 1;
        index += 1;
    }

    consumed_digits > 0
}

/// Evaluates PHP's `hash_equals(...)` over two eval expressions.
fn eval_builtin_hash_equals(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [known, user] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let known = eval_expr(known, context, scope, values)?;
    let user = eval_expr(user, context, scope, values)?;
    eval_hash_equals_result(known, user, values)
}

/// Compares two converted strings with PHP `hash_equals()` semantics.
fn eval_hash_equals_result(
    known: RuntimeCellHandle,
    user: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let known = values.string_bytes(known)?;
    let user = values.string_bytes(user)?;
    if known.len() != user.len() {
        return values.bool_value(false);
    }
    let mut diff = 0u8;
    for (known, user) in known.iter().zip(user.iter()) {
        diff |= known ^ user;
    }
    values.bool_value(diff == 0)
}

/// Evaluates PHP string comparison builtins over two eval expressions.
fn eval_builtin_string_compare(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [left, right] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let left = eval_expr(left, context, scope, values)?;
    let right = eval_expr(right, context, scope, values)?;
    eval_string_compare_result(name, left, right, values)
}

/// Compares two converted strings and returns -1, 0, or 1.
fn eval_string_compare_result(
    name: &str,
    left: RuntimeCellHandle,
    right: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut left = values.string_bytes(left)?;
    let mut right = values.string_bytes(right)?;
    match name {
        "strcmp" => {}
        "strcasecmp" => {
            left.make_ascii_lowercase();
            right.make_ascii_lowercase();
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let result = match left.cmp(&right) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    values.int(result)
}

/// Evaluates PHP's byte-string search predicates over two eval expressions.
fn eval_builtin_string_search(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_search_result(name, haystack, needle, values)
}

/// Checks one converted haystack for one converted needle using PHP byte-string semantics.
fn eval_string_search_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let matched = match name {
        "str_contains" => {
            needle.is_empty()
                || haystack
                    .windows(needle.len())
                    .any(|window| window == needle)
        }
        "str_starts_with" => haystack.starts_with(&needle),
        "str_ends_with" => haystack.ends_with(&needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    values.bool_value(matched)
}

/// Evaluates PHP byte-string position builtins over two eval expressions.
fn eval_builtin_string_position(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [haystack, needle] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let haystack = eval_expr(haystack, context, scope, values)?;
    let needle = eval_expr(needle, context, scope, values)?;
    eval_string_position_result(name, haystack, needle, values)
}

/// Returns the first or last byte offset of a converted needle, or PHP `false`.
fn eval_string_position_result(
    name: &str,
    haystack: RuntimeCellHandle,
    needle: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let haystack = values.string_bytes(haystack)?;
    let needle = values.string_bytes(needle)?;
    let position = match name {
        "strpos" if needle.is_empty() => Some(0),
        "strpos" => haystack
            .windows(needle.len())
            .position(|window| window == needle),
        "strrpos" if needle.is_empty() => Some(haystack.len()),
        "strrpos" => haystack
            .windows(needle.len())
            .rposition(|window| window == needle),
        _ => return Err(EvalStatus::UnsupportedConstruct),
    };
    match position {
        Some(position) => {
            let position = i64::try_from(position).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(position)
        }
        None => values.bool_value(false),
    }
}

const PHP_DEFAULT_TRIM_MASK: &[u8] = b" \n\r\t\x0B\x0C\0";

/// Evaluates PHP trim-like string builtins over one eval expression and optional mask.
fn eval_builtin_trim_like(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_trim_like_result(name, value, None, values)
        }
        [value, mask] => {
            let value = eval_expr(value, context, scope, values)?;
            let mask = eval_expr(mask, context, scope, values)?;
            eval_trim_like_result(name, value, Some(mask), values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Trims one converted string using PHP's default mask or a caller-provided byte mask.
fn eval_trim_like_result(
    name: &str,
    value: RuntimeCellHandle,
    mask: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let explicit_mask;
    let trim_mask = if let Some(mask) = mask {
        explicit_mask = values.string_bytes(mask)?;
        explicit_mask.as_slice()
    } else {
        PHP_DEFAULT_TRIM_MASK
    };

    let mut start = 0;
    let mut end = bytes.len();
    if matches!(name, "trim" | "ltrim") {
        while start < end && trim_mask.contains(&bytes[start]) {
            start += 1;
        }
    }
    if matches!(name, "trim" | "rtrim" | "chop") {
        while end > start && trim_mask.contains(&bytes[end - 1]) {
            end -= 1;
        }
    }
    if !matches!(name, "trim" | "ltrim" | "rtrim" | "chop") {
        return Err(EvalStatus::UnsupportedConstruct);
    }

    let value =
        String::from_utf8(bytes[start..end].to_vec()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates PHP ASCII case-conversion string builtins over one eval expression.
fn eval_builtin_string_case(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_string_case_result(name, value, values)
}

/// Converts one eval value through PHP string conversion and ASCII case mapping.
fn eval_string_case_result(
    name: &str,
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut bytes = values.string_bytes(value)?;
    match name {
        "strtolower" => {
            for byte in &mut bytes {
                if byte.is_ascii_uppercase() {
                    *byte += b'a' - b'A';
                }
            }
        }
        "strtoupper" => {
            for byte in &mut bytes {
                if byte.is_ascii_lowercase() {
                    *byte -= b'a' - b'A';
                }
            }
        }
        "ucfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_lowercase()) {
                bytes[0] -= b'a' - b'A';
            }
        }
        "lcfirst" => {
            if bytes.first().is_some_and(|byte| byte.is_ascii_uppercase()) {
                bytes[0] += b'a' - b'A';
            }
        }
        _ => return Err(EvalStatus::UnsupportedConstruct),
    }
    let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.string(&value)
}

/// Evaluates nested `eval(...)` calls against the current materialized scope.
fn eval_nested_eval(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [code] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let code = eval_expr(code, context, scope, values)?;
    let code = values.string_bytes(code)?;
    let program = parse_fragment(&code).map_err(|_| EvalStatus::ParseError)?;
    execute_program_with_context(context, &program, scope, values)
}

/// Evaluates the builtin `strlen(...)` for one PHP-coerced string argument.
fn eval_builtin_strlen(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let bytes = values.string_bytes(value)?;
    let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(len)
}

/// Evaluates the builtin `ord(...)` for the first byte of one coerced string.
fn eval_builtin_ord(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    eval_ord_result(value, values)
}

/// Returns the first byte of one converted string, or zero for an empty string.
fn eval_ord_result(
    value: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    values.int(i64::from(bytes.first().copied().unwrap_or(0)))
}

/// Evaluates the builtin `count(...)` for one runtime array-like argument.
fn eval_builtin_count(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let [value] = args else {
        return Err(EvalStatus::RuntimeFatal);
    };
    let value = eval_expr(value, context, scope, values)?;
    let len = values.array_len(value)?;
    let len = i64::try_from(len).map_err(|_| EvalStatus::RuntimeFatal)?;
    values.int(len)
}

/// Evaluates an eval-declared user function with PHP-style argument binding.
fn eval_dynamic_function(
    function: &EvalFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args =
        eval_function_call_args(function.params(), args, context, caller_scope, values)?;
    eval_dynamic_function_with_values(function, evaluated_args, context, values)
}

/// Evaluates and binds function-like arguments to parameter order.
fn eval_function_call_args(
    params: &[String],
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let evaluated_args = eval_call_arg_values(args, context, caller_scope, values)?;
    bind_evaluated_function_args(params, evaluated_args)
}

/// Evaluates source-order call arguments while preserving named-argument metadata.
fn eval_call_arg_values(
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    let mut saw_named = false;

    for arg in args {
        if arg.is_spread() {
            if saw_named {
                return Err(EvalStatus::RuntimeFatal);
            }
            let spread = eval_expr(arg.value(), context, caller_scope, values)?;
            if !values.is_array_like(spread)? {
                return Err(EvalStatus::RuntimeFatal);
            }
            append_unpacked_call_arg_values(spread, &mut evaluated_args, &mut saw_named, values)?;
            continue;
        }

        if let Some(name) = arg.name() {
            saw_named = true;
            let value = eval_expr(arg.value(), context, caller_scope, values)?;
            evaluated_args.push(EvaluatedCallArg {
                name: Some(name.to_string()),
                value,
            });
            continue;
        }

        if saw_named {
            return Err(EvalStatus::RuntimeFatal);
        }
        let value = eval_expr(arg.value(), context, caller_scope, values)?;
        evaluated_args.push(EvaluatedCallArg { name: None, value });
    }

    Ok(evaluated_args)
}

/// Converts a `call_user_func_array` argument array into ordered call arguments.
fn eval_array_call_arg_values(
    arg_array: RuntimeCellHandle,
    values: &mut impl RuntimeValueOps,
) -> Result<Vec<EvaluatedCallArg>, EvalStatus> {
    let len = values.array_len(arg_array)?;
    let mut evaluated_args = Vec::with_capacity(len);
    let mut saw_named = false;
    append_unpacked_call_arg_values(arg_array, &mut evaluated_args, &mut saw_named, values)?;
    Ok(evaluated_args)
}

/// Appends one unpacked array's values using PHP named-argument key semantics.
fn append_unpacked_call_arg_values(
    array: RuntimeCellHandle,
    evaluated_args: &mut Vec<EvaluatedCallArg>,
    saw_named: &mut bool,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let len = values.array_len(array)?;
    for position in 0..len {
        let key = values.array_iter_key(array, position)?;
        let value = values.array_get(array, key)?;
        match values.type_tag(key)? {
            EVAL_TAG_INT => {
                if *saw_named {
                    return Err(EvalStatus::RuntimeFatal);
                }
                evaluated_args.push(EvaluatedCallArg { name: None, value });
            }
            EVAL_TAG_STRING => {
                *saw_named = true;
                let name = values.string_bytes(key)?;
                let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
                evaluated_args.push(EvaluatedCallArg {
                    name: Some(name),
                    value,
                });
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        }
    }
    Ok(())
}

/// Binds evaluated positional and named values to declared parameter order.
fn bind_evaluated_function_args(
    params: &[String],
    evaluated_args: Vec<EvaluatedCallArg>,
) -> Result<Vec<RuntimeCellHandle>, EvalStatus> {
    let mut bound_args = vec![None; params.len()];
    let mut next_positional = 0;

    for arg in evaluated_args {
        if let Some(name) = arg.name {
            bind_dynamic_named_arg(params, &mut bound_args, &name, arg.value)?;
        } else {
            bind_dynamic_positional_arg(&mut bound_args, &mut next_positional, arg.value)?;
        }
    }

    bound_args
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(EvalStatus::RuntimeFatal)
}

/// Binds one positional dynamic-call value to the next declared parameter slot.
fn bind_dynamic_positional_arg(
    bound_args: &mut [Option<RuntimeCellHandle>],
    next_positional: &mut usize,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    if *next_positional >= bound_args.len() || bound_args[*next_positional].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[*next_positional] = Some(value);
    *next_positional += 1;
    Ok(())
}

/// Binds one named dynamic-call value to the matching declared parameter slot.
fn bind_dynamic_named_arg(
    params: &[String],
    bound_args: &mut [Option<RuntimeCellHandle>],
    name: &str,
    value: RuntimeCellHandle,
) -> Result<(), EvalStatus> {
    let Some(param_index) = params.iter().position(|param| param == name) else {
        return Err(EvalStatus::RuntimeFatal);
    };
    if bound_args[param_index].is_some() {
        return Err(EvalStatus::RuntimeFatal);
    }
    bound_args[param_index] = Some(value);
    Ok(())
}

/// Evaluates an eval-declared function after its positional arguments are prepared.
fn eval_dynamic_function_with_values(
    function: &EvalFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut function_scope = ElephcEvalScope::new();
    for (name, value) in function.params().iter().zip(evaluated_args) {
        function_scope.set(name.clone(), value, ScopeCellOwnership::Borrowed);
    }
    context.push_function(function.name());
    let result = execute_statements(function.body(), context, &mut function_scope, values);
    context.pop_function();
    match result? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Evaluates a registered AOT function through its descriptor-compatible invoker.
fn eval_native_function(
    function: NativeFunction,
    args: &[EvalCallArg],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let evaluated_args = if function.param_names().len() == function.param_count() {
        eval_function_call_args(function.param_names(), args, context, caller_scope, values)?
    } else {
        eval_positional_call_arg_values(args, context, caller_scope, values)?
    };
    eval_native_function_with_values(function, evaluated_args, values)
}

/// Invokes a registered AOT function after its positional arguments are prepared.
fn eval_native_function_with_values(
    function: NativeFunction,
    evaluated_args: Vec<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if evaluated_args.len() != function.param_count() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let arg_array = values.array_new(evaluated_args.len())?;
    for (index, value) in evaluated_args.into_iter().enumerate() {
        let index = values.int(index as i64)?;
        let _ = values.array_set(arg_array, index, value)?;
    }
    let result = unsafe { function.call(arg_array) };
    values.release(arg_array)?;
    if result.is_null() {
        return Err(EvalStatus::RuntimeFatal);
    }
    Ok(result)
}

/// Evaluates an indexed array literal into a boxed runtime Mixed array.
fn eval_indexed_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array = values.array_new(elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        let EvalArrayElement::Value(element) = element else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        let index = values.int(index as i64)?;
        let value = eval_expr(element, context, scope, values)?;
        let _ = values.array_set(array, index, value)?;
    }
    Ok(array)
}

/// Evaluates an associative array literal into a boxed runtime Mixed hash.
fn eval_assoc_array(
    elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array = values.assoc_new(elements.len())?;
    let mut next_key = None;
    for element in elements {
        let (key, value) = match element {
            EvalArrayElement::Value(value) => {
                let key = match next_key {
                    Some(next_key) => next_key,
                    None => values.int(0)?,
                };
                let one = values.int(1)?;
                next_key = Some(values.add(key, one)?);
                (key, value)
            }
            EvalArrayElement::KeyValue { key, value } => {
                let key = eval_expr(key, context, scope, values)?;
                next_key = eval_array_next_key_after_explicit_key(key, next_key, values)?;
                (key, value)
            }
        };
        let value = eval_expr(value, context, scope, values)?;
        let _ = values.array_set(array, key, value)?;
    }
    Ok(array)
}

/// Advances an array literal's automatic key after an integer-normalized explicit key.
fn eval_array_next_key_after_explicit_key(
    key: RuntimeCellHandle,
    current_next_key: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let key = match values.type_tag(key)? {
        EVAL_TAG_INT => key,
        EVAL_TAG_STRING => {
            let bytes = values.string_bytes(key)?;
            let Some(key) = eval_numeric_string_array_key(&bytes) else {
                return Ok(current_next_key);
            };
            values.int(key)?
        }
        _ => values.cast_int(key)?,
    };
    let one = values.int(1)?;
    let candidate = values.add(key, one)?;
    let replace = if let Some(current_next_key) = current_next_key {
        let is_greater = values.compare(EvalBinOp::Gt, candidate, current_next_key)?;
        values.truthy(is_greater)?
    } else {
        true
    };
    Ok(if replace {
        Some(candidate)
    } else {
        current_next_key
    })
}

/// Parses PHP integer-string array keys that normalize to integer keys.
fn eval_numeric_string_array_key(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() {
        return None;
    }

    let (negative, digits) = if bytes[0] == b'-' {
        if bytes.len() == 1 {
            return None;
        }
        (true, &bytes[1..])
    } else {
        (false, bytes)
    };

    if digits[0] == b'0' {
        return if !negative && digits.len() == 1 {
            Some(0)
        } else {
            None
        };
    }
    if digits.iter().any(|byte| !byte.is_ascii_digit()) {
        return None;
    }

    let limit = if negative {
        i64::MAX as u128 + 1
    } else {
        i64::MAX as u128
    };
    let mut value = 0u128;
    for digit in digits {
        value = (value * 10) + u128::from(digit - b'0');
        if value > limit {
            return None;
        }
    }

    if negative {
        if value == i64::MAX as u128 + 1 {
            Some(i64::MIN)
        } else {
            Some(-(value as i64))
        }
    } else {
        Some(value as i64)
    }
}

/// Converts one EvalIR constant into a runtime-cell handle.
fn eval_const(
    value: &EvalConst,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match value {
        EvalConst::Null => values.null(),
        EvalConst::Bool(value) => values.bool_value(*value),
        EvalConst::Int(value) => values.int(*value),
        EvalConst::Float(value) => values.float(*value),
        EvalConst::String(value) => values.string(value),
    }
}

/// Resolves one eval magic constant against fragment and dynamic-call metadata.
fn eval_magic_const(
    magic: &EvalMagicConst,
    context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match magic {
        EvalMagicConst::File => values.string(&context.eval_file_magic()),
        EvalMagicConst::Dir => values.string(context.call_dir()),
        EvalMagicConst::Line(line) => values.int(*line),
        EvalMagicConst::Function => values.string(context.current_function().unwrap_or("")),
        EvalMagicConst::Method => values.string(context.current_function().unwrap_or("")),
        EvalMagicConst::Class | EvalMagicConst::Namespace | EvalMagicConst::Trait => {
            values.string("")
        }
    }
}

/// Returns the current interpreter availability status for the ABI stub.
pub fn current_stub_status() -> EvalStatus {
    EvalStatus::UnsupportedConstruct
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::c_void;

    use crate::parser::parse_fragment;
    use crate::value::RuntimeCell;

    use super::*;

    /// Test-only array key representation for fake indexed and associative arrays.
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    enum FakeKey {
        Int(i64),
        String(String),
    }

    /// Test-only runtime value representation used behind opaque cell handles.
    #[derive(Clone, Debug, PartialEq)]
    enum FakeValue {
        Null,
        Bool(bool),
        Int(i64),
        Float(f64),
        String(String),
        Array(Vec<RuntimeCellHandle>),
        Assoc(Vec<(FakeKey, RuntimeCellHandle)>),
        Object(HashMap<String, RuntimeCellHandle>),
        Resource(i64),
    }

    /// Test runtime hooks that allocate stable fake handles and record echo output.
    #[derive(Default)]
    struct FakeOps {
        next_id: usize,
        values: HashMap<usize, FakeValue>,
        output: String,
        releases: Vec<RuntimeCellHandle>,
    }

    impl FakeOps {
        /// Allocates one fake runtime cell and returns its opaque handle.
        fn alloc(&mut self, value: FakeValue) -> RuntimeCellHandle {
            self.next_id += 1;
            let id = self.next_id;
            self.values.insert(id, value);
            RuntimeCellHandle::from_raw(id as *mut RuntimeCell)
        }

        /// Reads a fake runtime cell by opaque handle.
        fn get(&self, handle: RuntimeCellHandle) -> FakeValue {
            let id = handle.as_ptr() as usize;
            self.values.get(&id).cloned().expect("fake cell missing")
        }

        /// Converts a fake runtime cell into a normalized fake PHP array key.
        fn key(&self, handle: RuntimeCellHandle) -> Result<FakeKey, EvalStatus> {
            let value = self.get(handle);
            match value {
                FakeValue::Int(value) => Ok(FakeKey::Int(value)),
                FakeValue::String(value) => eval_numeric_string_array_key(value.as_bytes())
                    .map(FakeKey::Int)
                    .map_or_else(|| Ok(FakeKey::String(value)), Ok),
                value => Ok(FakeKey::Int(self.fake_int(&value))),
            }
        }

        /// Allocates a fake runtime cell for an existing PHP array key.
        fn alloc_key(&mut self, key: &FakeKey) -> Result<RuntimeCellHandle, EvalStatus> {
            match key {
                FakeKey::Int(value) => self.int(*value),
                FakeKey::String(value) => self.string(value),
            }
        }
    }

    impl RuntimeValueOps for FakeOps {
        /// Creates a fake indexed array cell.
        fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Array(Vec::with_capacity(capacity))))
        }

        /// Creates a fake associative array cell.
        fn assoc_new(&mut self, _capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Assoc(Vec::new())))
        }

        /// Reads one fake indexed array element.
        fn array_get(
            &mut self,
            array: RuntimeCellHandle,
            index: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let key = self.key(index)?;
            match self.get(array) {
                FakeValue::Array(elements) => {
                    let FakeKey::Int(index) = key else {
                        return self.null();
                    };
                    if index < 0 {
                        return self.null();
                    }
                    elements
                        .get(index as usize)
                        .copied()
                        .map_or_else(|| self.null(), Ok)
                }
                FakeValue::Assoc(entries) => entries
                    .iter()
                    .find_map(|(entry_key, value)| (entry_key == &key).then_some(*value))
                    .map_or_else(|| self.null(), Ok),
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Checks whether a fake array has the requested key without reading its value.
        fn array_key_exists(
            &mut self,
            key: RuntimeCellHandle,
            array: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let key = self.key(key)?;
            let exists = match self.get(array) {
                FakeValue::Array(elements) => {
                    matches!(key, FakeKey::Int(index) if index >= 0 && (index as usize) < elements.len())
                }
                FakeValue::Assoc(entries) => entries.iter().any(|(entry_key, _)| entry_key == &key),
                _ => return Err(EvalStatus::UnsupportedConstruct),
            };
            self.bool_value(exists)
        }

        /// Returns one fake foreach key by insertion-order position.
        fn array_iter_key(
            &mut self,
            array: RuntimeCellHandle,
            position: usize,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match self.get(array) {
                FakeValue::Array(elements) if position < elements.len() => {
                    self.int(position as i64)
                }
                FakeValue::Assoc(entries) => {
                    let Some((key, _)) = entries.get(position) else {
                        return self.null();
                    };
                    self.alloc_key(key)
                }
                FakeValue::Array(_) => self.null(),
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Writes one fake indexed or associative array element.
        fn array_set(
            &mut self,
            array: RuntimeCellHandle,
            index: RuntimeCellHandle,
            value: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let key = self.key(index)?;
            let id = array.as_ptr() as usize;
            match self.values.get_mut(&id) {
                Some(FakeValue::Array(elements)) => {
                    let FakeKey::Int(index) = key else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    if index < 0 {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    let index = index as usize;
                    while elements.len() <= index {
                        elements.push(RuntimeCellHandle::from_raw(std::ptr::null_mut()));
                    }
                    elements[index] = value;
                }
                Some(FakeValue::Assoc(entries)) => {
                    if let Some((_, existing_value)) =
                        entries.iter_mut().find(|(entry_key, _)| entry_key == &key)
                    {
                        *existing_value = value;
                    } else {
                        entries.push((key, value));
                    }
                }
                _ => return Err(EvalStatus::UnsupportedConstruct),
            }
            Ok(array)
        }

        /// Reads one fake object property by name.
        fn property_get(
            &mut self,
            object: RuntimeCellHandle,
            property: &str,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match self.get(object) {
                FakeValue::Object(properties) => properties
                    .get(property)
                    .copied()
                    .map_or_else(|| self.null(), Ok),
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Writes one fake object property by name.
        fn property_set(
            &mut self,
            object: RuntimeCellHandle,
            property: &str,
            value: RuntimeCellHandle,
        ) -> Result<(), EvalStatus> {
            let id = object.as_ptr() as usize;
            let Some(FakeValue::Object(properties)) = self.values.get_mut(&id) else {
                return Err(EvalStatus::UnsupportedConstruct);
            };
            properties.insert(property.to_string(), value);
            Ok(())
        }

        /// Calls one fake object method by name.
        fn method_call(
            &mut self,
            object: RuntimeCellHandle,
            method: &str,
            args: Vec<RuntimeCellHandle>,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(object), method) {
                (FakeValue::Object(_), "answer") if args.is_empty() => self.int(42),
                (FakeValue::Object(properties), "read_x") => {
                    if !args.is_empty() {
                        return Err(EvalStatus::UnsupportedConstruct);
                    }
                    properties.get("x").copied().map_or_else(|| self.null(), Ok)
                }
                (FakeValue::Object(properties), "add_x") => {
                    let [arg] = args.as_slice() else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let x = properties
                        .get("x")
                        .copied()
                        .ok_or(EvalStatus::RuntimeFatal)?;
                    let FakeValue::Int(x) = self.get(x) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let FakeValue::Int(arg) = self.get(*arg) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    self.int(x + arg)
                }
                (FakeValue::Object(properties), "add2_x") => {
                    let [left, right] = args.as_slice() else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let x = properties
                        .get("x")
                        .copied()
                        .ok_or(EvalStatus::RuntimeFatal)?;
                    let FakeValue::Int(x) = self.get(x) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let FakeValue::Int(left) = self.get(*left) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    let FakeValue::Int(right) = self.get(*right) else {
                        return Err(EvalStatus::UnsupportedConstruct);
                    };
                    self.int(x + left + right)
                }
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Returns the visible element count for fake array values.
        fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus> {
            match self.get(array) {
                FakeValue::Array(elements) => Ok(elements.len()),
                FakeValue::Assoc(entries) => Ok(entries.len()),
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }

        /// Returns whether a fake runtime cell is an indexed or associative array.
        fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
            Ok(matches!(
                self.get(value),
                FakeValue::Array(_) | FakeValue::Assoc(_)
            ))
        }

        /// Returns whether a fake runtime cell is null.
        fn is_null(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
            Ok(matches!(self.get(value), FakeValue::Null))
        }

        /// Returns the fake runtime tag corresponding to a test value.
        fn type_tag(&mut self, value: RuntimeCellHandle) -> Result<u64, EvalStatus> {
            Ok(match self.get(value) {
                FakeValue::Int(_) => EVAL_TAG_INT,
                FakeValue::String(_) => EVAL_TAG_STRING,
                FakeValue::Float(_) => EVAL_TAG_FLOAT,
                FakeValue::Bool(_) => EVAL_TAG_BOOL,
                FakeValue::Array(_) => EVAL_TAG_ARRAY,
                FakeValue::Assoc(_) => EVAL_TAG_ASSOC,
                FakeValue::Object(_) => EVAL_TAG_OBJECT,
                FakeValue::Resource(_) => EVAL_TAG_RESOURCE,
                FakeValue::Null => EVAL_TAG_NULL,
            })
        }

        /// Records fake releases without freeing handles needed for assertions.
        fn release(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
            self.releases.push(value);
            Ok(())
        }

        /// Creates a fake null cell.
        fn null(&mut self) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Null))
        }

        /// Creates a fake bool cell.
        fn bool_value(&mut self, value: bool) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Bool(value)))
        }

        /// Creates a fake int cell.
        fn int(&mut self, value: i64) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Int(value)))
        }

        /// Creates a fake float cell.
        fn float(&mut self, value: f64) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Float(value)))
        }

        /// Creates a fake string cell.
        fn string(&mut self, value: &str) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::String(value.to_string())))
        }

        /// Casts a fake runtime cell to a fake integer cell.
        fn cast_int(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            let value = self.fake_int(&value);
            self.int(value)
        }

        /// Casts a fake runtime cell to a fake float cell.
        fn cast_float(
            &mut self,
            value: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            let value = self.fake_numeric(&value);
            self.float(value)
        }

        /// Casts a fake runtime cell to a fake string cell.
        fn cast_string(
            &mut self,
            value: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.stringify(value);
            self.string(&value)
        }

        /// Casts a fake runtime cell to a fake boolean cell.
        fn cast_bool(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            let value = self.fake_truthy(&value);
            self.bool_value(value)
        }

        /// Computes fake PHP absolute value while preserving float payloads.
        fn abs(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            match self.get(value) {
                FakeValue::Float(value) => self.float(value.abs()),
                value => self.int(self.fake_int(&value).wrapping_abs()),
            }
        }

        /// Computes fake PHP ceiling through numeric conversion as a float result.
        fn ceil(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            self.float(self.fake_numeric(&value).ceil())
        }

        /// Computes fake PHP floor through numeric conversion as a float result.
        fn floor(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            self.float(self.fake_numeric(&value).floor())
        }

        /// Computes fake PHP square root through numeric conversion as a float result.
        fn sqrt(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.get(value);
            self.float(self.fake_numeric(&value).sqrt())
        }

        /// Reverses a fake string byte-wise for interpreter tests.
        fn strrev(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let mut bytes = self.stringify(value).into_bytes();
            bytes.reverse();
            let value = String::from_utf8(bytes).map_err(|_| EvalStatus::RuntimeFatal)?;
            self.string(&value)
        }

        /// Divides fake numeric cells with PHP `fdiv()` zero handling.
        fn fdiv(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.fake_numeric(&self.get(left));
            let right = self.fake_numeric(&self.get(right));
            self.float(left / right)
        }

        /// Computes fake floating-point modulo for interpreter tests.
        fn fmod(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.fake_numeric(&self.get(left));
            let right = self.fake_numeric(&self.get(right));
            self.float(left % right)
        }

        /// Adds fake numeric cells for interpreter tests.
        fn add(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(left), self.get(right)) {
                (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left + right),
                (left, right) => self.float(self.fake_numeric(&left) + self.fake_numeric(&right)),
            }
        }

        /// Subtracts fake numeric cells for interpreter tests.
        fn sub(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(left), self.get(right)) {
                (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left - right),
                (left, right) => self.float(self.fake_numeric(&left) - self.fake_numeric(&right)),
            }
        }

        /// Multiplies fake numeric cells for interpreter tests.
        fn mul(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(left), self.get(right)) {
                (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left * right),
                (left, right) => self.float(self.fake_numeric(&left) * self.fake_numeric(&right)),
            }
        }

        /// Divides fake numeric cells for interpreter tests.
        fn div(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let right = self.fake_numeric(&self.get(right));
            if right == 0.0 {
                return Err(EvalStatus::RuntimeFatal);
            }
            let left = self.fake_numeric(&self.get(left));
            self.float(left / right)
        }

        /// Computes fake integer modulo for interpreter tests.
        fn modulo(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let right = self.fake_int(&self.get(right));
            if right == 0 {
                return Err(EvalStatus::RuntimeFatal);
            }
            let left = self.fake_int(&self.get(left));
            self.int(left % right)
        }

        /// Raises fake numeric cells for interpreter tests.
        fn pow(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.fake_numeric(&self.get(left));
            let right = self.fake_numeric(&self.get(right));
            self.float(left.powf(right))
        }

        /// Rounds fake numeric cells with PHP's optional decimal precision.
        fn round(
            &mut self,
            value: RuntimeCellHandle,
            precision: Option<RuntimeCellHandle>,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.fake_numeric(&self.get(value));
            let precision = precision
                .map(|precision| self.fake_int(&self.get(precision)))
                .unwrap_or(0);
            let multiplier = 10_f64.powf(precision as f64);
            self.float((value * multiplier).round() / multiplier)
        }

        /// Applies fake integer bitwise and shift operations for interpreter tests.
        fn bitwise(
            &mut self,
            op: EvalBinOp,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.fake_int(&self.get(left));
            let right = self.fake_int(&self.get(right));
            let value = match op {
                EvalBinOp::BitAnd => left & right,
                EvalBinOp::BitOr => left | right,
                EvalBinOp::BitXor => left ^ right,
                EvalBinOp::ShiftLeft => {
                    if right < 0 {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    left.wrapping_shl(right as u32)
                }
                EvalBinOp::ShiftRight => {
                    if right < 0 {
                        return Err(EvalStatus::RuntimeFatal);
                    }
                    left.wrapping_shr(right as u32)
                }
                _ => return Err(EvalStatus::UnsupportedConstruct),
            };
            self.int(value)
        }

        /// Applies fake integer bitwise NOT for interpreter tests.
        fn bit_not(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
            let value = self.fake_int(&self.get(value));
            self.int(!value)
        }

        /// Concatenates fake cells with simple string conversion for interpreter tests.
        fn concat(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.stringify(left);
            let right = self.stringify(right);
            self.string(&(left + &right))
        }

        /// Compares fake scalar cells and returns a fake PHP boolean.
        fn compare(
            &mut self,
            op: EvalBinOp,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let result = match op {
                EvalBinOp::LooseEq => self.loose_eq(left, right),
                EvalBinOp::LooseNotEq => !self.loose_eq(left, right),
                EvalBinOp::StrictEq => self.strict_eq(left, right),
                EvalBinOp::StrictNotEq => !self.strict_eq(left, right),
                EvalBinOp::Lt => self.numeric(left)? < self.numeric(right)?,
                EvalBinOp::LtEq => self.numeric(left)? <= self.numeric(right)?,
                EvalBinOp::Gt => self.numeric(left)? > self.numeric(right)?,
                EvalBinOp::GtEq => self.numeric(left)? >= self.numeric(right)?,
                EvalBinOp::Add
                | EvalBinOp::Sub
                | EvalBinOp::Mul
                | EvalBinOp::Div
                | EvalBinOp::Mod
                | EvalBinOp::Pow
                | EvalBinOp::BitAnd
                | EvalBinOp::BitOr
                | EvalBinOp::BitXor
                | EvalBinOp::ShiftLeft
                | EvalBinOp::ShiftRight
                | EvalBinOp::Concat
                | EvalBinOp::Spaceship
                | EvalBinOp::LogicalAnd
                | EvalBinOp::LogicalOr
                | EvalBinOp::LogicalXor => {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
            };
            self.bool_value(result)
        }

        /// Compares fake numeric cells and returns a PHP spaceship integer.
        fn spaceship(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            let left = self.numeric(left)?;
            let right = self.numeric(right)?;
            let value = if left < right {
                -1
            } else if left > right {
                1
            } else {
                0
            };
            self.int(value)
        }

        /// Appends fake echo output for interpreter tests.
        fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
            let value = self.stringify(value);
            self.output.push_str(&value);
            Ok(())
        }

        /// Casts one fake runtime cell to bytes for nested eval parsing.
        fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus> {
            Ok(self.stringify(value).into_bytes())
        }

        /// Returns PHP-like truthiness for fake runtime cells.
        fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus> {
            Ok(match self.get(value) {
                FakeValue::Null => false,
                FakeValue::Bool(value) => value,
                FakeValue::Int(value) => value != 0,
                FakeValue::Float(value) => value != 0.0,
                FakeValue::String(value) => !value.is_empty() && value != "0",
                FakeValue::Array(value) => !value.is_empty(),
                FakeValue::Assoc(value) => !value.is_empty(),
                FakeValue::Object(_) => true,
                FakeValue::Resource(_) => true,
            })
        }
    }

    impl FakeOps {
        /// Compares fake scalar values with the same loose rules covered by eval tests.
        fn loose_eq(&self, left: RuntimeCellHandle, right: RuntimeCellHandle) -> bool {
            match (self.get(left), self.get(right)) {
                (FakeValue::Bool(left), right) => left == self.fake_truthy(&right),
                (left, FakeValue::Bool(right)) => self.fake_truthy(&left) == right,
                (FakeValue::Null, FakeValue::Null) => true,
                (FakeValue::Null, FakeValue::String(value))
                | (FakeValue::String(value), FakeValue::Null) => value.is_empty(),
                (FakeValue::String(left), FakeValue::String(right)) => {
                    match (left.parse::<f64>(), right.parse::<f64>()) {
                        (Ok(left), Ok(right)) => left == right,
                        _ => left == right,
                    }
                }
                (FakeValue::String(left), right) => left
                    .parse::<f64>()
                    .is_ok_and(|left| left == self.fake_numeric(&right)),
                (left, FakeValue::String(right)) => right
                    .parse::<f64>()
                    .is_ok_and(|right| self.fake_numeric(&left) == right),
                (left, right) => self.fake_numeric(&left) == self.fake_numeric(&right),
            }
        }

        /// Compares fake scalar values by PHP strict tag and payload equality.
        fn strict_eq(&self, left: RuntimeCellHandle, right: RuntimeCellHandle) -> bool {
            match (self.get(left), self.get(right)) {
                (FakeValue::Null, FakeValue::Null) => true,
                (FakeValue::Bool(left), FakeValue::Bool(right)) => left == right,
                (FakeValue::Int(left), FakeValue::Int(right)) => left == right,
                (FakeValue::Float(left), FakeValue::Float(right)) => left == right,
                (FakeValue::String(left), FakeValue::String(right)) => left == right,
                (FakeValue::Resource(left), FakeValue::Resource(right)) => left == right,
                _ => false,
            }
        }

        /// Converts one fake scalar cell to a numeric value for comparison tests.
        fn numeric(&self, handle: RuntimeCellHandle) -> Result<f64, EvalStatus> {
            Ok(self.fake_numeric(&self.get(handle)))
        }

        /// Converts a fake value to the numeric scalar used by comparison tests.
        fn fake_numeric(&self, value: &FakeValue) -> f64 {
            match value {
                FakeValue::Null => 0.0,
                FakeValue::Bool(false) => 0.0,
                FakeValue::Bool(true) => 1.0,
                FakeValue::Int(value) => *value as f64,
                FakeValue::Float(value) => *value,
                FakeValue::String(value) => value.parse::<f64>().unwrap_or(0.0),
                FakeValue::Array(value) => value.len() as f64,
                FakeValue::Assoc(value) => value.len() as f64,
                FakeValue::Object(_) => 1.0,
                FakeValue::Resource(value) => (*value + 1) as f64,
            }
        }

        /// Converts a fake value to the integer scalar used by modulo tests.
        fn fake_int(&self, value: &FakeValue) -> i64 {
            self.fake_numeric(value) as i64
        }

        /// Returns fake PHP truthiness for already-loaded test values.
        fn fake_truthy(&self, value: &FakeValue) -> bool {
            match value {
                FakeValue::Null => false,
                FakeValue::Bool(value) => *value,
                FakeValue::Int(value) => *value != 0,
                FakeValue::Float(value) => *value != 0.0,
                FakeValue::String(value) => !value.is_empty() && value != "0",
                FakeValue::Array(value) => !value.is_empty(),
                FakeValue::Assoc(value) => !value.is_empty(),
                FakeValue::Object(_) => true,
                FakeValue::Resource(_) => true,
            }
        }

        /// Converts a fake runtime cell to a PHP-like string for test echo/concat.
        fn stringify(&self, handle: RuntimeCellHandle) -> String {
            match self.get(handle) {
                FakeValue::Null => String::new(),
                FakeValue::Bool(false) => String::new(),
                FakeValue::Bool(true) => "1".to_string(),
                FakeValue::Int(value) => value.to_string(),
                FakeValue::Float(value) => value.to_string(),
                FakeValue::String(value) => value,
                FakeValue::Array(_) => "Array".to_string(),
                FakeValue::Assoc(_) => "Array".to_string(),
                FakeValue::Object(_) => "Object".to_string(),
                FakeValue::Resource(value) => format!("Resource id #{}", value + 1),
            }
        }
    }

    /// Test native invoker that returns the descriptor pointer as a runtime cell.
    unsafe extern "C" fn fake_native_return_descriptor(
        descriptor: *mut c_void,
        _args: *mut RuntimeCell,
    ) -> *mut RuntimeCell {
        descriptor.cast()
    }

    /// Verifies assignment writes a named scope entry and return reads it back.
    #[test]
    fn execute_program_stores_and_returns_scope_value() {
        let program = parse_fragment(b"$x = 3; return $x + 4;").expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.get(x), FakeValue::Int(3));
        assert_eq!(values.get(result), FakeValue::Int(7));
    }

    /// Verifies simple variable compound assignments read, compute, and write the scope value.
    #[test]
    fn execute_program_evaluates_compound_assignments() {
        let program =
            parse_fragment(br#"$x = 2; $x += 3; $x *= 4; $x -= 5; $s = "v"; $s .= $x; echo $s;"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.output, "v15");
        assert_eq!(values.get(x), FakeValue::Int(15));
    }

    /// Verifies division and modulo evaluate through fake runtime numeric hooks.
    #[test]
    fn execute_program_evaluates_division_and_modulo() {
        let program = parse_fragment(br#"$x = 20; $x /= 2; $x %= 6; echo $x; return 9 / 2;"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.output, "4");
        assert_eq!(values.get(x), FakeValue::Int(4));
        assert_eq!(values.get(result), FakeValue::Float(4.5));
    }

    /// Verifies exponentiation evaluates through fake runtime numeric hooks.
    #[test]
    fn execute_program_evaluates_exponentiation() {
        let program = parse_fragment(
            br#"$x = 2; $x **= 3; echo $x; echo ":"; echo -2 ** 2; return 2 ** 3 ** 2;"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.output, "8:-4");
        assert_eq!(values.get(x), FakeValue::Float(8.0));
        assert_eq!(values.get(result), FakeValue::Float(512.0));
    }

    /// Verifies bitwise and shift operators evaluate through fake runtime hooks.
    #[test]
    fn execute_program_evaluates_bitwise_and_shift_ops() {
        let program = parse_fragment(
            br#"$x = 6; $x &= 3; echo $x; echo ":";
$x = 4; $x |= 1; echo $x; echo ":";
$x = 7; $x ^= 3; echo $x; echo ":";
$x = 1; $x <<= 5; echo $x; echo ":";
$x = 64; $x >>= 3; echo $x; echo ":";
echo ~0; echo ":"; echo -16 >> 2;
return (1 << 4) | ((16 >> 2) ^ (3 & 1));"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2:5:4:32:8:-1:-4");
        assert_eq!(values.get(result), FakeValue::Int(21));
    }

    /// Verifies simple variable increment and decrement statements update the scope value.
    #[test]
    fn execute_program_evaluates_inc_dec_statements() {
        let program = parse_fragment(br#"$i = 1; $i++; ++$i; $i--; --$i; echo $i;"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let i = scope.visible_cell("i").expect("scope should contain i");

        assert_eq!(values.output, "1");
        assert_eq!(values.get(i), FakeValue::Int(1));
    }

    /// Verifies echo and unset operate through runtime hooks and scope metadata.
    #[test]
    fn execute_program_echoes_and_unsets_scope_value() {
        let program =
            parse_fragment(br#"echo "hi" . $name; unset($name);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let name = values.string(" Ada").expect("create fake string");
        scope.set("name", name, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "hi Ada");
        assert_eq!(values.get(result), FakeValue::Null);
        assert!(scope.entry("name").expect("unset marker").flags().unset);
    }

    /// Verifies comma-separated echo expressions are executed in source order.
    #[test]
    fn execute_program_echoes_comma_list() {
        let program = parse_fragment(br#"echo "a", $b, "c";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let b = values.string("b").expect("create fake string");
        scope.set("b", b, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "abc");
    }

    /// Verifies print writes output and returns integer 1.
    #[test]
    fn execute_program_print_returns_one() {
        let program = parse_fragment(br#"return print "p";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "p");
        assert_eq!(values.get(result), FakeValue::Int(1));
    }

    /// Verifies eval property reads and writes dispatch through runtime hooks.
    #[test]
    fn execute_program_reads_and_writes_object_property() {
        let program = parse_fragment(br#"$this->x = $this->x + 1; return $this->x;"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(1).expect("create fake int");
        let mut properties = HashMap::new();
        properties.insert("x".to_string(), x);
        let object = values.alloc(FakeValue::Object(properties));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(2));
        assert_eq!(
            values
                .property_get(object, "x")
                .map(|value| values.get(value))
                .expect("property should be readable"),
            FakeValue::Int(2)
        );
    }

    /// Verifies eval method calls dispatch through the runtime method hook.
    #[test]
    fn execute_program_calls_object_method() {
        let program = parse_fragment(br#"return $this->answer();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let object = values.alloc(FakeValue::Object(HashMap::new()));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(42));
    }

    /// Verifies eval method calls forward evaluated arguments to the runtime hook.
    #[test]
    fn execute_program_calls_object_method_with_argument() {
        let program = parse_fragment(br#"return $this->add_x(5);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(7).expect("create fake int");
        let mut properties = HashMap::new();
        properties.insert("x".to_string(), x);
        let object = values.alloc(FakeValue::Object(properties));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies eval method calls forward multiple evaluated arguments to the runtime hook.
    #[test]
    fn execute_program_calls_object_method_with_two_arguments() {
        let program =
            parse_fragment(br#"return $this->add2_x(5, 6);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(7).expect("create fake int");
        let mut properties = HashMap::new();
        properties.insert("x".to_string(), x);
        let object = values.alloc(FakeValue::Object(properties));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(18));
    }

    /// Verifies eval method calls forward numerically unpacked arguments.
    #[test]
    fn execute_program_calls_object_method_with_spread_arguments() {
        let program =
            parse_fragment(br#"return $this->add2_x(...[5, 6]);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(7).expect("create fake int");
        let mut properties = HashMap::new();
        properties.insert("x".to_string(), x);
        let object = values.alloc(FakeValue::Object(properties));
        scope.set("this", object, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(18));
    }

    /// Verifies if/else executes only the PHP-truthy branch.
    #[test]
    fn execute_program_if_else_uses_php_truthiness() {
        let program = parse_fragment(br#"if ($flag) { $x = "then"; } else { $x = "else"; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let flag = values.int(0).expect("create fake int");
        scope.set("flag", flag, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.get(x), FakeValue::String("else".to_string()));
    }

    /// Verifies elseif chains execute the first truthy branch and skip later branches.
    #[test]
    fn execute_program_elseif_uses_first_truthy_branch() {
        let program = parse_fragment(
            br#"if ($a) { $x = "a"; } elseif ($b) { $x = "b"; } else { $x = "c"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let a = values.bool_value(false).expect("create fake bool");
        let b = values.bool_value(true).expect("create fake bool");
        scope.set("a", a, ScopeCellOwnership::Owned);
        scope.set("b", b, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let x = scope.visible_cell("x").expect("scope should contain x");

        assert_eq!(values.get(x), FakeValue::String("b".to_string()));
    }

    /// Verifies while repeats while the condition remains truthy and propagates writes.
    #[test]
    fn execute_program_while_uses_php_truthiness() {
        let program = parse_fragment(br#"while ($flag) { echo $flag; $flag = false; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let flag = values.int(2).expect("create fake int");
        scope.set("flag", flag, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let flag = scope
            .visible_cell("flag")
            .expect("scope should contain flag");

        assert_eq!(values.output, "2");
        assert_eq!(values.get(flag), FakeValue::Bool(false));
    }

    /// Verifies do/while runs the body before testing the condition.
    #[test]
    fn execute_program_do_while_runs_body_before_condition() {
        let program = parse_fragment(br#"do { echo $i; $i = $i + 1; } while (false);"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let i = values.int(0).expect("create fake int");
        scope.set("i", i, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let i = scope.visible_cell("i").expect("scope should contain i");

        assert_eq!(values.output, "0");
        assert_eq!(values.get(i), FakeValue::Int(1));
    }

    /// Verifies switch uses loose matching and falls through after the matching case.
    #[test]
    fn execute_program_switch_matches_and_falls_through() {
        let program =
            parse_fragment(br#"switch ($x) { case 1: echo "one"; break; case 2: echo "two"; default: echo "default"; }"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(2).expect("create fake int");
        scope.set("x", x, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "twodefault");
    }

    /// Verifies for loops run init, condition, update, and body in PHP order.
    #[test]
    fn execute_program_for_loop_updates_after_body() {
        let program = parse_fragment(br#"for ($i = 3; $i; $i = $i - 1) { echo $i; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let i = scope.visible_cell("i").expect("scope should contain i");

        assert_eq!(values.output, "321");
        assert_eq!(values.get(i), FakeValue::Int(0));
    }

    /// Verifies `continue` in a for loop still runs the update clause.
    #[test]
    fn execute_program_for_continue_runs_update_clause() {
        let program = parse_fragment(
            br#"for ($i = 3; $i; $i = $i - 1) { if ($i - 1) { continue; } echo "done"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let i = scope.visible_cell("i").expect("scope should contain i");

        assert_eq!(values.output, "done");
        assert_eq!(values.get(i), FakeValue::Int(0));
    }

    /// Verifies comparison operators return boolean cells usable by echo and branches.
    #[test]
    fn execute_program_comparisons_return_bool_cells() {
        let program = parse_fragment(
            br#"echo 2 < 3; echo 3 <= 3; echo 4 > 3; echo 4 >= 4; if ("10" == 10) { echo "n"; } if ("a" != "b") { echo "s"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "1111ns");
    }

    /// Verifies spaceship comparisons return PHP -1/0/1 integer cells.
    #[test]
    fn execute_program_spaceship_returns_int_cells() {
        let program =
            parse_fragment(br#"echo 1 <=> 2; echo ":"; echo 2 <=> 2; echo ":"; echo 3 <=> 2;"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "-1:0:1");
    }

    /// Verifies strict equality keeps PHP type identity distinct from loose equality.
    #[test]
    fn execute_program_strict_equality_uses_type_identity() {
        let program = parse_fragment(
            br#"echo "10" == 10; echo "10" === 10; echo "10" === "10"; echo "10" !== 10;"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "111");
    }

    /// Verifies logical AND skips an unsupported right-hand expression after a false left side.
    #[test]
    fn execute_program_short_circuits_logical_and() {
        let program =
            parse_fragment(br#"return false && missing();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Bool(false));
    }

    /// Verifies logical OR skips an unsupported right-hand expression after a true left side.
    #[test]
    fn execute_program_short_circuits_logical_or() {
        let program = parse_fragment(br#"return true || missing();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies PHP keyword logical operators use PHP precedence and short-circuiting.
    #[test]
    fn execute_program_evaluates_keyword_logical_operators() {
        let program = parse_fragment(
            br#"echo (false || true and false) ? "T" : "F"; return true or missing();"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "F");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies PHP keyword `xor` evaluates both operands and returns a boolean cell.
    #[test]
    fn execute_program_evaluates_keyword_xor() {
        let program = parse_fragment(
            br#"echo (true xor false) ? "T" : "F"; echo (true xor true) ? "T" : "F";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "TF");
    }

    /// Verifies ternary expressions evaluate only the selected branch.
    #[test]
    fn execute_program_ternary_short_circuits_unselected_branch() {
        let program =
            parse_fragment(br#"echo true ? "yes" : missing(); echo false ? missing() : "no";"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "yesno");
    }

    /// Verifies the short ternary form returns the condition value when it is truthy.
    #[test]
    fn execute_program_short_ternary_reuses_truthy_condition() {
        let program = parse_fragment(br#"echo "x" ?: "fallback"; echo false ?: "fallback";"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "xfallback");
    }

    /// Verifies null coalescing uses the default for missing or null values.
    #[test]
    fn execute_program_null_coalesce_uses_default_for_missing_or_null() {
        let program =
            parse_fragment(br#"echo $missing ?? "fallback"; echo $x ?? "null-fallback";"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.null().expect("create fake null");
        scope.set("x", x, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "fallbacknull-fallback");
    }

    /// Verifies null coalescing skips the default expression for non-null values.
    #[test]
    fn execute_program_null_coalesce_short_circuits_non_null_value() {
        let program = parse_fragment(br#"echo "set" ?? missing();"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "set");
    }

    /// Verifies logical negation returns boolean cells using PHP truthiness.
    #[test]
    fn execute_program_evaluates_logical_not() {
        let program = parse_fragment(br#"echo !false; echo !"x";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "1");
    }

    /// Verifies unary numeric operators delegate to PHP numeric runtime operations.
    #[test]
    fn execute_program_evaluates_unary_numeric_ops() {
        let program = parse_fragment(br#"return -$x + +2;"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(5).expect("create fake int");
        scope.set("x", x, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(-3));
    }

    /// Verifies foreach assigns each indexed element to the value variable.
    #[test]
    fn execute_program_foreach_iterates_indexed_values() {
        let program = parse_fragment(br#"foreach (["a", "b"] as $item) { echo $item; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let item = scope
            .visible_cell("item")
            .expect("scope should contain last foreach item");

        assert_eq!(values.output, "ab");
        assert_eq!(values.get(item), FakeValue::String("b".to_string()));
    }

    /// Verifies foreach key-value targets receive indexed integer keys and values.
    #[test]
    fn execute_program_foreach_assigns_indexed_keys() {
        let program =
            parse_fragment(br#"foreach (["a", "b"] as $key => $item) { echo $key . $item; }"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let key = scope.visible_cell("key").expect("scope should contain key");
        let item = scope
            .visible_cell("item")
            .expect("scope should contain last foreach item");

        assert_eq!(values.output, "0a1b");
        assert_eq!(values.get(key), FakeValue::Int(1));
        assert_eq!(values.get(item), FakeValue::String("b".to_string()));
    }

    /// Verifies foreach over associative arrays preserves insertion-order keys and values.
    #[test]
    fn execute_program_foreach_iterates_assoc_keys_and_values() {
        let program = parse_fragment(
            br#"foreach (["a" => 1, "b" => 2] as $key => $item) { echo $key . ":" . $item . ";"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "a:1;b:2;");
    }

    /// Verifies value-only foreach over associative arrays still yields values in insertion order.
    #[test]
    fn execute_program_foreach_iterates_assoc_values_only() {
        let program = parse_fragment(br#"foreach (["a" => 1, "b" => 2] as $item) { echo $item; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "12");
    }

    /// Verifies break and continue control foreach execution inside eval.
    #[test]
    fn execute_program_foreach_honors_break_and_continue() {
        let program = parse_fragment(
            br#"foreach ([1, 2, 3] as $item) { if ($item == 1) { continue; } echo $item; break; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2");
    }

    /// Verifies indexed array literals and reads execute through runtime hooks.
    #[test]
    fn execute_program_reads_indexed_array_literal() {
        let program = parse_fragment(br#"return ["a", "b"][1];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("b".to_string()));
    }

    /// Verifies associative array literals and string-key reads execute through runtime hooks.
    #[test]
    fn execute_program_reads_assoc_array_literal() {
        let program =
            parse_fragment(br#"return ["name" => "Ada"]["name"];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("Ada".to_string()));
    }

    /// Verifies unkeyed assoc literal entries start at zero after string keys.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_string_key_starts_at_zero() {
        let program = parse_fragment(br#"return ["name" => "Ada", "Grace"][0];"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
    }

    /// Verifies unkeyed assoc literal entries use one plus the largest integer key.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_positive_int_key() {
        let program =
            parse_fragment(br#"return [2 => "two", "tail"][3];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies unkeyed assoc literal entries preserve PHP's negative-key rule.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_negative_int_key() {
        let program =
            parse_fragment(br#"return [-2 => "minus", "tail"][-1];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies numeric string literal keys update the next automatic key.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_numeric_string_key() {
        let program =
            parse_fragment(br#"return ["2" => "two", "tail"][3];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies leading-zero string literal keys do not update the automatic key.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_leading_zero_string_key() {
        let program =
            parse_fragment(br#"return ["02" => "two", "tail"][0];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies boolean literal keys update the next automatic key after integer normalization.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_bool_key() {
        let program =
            parse_fragment(br#"return [true => "yes", "tail"][2];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies false literal keys update the next automatic key from zero.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_false_key() {
        let program =
            parse_fragment(br#"return [false => "no", "tail"][1];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies float literal keys update the next automatic key after truncation.
    #[test]
    fn execute_program_assoc_array_literal_unkeyed_after_float_key() {
        let program =
            parse_fragment(br#"return [2.7 => "two", "tail"][3];"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies nested eval calls parse and execute against the same dynamic scope.
    #[test]
    fn execute_program_nested_eval_uses_same_scope() {
        let program =
            parse_fragment(br#"eval("$x = $x + 4;"); return $x;"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let x = values.int(1).expect("create fake int");
        scope.set("x", x, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies `__LINE__` inside eval uses the source line within the fragment.
    #[test]
    fn execute_program_magic_line_uses_fragment_line() {
        let program = parse_fragment(b"\nreturn __LINE__;").expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(2));
    }

    /// Verifies file-dependent eval magic constants use call-site metadata from the context.
    #[test]
    fn execute_program_magic_file_and_dir_use_context_call_site() {
        let program =
            parse_fragment(br#"return __FILE__ . "|" . __DIR__;"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        context.set_call_site("/tmp/main.php", "/tmp", 17);
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(
            values.get(result),
            FakeValue::String("/tmp/main.php(17) : eval()'d code|/tmp".to_string())
        );
    }

    /// Verifies eval class, namespace, and trait magic constants are empty in eval scope.
    #[test]
    fn execute_program_scope_magic_constants_are_empty_strings() {
        let program = parse_fragment(
            br#"return "[" . __CLASS__ . "|" . __NAMESPACE__ . "|" . __TRAIT__ . "]";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("[||]".to_string()));
    }

    /// Verifies eval-declared functions can be called by the same fragment.
    #[test]
    fn execute_program_calls_declared_function() {
        let program = parse_fragment(br#"function dyn($x) { return $x + 1; } return dyn(4);"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies eval-declared functions bind named arguments by parameter name.
    #[test]
    fn execute_program_calls_declared_function_with_named_args() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(y: 2, x: 1);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies eval-declared functions unpack indexed arrays as positional arguments.
    #[test]
    fn execute_program_calls_declared_function_with_spread_args() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(...[1, 2]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies string keys unpack as named arguments for eval-declared functions.
    #[test]
    fn execute_program_calls_declared_function_with_named_spread_args() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return ($x * 10) + $y; } return dyn(...["y" => 2], x: 1);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies named calls reject positional arguments that follow named arguments.
    #[test]
    fn execute_program_rejects_positional_after_named_arg() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return $x + $y; } return dyn(x: 1, print "late");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values);

        assert_eq!(result, Err(EvalStatus::RuntimeFatal));
        assert_eq!(values.output, "");
    }

    /// Verifies named calls reject argument unpacking after named arguments.
    #[test]
    fn execute_program_rejects_spread_after_named_arg() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return $x + $y; } return dyn(x: 1, ...[2]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values);

        assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    }

    /// Verifies function-scope magic constants keep the eval declaration spelling.
    #[test]
    fn execute_program_magic_function_and_method_use_eval_declared_name() {
        let program = parse_fragment(
            br#"function DynMagicCase() { return __FUNCTION__ . ":" . __METHOD__; } return dynmagiccase();"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.get(result),
            FakeValue::String("DynMagicCase:DynMagicCase".to_string())
        );
    }

    /// Verifies eval-declared functions persist in a shared eval context.
    #[test]
    fn execute_program_context_keeps_declared_function() {
        let define =
            parse_fragment(br#"function dyn($x) { return $x + 1; }"#).expect("parse eval fragment");
        let call = parse_fragment(br#"return dyn(4);"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
            .expect("execute eval ir");
        let result = execute_program_with_context(&mut context, &call, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies `call_user_func` inside eval can dispatch an eval-declared function.
    #[test]
    fn execute_program_call_user_func_dispatches_declared_function() {
        let program = parse_fragment(
            br#"function dyn($x) { return $x + 1; }
return call_user_func("dyn", 4);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(5));
    }

    /// Verifies `call_user_func` inside eval can dispatch a supported builtin.
    #[test]
    fn execute_program_call_user_func_dispatches_builtin() {
        let program = parse_fragment(br#"return call_user_func("strlen", "abcd");"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(4));
    }

    /// Verifies `call_user_func` inside eval can dispatch a registered native function.
    #[test]
    fn execute_program_call_user_func_dispatches_registered_native_function() {
        let program = parse_fragment(br#"return call_user_func("native_answer");"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies `call_user_func_array` inside eval can dispatch an eval-declared function.
    #[test]
    fn execute_program_call_user_func_array_dispatches_declared_function() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", [4, 5]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(9));
    }

    /// Verifies `call_user_func_array` string keys bind eval-declared parameters by name.
    #[test]
    fn execute_program_call_user_func_array_binds_declared_named_args() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return ($x * 10) + $y; }
return call_user_func_array("dyn", ["y" => 2, "x" => 1]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(12));
    }

    /// Verifies `call_user_func_array` rejects positional values after named keys.
    #[test]
    fn execute_program_call_user_func_array_rejects_positional_after_named_arg() {
        let program = parse_fragment(
            br#"function dyn($x, $y) { return $x + $y; }
return call_user_func_array("dyn", ["y" => 2, 1]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values);

        assert_eq!(result, Err(EvalStatus::RuntimeFatal));
    }

    /// Verifies `call_user_func_array` inside eval can dispatch a supported builtin.
    #[test]
    fn execute_program_call_user_func_array_dispatches_builtin() {
        let program = parse_fragment(br#"return call_user_func_array("strlen", ["abcd"]);"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(4));
    }

    /// Verifies `call_user_func_array` inside eval can dispatch a registered native function.
    #[test]
    fn execute_program_call_user_func_array_dispatches_registered_native_function() {
        let program = parse_fragment(br#"return call_user_func_array("native_answer", [4, 5]);"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies `call_user_func_array` named keys can bind registered native parameters.
    #[test]
    fn execute_program_call_user_func_array_binds_registered_native_named_args() {
        let program = parse_fragment(
            br#"return call_user_func_array("native_answer", ["right" => 2, "left" => 1]);"#,
        )
        .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let mut native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(native.set_param_name(0, "left"));
        assert!(native.set_param_name(1, "right"));
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies duplicate eval-declared function names fail in a shared context.
    #[test]
    fn execute_program_rejects_duplicate_declared_function() {
        let define =
            parse_fragment(br#"function dyn() { return 1; }"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
            .expect("execute first declaration");
        let err = execute_program_with_context(&mut context, &define, &mut scope, &mut values)
            .expect_err("duplicate function declaration should fail");

        assert_eq!(err, EvalStatus::RuntimeFatal);
    }

    /// Verifies dynamic builtin calls inside eval dispatch through runtime value hooks.
    #[test]
    fn execute_program_dispatches_simple_builtins() {
        let program = parse_fragment(br#"return strlen("abc") + count([1, 2, 3]);"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(6));
    }

    /// Verifies direct eval builtin calls bind named and unpacked arguments.
    #[test]
    fn execute_program_dispatches_named_and_spread_builtins() {
        let program = parse_fragment(
            br#"echo strlen(string: "abcd");
echo ":" . (array_key_exists(array: ["name" => 1], key: "name") ? "Y" : "N");
echo ":" . (str_contains(...["haystack" => "abc", "needle" => "b"]) ? "Y" : "N");
return round(precision: 1, num: 3.14);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "4:Y:Y");
        assert_eq!(values.get(result), FakeValue::Float(3.1));
    }

    /// Verifies eval `ord()` returns the first byte and supports callable dispatch.
    #[test]
    fn execute_program_dispatches_ord_builtin() {
        let program = parse_fragment(
            br#"echo ord("A");
echo ":" . ord("");
echo ":" . call_user_func("ord", "B");
echo ":" . call_user_func_array("ord", ["C"]);
echo ":"; echo function_exists("ord");
return ord("Z");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "65:0:66:67:1");
        assert_eq!(values.get(result), FakeValue::Int(90));
    }

    /// Verifies eval array aggregate builtins iterate array values and support callable dispatch.
    #[test]
    fn execute_program_dispatches_array_aggregate_builtins() {
        let program = parse_fragment(
            br#"echo array_sum([1, 2, 3]);
echo ":" . array_product([2, 3, 4]);
echo ":" . array_sum([]);
echo ":" . array_product([]);
echo ":" . array_sum(["a" => 2, "b" => 5]);
echo ":" . call_user_func("array_sum", [3, 4]);
echo ":" . call_user_func_array("array_product", [[2, 5]]);
echo ":"; echo function_exists("array_sum");
return function_exists("array_product");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "6:24:0:1:7:7:10:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval array projection builtins produce indexed key/value arrays.
    #[test]
    fn execute_program_dispatches_array_projection_builtins() {
        let program = parse_fragment(
            br#"$values = array_values(["a" => 10, "b" => 20]);
echo $values[0] . ":" . $values[1];
$keys = array_keys(["a" => 10, "b" => 20]);
echo ":" . $keys[0] . ":" . $keys[1];
echo ":" . count(array_values([]));
$call_keys = call_user_func("array_keys", ["z" => 7]);
echo ":" . $call_keys[0];
$call_values = call_user_func_array("array_values", [["q" => 8]]);
echo ":" . $call_values[0];
echo ":"; echo function_exists("array_keys");
return function_exists("array_values");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "10:20:a:b:0:z:8:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `array_key_exists()` distinguishes present null values from missing keys.
    #[test]
    fn execute_program_dispatches_array_key_exists_builtin() {
        let program = parse_fragment(
            br#"$map = ["name" => null, "age" => 30];
echo array_key_exists("name", $map) ? "Y" : "N"; echo ":";
echo array_key_exists("missing", $map) ? "bad" : "N"; echo ":";
echo array_key_exists(1, [10, null]) ? "Y" : "N"; echo ":";
echo array_key_exists(2, [10, null]) ? "bad" : "N"; echo ":";
echo call_user_func("array_key_exists", "age", $map) ? "Y" : "N"; echo ":";
echo call_user_func_array("array_key_exists", ["age", $map]) ? "Y" : "N"; echo ":";
return function_exists("array_key_exists");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Y:N:Y:N:Y:Y:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval array search builtins use loose comparison and return keys or booleans.
    #[test]
    fn execute_program_dispatches_array_search_builtins() {
        let program = parse_fragment(
            br#"echo in_array(2, [1, 2, 3]) ? "Y" : "bad";
echo ":"; echo in_array(4, [1, 2, 3]) ? "bad" : "N";
echo ":" . array_search(20, [10, 20, 30]);
echo ":" . array_search("Grace", ["name" => "Grace"]);
echo ":"; echo array_search("x", ["name" => "Grace"]) === false ? "F" : "bad";
echo ":"; echo call_user_func("in_array", "b", ["a", "b"]) ? "C" : "bad";
$found = call_user_func_array("array_search", ["v", ["k" => "v"]]);
echo ":" . $found;
echo ":"; echo function_exists("in_array");
return function_exists("array_search");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Y:N:1:name:F:C:k:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval ASCII string case builtins work directly and through callable dispatch.
    #[test]
    fn execute_program_dispatches_string_case_builtins() {
        let program = parse_fragment(
            br#"echo strtoupper("Hello World"); echo ":";
echo strtolower("LOUD"); echo ":";
echo ucfirst("eval"); echo ":";
echo lcfirst("LOUD"); echo ":";
echo call_user_func("strtoupper", "xy"); echo ":";
echo call_user_func_array("strtolower", ["ZZ"]); echo ":";
echo call_user_func("ucfirst", "case"); echo ":";
echo call_user_func_array("lcfirst", ["CASE"]);
echo ":"; echo function_exists("strtoupper"); echo function_exists("strtolower"); echo function_exists("ucfirst");
return function_exists("lcfirst");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "HELLO WORLD:loud:Eval:lOUD:XY:zz:Case:cASE:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `str_contains()` uses byte-string search and supports callable dispatch.
    #[test]
    fn execute_program_dispatches_str_contains_builtin() {
        let program = parse_fragment(
            br#"echo str_contains("Hello World", "World") ? "Y" : "N";
echo str_contains("Hello", "z") ? "bad" : ":N";
echo str_contains("Hello", "") ? ":E" : "bad";
echo call_user_func("str_contains", "abc", "b") ? ":C" : "bad";
echo call_user_func_array("str_contains", ["abc", "x"]) ? "bad" : ":A";
return function_exists("str_contains");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "Y:N:E:C:A");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval string position builtins return byte offsets or PHP false.
    #[test]
    fn execute_program_dispatches_string_position_builtins() {
        let program = parse_fragment(
            br#"echo strpos("banana", "na");
echo ":" . strrpos("banana", "na");
echo ":"; echo strpos("abc", "z") === false ? "F" : "bad";
echo ":" . strpos("abc", "");
echo ":" . strrpos("abc", "");
echo ":" . call_user_func("strpos", "abc", "b");
echo ":" . call_user_func_array("strrpos", ["ababa", "ba"]);
echo ":"; echo function_exists("strpos");
return function_exists("strrpos");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "2:4:F:0:3:1:3:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval prefix/suffix string search builtins use byte-string semantics.
    #[test]
    fn execute_program_dispatches_string_boundary_builtins() {
        let program = parse_fragment(
            br#"echo str_starts_with("Hello World", "Hello") ? "S" : "bad";
echo str_starts_with("Hello", "World") ? "bad" : ":s";
echo str_starts_with("Hello", "") ? ":se" : "bad";
echo str_ends_with("Hello World", "World") ? ":E" : "bad";
echo str_ends_with("Hello", "World") ? "bad" : ":e";
echo str_ends_with("Hello", "") ? ":ee" : "bad";
echo call_user_func("str_starts_with", "abc", "a") ? ":CS" : "bad";
echo call_user_func_array("str_ends_with", ["abc", "c"]) ? ":CE" : "bad";
echo ":"; echo function_exists("str_starts_with");
return function_exists("str_ends_with");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "S:s:se:E:e:ee:CS:CE:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval string comparison builtins return PHP-compatible scalar results.
    #[test]
    fn execute_program_dispatches_string_compare_builtins() {
        let program = parse_fragment(
            br#"echo strcmp("abc", "abc");
echo ":"; echo strcmp("abc", "abd") < 0 ? "lt" : "bad";
echo ":"; echo strcasecmp("Hello", "hello");
echo ":"; echo call_user_func("strcmp", "b", "a") > 0 ? "gt" : "bad";
echo ":"; echo call_user_func_array("strcasecmp", ["A", "a"]) === 0 ? "ci" : "bad";
echo ":"; echo hash_equals("abc", "abc") ? "heq" : "bad";
echo ":"; echo hash_equals("abc", "abcd") ? "bad" : "hlen";
echo ":"; echo call_user_func("hash_equals", "abc", "abd") ? "bad" : "hneq";
echo ":"; echo function_exists("strcmp"); echo function_exists("strcasecmp");
return function_exists("hash_equals");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "0:lt:0:gt:ci:heq:hlen:hneq:11");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval trim-like builtins strip default and explicit byte masks.
    #[test]
    fn execute_program_dispatches_trim_like_builtins() {
        let program = parse_fragment(
            br#"echo "[" . trim("  hello  ") . "]";
echo ":[" . ltrim("  left") . "]";
echo ":[" . rtrim("right  ") . "]";
echo ":[" . chop("tail... ", " .") . "]";
echo ":[" . trim("**boxed**", "*") . "]";
echo ":[" . call_user_func("trim", "  cuf  ") . "]";
echo ":[" . call_user_func_array("ltrim", ["0007", "0"]) . "]";
echo ":"; echo function_exists("trim"); echo function_exists("ltrim"); echo function_exists("rtrim");
return function_exists("chop");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "[hello]:[left]:[right]:[tail]:[boxed]:[cuf]:[7]:111"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval type-predicate builtins inspect boxed runtime tags directly and by callable.
    #[test]
    fn execute_program_dispatches_type_predicate_builtins() {
        let program = parse_fragment(
            br#"echo is_int(1); echo is_integer(1); echo is_long(1);
echo is_float(1.5); echo is_double(1.5); echo is_real(1.5);
echo is_string("x"); echo is_bool(false); echo is_null(null);
echo is_array([1]); echo is_array(["a" => 1]);
echo is_array(1) ? "bad" : "ok";
echo is_numeric(42); echo is_numeric(3.14); echo is_numeric("42");
echo is_numeric("-5"); echo is_numeric("3.14");
echo is_numeric("abc") ? "bad" : "N";
echo is_numeric(true) ? "bad" : "B";
echo is_resource(1) ? "bad" : "R";
echo ":"; echo call_user_func("is_string", "x");
echo call_user_func_array("is_array", [[1]]);
echo call_user_func("is_numeric", "12");
echo function_exists("is_numeric"); echo function_exists("is_resource");
return function_exists("is_double");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "11111111111ok11111NBR:11111");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `is_resource()` recognizes resource-tagged runtime cells from scope.
    #[test]
    fn execute_program_dispatches_is_resource_true() {
        let program = parse_fragment(
            br#"echo is_resource($handle) ? "R" : "bad";
echo ":" . gettype($handle);
return call_user_func("is_resource", $handle);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let handle = values.alloc(FakeValue::Resource(6));
        scope.set("handle".to_string(), handle, ScopeCellOwnership::Borrowed);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "R:resource");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval cast builtins return boxed scalar cells directly and by callable.
    #[test]
    fn execute_program_dispatches_cast_builtins() {
        let program = parse_fragment(
            br#"echo intval("42"); echo ":";
echo floatval("3.5"); echo ":";
echo strval(12); echo ":";
echo boolval("0") ? "bad" : "false";
echo ":"; echo call_user_func("strval", 7);
return call_user_func_array("intval", ["9"]);"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "42:3.5:12:false:7");
        assert_eq!(values.get(result), FakeValue::Int(9));
    }

    /// Verifies eval `gettype()` maps runtime tags to PHP type names directly and by callable.
    #[test]
    fn execute_program_dispatches_gettype_builtin() {
        let program = parse_fragment(
            br#"echo gettype(1); echo ":";
echo gettype(1.5); echo ":";
echo gettype("x"); echo ":";
echo gettype(false); echo ":";
echo gettype(null); echo ":";
echo gettype([1]); echo ":";
echo gettype(["a" => 1]); echo ":";
echo call_user_func("gettype", true); echo ":";
echo call_user_func_array("gettype", [null]);
return function_exists("gettype");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(
            values.output,
            "integer:double:string:boolean:NULL:array:array:boolean:NULL"
        );
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `abs()` dispatches through runtime numeric hooks directly and by callable.
    #[test]
    fn execute_program_dispatches_abs_builtin() {
        let program = parse_fragment(
            br#"echo abs(-5); echo ":";
echo abs(-2.5); echo ":";
echo gettype(abs(-2.5)); echo ":";
echo call_user_func("abs", -7); echo ":";
echo call_user_func_array("abs", [-9]);
return function_exists("abs");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "5:2.5:double:7:9");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `floor()` and `ceil()` dispatch as double-returning math builtins.
    #[test]
    fn execute_program_dispatches_floor_and_ceil_builtins() {
        let program = parse_fragment(
            br#"echo floor(3.7); echo ":";
echo gettype(floor(3)); echo ":";
echo ceil(3.2); echo ":";
echo gettype(ceil(3)); echo ":";
echo call_user_func("floor", 4.9); echo ":";
echo call_user_func_array("ceil", [4.1]);
echo ":"; echo function_exists("floor");
return function_exists("ceil");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3:double:4:double:4:5:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `fdiv()` and `fmod()` dispatch as floating-point binary builtins.
    #[test]
    fn execute_program_dispatches_float_binary_builtins() {
        let program = parse_fragment(
            br#"echo round(fdiv(10, 4), 2); echo ":";
echo gettype(fdiv(10, 4)); echo ":";
echo round(fmod(10.5, 3.2), 1); echo ":";
echo round(call_user_func("fdiv", 9, 2), 1); echo ":";
echo round(call_user_func_array("fmod", [10.5, 3.2]), 1); echo ":";
echo function_exists("fdiv");
return function_exists("fmod");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        assert_eq!(values.output, "2.5:double:0.9:4.5:0.9:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `pow()` dispatches through the existing exponentiation runtime hook.
    #[test]
    fn execute_program_dispatches_pow_builtin() {
        let program = parse_fragment(
            br#"echo pow(2, 3); echo ":";
echo gettype(pow(2, 3)); echo ":";
echo call_user_func("pow", 2, 5); echo ":";
echo call_user_func_array("pow", [3, 3]);
return function_exists("pow");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "8:double:32:27");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `round()` supports default and explicit precision through callable paths.
    #[test]
    fn execute_program_dispatches_round_builtin() {
        let program = parse_fragment(
            br#"echo round(3.5); echo ":";
echo round(3.14159, 2); echo ":";
echo gettype(round(3)); echo ":";
echo call_user_func("round", 2.5); echo ":";
echo call_user_func_array("round", [1.55, 1]);
return function_exists("round");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "4:3.14:double:3:1.6");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `min()` and `max()` select numeric values directly and by callable.
    #[test]
    fn execute_program_dispatches_min_max_builtins() {
        let program = parse_fragment(
            br#"echo min(3, 1, 2); echo ":";
echo max(1, 3, 2); echo ":";
echo min(2.5, 1.5); echo ":";
echo max(1.5, 2.5); echo ":";
echo call_user_func("min", 9, 4, 7); echo ":";
echo call_user_func_array("max", [4, 8, 6]); echo ":";
echo function_exists("min");
return function_exists("max");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "1:3:1.5:2.5:4:8:1");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `pi()` returns a double constant directly and through callable paths.
    #[test]
    fn execute_program_dispatches_pi_builtin() {
        let program = parse_fragment(
            br#"echo round(pi(), 2); echo ":";
echo gettype(pi()); echo ":";
echo round(call_user_func("pi"), 3); echo ":";
echo round(call_user_func_array("pi", []), 4); echo ":";
return function_exists("pi");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "3.14:double:3.142:3.1416:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `sqrt()` dispatches through runtime float hooks directly and by callable.
    #[test]
    fn execute_program_dispatches_sqrt_builtin() {
        let program = parse_fragment(
            br#"echo sqrt(16); echo ":";
echo gettype(sqrt(9)); echo ":";
echo call_user_func("sqrt", 25); echo ":";
echo call_user_func_array("sqrt", [36]);
return function_exists("sqrt");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "4:double:5:6");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies eval `strrev()` dispatches through direct and callable paths.
    #[test]
    fn execute_program_dispatches_strrev_builtin() {
        let program = parse_fragment(
            br#"echo strrev("Hello"); echo ":";
echo strrev(123); echo ":";
echo call_user_func("strrev", "ABC"); echo ":";
echo call_user_func_array("strrev", ["def"]); echo ":";
return function_exists("strrev");"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        assert_eq!(values.output, "olleH:321:CBA:fed:");
        assert_eq!(values.get(result), FakeValue::Bool(true));
    }

    /// Verifies `isset` distinguishes missing, null, and other falsey values.
    #[test]
    fn execute_program_isset_distinguishes_missing_null_and_falsey_values() {
        let program = parse_fragment(
            br#"if (isset($missing)) { echo "1"; } else { echo "0"; }
if (isset($nullish)) { echo "1"; } else { echo "0"; }
if (isset($zero)) { echo "1"; } else { echo "0"; }
if (isset($empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $empty)) { echo "1"; } else { echo "0"; }
if (isset($zero, $nullish)) { echo "1"; } else { echo "0"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let nullish = values.null().expect("create fake null");
        let zero = values.int(0).expect("create fake int");
        let empty = values.string("").expect("create fake string");
        scope.set("nullish", nullish, ScopeCellOwnership::Owned);
        scope.set("zero", zero, ScopeCellOwnership::Owned);
        scope.set("empty", empty, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "001110");
        assert_eq!(values.get(result), FakeValue::Null);
    }

    /// Verifies `empty` treats missing, null, and falsey values as empty.
    #[test]
    fn execute_program_empty_uses_php_truthiness_without_missing_warnings() {
        let program = parse_fragment(
            br#"if (empty($missing)) { echo "1"; } else { echo "0"; }
if (empty($nullish)) { echo "1"; } else { echo "0"; }
if (empty($zero)) { echo "1"; } else { echo "0"; }
if (empty($empty_string)) { echo "1"; } else { echo "0"; }
if (empty($zero_string)) { echo "1"; } else { echo "0"; }
if (empty($value)) { echo "1"; } else { echo "0"; }"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let nullish = values.null().expect("create fake null");
        let zero = values.int(0).expect("create fake int");
        let empty_string = values.string("").expect("create fake empty string");
        let zero_string = values.string("0").expect("create fake zero string");
        let value = values.string("x").expect("create fake non-empty string");
        scope.set("nullish", nullish, ScopeCellOwnership::Owned);
        scope.set("zero", zero, ScopeCellOwnership::Owned);
        scope.set("empty_string", empty_string, ScopeCellOwnership::Owned);
        scope.set("zero_string", zero_string, ScopeCellOwnership::Owned);
        scope.set("value", value, ScopeCellOwnership::Owned);

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "111110");
        assert_eq!(values.get(result), FakeValue::Null);
    }

    /// Verifies eval builtin probes see dynamic functions and supported PHP-visible builtins.
    #[test]
    fn execute_program_function_probes_use_eval_context() {
        let program = parse_fragment(
            br#"function dyn_probe() { return 1; }
echo function_exists("DYN_PROBE") . "x";
echo is_callable("dyn_probe") . "x";
echo function_exists("strlen") . "x";
echo function_exists("native_probe") . "x";
echo function_exists("eval") . "x";
echo function_exists("missing_probe") . "x";"#,
        )
        .expect("parse eval fragment");
        let native = NativeFunction::new(1usize as *mut c_void, fake_native_return_descriptor, 0);
        let mut context = ElephcEvalContext::new();
        assert!(context
            .define_native_function("native_probe", native)
            .is_ok());
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(values.output, "1x1x1x1xxx");
    }

    /// Verifies eval fragments can dispatch registered native AOT functions.
    #[test]
    fn execute_program_calls_registered_native_function() {
        let program = parse_fragment(br#"return native_answer();"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 0);
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies direct eval calls can bind registered native parameters by name.
    #[test]
    fn execute_program_calls_registered_native_function_with_named_args() {
        let program = parse_fragment(br#"return native_answer(right: 2, left: 1);"#)
            .expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let mut native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(native.set_param_name(0, "left"));
        assert!(native.set_param_name(1, "right"));
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies direct eval calls can unpack arrays into registered native parameters.
    #[test]
    fn execute_program_calls_registered_native_function_with_spread_args() {
        let program =
            parse_fragment(br#"return native_answer(...[1, 2]);"#).expect("parse eval fragment");
        let mut context = ElephcEvalContext::new();
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let expected = values.int(42).expect("allocate fake result");
        let mut native =
            NativeFunction::new(expected.as_ptr().cast(), fake_native_return_descriptor, 2);
        assert!(native.set_param_name(0, "left"));
        assert!(native.set_param_name(1, "right"));
        assert!(context
            .define_native_function("native_answer", native)
            .is_ok());

        let result = execute_program_with_context(&mut context, &program, &mut scope, &mut values)
            .expect("execute eval ir");

        assert_eq!(result, expected);
    }

    /// Verifies indexed array writes mutate an existing scope array.
    #[test]
    fn execute_program_writes_indexed_scope_array() {
        let program = parse_fragment(br#"$items = ["a"]; $items[1] = "b"; return $items[1];"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("b".to_string()));
    }

    /// Verifies indexed array append writes use the next visible index.
    #[test]
    fn execute_program_appends_indexed_scope_array() {
        let program = parse_fragment(br#"$items = ["a"]; $items[] = "b"; return $items[1];"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("b".to_string()));
    }

    /// Verifies associative append starts at key zero when only string keys exist.
    #[test]
    fn execute_program_appends_assoc_scope_array_with_string_keys() {
        let program =
            parse_fragment(br#"$items = ["name" => "Ada"]; $items[] = "Grace"; return $items[0];"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("Grace".to_string()));
    }

    /// Verifies associative append uses one plus the largest existing integer key.
    #[test]
    fn execute_program_appends_assoc_scope_array_after_positive_int_key() {
        let program = parse_fragment(
            br#"$items = [2 => "two", "name" => "Ada"]; $items[] = "tail"; return $items[3];"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies associative append preserves PHP's largest-negative-key behavior.
    #[test]
    fn execute_program_appends_assoc_scope_array_after_negative_int_key() {
        let program =
            parse_fragment(br#"$items = [-2 => "minus"]; $items[] = "tail"; return $items[-1];"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::String("tail".to_string()));
    }

    /// Verifies mutating a borrowed scope array does not make the eval scope own it.
    #[test]
    fn execute_program_preserves_borrowed_array_ownership() {
        let program = parse_fragment(br#"$items[0] = "b";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let array = values.array_new(1).expect("create fake array");
        scope.set("items", array, ScopeCellOwnership::Borrowed);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
        let entry = scope.entry("items").expect("scope should contain items");

        assert_eq!(entry.cell(), array);
        assert_eq!(entry.flags().ownership, ScopeCellOwnership::Borrowed);
        assert!(values.releases.is_empty());
    }

    /// Verifies replacing an eval-owned scope value releases the old cell.
    #[test]
    fn execute_program_releases_replaced_scope_value() {
        let program = parse_fragment(br#"$x = "old"; $x = "new";"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.releases.len(), 1);
        assert_eq!(
            values.get(values.releases[0]),
            FakeValue::String("old".to_string())
        );
    }

    /// Verifies unsetting an eval-owned scope value releases the old cell.
    #[test]
    fn execute_program_releases_unset_scope_value() {
        let program = parse_fragment(br#"$x = "old"; unset($x);"#).expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.releases.len(), 1);
        assert_eq!(
            values.get(values.releases[0]),
            FakeValue::String("old".to_string())
        );
    }

    /// Verifies break exits a runtime eval loop before later statements run.
    #[test]
    fn execute_program_break_exits_loop() {
        let program = parse_fragment(br#"while ($flag) { echo "a"; break; echo "b"; }"#)
            .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let flag = values.bool_value(true).expect("create fake bool");
        scope.set("flag", flag, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "a");
    }

    /// Verifies continue restarts a runtime eval loop and observes later scope updates.
    #[test]
    fn execute_program_continue_restarts_loop() {
        let program = parse_fragment(
            br#"while ($flag) { $flag = false; continue; echo "unreachable"; } echo "done";"#,
        )
        .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();
        let flag = values.bool_value(true).expect("create fake bool");
        scope.set("flag", flag, ScopeCellOwnership::Owned);

        let _ = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.output, "done");
    }
}
