use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Platform;
use crate::errors::CompileError;
use crate::parser::ast::{Expr, Program, StmtKind, TypeExpr};
use crate::types::{
    ctype_stack_size, ctype_to_php_type, packed_type_size,
    traits::{flatten_classes, FlattenedClass},
    ExternClassInfo, ExternFieldInfo, ExternFunctionSig, FunctionSig, PackedClassInfo,
    PackedFieldInfo, PhpType, TypeEnv,
};

use super::builtin_types::{
    inject_builtin_throwables, patch_builtin_exception_signatures, patch_magic_method_signatures,
    InterfaceDeclInfo,
};
use super::schema::{
    build_class_info_recursive, build_enum_info, build_interface_info_recursive,
};
use super::{Checker, FnDecl};

pub(super) fn check_types_impl(
    program: &Program,
    target_platform: Platform,
) -> Result<(Checker, TypeEnv), CompileError> {
    let mut checker = Checker::new(target_platform);
    let mut errors = Vec::new();

    checker.collect_function_decls(program);

    let (flattened_classes, flatten_errors) = flatten_classes(program);
    errors.extend(flatten_errors);
    let mut class_map: HashMap<String, FlattenedClass> = flattened_classes
        .iter()
        .cloned()
        .map(|class| (class.name.clone(), class))
        .collect();
    let mut interface_map = HashMap::new();
    checker.declared_classes = class_map.keys().cloned().collect();
    for stmt in program {
        if let StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } = &stmt.kind
        {
            if interface_map.contains_key(name) {
                errors.push(CompileError::new(
                    stmt.span,
                    &format!("Duplicate interface declaration: {}", name),
                ));
                continue;
            }
            interface_map.insert(
                name.clone(),
                InterfaceDeclInfo {
                    name: name.clone(),
                    extends: extends
                        .iter()
                        .map(|name| name.as_str().to_string())
                        .collect(),
                    methods: methods.clone(),
                    span: stmt.span,
                },
            );
        }
    }
    checker.declared_interfaces = interface_map.keys().cloned().collect();
    if let Err(error) = inject_builtin_throwables(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }

    let mut next_interface_id = 0u64;
    let mut building_interfaces = HashSet::new();
    let interface_names: Vec<String> = interface_map.keys().cloned().collect();
    for interface_name in interface_names {
        if let Err(error) = build_interface_info_recursive(
            &interface_name,
            &interface_map,
            &class_map,
            &mut checker,
            &mut next_interface_id,
            &mut building_interfaces,
        ) {
            errors.extend(error.flatten());
        }
    }

    let mut next_class_id = 0u64;
    let mut building = HashSet::new();
    let class_names: Vec<String> = class_map.keys().cloned().collect();
    for class_name in class_names {
        if let Err(error) = build_class_info_recursive(
            &class_name,
            &class_map,
            &mut checker,
            &mut next_class_id,
            &mut building,
        ) {
            errors.extend(error.flatten());
        }
    }
    for stmt in program {
        if let StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } = &stmt.kind
        {
            if let Err(error) = build_enum_info(
                name,
                backing_type.as_ref(),
                cases,
                stmt.span,
                &mut checker,
                &mut next_class_id,
            ) {
                errors.extend(error.flatten());
            }
        }
    }
    patch_builtin_exception_signatures(&mut checker);
    patch_magic_method_signatures(&mut checker);

    checker.prescan_extern_decls(program, &mut errors);

    let (global_env, initial_top_level_errors) = checker.check_top_level_program(program);

    checker.resolve_unchecked_functions(&mut errors);
    checker.type_check_methods_until_stable(&flattened_classes, &global_env, &mut errors)?;

    let (final_global_env, final_top_level_errors) = checker.check_top_level_program(program);
    for ((stmt, initial_errors), final_errors) in program
        .iter()
        .zip(initial_top_level_errors.into_iter())
        .zip(final_top_level_errors.into_iter())
    {
        if !final_errors.is_empty() {
            errors.extend(final_errors);
            continue;
        }
        if !Checker::can_suppress_initial_top_level_errors(stmt, &initial_errors) {
            errors.extend(initial_errors);
        }
    }

    if !errors.is_empty() {
        return Err(CompileError::from_many(errors));
    }

    Ok((checker, final_global_env))
}

