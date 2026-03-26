mod builtins;
mod functions;

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{CheckResult, FunctionSig, PhpType, TypeEnv};

pub(crate) struct Checker {
    pub fn_decls: HashMap<String, FnDecl>,
    pub functions: HashMap<String, FunctionSig>,
    pub constants: HashMap<String, PhpType>,
    /// Tracks the return type of closures assigned to variables.
    pub closure_return_types: HashMap<String, PhpType>,
}

#[derive(Clone)]
pub(crate) struct FnDecl {
    pub params: Vec<String>,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub variadic: Option<String>,
    pub body: Vec<Stmt>,
}

pub fn check_types(program: &Program) -> Result<CheckResult, CompileError> {
    let mut checker = Checker {
        fn_decls: HashMap::new(),
        functions: HashMap::new(),
        constants: HashMap::new(),
        closure_return_types: HashMap::new(),
    };

    for stmt in program {
        if let StmtKind::FunctionDecl { name, params, variadic, body } = &stmt.kind {
            let param_names: Vec<String> = params.iter().map(|(n, _, _)| n.clone()).collect();
            let defaults: Vec<Option<Expr>> = params.iter().map(|(_, d, _)| d.clone()).collect();
            let ref_flags: Vec<bool> = params.iter().map(|(_, _, r)| *r).collect();
            checker.fn_decls.insert(
                name.clone(),
                FnDecl {
                    params: param_names,
                    defaults,
                    ref_params: ref_flags,
                    variadic: variadic.clone(),
                    body: body.clone(),
                },
            );
        }
    }

    let mut global_env: TypeEnv = HashMap::new();
    global_env.insert("argc".to_string(), PhpType::Int);
    global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
    for stmt in program {
        checker.check_stmt(stmt, &mut global_env)?;
    }

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
    })
}

