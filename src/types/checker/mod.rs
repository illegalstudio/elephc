mod builtins;
mod functions;

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind, Program, Stmt, StmtKind};
use crate::types::{CheckResult, ClassInfo, FunctionSig, PhpType, TypeEnv};

/// Infer a function's return type by scanning its body for Return statements.
/// This is a syntactic/heuristic check — no full type inference.
/// Used for functions that are never called directly (only used as callbacks).
fn infer_return_type_syntactic(body: &[Stmt]) -> PhpType {
    for stmt in body {
        if let Some(ty) = find_return_type_syntactic(stmt) {
            return ty;
        }
    }
    PhpType::Int
}

fn find_return_type_syntactic(stmt: &Stmt) -> Option<PhpType> {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => Some(infer_expr_type_syntactic(expr)),
        StmtKind::If { then_body, elseif_clauses, else_body, .. } => {
            for s in then_body { if let Some(t) = find_return_type_syntactic(s) { return Some(t); } }
            for (_, body) in elseif_clauses { for s in body { if let Some(t) = find_return_type_syntactic(s) { return Some(t); } } }
            if let Some(body) = else_body { for s in body { if let Some(t) = find_return_type_syntactic(s) { return Some(t); } } }
            None
        }
        StmtKind::While { body, .. } | StmtKind::For { body, .. } | StmtKind::Foreach { body, .. } => {
            for s in body { if let Some(t) = find_return_type_syntactic(s) { return Some(t); } }
            None
        }
        _ => None,
    }
}

fn infer_expr_type_syntactic(expr: &Expr) -> PhpType {
    match &expr.kind {
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::BinaryOp { op: BinOp::Concat, .. } => PhpType::Str,
        ExprKind::Cast { target: CastType::String, .. } => PhpType::Str,
        ExprKind::Cast { target: CastType::Int, .. } => PhpType::Int,
        ExprKind::Cast { target: CastType::Float, .. } => PhpType::Float,
        ExprKind::Cast { target: CastType::Bool, .. } => PhpType::Bool,
        ExprKind::FunctionCall { name, .. } => {
            match name.as_str() {
                "substr" | "strtolower" | "strtoupper" | "trim" | "ltrim" | "rtrim"
                | "str_repeat" | "strrev" | "chr" | "str_replace" | "str_ireplace"
                | "ucfirst" | "lcfirst" | "ucwords" | "str_pad" | "implode"
                | "sprintf" | "nl2br" | "wordwrap" | "md5" | "sha1" | "hash"
                | "substr_replace" | "addslashes" | "stripslashes"
                | "htmlspecialchars" | "html_entity_decode" | "urlencode" | "urldecode"
                | "base64_encode" | "base64_decode" | "bin2hex" | "hex2bin"
                | "number_format" | "date" | "json_encode" | "gettype"
                | "str_word_count" | "chunk_split" => PhpType::Str,
                "strlen" | "strpos" | "strrpos" | "ord" | "count" | "intval"
                | "abs" | "floor" | "ceil" | "round" | "intdiv" | "rand" | "time" => PhpType::Int,
                "floatval" | "sqrt" | "pow" | "fmod" => PhpType::Float,
                _ => PhpType::Int,
            }
        }
        ExprKind::Ternary { then_expr, .. } => infer_expr_type_syntactic(then_expr),
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.clone()),
        _ => PhpType::Int,
    }
}

pub(crate) struct Checker {
    pub fn_decls: HashMap<String, FnDecl>,
    pub functions: HashMap<String, FunctionSig>,
    pub constants: HashMap<String, PhpType>,
    /// Tracks the return type of closures assigned to variables.
    pub closure_return_types: HashMap<String, PhpType>,
    /// Class definitions collected during first pass.
    pub classes: HashMap<String, ClassInfo>,
    /// Name of the class currently being type-checked (for $this).
    pub current_class: Option<String>,
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
        classes: HashMap::new(),
        current_class: None,
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

