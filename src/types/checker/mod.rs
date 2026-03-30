mod builtins;
mod functions;

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::{
    BinOp, CType, CastType, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, Visibility,
};
use crate::types::{
    ctype_stack_size, ctype_to_php_type, traits::{flatten_classes, FlattenedClass}, CheckResult,
    ClassInfo, ExternClassInfo, ExternFieldInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PhpType, TypeEnv,
};

/// Infer a function's return type by scanning its body for Return statements.
/// This is a syntactic/heuristic check — no full type inference.
/// Used for functions that are never called directly (only used as callbacks).
pub fn infer_return_type_syntactic(body: &[Stmt]) -> PhpType {
    let mut types = Vec::new();
    for stmt in body {
        collect_return_types_syntactic(stmt, &mut types);
    }
    if types.is_empty() {
        return PhpType::Int;
    }
    // Pick the widest type across all return statements
    let mut result = types[0].clone();
    for ty in &types[1..] {
        result = wider_type_syntactic(&result, ty);
    }
    result
}

fn collect_return_types_syntactic(stmt: &Stmt, types: &mut Vec<PhpType>) {
    match &stmt.kind {
        StmtKind::Return(Some(expr)) => {
            types.push(infer_expr_type_syntactic(expr));
        }
        StmtKind::Return(None) => {
            types.push(PhpType::Void);
        }
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_return_types_syntactic(s, types);
            }
            for (_, body) in elseif_clauses {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
            if let Some(body) = else_body {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
        }
        StmtKind::While { body, .. }
        | StmtKind::DoWhile { body, .. }
        | StmtKind::For { body, .. }
        | StmtKind::Foreach { body, .. } => {
            for s in body {
                collect_return_types_syntactic(s, types);
            }
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            for s in try_body {
                collect_return_types_syntactic(s, types);
            }
            for catch_clause in catches {
                for s in &catch_clause.body {
                    collect_return_types_syntactic(s, types);
                }
            }
            if let Some(body) = finally_body {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
        }
        StmtKind::Switch { cases, default, .. } => {
            for (_, body) in cases {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
            if let Some(body) = default {
                for s in body {
                    collect_return_types_syntactic(s, types);
                }
            }
        }
        _ => {}
    }
}

fn wider_type_syntactic(a: &PhpType, b: &PhpType) -> PhpType {
    if a == b {
        return a.clone();
    }
    if *a == PhpType::Str || *b == PhpType::Str {
        return PhpType::Str;
    }
    if *a == PhpType::Float || *b == PhpType::Float {
        return PhpType::Float;
    }
    if *a == PhpType::Void {
        return b.clone();
    }
    if *b == PhpType::Void {
        return a.clone();
    }
    a.clone()
}

pub fn infer_expr_type_syntactic(expr: &Expr) -> PhpType {
    match &expr.kind {
        ExprKind::StringLiteral(_) => PhpType::Str,
        ExprKind::IntLiteral(_) => PhpType::Int,
        ExprKind::FloatLiteral(_) => PhpType::Float,
        ExprKind::BoolLiteral(_) => PhpType::Bool,
        ExprKind::Cast {
            target: CastType::String,
            ..
        } => PhpType::Str,
        ExprKind::Cast {
            target: CastType::Int,
            ..
        } => PhpType::Int,
        ExprKind::Cast {
            target: CastType::Float,
            ..
        } => PhpType::Float,
        ExprKind::Cast {
            target: CastType::Bool,
            ..
        } => PhpType::Bool,
        ExprKind::FunctionCall { name, args } => match name.as_str() {
            "substr" | "strtolower" | "strtoupper" | "trim" | "ltrim" | "rtrim" | "str_repeat"
            | "strrev" | "chr" | "str_replace" | "str_ireplace" | "ucfirst" | "lcfirst"
            | "ucwords" | "str_pad" | "implode" | "sprintf" | "nl2br" | "wordwrap" | "md5"
            | "sha1" | "hash" | "substr_replace" | "addslashes" | "stripslashes"
            | "htmlspecialchars" | "html_entity_decode" | "urlencode" | "urldecode"
            | "base64_encode" | "base64_decode" | "bin2hex" | "hex2bin" | "number_format"
            | "date" | "json_encode" | "gettype" | "str_word_count" | "chunk_split" => PhpType::Str,
            "strlen" | "strpos" | "strrpos" | "ord" | "count" | "intval" | "abs" | "intdiv"
            | "rand" | "time" => PhpType::Int,
            "floatval" | "floor" | "ceil" | "round" | "sqrt" | "pow" | "fmod" | "sin" | "cos"
            | "tan" | "asin" | "acos" | "atan" | "atan2" | "sinh" | "cosh" | "tanh" | "log"
            | "log2" | "log10" | "exp" | "hypot" | "pi" | "deg2rad" | "rad2deg" => PhpType::Float,
            "ptr" | "ptr_null" => PhpType::Pointer(None),
            "ptr_offset" => {
                if let Some(first_arg) = args.first() {
                    match infer_expr_type_syntactic(first_arg) {
                        PhpType::Pointer(tag) => PhpType::Pointer(tag),
                        _ => PhpType::Pointer(None),
                    }
                } else {
                    PhpType::Pointer(None)
                }
            }
            "ptr_is_null" => PhpType::Bool,
            "ptr_sizeof" | "ptr_get" | "ptr_read8" | "ptr_read32" => PhpType::Int,
            _ => PhpType::Int,
        },
        ExprKind::NullCoalesce { value, default } => {
            let left_ty = infer_expr_type_syntactic(value);
            let right_ty = infer_expr_type_syntactic(default);
            wider_type_syntactic(&left_ty, &right_ty)
        }
        ExprKind::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            let then_ty = infer_expr_type_syntactic(then_expr);
            let else_ty = infer_expr_type_syntactic(else_expr);
            if then_ty == else_ty {
                then_ty
            } else if then_ty == PhpType::Str || else_ty == PhpType::Str {
                PhpType::Str
            } else if then_ty == PhpType::Float || else_ty == PhpType::Float {
                PhpType::Float
            } else {
                then_ty
            }
        }
        ExprKind::NewObject { class_name, .. } => PhpType::Object(class_name.clone()),
        ExprKind::This => PhpType::Object(String::new()),
        ExprKind::PtrCast { target_type, .. } => PhpType::Pointer(Some(target_type.clone())),
        ExprKind::BinaryOp { left, op, right } => match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Mod => {
                let lt = infer_expr_type_syntactic(left);
                let rt = infer_expr_type_syntactic(right);
                if lt == PhpType::Float || rt == PhpType::Float {
                    PhpType::Float
                } else {
                    PhpType::Int
                }
            }
            BinOp::Div | BinOp::Pow => PhpType::Float,
            BinOp::Eq
            | BinOp::NotEq
            | BinOp::Lt
            | BinOp::Gt
            | BinOp::LtEq
            | BinOp::GtEq
            | BinOp::StrictEq
            | BinOp::StrictNotEq
            | BinOp::And
            | BinOp::Or => PhpType::Bool,
            BinOp::Concat => PhpType::Str,
            _ => PhpType::Int,
        },
        _ => PhpType::Int,
    }
}

pub(crate) struct Checker {
    pub fn_decls: HashMap<String, FnDecl>,
    pub functions: HashMap<String, FunctionSig>,
    pub constants: HashMap<String, PhpType>,
    /// Tracks the return type of closures assigned to variables.
    pub closure_return_types: HashMap<String, PhpType>,
    /// Interface definitions collected during first pass.
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// Class definitions collected during first pass.
    pub classes: HashMap<String, ClassInfo>,
    /// Name of the class currently being type-checked (for $this).
    pub current_class: Option<String>,
    /// Name of the current method, when type-checking a class method body.
    pub current_method: Option<String>,
    /// Whether the current class method is static.
    pub current_method_is_static: bool,
    /// Extern function declarations.
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    /// Extern class (C struct) declarations.
    pub extern_classes: HashMap<String, ExternClassInfo>,
    /// Extern global variable declarations.
    pub extern_globals: HashMap<String, PhpType>,
    /// Libraries required by extern blocks.
    pub required_libraries: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct FnDecl {
    pub params: Vec<String>,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub variadic: Option<String>,
    pub body: Vec<Stmt>,
}

#[derive(Clone)]
struct InterfaceDeclInfo {
    name: String,
    extends: Vec<String>,
    methods: Vec<crate::parser::ast::ClassMethod>,
    span: crate::span::Span,
}

fn inject_builtin_throwables(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
) -> Result<(), CompileError> {
    for builtin_name in ["Throwable", "Exception"] {
        if interface_map.contains_key(builtin_name) || class_map.contains_key(builtin_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Cannot redeclare built-in exception type: {}", builtin_name),
            ));
        }
    }

    interface_map.insert(
        "Throwable".to_string(),
        InterfaceDeclInfo {
            name: "Throwable".to_string(),
            extends: Vec::new(),
            methods: Vec::new(),
            span: crate::span::Span::dummy(),
        },
    );
    class_map.insert(
        "Exception".to_string(),
        FlattenedClass {
            name: "Exception".to_string(),
            extends: None,
            implements: vec!["Throwable".to_string()],
            is_abstract: false,
            properties: Vec::new(),
            methods: Vec::new(),
        },
    );

    Ok(())
}

fn build_method_sig(method: &crate::parser::ast::ClassMethod) -> FunctionSig {
    let params: Vec<(String, PhpType)> = method
        .params
        .iter()
        .map(|(n, _, _)| (n.clone(), PhpType::Int))
        .collect();
    let defaults: Vec<Option<Expr>> = method.params.iter().map(|(_, d, _)| d.clone()).collect();
    let ref_params: Vec<bool> = method.params.iter().map(|(_, _, r)| *r).collect();
    let return_type = infer_return_type_syntactic(&method.body);
    FunctionSig {
        params,
        defaults,
        return_type,
        ref_params,
        variadic: method.variadic.clone(),
    }
}

fn build_constructor_param_map(methods: &[crate::parser::ast::ClassMethod]) -> Vec<Option<String>> {
    let mut param_to_prop = Vec::new();
    if let Some(constructor) = methods.iter().find(|m| m.name == "__construct") {
        param_to_prop = constructor
            .params
            .iter()
            .map(|(pname, _, _)| {
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
            })
            .collect();
    }
    param_to_prop
}