impl Checker {
    pub fn check_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Echo(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::Assign { name, value } => {
                let ty = self.infer_type(value, env)?;
                // Track closure return types for closure-returning-closure patterns
                if let ExprKind::Closure { body, .. } = &value.kind {
                    let ret_ty = self.infer_closure_return_type(body, env);
                    self.closure_return_types.insert(name.clone(), ret_ty);
                }
                if let Some(existing) = env.get(name) {
                    // Allow null (Void) to be assigned to any variable,
                    // Bool and Int are interchangeable, Int and Float are interchangeable
                    let compatible = *existing == ty
                        || ty == PhpType::Void
                        || *existing == PhpType::Void
                        || (matches!(*existing, PhpType::Int | PhpType::Bool | PhpType::Float)
                            && matches!(ty, PhpType::Int | PhpType::Bool | PhpType::Float));
                    if !compatible {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Type error: cannot reassign ${} from {:?} to {:?}",
                                name, existing, ty
                            ),
                        ));
                    }
                    // If variable was null and now gets a real type, update it
                    if *existing == PhpType::Void && ty != PhpType::Void {
                        env.insert(name.clone(), ty);
                    }
                } else {
                    env.insert(name.clone(), ty);
                }
                Ok(())
            }
            StmtKind::ArrayAssign { array, index, value } => {
                let arr_ty = env.get(array).cloned().ok_or_else(|| {
                    CompileError::new(stmt.span, &format!("Undefined variable: ${}", array))
                })?;
                self.infer_type(index, env)?;
                let val_ty = self.infer_type(value, env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    if *elem_ty != val_ty {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Array element type mismatch: expected {:?}, got {:?}",
                                elem_ty, val_ty
                            ),
                        ));
                    }
                }
                Ok(())
            }
            StmtKind::ArrayPush { array, value } => {
                let arr_ty = env.get(array).cloned().ok_or_else(|| {
                    CompileError::new(stmt.span, &format!("Undefined variable: ${}", array))
                })?;
                let val_ty = self.infer_type(value, env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    if *elem_ty != val_ty {
                        return Err(CompileError::new(stmt.span, "Array push type mismatch"));
                    }
                }
                Ok(())
            }
            StmtKind::Foreach { array, key_var, value_var, body } => {
                let arr_ty = self.infer_type(array, env)?;
                if let PhpType::Array(elem_ty) = &arr_ty {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), PhpType::Int);
                    }
                    env.insert(value_var.clone(), *elem_ty.clone());
                } else if let PhpType::AssocArray { key, value } = &arr_ty {
                    if let Some(k) = key_var {
                        env.insert(k.clone(), *key.clone());
                    }
                    env.insert(value_var.clone(), *value.clone());
                } else {
                    return Err(CompileError::new(stmt.span, "foreach requires an array"));
                }
                for s in body {
                    self.check_stmt(s, env)?;
                }
                Ok(())
            }
            StmtKind::Switch { subject, cases, default } => {
                self.infer_type(subject, env)?;
                for (values, body) in cases {
                    for v in values {
                        self.infer_type(v, env)?;
                    }
                    for s in body {
                        self.check_stmt(s, env)?;
                    }
                }
                if let Some(body) = default {
                    for s in body {
                        self.check_stmt(s, env)?;
                    }
                }
                Ok(())
            }
            StmtKind::If {
                condition, then_body, elseif_clauses, else_body,
            } => {
                self.infer_type(condition, env)?;
                for s in then_body { self.check_stmt(s, env)?; }
                for (cond, body) in elseif_clauses {
                    self.infer_type(cond, env)?;
                    for s in body { self.check_stmt(s, env)?; }
                }
                if let Some(body) = else_body {
                    for s in body { self.check_stmt(s, env)?; }
                }
                Ok(())
            }
            StmtKind::DoWhile { body, condition } => {
                for s in body { self.check_stmt(s, env)?; }
                self.infer_type(condition, env)?;
                Ok(())
            }
            StmtKind::While { condition, body } => {
                self.infer_type(condition, env)?;
                for s in body { self.check_stmt(s, env)?; }
                Ok(())
            }
            StmtKind::For { init, condition, update, body } => {
                if let Some(s) = init { self.check_stmt(s, env)?; }
                if let Some(c) = condition { self.infer_type(c, env)?; }
                if let Some(s) = update { self.check_stmt(s, env)?; }
                for s in body { self.check_stmt(s, env)?; }
                Ok(())
            }
            StmtKind::Include { .. } => {
                // Should have been resolved before type checking
                Err(CompileError::new(stmt.span, "Unresolved include statement"))
            }
            StmtKind::Break | StmtKind::Continue => Ok(()),
            StmtKind::ExprStmt(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::ConstDecl { name, value } => {
                let ty = self.infer_type(value, env)?;
                self.constants.insert(name.clone(), ty);
                Ok(())
            }
            StmtKind::ListUnpack { vars, value } => {
                let arr_ty = self.infer_type(value, env)?;
                match &arr_ty {
                    PhpType::Array(elem_ty) => {
                        for var in vars {
                            env.insert(var.clone(), *elem_ty.clone());
                        }
                    }
                    _ => {
                        return Err(CompileError::new(
                            stmt.span,
                            "List unpacking requires an array on the right-hand side",
                        ));
                    }
                }
                Ok(())
            }
            StmtKind::Global { vars } => {
                // global vars are accessible; they reference variables from the outer scope
                // Mark them in the environment if not already present
                for var in vars {
                    if !env.contains_key(var) {
                        // Default to Int — will be refined by actual usage
                        env.insert(var.clone(), PhpType::Int);
                    }
                }
                Ok(())
            }
            StmtKind::StaticVar { name, init } => {
                let ty = self.infer_type(init, env)?;
                env.insert(name.clone(), ty);
                Ok(())
            }
            StmtKind::FunctionDecl { .. } => Ok(()),
            StmtKind::Return(expr) => {
                if let Some(e) = expr { self.infer_type(e, env)?; }
                Ok(())
            }
        }
    }

    pub fn infer_type(&mut self, expr: &Expr, env: &TypeEnv) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::BoolLiteral(_) => Ok(PhpType::Bool),
            ExprKind::Null => Ok(PhpType::Void),
            ExprKind::StringLiteral(_) => Ok(PhpType::Str),
            ExprKind::IntLiteral(_) => Ok(PhpType::Int),
            ExprKind::FloatLiteral(_) => Ok(PhpType::Float),
            ExprKind::Variable(name) => env.get(name).cloned().ok_or_else(|| {
                CompileError::new(expr.span, &format!("Undefined variable: ${}", name))
            }),
            ExprKind::Negate(inner) => {
                let ty = self.infer_type(inner, env)?;
                match ty {
                    PhpType::Int => Ok(PhpType::Int),
                    PhpType::Float => Ok(PhpType::Float),
                    _ => Err(CompileError::new(expr.span, "Cannot negate a non-numeric value")),
                }
            }
            ExprKind::Not(inner) => {
                self.infer_type(inner, env)?;
                Ok(PhpType::Bool)
            }
            ExprKind::PreIncrement(name) | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name) | ExprKind::PostDecrement(name) => {
                match env.get(name) {
                    Some(PhpType::Int) | Some(PhpType::Bool) | Some(PhpType::Void) => Ok(PhpType::Int),
                    Some(other) => Err(CompileError::new(
                        expr.span,
                        &format!("Cannot increment/decrement ${} of type {:?}", name, other),
                    )),
                    None => Err(CompileError::new(
                        expr.span, &format!("Undefined variable: ${}", name),
                    )),
                }
            }
            ExprKind::ArrayLiteralAssoc(pairs) => {
                if pairs.is_empty() {
                    return Err(CompileError::new(
                        expr.span, "Cannot infer type of empty associative array literal",
                    ));
                }
                let key_ty = self.infer_type(&pairs[0].0, env)?;
                let val_ty = self.infer_type(&pairs[0].1, env)?;
                for (k, v) in &pairs[1..] {
                    let kt = self.infer_type(k, env)?;
                    let vt = self.infer_type(v, env)?;
                    if kt != key_ty {
                        return Err(CompileError::new(
                            k.span,
                            &format!("Assoc array key type mismatch: expected {:?}, got {:?}", key_ty, kt),
                        ));
                    }
                    if vt != val_ty {
                        return Err(CompileError::new(
                            v.span,
                            &format!("Assoc array value type mismatch: expected {:?}, got {:?}", val_ty, vt),
                        ));
                    }
                }
                Ok(PhpType::AssocArray {
                    key: Box::new(key_ty),
                    value: Box::new(val_ty),
                })
            }
            ExprKind::Match { subject, arms, default } => {
                self.infer_type(subject, env)?;
                let mut result_ty = None;
                for (conditions, result) in arms {
                    for c in conditions {
                        self.infer_type(c, env)?;
                    }
                    let ty = self.infer_type(result, env)?;
                    if result_ty.is_none() {
                        result_ty = Some(ty);
                    }
                }
                if let Some(d) = default {
                    let ty = self.infer_type(d, env)?;
                    if result_ty.is_none() {
                        result_ty = Some(ty);
                    }
                }
                Ok(result_ty.unwrap_or(PhpType::Void))
            }
            ExprKind::ArrayLiteral(elems) => {
                if elems.is_empty() {
                    return Ok(PhpType::Array(Box::new(PhpType::Int)));
                }
                let first_ty = self.infer_type(&elems[0], env)?;
                for elem in &elems[1..] {
                    let ty = self.infer_type(elem, env)?;
                    if ty != first_ty {
                        return Err(CompileError::new(
                            elem.span,
                            &format!("Array element type mismatch: expected {:?}, got {:?}", first_ty, ty),
                        ));
                    }
                }
                Ok(PhpType::Array(Box::new(first_ty)))
            }
            ExprKind::ArrayAccess { array, index } => {
                let arr_ty = self.infer_type(array, env)?;
                let idx_ty = self.infer_type(index, env)?;
                match &arr_ty {
                    PhpType::Array(elem_ty) => {
                        if idx_ty != PhpType::Int {
                            return Err(CompileError::new(expr.span, "Array index must be integer"));
                        }
                        Ok(*elem_ty.clone())
                    }
                    PhpType::AssocArray { value, .. } => {
                        // Assoc arrays accept string or int keys
                        Ok(*value.clone())
                    }
                    _ => Err(CompileError::new(expr.span, "Cannot index non-array")),
                }
            }
            ExprKind::Ternary { condition, then_expr, else_expr } => {
                self.infer_type(condition, env)?;
                let then_ty = self.infer_type(then_expr, env)?;
                let else_ty = self.infer_type(else_expr, env)?;
                if then_ty == else_ty {
                    Ok(then_ty)
                } else {
                    // Different types: return the then-branch type (PHP is dynamic)
                    Ok(then_ty)
                }
            }
            ExprKind::Cast { target, expr } => {
                self.infer_type(expr, env)?;
                use crate::parser::ast::CastType;
                Ok(match target {
                    CastType::Int => PhpType::Int,
                    CastType::Float => PhpType::Float,
                    CastType::String => PhpType::Str,
                    CastType::Bool => PhpType::Bool,
                    CastType::Array => PhpType::Array(Box::new(PhpType::Int)),
                })
            }
            ExprKind::FunctionCall { name, args } => {
                let name = name.clone();
                let args = args.clone();
                if let Some(ty) = self.check_builtin(&name, &args, expr.span, env)? {
                    return Ok(ty);
                }
                self.check_function_call(&name, &args, expr.span, env)
            }
            ExprKind::BitNot(inner) => {
                let ty = self.infer_type(inner, env)?;
                if !matches!(ty, PhpType::Int | PhpType::Bool | PhpType::Void) {
                    return Err(CompileError::new(expr.span, "Bitwise NOT requires integer operand"));
                }
                Ok(PhpType::Int)
            }
            ExprKind::NullCoalesce { value, default } => {
                let vt = self.infer_type(value, env)?;
                let dt = self.infer_type(default, env)?;
                // Result type is the non-null type, prefer left if both non-void
                if vt == PhpType::Void { Ok(dt) } else { Ok(vt) }
            }
            ExprKind::ConstRef(name) => {
                self.constants.get(name).cloned().ok_or_else(|| {
                    CompileError::new(expr.span, &format!("Undefined constant: {}", name))
                })
            }
            ExprKind::Closure { params, variadic, body, is_arrow: _ } => {
                // Type-check the closure body in its own environment
                let mut closure_env: TypeEnv = env.clone();
                // Add params as Int (simple default for now — they'll be refined at call site)
                for (p, _default, _is_ref) in params {
                    closure_env.insert(p.clone(), PhpType::Int);
                }
                if let Some(vp) = variadic {
                    closure_env.insert(vp.clone(), PhpType::Array(Box::new(PhpType::Int)));
                }
                for stmt in body {
                    self.check_stmt(stmt, &mut closure_env)?;
                }
                Ok(PhpType::Callable)
            }
            ExprKind::Spread(inner) => {
                let ty = self.infer_type(inner, env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(*elem_ty),
                    _ => Err(CompileError::new(expr.span, "Spread operator requires an array")),
                }
            }
            ExprKind::ClosureCall { var, args } => {
                let var_ty = env.get(var).cloned().ok_or_else(|| {
                    CompileError::new(expr.span, &format!("Undefined variable: ${}", var))
                })?;
                if var_ty != PhpType::Callable {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Cannot call ${} — not a callable (got {:?})", var, var_ty),
                    ));
                }
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                // Use tracked return type if available, otherwise default to Int.
                let ret_ty = self.closure_return_types.get(var).cloned().unwrap_or(PhpType::Int);
                Ok(ret_ty)
            }
            ExprKind::ExprCall { callee, args } => {
                let callee_ty = self.infer_type(callee, env)?;
                if callee_ty != PhpType::Callable {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Cannot call expression — not a callable (got {:?})", callee_ty),
                    ));
                }
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                Ok(PhpType::Int)
            }
            ExprKind::BinaryOp { left, op, right } => {
                let lt = self.infer_type(left, env)?;
                let rt = self.infer_type(right, env)?;
                match op {
                    BinOp::Pow => {
                        let lt_ok = matches!(lt, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void);
                        let rt_ok = matches!(rt, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void);
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span, "Exponentiation requires numeric operands",
                            ));
                        }
                        Ok(PhpType::Float)
                    }
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        let lt_ok = matches!(lt, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void);
                        let rt_ok = matches!(rt, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void);
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span, "Arithmetic operators require numeric operands",
                            ));
                        }
                        // Division always returns float (PHP compat: 10/3 → 3.333...)
                        if *op == BinOp::Div || lt == PhpType::Float || rt == PhpType::Float {
                            Ok(PhpType::Float)
                        } else {
                            Ok(PhpType::Int)
                        }
                    }
                    BinOp::Eq | BinOp::NotEq => {
                        // Loose comparison accepts any types — coerces at runtime
                        Ok(PhpType::Bool)
                    }
                    BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
                        let lt_ok = matches!(lt, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void);
                        let rt_ok = matches!(rt, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void);
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span, "Comparison operators require numeric operands",
                            ));
                        }
                        Ok(PhpType::Bool)
                    }
                    BinOp::StrictEq | BinOp::StrictNotEq => {
                        // Strict comparison accepts any types — compares both type and value
                        Ok(PhpType::Bool)
                    }
                    BinOp::Concat => Ok(PhpType::Str),
                    BinOp::And | BinOp::Or => Ok(PhpType::Bool),
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor
                    | BinOp::ShiftLeft | BinOp::ShiftRight => {
                        let lt_ok = matches!(lt, PhpType::Int | PhpType::Bool | PhpType::Void);
                        let rt_ok = matches!(rt, PhpType::Int | PhpType::Bool | PhpType::Void);
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span, "Bitwise operators require integer operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::Spaceship => {
                        let lt_ok = matches!(lt, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void);
                        let rt_ok = matches!(rt, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void);
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span, "Spaceship operator requires numeric operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::NullCoalesce => {
                        // Handled by ExprKind::NullCoalesce — shouldn't reach here
                        // but handle gracefully
                        if lt == PhpType::Void { Ok(rt) } else { Ok(lt) }
                    }
                }
            }
        }
    }

    /// Infer the return type of a closure by scanning its body for Return statements.
    fn infer_closure_return_type(&self, body: &[Stmt], env: &TypeEnv) -> PhpType {
        for stmt in body {
            if let StmtKind::Return(Some(expr)) = &stmt.kind {
                return match &expr.kind {
                    ExprKind::Closure { .. } => PhpType::Callable,
                    ExprKind::StringLiteral(_) => PhpType::Str,
                    ExprKind::FloatLiteral(_) => PhpType::Float,
                    ExprKind::BoolLiteral(_) => PhpType::Bool,
                    ExprKind::Null => PhpType::Void,
                    ExprKind::Variable(name) => {
                        env.get(name).cloned().unwrap_or(PhpType::Int)
                    }
                    _ => PhpType::Int,
                };
            }
        }
        PhpType::Int
    }
}
