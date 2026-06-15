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
use crate::eval_ir::{EvalArrayElement, EvalBinOp, EvalConst, EvalExpr, EvalProgram, EvalStmt};
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

    /// Emits one runtime cell to stdout using PHP echo semantics.
    fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus>;

    /// Converts one runtime cell to PHP boolean truthiness.
    fn truthy(&mut self, value: RuntimeCellHandle) -> Result<bool, EvalStatus>;
}

/// Executes an EvalIR program and returns the eval result cell.
pub fn execute_program(
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match execute_statements(program.statements(), scope, values)? {
        EvalControl::None => values.null(),
        EvalControl::Return(result) => Ok(result),
        EvalControl::Break | EvalControl::Continue => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Executes statements in source order and propagates the first eval `return`.
fn execute_statements(
    statements: &[EvalStmt],
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<EvalControl, EvalStatus> {
    for stmt in statements {
        match execute_stmt(stmt, scope, values)? {
            EvalControl::None => {}
            control => return Ok(control),
        }
    }
    Ok(EvalControl::None)
}

/// Executes one statement and returns `Some` only for eval `return`.
fn execute_stmt(
    stmt: &EvalStmt,
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
            let index = eval_expr(index, scope, values)?;
            let value = eval_expr(value, scope, values)?;
            let array = values.array_set(array, index, value)?;
            if let Some(replaced) = scope.set(name.clone(), array, ownership) {
                values.release(replaced)?;
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Break => Ok(EvalControl::Break),
        EvalStmt::Continue => Ok(EvalControl::Continue),
        EvalStmt::Echo(expr) => {
            let value = eval_expr(expr, scope, values)?;
            values.echo(value)?;
            Ok(EvalControl::None)
        }
        EvalStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = eval_expr(condition, scope, values)?;
            if values.truthy(condition)? {
                execute_statements(then_branch, scope, values)
            } else {
                execute_statements(else_branch, scope, values)
            }
        }
        EvalStmt::Return(Some(expr)) => Ok(EvalControl::Return(eval_expr(expr, scope, values)?)),
        EvalStmt::Return(None) => Ok(EvalControl::Return(values.null()?)),
        EvalStmt::StoreVar { name, value } => {
            let value = eval_expr(value, scope, values)?;
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
                let condition = eval_expr(condition, scope, values)?;
                values.truthy(condition)?
            } {
                match execute_statements(body, scope, values)? {
                    EvalControl::None | EvalControl::Continue => {}
                    EvalControl::Break => break,
                    EvalControl::Return(result) => return Ok(EvalControl::Return(result)),
                }
            }
            Ok(EvalControl::None)
        }
        EvalStmt::Expr(expr) => {
            let _ = eval_expr(expr, scope, values)?;
            Ok(EvalControl::None)
        }
    }
}

/// Evaluates one expression to an opaque runtime-cell handle.
fn eval_expr(
    expr: &EvalExpr,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match expr {
        EvalExpr::Array(elements) => {
            if elements
                .iter()
                .any(|element| matches!(element, EvalArrayElement::KeyValue { .. }))
            {
                eval_assoc_array(elements, scope, values)
            } else {
                eval_indexed_array(elements, scope, values)
            }
        }
        EvalExpr::ArrayGet { array, index } => {
            let array = eval_expr(array, scope, values)?;
            let index = eval_expr(index, scope, values)?;
            values.array_get(array, index)
        }
        EvalExpr::Const(value) => eval_const(value, values),
        EvalExpr::LoadVar(name) => scope.visible_cell(name).map_or_else(|| values.null(), Ok),
        EvalExpr::Print(inner) => {
            let value = eval_expr(inner, scope, values)?;
            values.echo(value)?;
            values.int(1)
        }
        EvalExpr::Binary { op, left, right } => {
            let left = eval_expr(left, scope, values)?;
            let right = eval_expr(right, scope, values)?;
            match op {
                EvalBinOp::Add => values.add(left, right),
                EvalBinOp::Sub => values.sub(left, right),
                EvalBinOp::Mul => values.mul(left, right),
                EvalBinOp::Concat => values.concat(left, right),
            }
        }
    }
}

/// Evaluates an indexed array literal into a boxed runtime Mixed array.
fn eval_indexed_array(
    elements: &[EvalArrayElement],
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let array = values.array_new(elements.len())?;
    for (index, element) in elements.iter().enumerate() {
        let EvalArrayElement::Value(element) = element else {
            return Err(EvalStatus::UnsupportedConstruct);
        };
        let index = values.int(index as i64)?;
        let value = eval_expr(element, scope, values)?;
        let _ = values.array_set(array, index, value)?;
    }
    Ok(array)
}

/// Evaluates an associative array literal into a boxed runtime Mixed hash.
fn eval_assoc_array(
    elements: &[EvalArrayElement],
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
                let key = eval_expr(key, scope, values)?;
                (key, value)
            }
        };
        let value = eval_expr(value, scope, values)?;
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

        /// Appends fake echo output for interpreter tests.
        fn echo(&mut self, value: RuntimeCellHandle) -> Result<(), EvalStatus> {
            let value = self.stringify(value);
            self.output.push_str(&value);
            Ok(())
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