fn visibility_rank(visibility: &Visibility) -> u8 {
    match visibility {
        Visibility::Private => 0,
        Visibility::Protected => 1,
        Visibility::Public => 2,
    }
}

fn required_param_count(sig: &FunctionSig) -> usize {
    sig.defaults.iter().filter(|default| default.is_none()).count()
}

fn validate_signature_compatibility(
    span: crate::span::Span,
    owner_name: &str,
    method_name: &str,
    child_sig: &FunctionSig,
    parent_sig: &FunctionSig,
    kind: &str,
    context: &str,
) -> Result<(), CompileError> {
    if child_sig.params.len() != parent_sig.params.len() {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change parameter count when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child_sig.ref_params != parent_sig.ref_params {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change pass-by-reference parameters when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    let child_defaults: Vec<bool> =
        child_sig.defaults.iter().map(|default| default.is_some()).collect();
    let parent_defaults: Vec<bool> =
        parent_sig.defaults.iter().map(|default| default.is_some()).collect();
    if child_defaults != parent_defaults {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change optional parameter layout when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child_sig.variadic != parent_sig.variadic {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change variadic parameter shape when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if required_param_count(child_sig) != required_param_count(parent_sig) {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change required parameter count when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    Ok(())
}

fn validate_override_signature(
    class_name: &str,
    method: &crate::parser::ast::ClassMethod,
    parent_sig: &FunctionSig,
    is_static: bool,
) -> Result<(), CompileError> {
    let kind = if is_static { "static method" } else { "method" };
    let child_sig = build_method_sig(method);
    validate_signature_compatibility(
        method.span,
        class_name,
        &method.name,
        &child_sig,
        parent_sig,
        kind,
        "overriding",
    )
}

fn build_interface_info_recursive(
    interface_name: &str,
    interface_map: &HashMap<String, InterfaceDeclInfo>,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_interface_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    if checker.interfaces.contains_key(interface_name) {
        return Ok(());
    }

    if !building.insert(interface_name.to_string()) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!("Circular interface inheritance detected involving {}", interface_name),
        ));
    }

    let interface = interface_map.get(interface_name).cloned().ok_or_else(|| {
        CompileError::new(
            crate::span::Span::dummy(),
            &format!("Unknown interface referenced during interface flattening: {}", interface_name),
        )
    })?;

    let mut methods = HashMap::new();
    let mut method_declaring_interfaces = HashMap::new();
    let mut method_order = Vec::new();
    let mut method_slots = HashMap::new();

    for parent_name in &interface.extends {
        if class_map.contains_key(parent_name) {
            return Err(CompileError::new(
                interface.span,
                &format!(
                    "Interface {} cannot extend class {}; only interfaces are allowed",
                    interface.name, parent_name
                ),
            ));
        }
        build_interface_info_recursive(
            parent_name,
            interface_map,
            class_map,
            checker,
            next_interface_id,
            building,
        )?;
        let parent_info = checker.interfaces.get(parent_name).cloned().ok_or_else(|| {
            CompileError::new(
                interface.span,
                &format!("Unknown parent interface: {}", parent_name),
            )
        })?;
        for method_name in &parent_info.method_order {
            let parent_sig = parent_info
                .methods
                .get(method_name)
                .expect("type checker bug: missing interface parent method signature");
            if let Some(existing_sig) = methods.get(method_name) {
                validate_signature_compatibility(
                    interface.span,
                    &interface.name,
                    method_name,
                    existing_sig,
                    parent_sig,
                    "method",
                    "combining interface parent",
                )?;
                continue;
            }
            methods.insert(method_name.clone(), parent_sig.clone());
            let declaring = parent_info
                .method_declaring_interfaces
                .get(method_name)
                .cloned()
                .unwrap_or_else(|| parent_name.clone());
            method_declaring_interfaces.insert(method_name.clone(), declaring);
            let slot = method_order.len();
            method_slots.insert(method_name.clone(), slot);
            method_order.push(method_name.clone());
        }
    }

    for method in &interface.methods {
        if method.visibility != Visibility::Public {
            return Err(CompileError::new(
                method.span,
                &format!("Interface methods must be public: {}::{}", interface.name, method.name),
            ));
        }
        if method.is_static {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Static interface methods are not supported yet: {}::{}",
                    interface.name, method.name
                ),
            ));
        }
        if method.has_body {
            return Err(CompileError::new(
                method.span,
                &format!(
                    "Interface methods cannot have a body: {}::{}",
                    interface.name, method.name
                ),
            ));
        }

        let sig = build_method_sig(method);
        if let Some(parent_sig) = methods.get(&method.name) {
            validate_signature_compatibility(
                method.span,
                &interface.name,
                &method.name,
                &sig,
                parent_sig,
                "method",
                "redeclaring interface",
            )?;
        }
        methods.insert(method.name.clone(), sig);
        method_declaring_interfaces.insert(method.name.clone(), interface.name.clone());
        if !method_slots.contains_key(&method.name) {
            let slot = method_order.len();
            method_slots.insert(method.name.clone(), slot);
            method_order.push(method.name.clone());
        }
    }

    checker.interfaces.insert(
        interface.name.clone(),
        InterfaceInfo {
            interface_id: *next_interface_id,
            parents: interface.extends.clone(),
            methods,
            method_declaring_interfaces,
            method_order,
            method_slots,
        },
    );
    *next_interface_id += 1;
    building.remove(interface_name);
    Ok(())
}