    // First pass: collect class declarations and build ClassInfo
    for stmt in program {
        if let StmtKind::ClassDecl { name, properties, methods } = &stmt.kind {
            let mut prop_types = Vec::new();
            for prop in properties {
                let ty = if let Some(default) = &prop.default {
                    infer_expr_type_syntactic(default)
                } else {
                    PhpType::Int // properties without defaults are set by constructor
                };
                prop_types.push((prop.name.clone(), ty));
            }
            let mut method_sigs = HashMap::new();
            let mut static_sigs = HashMap::new();
            for method in methods {
                let params: Vec<(String, PhpType)> = method.params.iter()
                    .map(|(n, _, _)| (n.clone(), PhpType::Int))
                    .collect();
                let defaults: Vec<Option<Expr>> = method.params.iter()
                    .map(|(_, d, _)| d.clone())
                    .collect();
                let ref_params: Vec<bool> = method.params.iter()
                    .map(|(_, _, r)| *r)
                    .collect();
                let return_type = infer_return_type_syntactic(&method.body);
                let sig = FunctionSig {
                    params,
                    defaults,
                    return_type,
                    ref_params,
                    variadic: method.variadic.clone(),
                };
                if method.is_static {
                    static_sigs.insert(method.name.clone(), sig);
                } else {
                    method_sigs.insert(method.name.clone(), sig);
                }
            }
            // Build constructor param → property mapping
            // Scan __construct body for $this->prop = $param patterns
            let mut param_to_prop = Vec::new();
            if let Some(constructor) = methods.iter().find(|m| m.name == "__construct") {
                // For each constructor param, check if it's directly assigned to a property
                param_to_prop = constructor.params.iter().map(|(pname, _, _)| {
                    for stmt in &constructor.body {
                        if let StmtKind::PropertyAssign { property, value, .. } = &stmt.kind {
                            if let ExprKind::Variable(vn) = &value.kind {
                                if vn == pname {
                                    return Some(property.clone());
                                }
                            }
                        }
                    }
                    None
                }).collect();
            }

            let defaults: Vec<Option<Expr>> = properties.iter()
                .map(|p| p.default.clone())
                .collect();
            checker.classes.insert(name.clone(), ClassInfo {
                properties: prop_types,
                defaults,
                methods: method_sigs,
                static_methods: static_sigs,
                constructor_param_to_prop: param_to_prop,
            });
        }
    }

    let mut global_env: TypeEnv = HashMap::new();
    global_env.insert("argc".to_string(), PhpType::Int);
    global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
    for stmt in program {
        checker.check_stmt(stmt, &mut global_env)?;
    }

    // Register provisional signatures for functions that were declared but never
    // called directly (e.g., used only as string callbacks in array_map).
    // This ensures their return types are available for callback type inference.
    let unchecked: Vec<String> = checker.fn_decls.keys()
        .filter(|name| !checker.functions.contains_key(*name))
        .cloned()
        .collect();
    for name in unchecked {
        if let Some(decl) = checker.fn_decls.get(&name) {
            let return_type = infer_return_type_syntactic(&decl.body);
            let params = decl.params.iter()
                .map(|p| (p.clone(), PhpType::Int))
                .collect();
            checker.functions.insert(name.clone(), FunctionSig {
                params,
                defaults: decl.defaults.clone(),
                return_type,
                ref_params: decl.ref_params.clone(),
                variadic: decl.variadic.clone(),
            });
        }
    }

