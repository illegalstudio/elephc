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

use crate::errors::EvalStatus;
use crate::context::ElephcEvalContext;
use crate::eval_ir::{
    EvalArrayElement, EvalBinOp, EvalConst, EvalExpr, EvalFunction, EvalProgram, EvalStmt,
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

    /// Writes one element to a runtime array-like Mixed cell and returns the target cell.
    fn array_set(
        &mut self,
        array: RuntimeCellHandle,
        index: RuntimeCellHandle,
        value: RuntimeCellHandle,
    ) -> Result<RuntimeCellHandle, EvalStatus>;

    /// Returns the visible element count for an array-like runtime cell.
    fn array_len(&mut self, array: RuntimeCellHandle) -> Result<usize, EvalStatus>;

    /// Returns whether a runtime cell can be indexed like an array by eval writes.
    fn is_array_like(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;

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

    /// Emits one runtime cell to stdout using PHP echo semantics.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Casts one runtime cell to a PHP string and copies its bytes for parsing.
    fn string_bytes(&mut self, value: RuntimeCellHandle) -> Result<Vec<u8>, EvalStatus>;

    /// Converts one runtime cell to PHP boolean truthiness.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;
}

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
        } => execute_for_stmt(init, condition.as_ref(), update, body, context, scope, values),
        EvalStmt::Foreach {
            array,
            value_name,
            body,
        } => execute_foreach_stmt(array, value_name, body, context, scope, values),
        EvalStmt::FunctionDecl { name, params, body } => {
            context
                .define_function(name.clone(), EvalFunction::new(params.clone(), body.clone()))
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
        EvalStmt::Return(Some(expr)) => {
            Ok(EvalControl::Return(eval_expr(expr, context, scope, values)?))
        }
        EvalStmt::Return(None) => Ok(EvalControl::Return(values.null()?)),
        EvalStmt::StoreVar { name, value } => {
            let value = eval_expr(value, context, scope, values)?;
            if let Some(replaced) = scope.set(name.clone(), value, ScopeCellOwnership::Owned) {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
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

/// Executes a value-only PHP `foreach` loop over indexed eval array values.
fn execute_foreach_stmt(
    array: &EvalExpr,
    value_name: &str,
    body: &[EvalStmt],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    let array = eval_expr(array, context, scope, values)?;
    let len = values.array_len(array)?;
    for index in 0..len {
        let index = values.int(index as i64)?;
        let value = values.array_get(array, index)?;
        if let Some(replaced) =
            scope.set(value_name.to_string(), value, ScopeCellOwnership::Owned)
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
        EvalExpr::Print(inner) => {
            let value = eval_expr(inner, context, scope, values)?;
            values.echo(value)?;
            values.int(1)
        }
        EvalExpr::Binary { op, left, right } => {
            let left = eval_expr(left, context, scope, values)?;
            let right = eval_expr(right, context, scope, values)?;
            match op {
                EvalBinOp::Add => values.add(left, right),
                EvalBinOp::Sub => values.sub(left, right),
                EvalBinOp::Mul => values.mul(left, right),
                EvalBinOp::Concat => values.concat(left, right),
                EvalBinOp::LooseEq
                | EvalBinOp::LooseNotEq
                | EvalBinOp::Lt
                | EvalBinOp::LtEq
                | EvalBinOp::Gt
                | EvalBinOp::GtEq => values.compare(*op, left, right),
            }
        }
    }
}

/// Evaluates supported function-like calls from a runtime eval fragment.
fn eval_call(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match name {
        "count" => eval_builtin_count(args, context, scope, values),
        "eval" => eval_nested_eval(args, context, scope, values),
        "strlen" => eval_builtin_strlen(args, context, scope, values),
        _ => context
            .function(name)
            .cloned()
            .map_or(Err(EvalStatus::UnsupportedConstruct), |function| {
                eval_dynamic_function(&function, args, context, scope, values)
            }),
    }
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

/// Evaluates an eval-declared user function with positional argument binding.
fn eval_dynamic_function(
    function: &EvalFunction,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    caller_scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    if args.len() != function.params().len() {
        return Err(EvalStatus::RuntimeFatal);
    }
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, caller_scope, values)?);
    }
    eval_dynamic_function_with_values(function, evaluated_args, context, values)
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
    match execute_statements(function.body(), context, &mut function_scope, values)? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
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
    let mut next_index = 0;
    for element in elements {
        let (key, value) = match element {
            EvalArrayElement::Value(value) => {
                let key = values.int(next_index)?;
                next_index += 1;
                (key, value)
            }
            EvalArrayElement::KeyValue { key, value } => {
                let key = eval_expr(key, context, scope, values)?;
                (key, value)
            }
        };
        let value = eval_expr(value, context, scope, values)?;
        let _ = values.array_set(array, key, value)?;
    }
    Ok(array)
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

/// Returns the current interpreter availability status for the ABI stub.
pub fn current_stub_status() -> EvalStatus {
    EvalStatus::UnsupportedConstruct
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

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
        Assoc(HashMap<FakeKey, RuntimeCellHandle>),
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

        /// Converts a fake runtime cell into a fake PHP array key.
        fn key(&self, handle: RuntimeCellHandle) -> Result<FakeKey, EvalStatus> {
            match self.get(handle) {
                FakeValue::Int(value) => Ok(FakeKey::Int(value)),
                FakeValue::String(value) => Ok(FakeKey::String(value)),
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
        }
    }

    impl RuntimeValueOps for FakeOps {
        /// Creates a fake indexed array cell.
        fn array_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Array(Vec::with_capacity(capacity))))
        }

        /// Creates a fake associative array cell.
        fn assoc_new(&mut self, capacity: usize) -> Result<RuntimeCellHandle, EvalStatus> {
            Ok(self.alloc(FakeValue::Assoc(HashMap::with_capacity(capacity))))
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
                FakeValue::Assoc(entries) => {
                    entries.get(&key).copied().map_or_else(|| self.null(), Ok)
                }
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
                    entries.insert(key, value);
                }
                _ => return Err(EvalStatus::UnsupportedConstruct),
            }
            Ok(array)
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

        /// Adds fake numeric cells for interpreter tests.
        fn add(
            &mut self,
            left: RuntimeCellHandle,
            right: RuntimeCellHandle,
        ) -> Result<RuntimeCellHandle, EvalStatus> {
            match (self.get(left), self.get(right)) {
                (FakeValue::Int(left), FakeValue::Int(right)) => self.int(left + right),
                _ => Err(EvalStatus::UnsupportedConstruct),
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
                _ => Err(EvalStatus::UnsupportedConstruct),
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
                _ => Err(EvalStatus::UnsupportedConstruct),
            }
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
                EvalBinOp::Lt => self.numeric(left)? < self.numeric(right)?,
                EvalBinOp::LtEq => self.numeric(left)? <= self.numeric(right)?,
                EvalBinOp::Gt => self.numeric(left)? > self.numeric(right)?,
                EvalBinOp::GtEq => self.numeric(left)? >= self.numeric(right)?,
                EvalBinOp::Add | EvalBinOp::Sub | EvalBinOp::Mul | EvalBinOp::Concat => {
                    return Err(EvalStatus::UnsupportedConstruct);
                }
            };
            self.bool_value(result)
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
            }
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
            }
        }
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

    /// Verifies foreach assigns each indexed element to the value variable.
    #[test]
    fn execute_program_foreach_iterates_indexed_values() {
        let program =
            parse_fragment(br#"foreach (["a", "b"] as $item) { echo $item; }"#)
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

    /// Verifies duplicate eval-declared function names fail in a shared context.
    #[test]
    fn execute_program_rejects_duplicate_declared_function() {
        let define = parse_fragment(br#"function dyn() { return 1; }"#).expect("parse eval fragment");
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
        let program =
            parse_fragment(br#"return strlen("abc") + count([1, 2, 3]);"#)
                .expect("parse eval fragment");
        let mut scope = ElephcEvalScope::new();
        let mut values = FakeOps::default();

        let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

        assert_eq!(values.get(result), FakeValue::Int(6));
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