fn build_class_info_recursive(
    class_name: &str,
    class_map: &HashMap<String, FlattenedClass>,
    checker: &mut Checker,
    next_class_id: &mut u64,
    building: &mut HashSet<String>,
) -> Result<(), CompileError> {
    if checker.classes.contains_key(class_name) {
        return Ok(());
    }

    if !building.insert(class_name.to_string()) {
        return Err(CompileError::new(
            crate::span::Span::dummy(),
            &format!("Circular inheritance detected involving class {}", class_name),
        ));
    }

    let class = class_map.get(class_name).cloned().ok_or_else(|| {
        CompileError::new(
            crate::span::Span::dummy(),
            &format!("Unknown class referenced during inheritance flattening: {}", class_name),
        )
    })?;

    let parent_info = if let Some(parent_name) = &class.extends {
        if checker.interfaces.contains_key(parent_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Class {} cannot extend interface {}; use implements instead",
                    class_name, parent_name
                ),
            ));
        }
        build_class_info_recursive(parent_name, class_map, checker, next_class_id, building)?;
        Some(checker.classes.get(parent_name).cloned().ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown parent class: {}", parent_name),
            )
        })?)
    } else {
        None
    };

    let mut prop_types = Vec::new();
    let mut property_offsets = HashMap::new();
    let mut property_declaring_classes = HashMap::new();
    let mut defaults = Vec::new();
    let mut property_visibilities = HashMap::new();
    let mut readonly_properties = std::collections::HashSet::new();

    let mut method_sigs = HashMap::new();
    let mut static_sigs = HashMap::new();
    let mut method_visibilities = HashMap::new();
    let mut method_declaring_classes = HashMap::new();
    let mut method_impl_classes = HashMap::new();
    let mut vtable_methods = Vec::new();
    let mut vtable_slots = HashMap::new();
    let mut static_method_visibilities = HashMap::new();
    let mut static_method_declaring_classes = HashMap::new();
    let mut static_method_impl_classes = HashMap::new();
    let mut static_vtable_methods = Vec::new();
    let mut static_vtable_slots = HashMap::new();
    let mut interfaces = Vec::new();

    if let Some(parent) = &parent_info {
        for (index, (name, ty)) in parent.properties.iter().enumerate() {
            prop_types.push((name.clone(), ty.clone()));
            property_offsets.insert(name.clone(), 8 + index * 16);
            defaults.push(parent.defaults[index].clone());
            if let Some(visibility) = parent.property_visibilities.get(name) {
                property_visibilities.insert(name.clone(), visibility.clone());
            }
            if let Some(declaring_class) = parent.property_declaring_classes.get(name) {
                property_declaring_classes.insert(name.clone(), declaring_class.clone());
            }
            if parent.readonly_properties.contains(name) {
                readonly_properties.insert(name.clone());
            }
        }

        for (name, sig) in &parent.methods {
            if parent.method_visibilities.get(name) == Some(&Visibility::Private) {
                continue;
            }
            method_sigs.insert(name.clone(), sig.clone());
            if let Some(visibility) = parent.method_visibilities.get(name) {
                method_visibilities.insert(name.clone(), visibility.clone());
            }
            if let Some(declaring_class) = parent.method_declaring_classes.get(name) {
                method_declaring_classes.insert(name.clone(), declaring_class.clone());
            }
            if let Some(impl_class) = parent.method_impl_classes.get(name) {
                method_impl_classes.insert(name.clone(), impl_class.clone());
            }
        }
        vtable_methods = parent.vtable_methods.clone();
        vtable_slots = parent.vtable_slots.clone();

        for (name, sig) in &parent.static_methods {
            if parent.static_method_visibilities.get(name) == Some(&Visibility::Private) {
                continue;
            }
            static_sigs.insert(name.clone(), sig.clone());
            if let Some(visibility) = parent.static_method_visibilities.get(name) {
                static_method_visibilities.insert(name.clone(), visibility.clone());
            }
            if let Some(declaring_class) = parent.static_method_declaring_classes.get(name) {
                static_method_declaring_classes.insert(name.clone(), declaring_class.clone());
            }
            if let Some(impl_class) = parent.static_method_impl_classes.get(name) {
                static_method_impl_classes.insert(name.clone(), impl_class.clone());
            }
        }
        static_vtable_methods = parent.static_vtable_methods.clone();
        static_vtable_slots = parent.static_vtable_slots.clone();
        interfaces = parent.interfaces.clone();
    }

    for prop in &class.properties {
        if property_declaring_classes.contains_key(&prop.name) {
            return Err(CompileError::new(
                prop.span,
                &format!(
                    "Property redeclaration across inheritance is not yet supported: {}::{}",
                    class.name, prop.name
                ),
            ));
        }

        let ty = if let Some(default) = &prop.default {
            infer_expr_type_syntactic(default)
        } else {
            PhpType::Int
        };
        let slot_index = prop_types.len();
        prop_types.push((prop.name.clone(), ty));
        property_offsets.insert(prop.name.clone(), 8 + slot_index * 16);
        property_declaring_classes.insert(prop.name.clone(), class.name.clone());
        defaults.push(prop.default.clone());
        property_visibilities.insert(prop.name.clone(), prop.visibility.clone());
        if prop.readonly {
            readonly_properties.insert(prop.name.clone());
        }
    }

    for method in &class.methods {
        let sig = build_method_sig(method);
        if method.is_abstract && method.has_body {
            return Err(CompileError::new(
                method.span,
                &format!("Abstract method cannot have a body: {}::{}", class.name, method.name),
            ));
        }
        if !method.is_abstract && !method.has_body {
            return Err(CompileError::new(
                method.span,
                &format!("Non-abstract method must have a body: {}::{}", class.name, method.name),
            ));
        }
        if method.is_abstract && method.visibility == Visibility::Private {
            return Err(CompileError::new(
                method.span,
                &format!("Private abstract methods are not supported: {}::{}", class.name, method.name),
            ));
        }
        if method.is_static {
            if method_sigs.contains_key(&method.name) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot change method kind when overriding {}::{}",
                        class.name, method.name
                    ),
                ));
            }
            if let Some(parent_visibility) = static_method_visibilities.get(&method.name) {
                if visibility_rank(&method.visibility) < visibility_rank(parent_visibility) {
                    return Err(CompileError::new(
                        method.span,
                        &format!(
                            "Cannot reduce visibility when overriding static method: {}::{}",
                            class.name, method.name
                        ),
                    ));
                }
            }
            if let Some(parent_sig) = static_sigs.get(&method.name) {
                validate_override_signature(&class.name, method, parent_sig, true)?;
            }
            if method.is_abstract && static_method_impl_classes.contains_key(&method.name) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot make concrete static method abstract: {}::{}",
                        class.name, method.name
                    ),
                ));
            }
            static_sigs.insert(method.name.clone(), sig);
            static_method_visibilities.insert(method.name.clone(), method.visibility.clone());
            static_method_declaring_classes.insert(method.name.clone(), class.name.clone());
            if method.is_abstract {
                static_method_impl_classes.remove(&method.name);
            } else {
                static_method_impl_classes.insert(method.name.clone(), class.name.clone());
            }
            if method.visibility != Visibility::Private
                && !static_vtable_slots.contains_key(&method.name)
            {
                let slot = static_vtable_methods.len();
                static_vtable_slots.insert(method.name.clone(), slot);
                static_vtable_methods.push(method.name.clone());
            }
        } else {
            if static_sigs.contains_key(&method.name) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot change method kind when overriding {}::{}",
                        class.name, method.name
                    ),
                ));
            }
            if let Some(parent_visibility) = method_visibilities.get(&method.name) {
                if visibility_rank(&method.visibility) < visibility_rank(parent_visibility) {
                    return Err(CompileError::new(
                        method.span,
                        &format!(
                            "Cannot reduce visibility when overriding method: {}::{}",
                            class.name, method.name
                        ),
                    ));
                }
            }
            if let Some(parent_sig) = method_sigs.get(&method.name) {
                validate_override_signature(&class.name, method, parent_sig, false)?;
            }
            if method.is_abstract && method_impl_classes.contains_key(&method.name) {
                return Err(CompileError::new(
                    method.span,
                    &format!(
                        "Cannot make concrete method abstract: {}::{}",
                        class.name, method.name
                    ),
                ));
            }
            method_sigs.insert(method.name.clone(), sig);
            method_visibilities.insert(method.name.clone(), method.visibility.clone());
            method_declaring_classes.insert(method.name.clone(), class.name.clone());
            if method.is_abstract {
                method_impl_classes.remove(&method.name);
            } else {
                method_impl_classes.insert(method.name.clone(), class.name.clone());
            }
            if method.visibility != Visibility::Private && !vtable_slots.contains_key(&method.name) {
                let slot = vtable_methods.len();
                vtable_slots.insert(method.name.clone(), slot);
                vtable_methods.push(method.name.clone());
            }
        }
    }

    let mut seen_interfaces: HashSet<String> = interfaces.iter().cloned().collect();
    let mut queue = Vec::new();
    for interface_name in class.implements.iter().rev() {
        if class_map.contains_key(interface_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Class {} cannot implement non-interface {}",
                    class.name, interface_name
                ),
            ));
        }
        if !checker.interfaces.contains_key(interface_name) {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            ));
        }
        queue.push(interface_name.clone());
    }
    while let Some(interface_name) = queue.pop() {
        if !seen_interfaces.insert(interface_name.clone()) {
            continue;
        }
        let interface_info = checker.interfaces.get(&interface_name).ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            )
        })?;
        for parent_name in interface_info.parents.iter().rev() {
            queue.push(parent_name.clone());
        }
        interfaces.push(interface_name);
    }

    for interface_name in &interfaces {
        let interface_info = checker.interfaces.get(interface_name).ok_or_else(|| {
            CompileError::new(
                crate::span::Span::dummy(),
                &format!("Unknown interface: {}", interface_name),
            )
        })?;
        for method_name in &interface_info.method_order {
            if static_sigs.contains_key(method_name) {
                return Err(CompileError::new(
                    crate::span::Span::dummy(),
                    &format!(
                        "Cannot use static method to satisfy interface contract: {}::{}",
                        class.name, method_name
                    ),
                ));
            }
            let required_sig = interface_info
                .methods
                .get(method_name)
                .expect("type checker bug: missing interface method signature");
            let actual_sig = match method_sigs.get(method_name) {
                Some(sig) => sig,
                None if class.is_abstract => {
                    method_sigs.insert(method_name.clone(), required_sig.clone());
                    method_visibilities.insert(method_name.clone(), Visibility::Public);
                    method_declaring_classes.insert(method_name.clone(), class.name.clone());
                    method_impl_classes.remove(method_name);
                    if !vtable_slots.contains_key(method_name) {
                        let slot = vtable_methods.len();
                        vtable_slots.insert(method_name.clone(), slot);
                        vtable_methods.push(method_name.clone());
                    }
                    continue;
                }
                None => {
                    return Err(CompileError::new(
                        crate::span::Span::dummy(),
                        &format!(
                            "Class {} must implement interface method {}::{}",
                            class.name, interface_name, method_name
                        ),
                    ))
                }
            };
            validate_signature_compatibility(
                crate::span::Span::dummy(),
                &class.name,
                method_name,
                actual_sig,
                required_sig,
                "method",
                "implementing interface",
            )?;
            if method_visibilities.get(method_name) != Some(&Visibility::Public) {
                return Err(CompileError::new(
                    crate::span::Span::dummy(),
                    &format!(
                        "Interface method implementation must be public: {}::{}",
                        class.name, method_name
                    ),
                ));
            }
        }
    }

    if !class.is_abstract {
        if let Some(method_name) = method_sigs
            .keys()
            .find(|name| !method_impl_classes.contains_key(*name))
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Concrete class {} must implement abstract method {}::{}",
                    class.name, class.name, method_name
                ),
            ));
        }
        if let Some(method_name) = static_sigs
            .keys()
            .find(|name| !static_method_impl_classes.contains_key(*name))
        {
            return Err(CompileError::new(
                crate::span::Span::dummy(),
                &format!(
                    "Concrete class {} must implement abstract static method {}::{}",
                    class.name, class.name, method_name
                ),
            ));
        }
    }

    let constructor_param_to_prop = if class.methods.iter().any(|m| m.name == "__construct") {
        build_constructor_param_map(&class.methods)
    } else if let Some(parent) = &parent_info {
        parent.constructor_param_to_prop.clone()
    } else {
        Vec::new()
    };

    checker.classes.insert(
        class.name.clone(),
        ClassInfo {
            class_id: *next_class_id,
            parent: class.extends.clone(),
            is_abstract: class.is_abstract,
            properties: prop_types,
            property_offsets,
            property_declaring_classes,
            defaults,
            property_visibilities,
            readonly_properties,
            method_decls: class.methods.clone(),
            methods: method_sigs,
            static_methods: static_sigs,
            method_visibilities,
            method_declaring_classes,
            method_impl_classes,
            vtable_methods,
            vtable_slots,
            static_method_visibilities,
            static_method_declaring_classes,
            static_method_impl_classes,
            static_vtable_methods,
            static_vtable_slots,
            interfaces,
            constructor_param_to_prop,
        },
    );
    *next_class_id += 1;
    building.remove(class_name);
    Ok(())
}

fn propagate_abstract_return_types(checker: &mut Checker) {
    let mut sorted_classes: Vec<(String, u64)> = checker
        .classes
        .iter()
        .map(|(name, info)| (name.clone(), info.class_id))
        .collect();
    sorted_classes.sort_by_key(|(_, class_id)| std::cmp::Reverse(*class_id));

    for (class_name, _) in sorted_classes {
        let Some(class_info) = checker.classes.get(&class_name).cloned() else {
            continue;
        };

        for (method_name, sig) in &class_info.methods {
            let mut parent_name = class_info.parent.clone();
            while let Some(name) = parent_name {
                let Some(parent_info) = checker.classes.get(&name).cloned() else {
                    break;
                };
                if !parent_info.methods.contains_key(method_name) {
                    break;
                }
                if parent_info.method_impl_classes.contains_key(method_name) {
                    break;
                }
                if let Some(parent_mut) = checker.classes.get_mut(&name) {
                    if let Some(parent_sig) = parent_mut.methods.get_mut(method_name) {
                        parent_sig.return_type = sig.return_type.clone();
                    }
                }
                parent_name = parent_info.parent.clone();
            }
        }

        for (method_name, sig) in &class_info.static_methods {
            let mut parent_name = class_info.parent.clone();
            while let Some(name) = parent_name {
                let Some(parent_info) = checker.classes.get(&name).cloned() else {
                    break;
                };
                if !parent_info.static_methods.contains_key(method_name) {
                    break;
                }
                if parent_info.static_method_impl_classes.contains_key(method_name) {
                    break;
                }
                if let Some(parent_mut) = checker.classes.get_mut(&name) {
                    if let Some(parent_sig) = parent_mut.static_methods.get_mut(method_name) {
                        parent_sig.return_type = sig.return_type.clone();
                    }
                }
                parent_name = parent_info.parent.clone();
            }
        }
    }
}