    // Post-pass: type-check class method bodies NOW that property types
    // have been updated from new ClassName(args) calls in the main scope.
    // This ensures methods see correct property types (e.g., Str not Int).
    for stmt in program {
        if let StmtKind::ClassDecl { name, methods, .. } = &stmt.kind {
            for method in methods {
                let mut method_env: TypeEnv = global_env.clone();
                if !method.is_static {
                    method_env.insert("this".to_string(), PhpType::Object(name.clone()));
                }
                // Infer param types from constructor mapping + class info
                for (pname, _, _) in &method.params {
                    method_env.insert(pname.clone(), PhpType::Int);
                }
                // For __construct: infer param types from property types
                // This updates both the env (for body type-checking) and the sig
                // (for correct register assignment in codegen prologue)
                if method.name == "__construct" {
                    if let Some(ci) = checker.classes.get(name).cloned() {
                        for (i, (pname, _, _)) in method.params.iter().enumerate() {
                            if let Some(Some(prop_name)) = ci.constructor_param_to_prop.get(i) {
                                if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == prop_name) {
                                    method_env.insert(pname.clone(), ty.clone());
                                    // Also update the sig in ClassInfo
                                    // (sig.params has user params only, $this added by codegen)
                                    if let Some(ci_mut) = checker.classes.get_mut(name) {
                                        if let Some(sig) = ci_mut.methods.get_mut("__construct") {
                                            if i < sig.params.len() {
                                                sig.params[i].1 = ty.clone();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                checker.current_class = Some(name.clone());
                for s in &method.body {
                    let _ = checker.check_stmt(s, &mut method_env);
                }
                checker.current_class = None;

                // Update method return type from full type inference
                if !method.is_static {
                    for s in &method.body {
                        if let Some(ty) = checker.find_return_type(s, &method_env) {
                            if let Some(ci) = checker.classes.get_mut(name) {
                                if let Some(sig) = ci.methods.get_mut(&method.name) {
                                    sig.return_type = ty;
                                }
                            }
                            break;
                        }
                    }
                } else {
                    for s in &method.body {
                        if let Some(ty) = checker.find_return_type(s, &method_env) {
                            if let Some(ci) = checker.classes.get_mut(name) {
                                if let Some(sig) = ci.static_methods.get_mut(&method.name) {
                                    sig.return_type = ty;
                                }
                            }
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
        classes: checker.classes,
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
                if let PhpType::Array(elem_ty) = &arr_ty {
                    if **elem_ty != val_ty {
                        // Upgrade array element type when assigning a
                        // different type (e.g. empty [] defaults to
                        // Array(Int), first string assign upgrades it)
                        env.insert(array.clone(), PhpType::Array(Box::new(val_ty)));
                    }
                }
                Ok(())
            }
            StmtKind::ArrayPush { array, value } => {
                let arr_ty = env.get(array).cloned().ok_or_else(|| {
                    CompileError::new(stmt.span, &format!("Undefined variable: ${}", array))
                })?;
                let val_ty = self.infer_type(value, env)?;
                if let PhpType::Array(elem_ty) = &arr_ty {
                    if **elem_ty != val_ty {
                        // Upgrade array type when pushing a different type
                        // (e.g. empty [] defaults to Array(Int), first push
                        // of a string should upgrade to Array(Str))
                        env.insert(array.clone(), PhpType::Array(Box::new(val_ty)));
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
            StmtKind::ClassDecl { .. } => {
                // Method bodies are type-checked in a post-pass (after all new ClassName()
                // calls have updated property types from constructor arg types)
                Ok(())
            }
            StmtKind::PropertyAssign { object, property, value } => {
                let obj_ty = self.infer_type(object, env)?;
                if let PhpType::Object(class_name) = &obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        if !class_info.properties.iter().any(|(n, _)| n == property) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined property: {}::{}", class_name, property),
                            ));
                        }
                    }
                }
                self.infer_type(value, env)?;
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
            ExprKind::Closure { params, variadic, body, is_arrow: _, captures } => {
                // Verify captured variables exist in the enclosing scope
                for cap in captures {
                    if !env.contains_key(cap) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!("Undefined variable in use(): ${}", cap),
                        ));
                    }
                }
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
            ExprKind::NewObject { class_name, args } => {
                if !self.classes.contains_key(class_name) {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Undefined class: {}", class_name),
                    ));
                }
                // Infer arg types and propagate to property types via constructor mapping
                let param_to_prop = self.classes.get(class_name)
                    .map(|c| c.constructor_param_to_prop.clone())
                    .unwrap_or_default();
                for (i, arg) in args.iter().enumerate() {
                    let arg_ty = self.infer_type(arg, env)?;
                    // If this arg maps to a property, update the property type
                    if let Some(Some(prop_name)) = param_to_prop.get(i) {
                        if let Some(class_info) = self.classes.get_mut(class_name) {
                            if let Some(prop) = class_info.properties.iter_mut().find(|(n, _)| n == prop_name) {
                                prop.1 = arg_ty;
                            }
                        }
                    }
                }
                Ok(PhpType::Object(class_name.clone()))
            }
            ExprKind::PropertyAccess { object, property } => {
                let obj_ty = self.infer_type(object, env)?;
                if let PhpType::Object(class_name) = &obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        if let Some((_, ty)) = class_info.properties.iter().find(|(n, _)| n == property) {
                            return Ok(ty.clone());
                        }
                        return Err(CompileError::new(
                            expr.span,
                            &format!("Undefined property: {}::{}", class_name, property),
                        ));
                    }
                }
                Ok(PhpType::Int)
            }
            ExprKind::MethodCall { object, method, args } => {
                let obj_ty = self.infer_type(object, env)?;
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                if let PhpType::Object(class_name) = &obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        if let Some(sig) = class_info.methods.get(method) {
                            return Ok(sig.return_type.clone());
                        }
                    }
                }
                Ok(PhpType::Int)
            }
            ExprKind::StaticMethodCall { class_name, method, args } => {
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                if let Some(class_info) = self.classes.get(class_name) {
                    if let Some(sig) = class_info.static_methods.get(method) {
                        return Ok(sig.return_type.clone());
                    }
                }
                Ok(PhpType::Int)
            }
            ExprKind::This => {
                if let Some(class_name) = &self.current_class {
                    Ok(PhpType::Object(class_name.clone()))
                } else {
                    Err(CompileError::new(expr.span, "Cannot use $this outside of a class method"))
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
