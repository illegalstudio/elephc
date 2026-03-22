use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{CheckResult, FunctionSig, PhpType, TypeEnv};

struct Checker {
    fn_decls: HashMap<String, FnDecl>,
    functions: HashMap<String, FunctionSig>,
}

#[derive(Clone)]
struct FnDecl {
    params: Vec<String>,
    body: Vec<Stmt>,
}

pub fn check_types(program: &Program) -> Result<CheckResult, CompileError> {
    let mut checker = Checker {
        fn_decls: HashMap::new(),
        functions: HashMap::new(),
    };

    // Pass 1: collect function declarations
    for stmt in program {
        if let StmtKind::FunctionDecl { name, params, body } = &stmt.kind {
            checker.fn_decls.insert(
                name.clone(),
                FnDecl {
                    params: params.clone(),
                    body: body.clone(),
                },
            );
        }
    }

    // Pass 2: type-check global statements
    let mut global_env: TypeEnv = HashMap::new();
    // Pre-define $argc as a global integer variable
    global_env.insert("argc".to_string(), PhpType::Int);
    for stmt in program {
        checker.check_stmt(stmt, &mut global_env)?;
    }

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
    })
}

impl Checker {
    fn check_stmt(&mut self, stmt: &Stmt, env: &mut TypeEnv) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Echo(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::Assign { name, value } => {
                let ty = self.infer_type(value, env)?;
                if let Some(existing) = env.get(name) {
                    if *existing != ty {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Type error: cannot reassign ${} from {:?} to {:?}",
                                name, existing, ty
                            ),
                        ));
                    }
                } else {
                    env.insert(name.clone(), ty);
                }
                Ok(())
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                self.infer_type(condition, env)?;
                for s in then_body {
                    self.check_stmt(s, env)?;
                }
                for (cond, body) in elseif_clauses {
                    self.infer_type(cond, env)?;
                    for s in body {
                        self.check_stmt(s, env)?;
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        self.check_stmt(s, env)?;
                    }
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
                        return Err(CompileError::new(
                            stmt.span,
                            "Array push type mismatch",
                        ));
                    }
                }
                Ok(())
            }
            StmtKind::Foreach {
                array,
                value_var,
                body,
            } => {
                let arr_ty = self.infer_type(array, env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    env.insert(value_var.clone(), *elem_ty);
                } else {
                    return Err(CompileError::new(
                        stmt.span,
                        "foreach requires an array",
                    ));
                }
                for s in body {
                    self.check_stmt(s, env)?;
                }
                Ok(())
            }
            StmtKind::DoWhile { body, condition } => {
                for s in body {
                    self.check_stmt(s, env)?;
                }
                self.infer_type(condition, env)?;
                Ok(())
            }
            StmtKind::While { condition, body } => {
                self.infer_type(condition, env)?;
                for s in body {
                    self.check_stmt(s, env)?;
                }
                Ok(())
            }
            StmtKind::For {
                init,
                condition,
                update,
                body,
            } => {
                if let Some(s) = init {
                    self.check_stmt(s, env)?;
                }
                if let Some(c) = condition {
                    self.infer_type(c, env)?;
                }
                if let Some(s) = update {
                    self.check_stmt(s, env)?;
                }
                for s in body {
                    self.check_stmt(s, env)?;
                }
                Ok(())
            }
            StmtKind::Break | StmtKind::Continue => Ok(()),
            StmtKind::ExprStmt(expr) => {
                self.infer_type(expr, env)?;
                Ok(())
            }
            StmtKind::FunctionDecl { .. } => Ok(()),
            StmtKind::Return(expr) => {
                if let Some(e) = expr {
                    self.infer_type(e, env)?;
                }
                Ok(())
            }
        }
    }

    fn infer_type(&mut self, expr: &Expr, env: &TypeEnv) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::StringLiteral(_) => Ok(PhpType::Str),
            ExprKind::IntLiteral(_) => Ok(PhpType::Int),
            ExprKind::Variable(name) => env.get(name).cloned().ok_or_else(|| {
                CompileError::new(expr.span, &format!("Undefined variable: ${}", name))
            }),
            ExprKind::Negate(inner) => {
                let ty = self.infer_type(inner, env)?;
                if ty != PhpType::Int {
                    return Err(CompileError::new(expr.span, "Cannot negate a non-integer"));
                }
                Ok(PhpType::Int)
            }
            ExprKind::Not(inner) => {
                self.infer_type(inner, env)?;
                Ok(PhpType::Int) // !x returns 0 or 1
            }
            ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name) => match env.get(name) {
                Some(PhpType::Int) => Ok(PhpType::Int),
                Some(other) => Err(CompileError::new(
                    expr.span,
                    &format!("Cannot increment/decrement ${} of type {:?}", name, other),
                )),
                None => Err(CompileError::new(
                    expr.span,
                    &format!("Undefined variable: ${}", name),
                )),
            },
            ExprKind::ArrayLiteral(elems) => {
                if elems.is_empty() {
                    return Err(CompileError::new(
                        expr.span,
                        "Cannot infer type of empty array literal",
                    ));
                }
                let first_ty = self.infer_type(&elems[0], env)?;
                for elem in &elems[1..] {
                    let ty = self.infer_type(elem, env)?;
                    if ty != first_ty {
                        return Err(CompileError::new(
                            elem.span,
                            &format!(
                                "Array element type mismatch: expected {:?}, got {:?}",
                                first_ty, ty
                            ),
                        ));
                    }
                }
                Ok(PhpType::Array(Box::new(first_ty)))
            }
            ExprKind::ArrayAccess { array, index } => {
                let arr_ty = self.infer_type(array, env)?;
                let idx_ty = self.infer_type(index, env)?;
                if idx_ty != PhpType::Int {
                    return Err(CompileError::new(expr.span, "Array index must be integer"));
                }
                match arr_ty {
                    PhpType::Array(elem_ty) => Ok(*elem_ty),
                    _ => Err(CompileError::new(expr.span, "Cannot index non-array")),
                }
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.infer_type(condition, env)?;
                let then_ty = self.infer_type(then_expr, env)?;
                let else_ty = self.infer_type(else_expr, env)?;
                if then_ty != else_ty {
                    return Err(CompileError::new(
                        expr.span,
                        &format!(
                            "Ternary branches must have the same type: {:?} vs {:?}",
                            then_ty, else_ty
                        ),
                    ));
                }
                Ok(then_ty)
            }
            ExprKind::FunctionCall { name, args } => {
                let name = name.clone();
                let args = args.clone();
                // Check built-in functions first
                if let Some(ty) = self.check_builtin(&name, &args, expr.span, env)? {
                    return Ok(ty);
                }
                self.check_function_call(&name, &args, expr.span, env)
            }
            ExprKind::BinaryOp { left, op, right } => {
                let lt = self.infer_type(left, env)?;
                let rt = self.infer_type(right, env)?;
                match op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        if lt != PhpType::Int || rt != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span,
                                "Arithmetic operators require integer operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Gt | BinOp::LtEq
                    | BinOp::GtEq => {
                        if lt != PhpType::Int || rt != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span,
                                "Comparison operators require integer operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::Concat => Ok(PhpType::Str),
                BinOp::And | BinOp::Or => {
                    // Both sides can be any type (truthy/falsy), result is Int (0 or 1)
                    Ok(PhpType::Int)
                }
                }
            }
        }
    }

    fn check_builtin(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<Option<PhpType>, CompileError> {
        match name {
            "exit" | "die" => {
                if args.len() > 1 {
                    return Err(CompileError::new(span, "exit() takes 0 or 1 arguments"));
                }
                if let Some(arg) = args.first() {
                    let ty = self.infer_type(arg, env)?;
                    if ty != PhpType::Int {
                        return Err(CompileError::new(span, "exit() argument must be integer"));
                    }
                }
                Ok(Some(PhpType::Void))
            }
            "strlen" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "strlen() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if ty != PhpType::Str {
                    return Err(CompileError::new(span, "strlen() argument must be string"));
                }
                Ok(Some(PhpType::Int))
            }
            "intval" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "intval() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "is_null" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "is_null() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "count" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "count() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_)) {
                    return Err(CompileError::new(span, "count() argument must be array"));
                }
                Ok(Some(PhpType::Int))
            }
            "array_pop" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "array_pop() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                match ty {
                    PhpType::Array(elem_ty) => Ok(Some(*elem_ty)),
                    _ => Err(CompileError::new(span, "array_pop() argument must be array")),
                }
            }
            "in_array" => {
                if args.len() != 2 {
                    return Err(CompileError::new(span, "in_array() takes exactly 2 arguments"));
                }
                self.infer_type(&args[0], env)?;
                let arr_ty = self.infer_type(&args[1], env)?;
                if !matches!(arr_ty, PhpType::Array(_)) {
                    return Err(CompileError::new(span, "in_array() second argument must be array"));
                }
                Ok(Some(PhpType::Int)) // returns 0 or 1
            }
            "array_keys" | "array_values" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 1 argument", name)));
                }
                let ty = self.infer_type(&args[0], env)?;
                match (name, &ty) {
                    ("array_keys", PhpType::Array(_)) => Ok(Some(PhpType::Array(Box::new(PhpType::Int)))),
                    ("array_values", PhpType::Array(elem_ty)) => Ok(Some(PhpType::Array(elem_ty.clone()))),
                    _ => Err(CompileError::new(span, &format!("{}() argument must be array", name))),
                }
            }
            "sort" | "rsort" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, &format!("{}() takes exactly 1 argument", name)));
                }
                let ty = self.infer_type(&args[0], env)?;
                if !matches!(ty, PhpType::Array(_)) {
                    return Err(CompileError::new(span, &format!("{}() argument must be array", name)));
                }
                Ok(Some(PhpType::Void))
            }
            "isset" => {
                // isset can take any expression, returns int
                if args.len() != 1 {
                    return Err(CompileError::new(span, "isset() takes exactly 1 argument"));
                }
                self.infer_type(&args[0], env)?;
                Ok(Some(PhpType::Int))
            }
            "array_push" => {
                if args.len() != 2 {
                    return Err(CompileError::new(
                        span,
                        "array_push() takes exactly 2 arguments",
                    ));
                }
                let arr_ty = self.infer_type(&args[0], env)?;
                let val_ty = self.infer_type(&args[1], env)?;
                if let PhpType::Array(elem_ty) = arr_ty {
                    if *elem_ty != val_ty {
                        return Err(CompileError::new(span, "array_push() type mismatch"));
                    }
                } else {
                    return Err(CompileError::new(
                        span,
                        "array_push() first argument must be array",
                    ));
                }
                Ok(Some(PhpType::Void))
            }
            "argv" => {
                if args.len() != 1 {
                    return Err(CompileError::new(span, "argv() takes exactly 1 argument"));
                }
                let ty = self.infer_type(&args[0], env)?;
                if ty != PhpType::Int {
                    return Err(CompileError::new(span, "argv() argument must be integer"));
                }
                Ok(Some(PhpType::Str))
            }
            _ => Ok(None), // not a built-in
        }
    }

    fn check_function_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        // Already resolved or being resolved (recursive)?
        if let Some(sig) = self.functions.get(name).cloned() {
            if sig.params.len() != args.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name,
                        sig.params.len(),
                        args.len()
                    ),
                ));
            }
            for (i, arg) in args.iter().enumerate() {
                let arg_ty = self.infer_type(arg, caller_env)?;
                if arg_ty != sig.params[i].1 {
                    return Err(CompileError::new(
                        arg.span,
                        &format!(
                            "Argument {} type mismatch: expected {:?}, got {:?}",
                            i + 1,
                            sig.params[i].1,
                            arg_ty
                        ),
                    ));
                }
            }
            return Ok(sig.return_type);
        }

        // Look up declaration
        let decl = self
            .fn_decls
            .get(name)
            .cloned()
            .ok_or_else(|| {
                CompileError::new(span, &format!("Undefined function: {}", name))
            })?;

        if decl.params.len() != args.len() {
            return Err(CompileError::new(
                span,
                &format!(
                    "Function '{}' expects {} arguments, got {}",
                    name,
                    decl.params.len(),
                    args.len()
                ),
            ));
        }

        // Infer parameter types from arguments
        let mut param_types = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let ty = self.infer_type(arg, caller_env)?;
            param_types.push((decl.params[i].clone(), ty));
        }

        // Create local environment with parameters
        let mut local_env: TypeEnv = HashMap::new();
        for (pname, pty) in &param_types {
            local_env.insert(pname.clone(), pty.clone());
        }

        // Insert a provisional signature to handle recursive calls.
        // Return type defaults to Int; will be updated after body analysis.
        let provisional_sig = FunctionSig {
            params: param_types.clone(),
            return_type: PhpType::Int,
        };
        self.functions.insert(name.to_string(), provisional_sig);

        // Type-check function body
        let mut return_type = PhpType::Void;
        for stmt in &decl.body {
            self.check_stmt(stmt, &mut local_env)?;
            if let Some(rt) = self.find_return_type(stmt, &local_env) {
                return_type = rt;
            }
        }

        // Store signature
        let sig = FunctionSig {
            params: param_types,
            return_type: return_type.clone(),
        };
        self.functions.insert(name.to_string(), sig);

        Ok(return_type)
    }

    fn find_return_type(&mut self, stmt: &Stmt, env: &TypeEnv) -> Option<PhpType> {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => self.infer_type(expr, env).ok(),
            StmtKind::Return(None) => Some(PhpType::Void),
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for s in then_body {
                    if let Some(t) = self.find_return_type(s, env) {
                        return Some(t);
                    }
                }
                for (_, body) in elseif_clauses {
                    for s in body {
                        if let Some(t) = self.find_return_type(s, env) {
                            return Some(t);
                        }
                    }
                }
                if let Some(body) = else_body {
                    for s in body {
                        if let Some(t) = self.find_return_type(s, env) {
                            return Some(t);
                        }
                    }
                }
                None
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. } => {
                for s in body {
                    if let Some(t) = self.find_return_type(s, env) {
                        return Some(t);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