pub fn check_types(program: &Program) -> Result<CheckResult, CompileError> {
    let mut checker = Checker {
        fn_decls: HashMap::new(),
        functions: HashMap::new(),
        constants: HashMap::new(),
        closure_return_types: HashMap::new(),
        interfaces: HashMap::new(),
        classes: HashMap::new(),
        current_class: None,
        current_method: None,
        current_method_is_static: false,
        extern_functions: HashMap::new(),
        extern_classes: HashMap::new(),
        extern_globals: HashMap::new(),
        required_libraries: Vec::new(),
    };

    for stmt in program {
        if let StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            body,
        } = &stmt.kind
        {
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

    let flattened_classes = flatten_classes(program)?;
    let class_map: HashMap<String, FlattenedClass> = flattened_classes
        .iter()
        .cloned()
        .map(|class| (class.name.clone(), class))
        .collect();
    let mut class_map = class_map;
    let mut interface_map = HashMap::new();
    for stmt in program {
        if let StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } = &stmt.kind
        {
            if interface_map.contains_key(name) {
                return Err(CompileError::new(
                    stmt.span,
                    &format!("Duplicate interface declaration: {}", name),
                ));
            }
            interface_map.insert(
                name.clone(),
                InterfaceDeclInfo {
                    name: name.clone(),
                    extends: extends.clone(),
                    methods: methods.clone(),
                    span: stmt.span,
                },
            );
        }
    }
    inject_builtin_throwables(&mut interface_map, &mut class_map)?;

    let mut next_interface_id = 0u64;
    let mut building_interfaces = HashSet::new();
    let interface_names: Vec<String> = interface_map.keys().cloned().collect();
    for interface_name in interface_names {
        build_interface_info_recursive(
            &interface_name,
            &interface_map,
            &class_map,
            &mut checker,
            &mut next_interface_id,
            &mut building_interfaces,
        )?;
    }

    // First pass: collect flattened class declarations and build ClassInfo
    let mut next_class_id = 0u64;
    let mut building = HashSet::new();
    let class_names: Vec<String> = class_map.keys().cloned().collect();
    for class_name in class_names {
        build_class_info_recursive(
            &class_name,
            &class_map,
            &mut checker,
            &mut next_class_id,
            &mut building,
        )?;
    }

    // Pre-scan: collect extern declarations
    for stmt in program {
        match &stmt.kind {
            StmtKind::ExternFunctionDecl {
                name,
                params,
                return_type,
                library,
            } => {
                if checker.extern_functions.contains_key(name)
                    || checker.fn_decls.contains_key(name)
                {
                    return Err(CompileError::new(
                        stmt.span,
                        &format!("Duplicate function declaration: {}", name),
                    ));
                }
                let php_params: Vec<(String, PhpType)> = params
                    .iter()
                    .map(|p| (p.name.clone(), ctype_to_php_type(&p.c_type)))
                    .collect();
                let php_ret = ctype_to_php_type(return_type);
                checker.validate_extern_function_decl(
                    name,
                    params,
                    return_type,
                    &php_params,
                    &php_ret,
                    stmt.span,
                )?;
                // Register as a regular function sig so call-site type checking works
                let sig = FunctionSig {
                    params: php_params.clone(),
                    defaults: params.iter().map(|_| None).collect(),
                    return_type: php_ret.clone(),
                    ref_params: params.iter().map(|_| false).collect(),
                    variadic: None,
                };
                checker.functions.insert(name.clone(), sig);
                checker.extern_functions.insert(
                    name.clone(),
                    ExternFunctionSig {
                        name: name.clone(),
                        params: php_params,
                        return_type: php_ret,
                        library: library.clone(),
                    },
                );
                if let Some(lib) = library {
                    if !checker.required_libraries.contains(lib) {
                        checker.required_libraries.push(lib.clone());
                    }
                }
            }
            StmtKind::ExternClassDecl { name, fields } => {
                if checker.extern_classes.contains_key(name) || checker.classes.contains_key(name) {
                    return Err(CompileError::new(
                        stmt.span,
                        &format!("Duplicate class declaration: {}", name),
                    ));
                }
                let mut extern_fields = Vec::new();
                let mut offset = 0usize;
                let mut seen_fields = std::collections::HashSet::new();
                for f in fields {
                    checker.validate_extern_field_decl(name, f, stmt.span)?;
                    if !seen_fields.insert(f.name.clone()) {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Duplicate extern field: {}::{}", name, f.name),
                        ));
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
                checker.extern_classes.insert(
                    name.clone(),
                    ExternClassInfo {
                        name: name.clone(),
                        total_size: offset,
                        fields: extern_fields,
                    },
                );
            }
            StmtKind::ExternGlobalDecl { name, c_type } => {
                checker.validate_extern_global_decl(name, c_type, stmt.span)?;
                let php_type = ctype_to_php_type(c_type);
                checker.extern_globals.insert(name.clone(), php_type);
            }
            _ => {}
        }
    }

    let mut global_env: TypeEnv = HashMap::new();
    global_env.insert("argc".to_string(), PhpType::Int);
    global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
    // Add extern globals to the global environment
    for (name, ty) in &checker.extern_globals {
        global_env.insert(name.clone(), ty.clone());
    }
    for stmt in program {
        checker.check_stmt(stmt, &mut global_env)?;
    }

    // Register provisional signatures for functions that were declared but never
    // called directly (e.g., used only as string callbacks in array_map).
    // This ensures their return types are available for callback type inference.
    let unchecked: Vec<String> = checker
        .fn_decls
        .keys()
        .filter(|name| !checker.functions.contains_key(*name))
        .cloned()
        .collect();
    for name in unchecked {
        if let Some(decl) = checker.fn_decls.get(&name) {
            let return_type = infer_return_type_syntactic(&decl.body);
            let params = decl
                .params
                .iter()
                .map(|p| (p.clone(), PhpType::Int))
                .collect();
            checker.functions.insert(
                name.clone(),
                FunctionSig {
                    params,
                    defaults: decl.defaults.clone(),
                    return_type,
                    ref_params: decl.ref_params.clone(),
                    variadic: decl.variadic.clone(),
                },
            );
        }
    }

    // Post-pass: type-check class method bodies NOW that property types
    // have been updated from new ClassName(args) calls in the main scope.
    // This ensures methods see correct property types (e.g., Str not Int).
    for class in &flattened_classes {
        for method in &class.methods {
                if method.is_abstract {
                    continue;
                }
                let mut method_env: TypeEnv = global_env.clone();
                if !method.is_static {
                    method_env.insert("this".to_string(), PhpType::Object(class.name.clone()));
                }
                // Use param types from ClassInfo sig (updated by MethodCall inference)
                let method_sig_key = if method.is_static {
                    "static"
                } else {
                    "instance"
                };
                let _ = method_sig_key;
                let sig_params = if method.is_static {
                    checker
                        .classes
                        .get(&class.name)
                        .and_then(|c| c.static_methods.get(&method.name))
                        .map(|s| s.params.clone())
                } else {
                    checker
                        .classes
                        .get(&class.name)
                        .and_then(|c| c.methods.get(&method.name))
                        .map(|s| s.params.clone())
                };
                for (i, (pname, _, _)) in method.params.iter().enumerate() {
                    let ty = sig_params
                        .as_ref()
                        .and_then(|p| p.get(i))
                        .map(|(_, t)| t.clone())
                        .unwrap_or(PhpType::Int);
                    method_env.insert(pname.clone(), ty);
                }
                // For __construct: infer param types from property types
                // This updates both the env (for body type-checking) and the sig
                // (for correct register assignment in codegen prologue)
                if method.name == "__construct" {
                    if let Some(ci) = checker.classes.get(&class.name).cloned() {
                        for (i, (pname, _, _)) in method.params.iter().enumerate() {
                            if let Some(Some(prop_name)) = ci.constructor_param_to_prop.get(i) {
                                if let Some((_, ty)) =
                                    ci.properties.iter().find(|(n, _)| n == prop_name)
                                {
                                    method_env.insert(pname.clone(), ty.clone());
                                    // Also update the sig in ClassInfo
                                    // (sig.params has user params only, $this added by codegen)
                                    if let Some(ci_mut) = checker.classes.get_mut(&class.name) {
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
                checker.current_class = Some(class.name.clone());
                checker.current_method = Some(method.name.clone());
                checker.current_method_is_static = method.is_static;
                for s in &method.body {
                    checker.check_stmt(s, &mut method_env)?;
                }

                // Update method return type from full type inference
                // (must run while current_class is still set so $this resolves)
                if !method.is_static {
                    for s in &method.body {
                        if let Some(ty) = checker.find_return_type(s, &method_env) {
                            if let Some(ci) = checker.classes.get_mut(&class.name) {
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
                            if let Some(ci) = checker.classes.get_mut(&class.name) {
                                if let Some(sig) = ci.static_methods.get_mut(&method.name) {
                                    sig.return_type = ty;
                                }
                            }
                            break;
                        }
                    }
                }
                checker.current_class = None;
                checker.current_method = None;
                checker.current_method_is_static = false;
        }
    }

    propagate_abstract_return_types(&mut checker);

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
        interfaces: checker.interfaces,
        classes: checker.classes,
        extern_functions: checker.extern_functions,
        extern_classes: checker.extern_classes,
        extern_globals: checker.extern_globals,
        required_libraries: checker.required_libraries,
    })
}

impl Checker {
    fn can_access_member(&self, declaring_class: &str, visibility: &Visibility) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Protected => self
                .current_class
                .as_deref()
                .is_some_and(|current| current == declaring_class || self.is_subclass_of(current, declaring_class)),
            Visibility::Private => self.current_class.as_deref() == Some(declaring_class),
        }
    }

    fn visibility_label(visibility: &Visibility) -> &'static str {
        match visibility {
            Visibility::Public => "public",
            Visibility::Protected => "protected",
            Visibility::Private => "private",
        }
    }

    fn is_subclass_of(&self, class_name: &str, ancestor_name: &str) -> bool {
        let mut current = self.classes.get(class_name).and_then(|class| class.parent.clone());
        while let Some(parent_name) = current {
            if parent_name == ancestor_name {
                return true;
            }
            current = self
                .classes
                .get(&parent_name)
                .and_then(|class| class.parent.clone());
        }
        false
    }

    fn class_implements_interface(&self, class_name: &str, interface_name: &str) -> bool {
        self.classes
            .get(class_name)
            .is_some_and(|class_info| class_info.interfaces.iter().any(|name| name == interface_name))
    }

    fn interface_extends_interface(&self, interface_name: &str, ancestor_name: &str) -> bool {
        if interface_name == ancestor_name {
            return true;
        }
        let mut stack = vec![interface_name.to_string()];
        let mut seen = HashSet::new();
        while let Some(current_name) = stack.pop() {
            if !seen.insert(current_name.clone()) {
                continue;
            }
            let Some(interface_info) = self.interfaces.get(&current_name) else {
                continue;
            };
            for parent_name in &interface_info.parents {
                if parent_name == ancestor_name {
                    return true;
                }
                stack.push(parent_name.clone());
            }
        }
        false
    }

    fn object_type_implements_throwable(&self, type_name: &str) -> bool {
        if self.classes.contains_key(type_name) {
            return self.class_implements_interface(type_name, "Throwable");
        }
        if self.interfaces.contains_key(type_name) {
            return self.interface_extends_interface(type_name, "Throwable");
        }
        false
    }

    fn resolve_catch_type_name(
        &self,
        raw_name: &str,
        span: crate::span::Span,
    ) -> Result<String, CompileError> {
        match raw_name {
            "self" => self.current_class.clone().ok_or_else(|| {
                CompileError::new(span, "Cannot use self in catch outside of a class context")
            }),
            "parent" => {
                let current_class = self.current_class.as_ref().ok_or_else(|| {
                    CompileError::new(span, "Cannot use parent in catch outside of a class context")
                })?;
                self.classes
                    .get(current_class)
                    .and_then(|class_info| class_info.parent.clone())
                    .ok_or_else(|| CompileError::new(span, "Class has no parent class"))
            }
            _ => Ok(raw_name.to_string()),
        }
    }

    fn is_pointer_type(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Pointer(_))
    }

    fn pointer_types_compatible(left: &PhpType, right: &PhpType) -> bool {
        matches!((left, right), (PhpType::Pointer(_), PhpType::Pointer(_)))
    }

    fn merged_assignment_type(&self, existing: &PhpType, new_ty: &PhpType) -> Option<PhpType> {
        if existing == new_ty {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Mixed) || matches!(new_ty, PhpType::Mixed) {
            return Some(PhpType::Mixed);
        }
        if *new_ty == PhpType::Void {
            return Some(existing.clone());
        }
        if *existing == PhpType::Void {
            return Some(new_ty.clone());
        }
        if matches!(existing, PhpType::Int | PhpType::Bool | PhpType::Float)
            && matches!(new_ty, PhpType::Int | PhpType::Bool | PhpType::Float)
        {
            return Some(existing.clone());
        }
        if Self::pointer_types_compatible(existing, new_ty) {
            return Some(match (existing, new_ty) {
                (PhpType::Pointer(Some(left)), PhpType::Pointer(Some(right))) if left == right => {
                    PhpType::Pointer(Some(left.clone()))
                }
                (PhpType::Pointer(None), PhpType::Pointer(Some(tag)))
                | (PhpType::Pointer(Some(tag)), PhpType::Pointer(None)) => {
                    PhpType::Pointer(Some(tag.clone()))
                }
                _ => PhpType::Pointer(None),
            });
        }
        None
    }

    fn common_object_type(&self, left: &str, right: &str) -> Option<PhpType> {
        if left == right {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.interfaces.contains_key(left) && self.class_implements_interface(right, left) {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.interfaces.contains_key(right) && self.class_implements_interface(left, right) {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.is_subclass_of(left, right) {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.is_subclass_of(right, left) {
            return Some(PhpType::Object(left.to_string()));
        }

        let mut left_ancestors = HashSet::new();
        let mut current = Some(left.to_string());
        while let Some(class_name) = current {
            left_ancestors.insert(class_name.clone());
            current = self
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.clone());
        }

        let mut current = Some(right.to_string());
        while let Some(class_name) = current {
            if left_ancestors.contains(&class_name) {
                return Some(PhpType::Object(class_name));
            }
            current = self
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.clone());
        }

        None
    }

    fn merge_array_element_type(&self, existing: &PhpType, new_ty: &PhpType) -> Option<PhpType> {
        if existing == new_ty {
            return Some(existing.clone());
        }
        if matches!(existing, PhpType::Mixed) || matches!(new_ty, PhpType::Mixed) {
            return Some(PhpType::Mixed);
        }

        match (existing, new_ty) {
            (PhpType::Object(left), PhpType::Object(right)) => self.common_object_type(left, right),
            _ => None,
        }
    }

    fn propagate_constructor_arg_type(
        &mut self,
        instantiated_class: &str,
        param_index: usize,
        arg_ty: &PhpType,
    ) {
        let Some((prop_name, declaring_class)) = self.classes.get(instantiated_class).and_then(|class_info| {
            class_info
                .constructor_param_to_prop
                .get(param_index)
                .and_then(|mapped| mapped.as_ref())
                .map(|prop_name| {
                    let declaring_class = class_info
                        .property_declaring_classes
                        .get(prop_name)
                        .cloned()
                        .unwrap_or_else(|| instantiated_class.to_string());
                    (prop_name.clone(), declaring_class)
                })
        }) else {
            return;
        };

        for class_info in self.classes.values_mut() {
            let shares_inherited_property = class_info
                .property_declaring_classes
                .get(&prop_name)
                .is_some_and(|owner| owner == &declaring_class);

            if !shares_inherited_property {
                continue;
            }

            if let Some(prop) = class_info
                .properties
                .iter_mut()
                .find(|(name, _)| name == &prop_name)
            {
                prop.1 = arg_ty.clone();
            }

            if let Some(sig) = class_info.methods.get_mut("__construct") {
                if let Some((_, param_ty)) = sig.params.get_mut(param_index) {
                    *param_ty = arg_ty.clone();
                }
            }
        }
    }

    fn normalize_pointer_target_type(&self, target_type: &str) -> Option<String> {
        match target_type {
            "int" | "integer" => Some("int".to_string()),
            "float" | "double" | "real" => Some("float".to_string()),
            "bool" | "boolean" => Some("bool".to_string()),
            "string" => Some("string".to_string()),
            "ptr" | "pointer" => Some("ptr".to_string()),
            class_name if self.classes.contains_key(class_name) => Some(class_name.to_string()),
            class_name if self.extern_classes.contains_key(class_name) => {
                Some(class_name.to_string())
            }
            _ => None,
        }
    }

    fn extern_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.extern_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    fn ensure_pointer_type(
        &self,
        ty: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if Self::is_pointer_type(ty) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                &format!("{} requires a pointer argument", context),
            ))
        }
    }

    fn ensure_word_pointer_value(
        &self,
        ty: &PhpType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if matches!(
            ty,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Pointer(_)
        ) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                "ptr_set() value must be int, bool, null, or pointer",
            ))
        }
    }

    fn validate_extern_function_decl(
        &self,
        name: &str,
        params: &[crate::parser::ast::ExternParam],
        return_type: &CType,
        php_params: &[(String, PhpType)],
        php_ret: &PhpType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let mut seen = std::collections::HashSet::new();
        let mut int_regs = 0usize;
        let mut float_regs = 0usize;

        for (param, (_, php_ty)) in params.iter().zip(php_params.iter()) {
            if !seen.insert(param.name.clone()) {
                return Err(CompileError::new(
                    span,
                    &format!("Duplicate extern parameter: ${}", param.name),
                ));
            }
            if matches!(param.c_type, CType::Void) {
                return Err(CompileError::new(
                    span,
                    "Extern parameters cannot use type void",
                ));
            }
            match php_ty {
                PhpType::Float => float_regs += 1,
                PhpType::Str
                | PhpType::Int
                | PhpType::Bool
                | PhpType::Pointer(_)
                | PhpType::Callable => {
                    int_regs += 1;
                }
                PhpType::Void
                | PhpType::Mixed
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_) => {
                    return Err(CompileError::new(
                        span,
                        &format!("Unsupported extern parameter type in {}()", name),
                    ));
                }
            }
        }

        if int_regs > 8 || float_regs > 8 {
            return Err(CompileError::new(
                span,
                &format!(
                    "Extern function '{}' exceeds supported ARM64 register ABI limits (max 8 integer and 8 float arguments)",
                    name
                ),
            ));
        }

        if matches!(return_type, CType::Callable)
            || matches!(
                php_ret,
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
            )
        {
            return Err(CompileError::new(
                span,
                &format!("Extern function '{}' has an unsupported return type", name),
            ));
        }

        Ok(())
    }

    fn validate_extern_field_decl(
        &self,
        class_name: &str,
        field: &crate::parser::ast::ExternField,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if matches!(field.c_type, CType::Void | CType::Callable) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Extern class '{}' field ${} uses an unsupported type",
                    class_name, field.name
                ),
            ));
        }
        Ok(())
    }

    fn validate_extern_global_decl(
        &self,
        name: &str,
        c_type: &CType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if name == "argc" || name == "argv" {
            return Err(CompileError::new(
                span,
                &format!(
                    "extern global ${} would shadow a reserved superglobal",
                    name
                ),
            ));
        }
        if self.extern_globals.contains_key(name) {
            return Err(CompileError::new(
                span,
                &format!("Duplicate extern global declaration: ${}", name),
            ));
        }
        if matches!(c_type, CType::Void | CType::Callable) {
            return Err(CompileError::new(
                span,
                &format!("Extern global ${} uses an unsupported type", name),
            ));
        }
        Ok(())
    }

    fn callback_type_is_c_compatible(ty: &PhpType) -> bool {
        matches!(
            ty,
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Pointer(_) | PhpType::Void
        )
    }

    fn register_callback_function(
        &mut self,
        callback_name: &str,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let decl = self.fn_decls.get(callback_name).cloned().ok_or_else(|| {
            CompileError::new(
                span,
                &format!("Undefined callback function: {}", callback_name),
            )
        })?;

        if decl.variadic.is_some() {
            return Err(CompileError::new(
                span,
                &format!("Callback function '{}' cannot be variadic", callback_name),
            ));
        }
        if decl.defaults.iter().any(|d| d.is_some()) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Callback function '{}' cannot use default parameters",
                    callback_name
                ),
            ));
        }
        if decl.ref_params.iter().any(|is_ref| *is_ref) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Callback function '{}' cannot use pass-by-reference parameters",
                    callback_name
                ),
            ));
        }
        if let Some(sig) = self.functions.get(callback_name) {
            if sig
                .params
                .iter()
                .any(|(_, ty)| !Self::callback_type_is_c_compatible(ty))
                || !Self::callback_type_is_c_compatible(&sig.return_type)
            {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Callback function '{}' uses unsupported C callback types; only int, float, bool, ptr, and void are supported",
                        callback_name
                    ),
                ));
            }
        } else {
            let return_type = infer_return_type_syntactic(&decl.body);
            if !Self::callback_type_is_c_compatible(&return_type) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Callback function '{}' uses an unsupported return type; only int, float, bool, ptr, and void are supported",
                        callback_name
                    ),
                ));
            }

            let params: Vec<(String, PhpType)> = decl
                .params
                .iter()
                .map(|name| (name.clone(), PhpType::Int))
                .collect();
            self.functions.insert(
                callback_name.to_string(),
                FunctionSig {
                    params,
                    defaults: decl.defaults.clone(),
                    return_type,
                    ref_params: decl.ref_params.clone(),
                    variadic: decl.variadic.clone(),
                },
            );
        }

        let _ = decl;
        Ok(())
    }

    fn check_extern_function_call(
        &mut self,
        name: &str,
        args: &[Expr],
        span: crate::span::Span,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let extern_sig = self.extern_functions.get(name).cloned().ok_or_else(|| {
            CompileError::new(span, &format!("Undefined extern function: {}", name))
        })?;

        let sig = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| CompileError::new(span, &format!("Undefined function: {}", name)))?;

        self.check_call_arity("Extern function", name, &sig, args, span)?;

        for (idx, arg) in args.iter().enumerate() {
            let Some((param_name, expected_ty)) = extern_sig.params.get(idx) else {
                break;
            };

            if *expected_ty == PhpType::Callable {
                match &arg.kind {
                    ExprKind::StringLiteral(callback_name) => {
                        self.register_callback_function(callback_name, span)?;
                    }
                    _ => {
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "Extern function '{}' parameter ${} expects a string literal naming a user function",
                                name, param_name
                            ),
                        ));
                    }
                }
                continue;
            }

            let actual_ty = self.infer_type(arg, env)?;
            self.require_compatible_arg_type(
                expected_ty,
                &actual_ty,
                arg.span,
                &format!("Extern function '{}' parameter ${}", name, param_name),
            )?;
        }

        Ok(extern_sig.return_type)
    }

    fn check_call_arity(
        &self,
        kind: &str,
        name: &str,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let effective_arg_count = args
            .iter()
            .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
            .count();
        let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));
        if has_spread {
            return Ok(());
        }

        let required = sig.defaults.iter().filter(|d| d.is_none()).count();
        if sig.variadic.is_some() {
            if effective_arg_count < required {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{} '{}' expects at least {} arguments, got {}",
                        kind, name, required, effective_arg_count
                    ),
                ));
            }
        } else if effective_arg_count < required || effective_arg_count > sig.params.len() {
            let expected = if required == sig.params.len() {
                format!("{}", required)
            } else {
                format!("{} to {}", required, sig.params.len())
            };
            return Err(CompileError::new(
                span,
                &format!(
                    "{} '{}' expects {} arguments, got {}",
                    kind, name, expected, effective_arg_count
                ),
            ));
        }

        Ok(())
    }

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
                    let merged_ty = self.merged_assignment_type(existing, &ty);
                    if merged_ty.is_none() {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Type error: cannot reassign ${} from {:?} to {:?}",
                                name, existing, ty
                            ),
                        ));
                    }
                    if let Some(merged_ty) = merged_ty {
                        if &merged_ty != existing {
                            env.insert(name.clone(), merged_ty);
                        }
                    }
                } else {
                    env.insert(name.clone(), ty);
                }
                Ok(())
            }
            StmtKind::ArrayAssign {
                array,
                index,
                value,
            } => {
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
                        let merged_ty = self
                            .merge_array_element_type(elem_ty, &val_ty)
                            .unwrap_or(val_ty);
                        env.insert(array.clone(), PhpType::Array(Box::new(merged_ty)));
                    }
                } else if let PhpType::AssocArray { key, value: existing_value } = &arr_ty {
                    let merged_value = if **existing_value == val_ty {
                        *existing_value.clone()
                    } else {
                        PhpType::Mixed
                    };
                    env.insert(
                        array.clone(),
                        PhpType::AssocArray {
                            key: key.clone(),
                            value: Box::new(merged_value),
                        },
                    );
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
                        let merged_ty = self
                            .merge_array_element_type(elem_ty, &val_ty)
                            .unwrap_or(val_ty);
                        env.insert(array.clone(), PhpType::Array(Box::new(merged_ty)));
                    }
                }
                Ok(())
            }
            StmtKind::Foreach {
                array,
                key_var,
                value_var,
                body,
            } => {
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
            StmtKind::Switch {
                subject,
                cases,
                default,
            } => {
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
            StmtKind::Throw(expr) => {
                let thrown_ty = self.infer_type(expr, env)?;
                match thrown_ty {
                    PhpType::Object(type_name) if self.object_type_implements_throwable(&type_name) => {
                        Ok(())
                    }
                    PhpType::Object(_) => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object implementing Throwable",
                    )),
                    _ => Err(CompileError::new(
                        stmt.span,
                        "Type error: throw requires an object value",
                    )),
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for s in try_body {
                    self.check_stmt(s, env)?;
                }
                for catch_clause in catches {
                    let exception_type =
                        self.resolve_catch_type_name(&catch_clause.exception_type, stmt.span)?;
                    if !self.classes.contains_key(&exception_type)
                        && !self.interfaces.contains_key(&exception_type)
                    {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Undefined class: {}", exception_type),
                        ));
                    }
                    if !self.object_type_implements_throwable(&exception_type) {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!(
                                "Catch type must extend or implement Throwable: {}",
                                exception_type
                            ),
                        ));
                    }
                    env.insert(
                        catch_clause.variable.clone(),
                        PhpType::Object(exception_type),
                    );
                    for s in &catch_clause.body {
                        self.check_stmt(s, env)?;
                    }
                }
                if let Some(body) = finally_body {
                    for s in body {
                        self.check_stmt(s, env)?;
                    }
                }
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
                if let Some(e) = expr {
                    self.infer_type(e, env)?;
                }
                Ok(())
            }
            StmtKind::ClassDecl { .. }
            | StmtKind::InterfaceDecl { .. }
            | StmtKind::TraitDecl { .. } => {
                // Method bodies are type-checked in a post-pass (after all new ClassName()
                // calls have updated property types from constructor arg types)
                Ok(())
            }
            StmtKind::ExternFunctionDecl { .. }
            | StmtKind::ExternClassDecl { .. }
            | StmtKind::ExternGlobalDecl { .. } => {
                // Extern declarations are processed in the pre-scan pass
                Ok(())
            }
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            } => {
                let obj_ty = self.infer_type(object, env)?;
                let val_ty = self.infer_type(value, env)?;
                if let PhpType::Object(class_name) = &obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        if !class_info.properties.iter().any(|(n, _)| n == property) {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!("Undefined property: {}::{}", class_name, property),
                            ));
                        }
                        if let Some(visibility) = class_info.property_visibilities.get(property) {
                            let declaring_class = class_info
                                .property_declaring_classes
                                .get(property)
                                .map(String::as_str)
                                .unwrap_or(class_name);
                            if !self.can_access_member(declaring_class, visibility) {
                                return Err(CompileError::new(
                                    stmt.span,
                                    &format!(
                                        "Cannot access {} property: {}::{}",
                                        Self::visibility_label(visibility),
                                        class_name,
                                        property
                                    ),
                                ));
                            }
                        }
                        if class_info.readonly_properties.contains(property)
                            && !(self.current_class.as_deref()
                                == class_info
                                    .property_declaring_classes
                                    .get(property)
                                    .map(String::as_str)
                                && self.current_method.as_deref() == Some("__construct"))
                        {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Cannot assign to readonly property outside constructor: {}::{}",
                                    class_name, property
                                ),
                            ));
                        }
                    }
                    // Update property type from assigned value (e.g., Object type from $a->next = $b)
                    if let Some(class_info) = self.classes.get_mut(class_name) {
                        if let Some(prop) = class_info
                            .properties
                            .iter_mut()
                            .find(|(n, _)| n == property)
                        {
                            if prop.1 == PhpType::Int && val_ty != PhpType::Int {
                                prop.1 = val_ty.clone();
                            }
                        }
                    }
                }
                if let PhpType::Pointer(Some(class_name)) = &obj_ty {
                    if let Some(field_ty) = self.extern_field_type(class_name, property) {
                        if field_ty == PhpType::Int && val_ty != PhpType::Int {
                            return Err(CompileError::new(
                                stmt.span,
                                &format!(
                                    "Type error: cannot assign {:?} to extern field {}::{} of type {:?}",
                                    val_ty, class_name, property, field_ty
                                ),
                            ));
                        }
                    } else if self.extern_classes.contains_key(class_name) {
                        return Err(CompileError::new(
                            stmt.span,
                            &format!("Undefined extern field: {}::{}", class_name, property),
                        ));
                    }
                }
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
                    _ => Err(CompileError::new(
                        expr.span,
                        "Cannot negate a non-numeric value",
                    )),
                }
            }
            ExprKind::Not(inner) => {
                self.infer_type(inner, env)?;
                Ok(PhpType::Bool)
            }
            ExprKind::PreIncrement(name)
            | ExprKind::PostIncrement(name)
            | ExprKind::PreDecrement(name)
            | ExprKind::PostDecrement(name) => match env.get(name) {
                Some(PhpType::Int) | Some(PhpType::Bool) | Some(PhpType::Void) => Ok(PhpType::Int),
                Some(other) => Err(CompileError::new(
                    expr.span,
                    &format!("Cannot increment/decrement ${} of type {:?}", name, other),
                )),
                None => Err(CompileError::new(
                    expr.span,
                    &format!("Undefined variable: ${}", name),
                )),
            },
            ExprKind::ArrayLiteralAssoc(pairs) => {
                if pairs.is_empty() {
                    return Err(CompileError::new(
                        expr.span,
                        "Cannot infer type of empty associative array literal",
                    ));
                }
                let key_ty = self.infer_type(&pairs[0].0, env)?;
                let mut val_ty = self.infer_type(&pairs[0].1, env)?;
                for (k, v) in &pairs[1..] {
                    let kt = self.infer_type(k, env)?;
                    let vt = self.infer_type(v, env)?;
                    if kt != key_ty {
                        return Err(CompileError::new(
                            k.span,
                            &format!(
                                "Assoc array key type mismatch: expected {:?}, got {:?}",
                                key_ty, kt
                            ),
                        ));
                    }
                    if vt != val_ty {
                        val_ty = PhpType::Mixed;
                    }
                }
                Ok(PhpType::AssocArray {
                    key: Box::new(key_ty),
                    value: Box::new(val_ty),
                })
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
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
                let mut elem_ty = self.infer_type(&elems[0], env)?;
                for elem in &elems[1..] {
                    let ty = self.infer_type(elem, env)?;
                    if ty != elem_ty {
                        if let Some(merged_ty) = self.merge_array_element_type(&elem_ty, &ty) {
                            elem_ty = merged_ty;
                            continue;
                        }
                        return Err(CompileError::new(
                            elem.span,
                            &format!(
                                "Array element type mismatch: expected {:?}, got {:?}",
                                elem_ty, ty
                            ),
                        ));
                    }
                }
                Ok(PhpType::Array(Box::new(elem_ty)))
            }
            ExprKind::ArrayAccess { array, index } => {
                let arr_ty = self.infer_type(array, env)?;
                let idx_ty = self.infer_type(index, env)?;
                match &arr_ty {
                    PhpType::Array(elem_ty) => {
                        if idx_ty != PhpType::Int {
                            return Err(CompileError::new(
                                expr.span,
                                "Array index must be integer",
                            ));
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
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.infer_type(condition, env)?;
                let then_ty = self.infer_type(then_expr, env)?;
                let else_ty = self.infer_type(else_expr, env)?;
                let result_ty = if then_ty == else_ty {
                    then_ty
                } else if then_ty == PhpType::Str || else_ty == PhpType::Str {
                    PhpType::Str
                } else if then_ty == PhpType::Float || else_ty == PhpType::Float {
                    PhpType::Float
                } else {
                    then_ty
                };
                Ok(result_ty)
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
                if self.extern_functions.contains_key(&name) {
                    return self.check_extern_function_call(&name, &args, expr.span, env);
                }
                if let Some(ty) = self.check_builtin(&name, &args, expr.span, env)? {
                    return Ok(ty);
                }
                self.check_function_call(&name, &args, expr.span, env)
            }
            ExprKind::BitNot(inner) => {
                let ty = self.infer_type(inner, env)?;
                if !matches!(ty, PhpType::Int | PhpType::Bool | PhpType::Void) {
                    return Err(CompileError::new(
                        expr.span,
                        "Bitwise NOT requires integer operand",
                    ));
                }
                Ok(PhpType::Int)
            }
            ExprKind::NullCoalesce { value, default } => {
                let vt = self.infer_type(value, env)?;
                let dt = self.infer_type(default, env)?;
                Ok(wider_type_syntactic(&vt, &dt))
            }
            ExprKind::ConstRef(name) => self.constants.get(name).cloned().ok_or_else(|| {
                CompileError::new(expr.span, &format!("Undefined constant: {}", name))
            }),
            ExprKind::Closure {
                params,
                variadic,
                body,
                is_arrow: _,
                captures,
            } => {
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
                    _ => Err(CompileError::new(
                        expr.span,
                        "Spread operator requires an array",
                    )),
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
                let ret_ty = self
                    .closure_return_types
                    .get(var)
                    .cloned()
                    .unwrap_or(PhpType::Int);
                Ok(ret_ty)
            }
            ExprKind::ExprCall { callee, args } => {
                let callee_ty = self.infer_type(callee, env)?;
                if callee_ty != PhpType::Callable {
                    return Err(CompileError::new(
                        expr.span,
                        &format!(
                            "Cannot call expression — not a callable (got {:?})",
                            callee_ty
                        ),
                    ));
                }
                for arg in args {
                    self.infer_type(arg, env)?;
                }
                // Try to determine return type from closure signature
                match &callee.kind {
                    ExprKind::Variable(var_name) => {
                        if let Some(ret_ty) = self.closure_return_types.get(var_name) {
                            return Ok(ret_ty.clone());
                        }
                    }
                    ExprKind::ArrayAccess { array, .. } => {
                        if let ExprKind::Variable(arr_name) = &array.kind {
                            if let Some(ret_ty) = self.closure_return_types.get(arr_name) {
                                return Ok(ret_ty.clone());
                            }
                        }
                    }
                    ExprKind::Closure { body, .. } => {
                        return Ok(infer_return_type_syntactic(body));
                    }
                    _ => {}
                }
                Ok(PhpType::Int) // fallback for unknown callables
            }
            ExprKind::BinaryOp { left, op, right } => {
                let lt = self.infer_type(left, env)?;
                let rt = self.infer_type(right, env)?;
                match op {
                    BinOp::Pow => {
                        let lt_ok = matches!(
                            lt,
                            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                        );
                        let rt_ok = matches!(
                            rt,
                            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                        );
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span,
                                "Exponentiation requires numeric operands",
                            ));
                        }
                        Ok(PhpType::Float)
                    }
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                        let lt_ok = matches!(
                            lt,
                            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                        );
                        let rt_ok = matches!(
                            rt,
                            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                        );
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span,
                                "Arithmetic operators require numeric operands",
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
                        if Self::is_pointer_type(&lt) || Self::is_pointer_type(&rt) {
                            return Err(CompileError::new(
                                expr.span,
                                "Loose pointer comparison is not supported; use === or !==",
                            ));
                        }
                        // Loose comparison accepts any types — coerces at runtime
                        Ok(PhpType::Bool)
                    }
                    BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq => {
                        let lt_ok = matches!(
                            lt,
                            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                        );
                        let rt_ok = matches!(
                            rt,
                            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                        );
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span,
                                "Comparison operators require numeric operands",
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
                    BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::ShiftLeft
                    | BinOp::ShiftRight => {
                        let lt_ok = matches!(lt, PhpType::Int | PhpType::Bool | PhpType::Void);
                        let rt_ok = matches!(rt, PhpType::Int | PhpType::Bool | PhpType::Void);
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span,
                                "Bitwise operators require integer operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::Spaceship => {
                        let lt_ok = matches!(
                            lt,
                            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                        );
                        let rt_ok = matches!(
                            rt,
                            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
                        );
                        if !lt_ok || !rt_ok {
                            return Err(CompileError::new(
                                expr.span,
                                "Spaceship operator requires numeric operands",
                            ));
                        }
                        Ok(PhpType::Int)
                    }
                    BinOp::NullCoalesce => {
                        // Handled by ExprKind::NullCoalesce — shouldn't reach here
                        // but handle gracefully
                        if lt == PhpType::Void {
                            Ok(rt)
                        } else {
                            Ok(lt)
                        }
                    }
                }
            }
            ExprKind::NewObject { class_name, args } => {
                if self.interfaces.contains_key(class_name) {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Cannot instantiate interface: {}", class_name),
                    ));
                }
                if !self.classes.contains_key(class_name) {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Undefined class: {}", class_name),
                    ));
                }
                if let Some(class_info) = self.classes.get(class_name) {
                    if class_info.is_abstract {
                        return Err(CompileError::new(
                            expr.span,
                            &format!("Cannot instantiate abstract class: {}", class_name),
                        ));
                    }
                    if let Some(sig) = class_info.methods.get("__construct") {
                        self.check_call_arity(
                            "Constructor",
                            &format!("{}::__construct", class_name),
                            sig,
                            args,
                            expr.span,
                        )?;
                    } else if !args.is_empty() {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Constructor '{}::__construct' expects 0 arguments, got {}",
                                class_name,
                                args.len()
                            ),
                        ));
                    }
                }
                // Infer arg types and propagate to property types via constructor mapping
                let param_to_prop = self
                    .classes
                    .get(class_name)
                    .map(|c| c.constructor_param_to_prop.clone())
                    .unwrap_or_default();
                for (i, arg) in args.iter().enumerate() {
                    let arg_ty = self.infer_type(arg, env)?;
                    // If this arg maps to a property, keep inherited property metadata and
                    // inherited constructor signatures in sync with the specialized arg type.
                    if param_to_prop.get(i).is_some_and(|mapped| mapped.is_some()) {
                        self.propagate_constructor_arg_type(class_name, i, &arg_ty);
                    }
                }
                Ok(PhpType::Object(class_name.clone()))
            }
            ExprKind::PropertyAccess { object, property } => {
                let obj_ty = self.infer_type(object, env)?;
                if let PhpType::Object(class_name) = &obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        if let Some(visibility) = class_info.property_visibilities.get(property) {
                            let declaring_class = class_info
                                .property_declaring_classes
                                .get(property)
                                .map(String::as_str)
                                .unwrap_or(class_name);
                            if !self.can_access_member(declaring_class, visibility) {
                                return Err(CompileError::new(
                                    expr.span,
                                    &format!(
                                        "Cannot access {} property: {}::{}",
                                        Self::visibility_label(visibility),
                                        class_name,
                                        property
                                    ),
                                ));
                            }
                        }
                        if let Some((_, ty)) =
                            class_info.properties.iter().find(|(n, _)| n == property)
                        {
                            return Ok(ty.clone());
                        }
                        return Err(CompileError::new(
                            expr.span,
                            &format!("Undefined property: {}::{}", class_name, property),
                        ));
                    }
                }
                if let PhpType::Pointer(Some(class_name)) = &obj_ty {
                    if let Some(field_ty) = self.extern_field_type(class_name, property) {
                        return Ok(field_ty);
                    }
                    if self.extern_classes.contains_key(class_name) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!("Undefined extern field: {}::{}", class_name, property),
                        ));
                    }
                }
                Err(CompileError::new(
                    expr.span,
                    "Property access requires an object or typed extern pointer",
                ))
            }
            ExprKind::MethodCall {
                object,
                method,
                args,
            } => {
                let obj_ty = self.infer_type(object, env)?;
                // Infer arg types and propagate to method sig params
                let mut arg_types = Vec::new();
                for arg in args {
                    arg_types.push(self.infer_type(arg, env)?);
                }
                if let PhpType::Object(class_name) = &obj_ty {
                    if let Some(class_info) = self.classes.get(class_name) {
                        if let Some(sig) = class_info.methods.get(method) {
                            if let Some(visibility) = class_info.method_visibilities.get(method) {
                                let declaring_class = class_info
                                    .method_declaring_classes
                                    .get(method)
                                    .map(String::as_str)
                                    .unwrap_or(class_name);
                                if !self.can_access_member(declaring_class, visibility) {
                                    return Err(CompileError::new(
                                        expr.span,
                                        &format!(
                                            "Cannot access {} method: {}::{}",
                                            Self::visibility_label(visibility),
                                            class_name,
                                            method
                                        ),
                                    ));
                                }
                            }
                            self.check_call_arity(
                                "Method",
                                &format!("{}::{}", class_name, method),
                                sig,
                                args,
                                expr.span,
                            )?;
                        } else {
                            return Err(CompileError::new(
                                expr.span,
                                &format!("Undefined method: {}::{}", class_name, method),
                            ));
                        }
                    }

                    let impl_class_name = self
                        .classes
                        .get(class_name)
                        .and_then(|class_info| class_info.method_impl_classes.get(method))
                        .cloned()
                        .unwrap_or_else(|| class_name.clone());
                    if let Some(class_info) = self.classes.get_mut(&impl_class_name) {
                        if let Some(sig) = class_info.methods.get_mut(method) {
                            for (i, arg_ty) in arg_types.iter().enumerate() {
                                if i < sig.params.len()
                                    && sig.params[i].1 == PhpType::Int
                                    && *arg_ty != PhpType::Int
                                {
                                    sig.params[i].1 = arg_ty.clone();
                                }
                            }
                            return Ok(sig.return_type.clone());
                        }
                    }
                }
                Ok(PhpType::Int)
            }
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            } => {
                // Infer arg types and propagate to static method sig params
                let mut arg_types = Vec::new();
                for arg in args {
                    arg_types.push(self.infer_type(arg, env)?);
                }
                let parent_call = matches!(receiver, StaticReceiver::Parent);
                let self_call = matches!(receiver, StaticReceiver::Self_);
                let resolved_class_name = match receiver {
                    StaticReceiver::Named(class_name) => class_name.clone(),
                    StaticReceiver::Self_ => {
                        self.current_class.as_ref().cloned().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use self:: outside class method scope",
                            )
                        })?
                    }
                    StaticReceiver::Static => {
                        self.current_class.as_ref().cloned().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use static:: outside class method scope",
                            )
                        })?
                    }
                    StaticReceiver::Parent => {
                        let current_class = self.current_class.as_ref().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                "Cannot use parent:: outside class method scope",
                            )
                        })?;
                        let current_info = self.classes.get(current_class).ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                &format!("Undefined class: {}", current_class),
                            )
                        })?;
                        current_info.parent.as_ref().cloned().ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                &format!("Class {} has no parent class", current_class),
                            )
                        })?
                    }
                };
                let class_name = resolved_class_name.as_str();
                if let Some(class_info) = self.classes.get(class_name) {
                    if let Some(sig) = class_info.static_methods.get(method) {
                        if let Some(visibility) = class_info.static_method_visibilities.get(method)
                        {
                            let declaring_class = class_info
                                .static_method_declaring_classes
                                .get(method)
                                .map(String::as_str)
                                .unwrap_or(class_name);
                            if !self.can_access_member(declaring_class, visibility) {
                                return Err(CompileError::new(
                                    expr.span,
                                    &format!(
                                        "Cannot access {} method: {}::{}",
                                        Self::visibility_label(visibility),
                                        class_name,
                                        method
                                    ),
                                ));
                            }
                        }
                        self.check_call_arity(
                            "Static method",
                            &format!("{}::{}", class_name, method),
                            sig,
                            args,
                            expr.span,
                        )?;
                    } else if parent_call || self_call {
                        if self.current_method_is_static {
                            return Err(CompileError::new(
                                expr.span,
                                if parent_call {
                                    "Cannot call parent instance method from a static method"
                                } else {
                                    "Cannot call self instance method from a static method"
                                },
                            ));
                        }
                        let sig = class_info.methods.get(method).ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                &format!("Undefined method: {}::{}", class_name, method),
                            )
                        })?;
                        if let Some(visibility) = class_info.method_visibilities.get(method) {
                            let declaring_class = class_info
                                .method_declaring_classes
                                .get(method)
                                .map(String::as_str)
                                .unwrap_or(class_name);
                            if !self.can_access_member(declaring_class, visibility) {
                                return Err(CompileError::new(
                                    expr.span,
                                    &format!(
                                        "Cannot access {} method: {}::{}",
                                        Self::visibility_label(visibility),
                                        class_name,
                                        method
                                    ),
                                ));
                            }
                        }
                        self.check_call_arity(
                            if parent_call { "Parent method" } else { "Self method" },
                            &format!("{}::{}", class_name, method),
                            sig,
                            args,
                            expr.span,
                        )?;
                    } else if class_info.methods.contains_key(method) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Cannot call instance method statically: {}::{}",
                                class_name, method
                            ),
                        ));
                    } else {
                        return Err(CompileError::new(
                            expr.span,
                            &format!("Undefined method: {}::{}", class_name, method),
                        ));
                    }
                } else {
                    return Err(CompileError::new(
                        expr.span,
                        &format!("Undefined class: {}", class_name),
                    ));
                }

                let direct_impl_class_name = if parent_call || self_call {
                    self.classes
                        .get(class_name)
                        .and_then(|class_info| class_info.method_impl_classes.get(method))
                        .cloned()
                        .unwrap_or_else(|| class_name.to_string())
                } else {
                    String::new()
                };
                if let Some(class_info) = self.classes.get_mut(class_name) {
                    if let Some(sig) = class_info.static_methods.get_mut(method) {
                        for (i, arg_ty) in arg_types.iter().enumerate() {
                            if i < sig.params.len()
                                && sig.params[i].1 == PhpType::Int
                                && *arg_ty != PhpType::Int
                            {
                                sig.params[i].1 = arg_ty.clone();
                            }
                        }
                        return Ok(sig.return_type.clone());
                    }
                }
                if parent_call || self_call {
                    if let Some(sig) = self
                        .classes
                        .get_mut(&direct_impl_class_name)
                        .and_then(|class_info| class_info.methods.get_mut(method))
                    {
                        for (i, arg_ty) in arg_types.iter().enumerate() {
                            if i < sig.params.len()
                                && sig.params[i].1 == PhpType::Int
                                && *arg_ty != PhpType::Int
                            {
                                sig.params[i].1 = arg_ty.clone();
                            }
                        }
                        return Ok(sig.return_type.clone());
                    }
                }
                Ok(PhpType::Int)
            }
            ExprKind::This => {
                if self.current_method_is_static {
                    return Err(CompileError::new(
                        expr.span,
                        "Cannot use $this inside a static method",
                    ));
                }
                if let Some(class_name) = &self.current_class {
                    Ok(PhpType::Object(class_name.clone()))
                } else {
                    Err(CompileError::new(
                        expr.span,
                        "Cannot use $this outside of a class method",
                    ))
                }
            }
            ExprKind::PtrCast {
                target_type,
                expr: inner,
            } => {
                let inner_ty = self.infer_type(inner, env)?;
                self.ensure_pointer_type(&inner_ty, expr.span, "ptr_cast()")?;
                let normalized =
                    self.normalize_pointer_target_type(target_type)
                        .ok_or_else(|| {
                            CompileError::new(
                                expr.span,
                                &format!("Unknown ptr_cast target type: {}", target_type),
                            )
                        })?;
                Ok(PhpType::Pointer(Some(normalized)))
            }
        }
    }

    /// Infer the return type of a closure by scanning its body for Return statements.
    fn infer_closure_return_type(&mut self, body: &[Stmt], env: &TypeEnv) -> PhpType {
        let mut return_types = Vec::new();
        for stmt in body {
            self.collect_closure_return_types(stmt, env, &mut return_types);
        }
        if return_types.is_empty() {
            return PhpType::Int;
        }
        let mut result = return_types[0].clone();
        for ty in &return_types[1..] {
            result = wider_type_syntactic(&result, ty);
        }
        result
    }

    fn collect_closure_return_types(
        &mut self,
        stmt: &Stmt,
        env: &TypeEnv,
        return_types: &mut Vec<PhpType>,
    ) {
        match &stmt.kind {
            StmtKind::Return(Some(expr)) => {
                let ty = self
                    .infer_type(expr, env)
                    .unwrap_or_else(|_| infer_expr_type_syntactic(expr));
                return_types.push(ty);
            }
            StmtKind::Return(None) => {
                return_types.push(PhpType::Void);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                for stmt in then_body {
                    self.collect_closure_return_types(stmt, env, return_types);
                }
                for (_, body) in elseif_clauses {
                    for stmt in body {
                        self.collect_closure_return_types(stmt, env, return_types);
                    }
                }
                if let Some(body) = else_body {
                    for stmt in body {
                        self.collect_closure_return_types(stmt, env, return_types);
                    }
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                for stmt in body {
                    self.collect_closure_return_types(stmt, env, return_types);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                for stmt in try_body {
                    self.collect_closure_return_types(stmt, env, return_types);
                }
                for catch_clause in catches {
                    for stmt in &catch_clause.body {
                        self.collect_closure_return_types(stmt, env, return_types);
                    }
                }
                if let Some(body) = finally_body {
                    for stmt in body {
                        self.collect_closure_return_types(stmt, env, return_types);
                    }
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    for stmt in body {
                        self.collect_closure_return_types(stmt, env, return_types);
                    }
                }
                if let Some(body) = default {
                    for stmt in body {
                        self.collect_closure_return_types(stmt, env, return_types);
                    }
                }
            }
            _ => {}
        }
    }
}