impl Checker {
    fn new(target_platform: Platform) -> Self {
        let mut constants = HashMap::new();
        constants.insert("PHP_OS".to_string(), PhpType::Str);

        Self {
            target_platform,
            fn_decls: HashMap::new(),
            functions: HashMap::new(),
            constants,
            closure_return_types: HashMap::new(),
            callable_sigs: HashMap::new(),
            first_class_callable_targets: HashMap::new(),
            interfaces: HashMap::new(),
            classes: HashMap::new(),
            declared_classes: HashSet::new(),
            enums: HashMap::new(),
            declared_interfaces: HashSet::new(),
            current_class: None,
            current_method: None,
            current_method_is_static: false,
            extern_functions: HashMap::new(),
            extern_classes: HashMap::new(),
            packed_classes: HashMap::new(),
            extern_globals: HashMap::new(),
            required_libraries: Vec::new(),
            top_level_env: HashMap::new(),
            active_ref_params: HashSet::new(),
            active_globals: HashSet::new(),
            active_statics: HashSet::new(),
        }
    }

    fn collect_function_decls(&mut self, program: &Program) {
        for stmt in program {
            if let StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                return_type,
                body,
                ..
            } = &stmt.kind
            {
                let param_names: Vec<String> = params.iter().map(|(n, _, _, _)| n.clone()).collect();
                let param_type_anns: Vec<Option<TypeExpr>> =
                    params.iter().map(|(_, t, _, _)| t.clone()).collect();
                let defaults: Vec<Option<Expr>> =
                    params.iter().map(|(_, _, d, _)| d.clone()).collect();
                let ref_flags: Vec<bool> = params.iter().map(|(_, _, _, r)| *r).collect();
                self.fn_decls.insert(
                    name.clone(),
                    FnDecl {
                        params: param_names,
                        param_types: param_type_anns,
                        defaults,
                        ref_params: ref_flags,
                        variadic: variadic.clone(),
                        return_type: return_type.clone(),
                        span: stmt.span,
                        body: body.clone(),
                    },
                );
            }
        }
    }

    fn prescan_extern_decls(&mut self, program: &Program, errors: &mut Vec<CompileError>) {
        for stmt in program {
            match &stmt.kind {
                StmtKind::ExternFunctionDecl {
                    name,
                    params,
                    return_type,
                    library,
                } => {
                    if self.extern_functions.contains_key(name) || self.fn_decls.contains_key(name) {
                        errors.push(CompileError::new(
                            stmt.span,
                            &format!("Duplicate function declaration: {}", name),
                        ));
                        continue;
                    }
                    let php_params: Vec<(String, PhpType)> = params
                        .iter()
                        .map(|p| (p.name.clone(), ctype_to_php_type(&p.c_type)))
                        .collect();
                    let php_ret = ctype_to_php_type(return_type);
                    if let Err(error) = self.validate_extern_function_decl(
                        name,
                        params,
                        return_type,
                        &php_params,
                        &php_ret,
                        stmt.span,
                    ) {
                        errors.extend(error.flatten());
                        continue;
                    }
                    let sig = FunctionSig {
                        params: php_params.clone(),
                        defaults: params.iter().map(|_| None).collect(),
                        return_type: php_ret.clone(),
                        ref_params: params.iter().map(|_| false).collect(),
                        declared_params: vec![true; php_params.len()],
                        variadic: None,
                    };
                    self.functions.insert(name.clone(), sig);
                    self.extern_functions.insert(
                        name.clone(),
                        ExternFunctionSig {
                            name: name.clone(),
                            params: php_params,
                            return_type: php_ret,
                            library: library.clone(),
                        },
                    );
                    if let Some(lib) = library {
                        if !self.required_libraries.contains(lib) {
                            self.required_libraries.push(lib.clone());
                        }
                    }
                }
                StmtKind::ExternClassDecl { name, fields } => {
                    if self.extern_classes.contains_key(name) || self.classes.contains_key(name) {
                        errors.push(CompileError::new(
                            stmt.span,
                            &format!("Duplicate class declaration: {}", name),
                        ));
                        continue;
                    }
                    let mut extern_fields = Vec::new();
                    let mut offset = 0usize;
                    let mut seen_fields = std::collections::HashSet::new();
                    let mut class_has_errors = false;
                    for f in fields {
                        if let Err(error) = self.validate_extern_field_decl(name, f, stmt.span) {
                            errors.extend(error.flatten());
                            class_has_errors = true;
                            continue;
                        }
                        if !seen_fields.insert(f.name.clone()) {
                            errors.push(CompileError::new(
                                stmt.span,
                                &format!("Duplicate extern field: {}::{}", name, f.name),
                            ));
                            class_has_errors = true;
                            continue;
                        }
                        let php_type = ctype_to_php_type(&f.c_type);
                        let size = ctype_stack_size(&f.c_type);
                        extern_fields.push(ExternFieldInfo {
                            name: f.name.clone(),
                            php_type,
                            offset,
                        });
                        offset += size;
                    }
                    if class_has_errors {
                        continue;
                    }
                    self.extern_classes.insert(
                        name.clone(),
                        ExternClassInfo {
                            name: name.clone(),
                            total_size: offset,
                            fields: extern_fields,
                        },
                    );
                }
                StmtKind::PackedClassDecl { name, fields } => {
                    if self.packed_classes.contains_key(name)
                        || self.classes.contains_key(name)
                        || self.extern_classes.contains_key(name)
                    {
                        errors.push(CompileError::new(
                            stmt.span,
                            &format!("Duplicate packed class declaration: {}", name),
                        ));
                        continue;
                    }
                    let mut packed_fields = Vec::new();
                    let mut offset = 0usize;
                    let mut seen_fields = std::collections::HashSet::new();
                    let mut class_has_errors = false;
                    for field in fields {
                        if !seen_fields.insert(field.name.clone()) {
                            errors.push(CompileError::new(
                                field.span,
                                &format!("Duplicate packed field: {}::{}", name, field.name),
                            ));
                            class_has_errors = true;
                            continue;
                        }
                        let php_type = match self.resolve_type_expr(&field.type_expr, field.span) {
                            Ok(php_type) => php_type,
                            Err(error) => {
                                errors.extend(error.flatten());
                                class_has_errors = true;
                                continue;
                            }
                        };
                        let Some(size) = packed_type_size(&php_type, &self.packed_classes) else {
                            errors.push(CompileError::new(
                                field.span,
                                "Packed class fields must use POD scalars, pointers, or packed classes",
                            ));
                            class_has_errors = true;
                            continue;
                        };
                        packed_fields.push(PackedFieldInfo {
                            name: field.name.clone(),
                            php_type,
                            offset,
                        });
                        offset += size;
                    }
                    if class_has_errors {
                        continue;
                    }
                    self.packed_classes.insert(
                        name.clone(),
                        PackedClassInfo {
                            fields: packed_fields,
                            total_size: offset,
                        },
                    );
                }
                StmtKind::ExternGlobalDecl { name, c_type } => {
                    if let Err(error) = self.validate_extern_global_decl(name, c_type, stmt.span) {
                        errors.extend(error.flatten());
                        continue;
                    }
                    let php_type = ctype_to_php_type(c_type);
                    self.extern_globals.insert(name.clone(), php_type);
                }
                _ => {}
            }
        }
    }

    fn resolve_unchecked_functions(&mut self, errors: &mut Vec<CompileError>) {
        let unchecked: Vec<String> = self
            .fn_decls
            .keys()
            .filter(|name| !self.functions.contains_key(*name))
            .cloned()
            .collect();
        for name in unchecked {
            if let Some(decl) = self.fn_decls.get(&name).cloned() {
                match self.initial_function_param_types(&name, &decl) {
                    Ok(param_types) => {
                        if let Err(error) =
                            self.resolve_function_signature(&name, &decl, param_types)
                        {
                            errors.extend(error.flatten());
                        }
                    }
                    Err(error) => errors.extend(error.flatten()),
                }
            }
        }
    }

    fn seed_global_env(&self) -> TypeEnv {
        let mut global_env: TypeEnv = HashMap::new();
        global_env.insert("argc".to_string(), PhpType::Int);
        global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
        for (name, ty) in &self.extern_globals {
            global_env.insert(name.clone(), ty.clone());
        }
        global_env
    }

    fn check_top_level_program(
        &mut self,
        program: &Program,
    ) -> (TypeEnv, Vec<Vec<CompileError>>) {
        let mut global_env = self.seed_global_env();
        let mut all_errors = Vec::with_capacity(program.len());
        for stmt in program {
            self.top_level_env = global_env.clone();
            let stmt_errors = self
                .check_stmt(stmt, &mut global_env)
                .err()
                .map(|error| error.flatten())
                .unwrap_or_default();
            all_errors.push(stmt_errors);
        }
        (global_env, all_errors)
    }

    fn can_suppress_initial_top_level_errors(
        stmt: &crate::parser::ast::Stmt,
        errors: &[CompileError],
    ) -> bool {
        !errors.is_empty()
            && Self::stmt_contains_method_call(stmt)
            && errors.iter().all(|error| {
                matches!(
                    error.message.as_str(),
                    "Cannot index non-array"
                        | "Property access requires an object or typed pointer"
                )
            })
    }

    fn stmt_contains_method_call(stmt: &crate::parser::ast::Stmt) -> bool {
        match &stmt.kind {
            StmtKind::ExprStmt(expr)
            | StmtKind::Echo(expr)
            | StmtKind::Return(Some(expr)) => Self::expr_contains_method_call(expr),
            StmtKind::Assign { value, .. }
            | StmtKind::TypedAssign { value, .. }
            | StmtKind::ConstDecl { value, .. }
            | StmtKind::ListUnpack { value, .. } => Self::expr_contains_method_call(value),
            StmtKind::ArrayAssign { index, value, .. } => {
                Self::expr_contains_method_call(index) || Self::expr_contains_method_call(value)
            }
            StmtKind::ArrayPush { value, .. } => Self::expr_contains_method_call(value),
            StmtKind::StaticPropertyAssign { value, .. }
            | StmtKind::StaticPropertyArrayPush { value, .. } => {
                Self::expr_contains_method_call(value)
            }
            StmtKind::StaticPropertyArrayAssign { index, value, .. } => {
                Self::expr_contains_method_call(index) || Self::expr_contains_method_call(value)
            }
            StmtKind::PropertyAssign { object, value, .. } => {
                Self::expr_contains_method_call(object) || Self::expr_contains_method_call(value)
            }
            StmtKind::PropertyArrayPush { object, value, .. } => {
                Self::expr_contains_method_call(object) || Self::expr_contains_method_call(value)
            }
            StmtKind::PropertyArrayAssign {
                object,
                index,
                value,
                ..
            } => {
                Self::expr_contains_method_call(object)
                    || Self::expr_contains_method_call(index)
                    || Self::expr_contains_method_call(value)
            }
            _ => false,
        }
    }

    fn expr_contains_method_call(expr: &Expr) -> bool {
        match &expr.kind {
            crate::parser::ast::ExprKind::MethodCall { object, args, .. } => {
                Self::expr_contains_method_call(object)
                    || args.iter().any(Self::expr_contains_method_call)
                    || true
            }
            crate::parser::ast::ExprKind::PropertyAccess { object, .. }
            | crate::parser::ast::ExprKind::Negate(object)
            | crate::parser::ast::ExprKind::Not(object)
            | crate::parser::ast::ExprKind::BitNot(object)
            | crate::parser::ast::ExprKind::Spread(object)
            | crate::parser::ast::ExprKind::Throw(object) => Self::expr_contains_method_call(object),
            crate::parser::ast::ExprKind::ArrayAccess { array, index } => {
                Self::expr_contains_method_call(array) || Self::expr_contains_method_call(index)
            }
            crate::parser::ast::ExprKind::BinaryOp { left, right, .. } => {
                Self::expr_contains_method_call(left) || Self::expr_contains_method_call(right)
            }
            crate::parser::ast::ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                Self::expr_contains_method_call(condition)
                    || Self::expr_contains_method_call(then_expr)
                    || Self::expr_contains_method_call(else_expr)
            }
            crate::parser::ast::ExprKind::ShortTernary { value, default } => {
                Self::expr_contains_method_call(value)
                    || Self::expr_contains_method_call(default)
            }
            crate::parser::ast::ExprKind::NullCoalesce { value, default } => {
                Self::expr_contains_method_call(value) || Self::expr_contains_method_call(default)
            }
            crate::parser::ast::ExprKind::FunctionCall { args, .. }
            | crate::parser::ast::ExprKind::ClosureCall { args, .. }
            | crate::parser::ast::ExprKind::ExprCall { args, .. }
            | crate::parser::ast::ExprKind::StaticMethodCall { args, .. }
            | crate::parser::ast::ExprKind::NewObject { args, .. } => {
                args.iter().any(Self::expr_contains_method_call)
            }
            crate::parser::ast::ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                Self::expr_contains_method_call(subject)
                    || arms.iter().any(|(conditions, result)| {
                        conditions.iter().any(Self::expr_contains_method_call)
                            || Self::expr_contains_method_call(result)
                    })
                    || default
                        .as_ref()
                        .map(|expr| Self::expr_contains_method_call(expr))
                        .unwrap_or(false)
            }
            crate::parser::ast::ExprKind::ArrayLiteral(items) => {
                items.iter().any(Self::expr_contains_method_call)
            }
            crate::parser::ast::ExprKind::ArrayLiteralAssoc(items) => items.iter().any(
                |(key, value)| {
                    Self::expr_contains_method_call(key) || Self::expr_contains_method_call(value)
                },
            ),
            crate::parser::ast::ExprKind::Cast { expr, .. }
            | crate::parser::ast::ExprKind::PtrCast { expr, .. }
            | crate::parser::ast::ExprKind::NamedArg { value: expr, .. } => {
                Self::expr_contains_method_call(expr)
            }
            crate::parser::ast::ExprKind::Closure { .. }
            | crate::parser::ast::ExprKind::FirstClassCallable(_)
            | crate::parser::ast::ExprKind::EnumCase { .. }
            | crate::parser::ast::ExprKind::StaticPropertyAccess { .. }
            | crate::parser::ast::ExprKind::BoolLiteral(_)
            | crate::parser::ast::ExprKind::Null
            | crate::parser::ast::ExprKind::StringLiteral(_)
            | crate::parser::ast::ExprKind::IntLiteral(_)
            | crate::parser::ast::ExprKind::FloatLiteral(_)
            | crate::parser::ast::ExprKind::Variable(_)
            | crate::parser::ast::ExprKind::PreIncrement(_)
            | crate::parser::ast::ExprKind::PostIncrement(_)
            | crate::parser::ast::ExprKind::PreDecrement(_)
            | crate::parser::ast::ExprKind::PostDecrement(_)
            | crate::parser::ast::ExprKind::ConstRef(_)
            | crate::parser::ast::ExprKind::This
            | crate::parser::ast::ExprKind::BufferNew { .. } => false,
        }
    }
}
