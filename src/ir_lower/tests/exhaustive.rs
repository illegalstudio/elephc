//! Purpose:
//! Exhaustive AST-variant smoke tests for AST-to-EIR lowering.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - These tests construct AST directly so synthetic variants without stable
//!   PHP source syntax still exercise their explicit lowering branches.
//! - Success means lowering returned a validated module and printable EIR.

use std::collections::HashMap;

use crate::codegen::platform::Target;
use crate::ir::print_module;
use crate::names::Name;
use crate::parser::ast::{
    BinOp, CallableTarget, CastType, CatchClause, ClassConst, ClassMethod, ClassProperty,
    CType, EnumCaseDecl, Expr, ExprKind, ExternField, ExternParam, InstanceOfTarget,
    MagicConstant, PackedField, Program, PropertyHooks, StaticReceiver, Stmt, StmtKind,
    TraitUse, TypeExpr, UseItem, UseKind, Visibility,
};
use crate::span::Span;
use crate::types::{
    CheckResult, ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig,
    PackedClassInfo, PhpType,
};

/// Returns a dummy source span for synthetic AST nodes.
fn sp() -> Span {
    Span::dummy()
}

/// Constructs an expression with a dummy span.
fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, sp())
}

/// Constructs a statement with a dummy span.
fn stmt(kind: StmtKind) -> Stmt {
    Stmt::new(kind, sp())
}

/// Constructs an integer literal.
fn int(value: i64) -> Expr {
    expr(ExprKind::IntLiteral(value))
}

/// Constructs a string literal.
fn str_lit(value: &str) -> Expr {
    expr(ExprKind::StringLiteral(value.to_string()))
}

/// Constructs a boolean literal.
fn bool_lit(value: bool) -> Expr {
    expr(ExprKind::BoolLiteral(value))
}

/// Constructs a variable reference.
fn var(name: &str) -> Expr {
    expr(ExprKind::Variable(name.to_string()))
}

/// Constructs a function or class name.
fn name(value: &str) -> Name {
    Name::unqualified(value)
}

/// Constructs a minimal checker result with function/class/extern metadata used by tests.
fn dummy_check_result() -> CheckResult {
    let mut functions = HashMap::new();
    functions.insert(
        "f".to_string(),
        FunctionSig {
            params: vec![("x".to_string(), PhpType::Int)],
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
            deprecation: None,
        },
    );

    let mut classes = HashMap::new();
    classes.insert("C".to_string(), class_info("C"));

    let mut extern_functions = HashMap::new();
    extern_functions.insert(
        "ef".to_string(),
        ExternFunctionSig {
            name: "ef".to_string(),
            params: vec![("x".to_string(), PhpType::Int)],
            return_type: PhpType::Int,
            library: Some("c".to_string()),
        },
    );

    let mut extern_classes = HashMap::new();
    extern_classes.insert(
        "EC".to_string(),
        ExternClassInfo {
            name: "EC".to_string(),
            fields: Vec::new(),
            total_size: 0,
        },
    );

    let mut enums = HashMap::new();
    enums.insert("E".to_string(), EnumInfo { backing_type: None, cases: Vec::new() });

    let mut packed_classes = HashMap::new();
    packed_classes.insert(
        "P".to_string(),
        PackedClassInfo { fields: Vec::new(), total_size: 0 },
    );

    CheckResult {
        global_env: HashMap::new(),
        functions,
        callable_param_sigs: HashMap::new(),
        callable_return_sigs: HashMap::new(),
        callable_array_return_sigs: HashMap::new(),
        interfaces: HashMap::new(),
        classes,
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals: HashMap::new(),
        required_libraries: Vec::new(),
        warnings: Vec::new(),
    }
}

/// Constructs minimal class metadata with one instance and one static method.
fn class_info(_class_name: &str) -> ClassInfo {
    let method_sig = FunctionSig {
        params: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Int,
        declared_return: true,
        by_ref_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
    };
    let mut methods = HashMap::new();
    methods.insert("m".to_string(), method_sig.clone());
    let mut static_methods = HashMap::new();
    static_methods.insert("sm".to_string(), method_sig);
    ClassInfo {
        class_id: 1,
        parent: None,
        is_abstract: false,
        is_final: false,
        is_readonly_class: false,
        allow_dynamic_properties: true,
        constants: HashMap::new(),
        attribute_names: Vec::new(),
        attribute_args: Vec::new(),
        method_attribute_names: HashMap::new(),
        method_attribute_args: HashMap::new(),
        property_attribute_names: HashMap::new(),
        property_attribute_args: HashMap::new(),
        used_traits: Vec::new(),
        properties: Vec::new(),
        property_offsets: HashMap::new(),
        property_declaring_classes: HashMap::new(),
        defaults: Vec::new(),
        property_visibilities: HashMap::new(),
        property_set_visibilities: HashMap::new(),
        declared_properties: Default::default(),
        final_properties: Default::default(),
        readonly_properties: Default::default(),
        reference_properties: Default::default(),
        owned_reference_properties: Default::default(),
        abstract_properties: Default::default(),
        abstract_property_hooks: HashMap::new(),
        static_properties: Vec::new(),
        static_defaults: Vec::new(),
        static_property_declaring_classes: HashMap::new(),
        static_property_visibilities: HashMap::new(),
        declared_static_properties: Default::default(),
        final_static_properties: Default::default(),
        method_decls: Vec::new(),
        methods,
        static_methods,
        callable_method_return_sigs: HashMap::new(),
        callable_array_method_return_sigs: HashMap::new(),
        method_visibilities: HashMap::new(),
        final_methods: Default::default(),
        method_declaring_classes: HashMap::new(),
        method_impl_classes: HashMap::new(),
        vtable_methods: Vec::new(),
        vtable_slots: HashMap::new(),
        static_method_visibilities: HashMap::new(),
        final_static_methods: Default::default(),
        static_method_declaring_classes: HashMap::new(),
        static_method_impl_classes: HashMap::new(),
        static_vtable_methods: Vec::new(),
        static_vtable_slots: HashMap::new(),
        interfaces: Vec::new(),
        constructor_param_to_prop: Vec::new(),
    }
}

/// Lowers one synthetic program and returns printable EIR.
fn lower_program(program: Program) -> String {
    let target = Target::detect_host();
    let check_result = dummy_check_result();
    let module = crate::ir_lower::lower_program(&program, &check_result, target)
        .expect("synthetic AST should lower to valid EIR");
    let text = print_module(&module);
    assert!(text.contains("function main"), "expected main function in {text}");
    text
}

/// Verifies scalar, unary, binary, call, object, FFI, generator, and utility expression variants.
#[test]
fn lowers_every_expr_variant_smoke() {
    let object = expr(ExprKind::NewObject { class_name: name("C"), args: Vec::new() });
    let expressions = vec![
        str_lit("s"),
        int(1),
        expr(ExprKind::FloatLiteral(1.5)),
        var("x"),
        expr(ExprKind::BinaryOp {
            left: Box::new(int(1)),
            op: BinOp::Add,
            right: Box::new(int(2)),
        }),
        expr(ExprKind::InstanceOf {
            value: Box::new(object.clone()),
            target: InstanceOfTarget::Name(name("C")),
        }),
        expr(ExprKind::InstanceOf {
            value: Box::new(object.clone()),
            target: InstanceOfTarget::Expr(Box::new(str_lit("C"))),
        }),
        bool_lit(true),
        expr(ExprKind::Null),
        expr(ExprKind::Negate(Box::new(int(1)))),
        expr(ExprKind::Not(Box::new(bool_lit(false)))),
        expr(ExprKind::BitNot(Box::new(int(1)))),
        expr(ExprKind::Throw(Box::new(object.clone()))),
        expr(ExprKind::ErrorSuppress(Box::new(var("x")))),
        expr(ExprKind::Print(Box::new(str_lit("p")))),
        expr(ExprKind::NullCoalesce { value: Box::new(expr(ExprKind::Null)), default: Box::new(int(4)) }),
        expr(ExprKind::Pipe { value: Box::new(int(1)), callable: Box::new(var("callable")) }),
        expr(ExprKind::Assignment {
            target: Box::new(var("x")),
            value: Box::new(int(8)),
            result_target: None,
            prelude: vec![stmt(StmtKind::Assign { name: "pre".to_string(), value: int(1) })],
            conditional_value_temp: None,
        }),
        expr(ExprKind::PreIncrement("x".to_string())),
        expr(ExprKind::PostIncrement("x".to_string())),
        expr(ExprKind::PreDecrement("x".to_string())),
        expr(ExprKind::PostDecrement("x".to_string())),
        expr(ExprKind::FunctionCall { name: name("f"), args: vec![int(1)] }),
        expr(ExprKind::FunctionCall { name: name("ef"), args: vec![int(1)] }),
        expr(ExprKind::FunctionCall { name: name("strlen"), args: vec![str_lit("abc")] }),
        expr(ExprKind::ArrayLiteral(vec![int(1), int(2)])),
        expr(ExprKind::ArrayLiteralAssoc(vec![(str_lit("k"), int(1))])),
        expr(ExprKind::Match {
            subject: Box::new(int(1)),
            arms: vec![(vec![int(1)], str_lit("one"))],
            default: Some(Box::new(str_lit("other"))),
        }),
        expr(ExprKind::ArrayAccess { array: Box::new(expr(ExprKind::ArrayLiteral(vec![int(1)]))), index: Box::new(int(0)) }),
        expr(ExprKind::Ternary { condition: Box::new(bool_lit(true)), then_expr: Box::new(int(1)), else_expr: Box::new(int(0)) }),
        expr(ExprKind::ShortTernary { value: Box::new(int(1)), default: Box::new(int(0)) }),
        expr(ExprKind::Cast { target: CastType::String, expr: Box::new(int(1)) }),
        expr(ExprKind::Closure {
            params: Vec::new(),
            variadic: None,
            variadic_type: None,
            return_type: None,
            by_ref_return: false,
            body: vec![stmt(StmtKind::Return(Some(int(1))))],
            is_arrow: false,
            is_static: false,
            captures: vec!["x".to_string()],
            capture_refs: Vec::new(),
        }),
        expr(ExprKind::NamedArg { name: "x".to_string(), value: Box::new(int(1)) }),
        expr(ExprKind::Spread(Box::new(expr(ExprKind::ArrayLiteral(vec![int(1)]))))),
        expr(ExprKind::ClosureCall { var: "callable".to_string(), args: vec![int(1)] }),
        expr(ExprKind::ExprCall { callee: Box::new(var("callable")), args: vec![int(1)] }),
        expr(ExprKind::ConstRef(name("MY_CONST"))),
        object.clone(),
        expr(ExprKind::NewDynamicObject {
            class_name: Box::new(str_lit("C")),
            fallback_class: name("C"),
            required_parent: name("C"),
            args: Vec::new(),
        }),
        expr(ExprKind::PropertyAccess { object: Box::new(object.clone()), property: "p".to_string() }),
        expr(ExprKind::DynamicPropertyAccess { object: Box::new(object.clone()), property: Box::new(str_lit("p")) }),
        expr(ExprKind::NullsafePropertyAccess { object: Box::new(object.clone()), property: "p".to_string() }),
        expr(ExprKind::NullsafeDynamicPropertyAccess { object: Box::new(object.clone()), property: Box::new(str_lit("p")) }),
        expr(ExprKind::StaticPropertyAccess { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string() }),
        expr(ExprKind::MethodCall { object: Box::new(object.clone()), method: "m".to_string(), args: Vec::new() }),
        expr(ExprKind::NullsafeMethodCall { object: Box::new(object.clone()), method: "m".to_string(), args: Vec::new() }),
        expr(ExprKind::StaticMethodCall { receiver: StaticReceiver::Named(name("C")), method: "sm".to_string(), args: Vec::new() }),
        expr(ExprKind::FirstClassCallable(CallableTarget::Function(name("f")))),
        expr(ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
            receiver: StaticReceiver::Named(name("C")),
            method: "sm".to_string(),
        })),
        expr(ExprKind::FirstClassCallable(CallableTarget::Method {
            object: Box::new(object.clone()),
            method: "m".to_string(),
        })),
        expr(ExprKind::This),
        expr(ExprKind::PtrCast { target_type: "C".to_string(), expr: Box::new(int(1)) }),
        expr(ExprKind::BufferNew { element_type: TypeExpr::Int, len: Box::new(int(4)) }),
        expr(ExprKind::ClassConstant { receiver: StaticReceiver::Named(name("C")) }),
        expr(ExprKind::ClassConstant { receiver: StaticReceiver::Self_ }),
        expr(ExprKind::ClassConstant { receiver: StaticReceiver::Static }),
        expr(ExprKind::ClassConstant { receiver: StaticReceiver::Parent }),
        expr(ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Named(name("C")), name: "K".to_string() }),
        expr(ExprKind::NewScopedObject { receiver: StaticReceiver::Named(name("C")), args: Vec::new() }),
        expr(ExprKind::MagicConstant(MagicConstant::File)),
        expr(ExprKind::Yield { key: Some(Box::new(int(1))), value: Some(Box::new(str_lit("v"))) }),
        expr(ExprKind::Yield { key: None, value: None }),
        expr(ExprKind::YieldFrom(Box::new(expr(ExprKind::ArrayLiteral(vec![int(1)]))))),
    ];
    let program = expressions
        .into_iter()
        .map(|expr| stmt(StmtKind::ExprStmt(expr)))
        .collect();
    let text = lower_program(program);
    assert!(text.contains("function main"), "unexpected empty EIR: {text}");
}

/// Verifies statement variants lower without panicking and produce valid EIR.
#[test]
fn lowers_every_stmt_variant_smoke() {
    let method = class_method("m", false);
    let static_method = class_method("sm", true);
    let property = ClassProperty {
        name: "p".to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: Some(TypeExpr::Int),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default: Some(int(0)),
        span: sp(),
        attributes: Vec::new(),
    };
    let class_const = ClassConst {
        name: "K".to_string(),
        visibility: Visibility::Public,
        is_final: false,
        value: int(1),
        span: sp(),
        attributes: Vec::new(),
    };
    let trait_use = TraitUse { trait_names: vec![name("T")], adaptations: Vec::new(), span: sp() };
    let object = expr(ExprKind::NewObject { class_name: name("C"), args: Vec::new() });

    let program = vec![
        stmt(StmtKind::Echo(str_lit("echo"))),
        stmt(StmtKind::Assign { name: "x".to_string(), value: int(1) }),
        stmt(StmtKind::RefAssign { target: "rx".to_string(), source: var("x") }),
        stmt(StmtKind::If {
            condition: bool_lit(true),
            then_body: vec![stmt(StmtKind::Echo(str_lit("then")))],
            elseif_clauses: vec![(bool_lit(false), vec![stmt(StmtKind::Echo(str_lit("elseif")))])],
            else_body: Some(vec![stmt(StmtKind::Echo(str_lit("else")))]),
        }),
        stmt(StmtKind::IfDef { symbol: "FEATURE".to_string(), then_body: vec![stmt(StmtKind::Echo(str_lit("ifdef")))], else_body: None }),
        stmt(StmtKind::While { condition: bool_lit(false), body: vec![stmt(StmtKind::Continue(1))] }),
        stmt(StmtKind::DoWhile { body: vec![stmt(StmtKind::Break(1))], condition: bool_lit(false) }),
        stmt(StmtKind::For {
            init: Some(Box::new(stmt(StmtKind::Assign { name: "i".to_string(), value: int(0) }))),
            condition: Some(bool_lit(false)),
            update: Some(Box::new(stmt(StmtKind::ExprStmt(expr(ExprKind::PostIncrement("i".to_string())))))),
            body: vec![stmt(StmtKind::Continue(1))],
        }),
        stmt(StmtKind::Assign { name: "arr".to_string(), value: expr(ExprKind::ArrayLiteral(vec![int(1)])) }),
        stmt(StmtKind::ArrayAssign { array: "arr".to_string(), index: int(0), value: int(2) }),
        stmt(StmtKind::NestedArrayAssign { target: expr(ExprKind::ArrayAccess { array: Box::new(var("arr")), index: Box::new(int(0)) }), value: int(3) }),
        stmt(StmtKind::ArrayPush { array: "arr".to_string(), value: int(4) }),
        stmt(StmtKind::TypedAssign { type_expr: TypeExpr::Int, name: "typed".to_string(), value: int(5) }),
        stmt(StmtKind::Foreach {
            array: var("arr"),
            key_var: Some("k".to_string()),
            value_var: "v".to_string(),
            value_by_ref: false,
            body: vec![stmt(StmtKind::Echo(var("v")))],
        }),
        stmt(StmtKind::Switch {
            subject: int(1),
            cases: vec![(vec![int(1)], vec![stmt(StmtKind::Echo(str_lit("case")))])],
            default: Some(vec![stmt(StmtKind::Echo(str_lit("default")))]),
        }),
        stmt(StmtKind::Include { path: str_lit("file.php"), once: false, required: false }),
        stmt(StmtKind::IncludeOnceMark { label: "inc".to_string() }),
        stmt(StmtKind::IncludeOnceGuard { label: "inc".to_string(), body: vec![stmt(StmtKind::Echo(str_lit("guard")))] }),
        stmt(StmtKind::Throw(object.clone())),
        stmt(StmtKind::Synthetic(vec![stmt(StmtKind::Echo(str_lit("synthetic")))])),
        stmt(StmtKind::Try {
            try_body: vec![stmt(StmtKind::Echo(str_lit("try")))],
            catches: vec![CatchClause { exception_types: vec![name("Exception")], variable: Some("e".to_string()), body: vec![stmt(StmtKind::Echo(str_lit("catch")))] }],
            finally_body: Some(vec![stmt(StmtKind::Echo(str_lit("finally")))]),
        }),
        stmt(StmtKind::ExprStmt(int(0))),
        stmt(StmtKind::NamespaceDecl { name: Some(name("Ns")) }),
        stmt(StmtKind::NamespaceBlock { name: Some(name("Ns")), body: vec![stmt(StmtKind::Echo(str_lit("ns")))] }),
        stmt(StmtKind::UseDecl { imports: vec![UseItem { kind: UseKind::Class, name: name("Other"), alias: "Other".to_string() }] }),
        stmt(StmtKind::UseDecl { imports: vec![UseItem { kind: UseKind::Function, name: name("fn_name"), alias: "fn_name".to_string() }] }),
        stmt(StmtKind::UseDecl { imports: vec![UseItem { kind: UseKind::Const, name: name("CONST_NAME"), alias: "CONST_NAME".to_string() }] }),
        stmt(StmtKind::FunctionDecl {
            name: "f".to_string(),
            params: vec![("x".to_string(), Some(TypeExpr::Int), None, false)],
            variadic: None,
            variadic_type: None,
            return_type: Some(TypeExpr::Int),
            by_ref_return: false,
            body: vec![stmt(StmtKind::Return(Some(var("x"))))],
        }),
        stmt(StmtKind::FunctionVariantGroup { name: "f".to_string(), variants: vec!["a".to_string()] }),
        stmt(StmtKind::FunctionVariantMark { name: "f".to_string(), variant: "a".to_string() }),
        stmt(StmtKind::Return(None)),
        stmt(StmtKind::ConstDecl { name: "CST".to_string(), value: int(1) }),
        stmt(StmtKind::ListUnpack { vars: vec!["a".to_string(), "b".to_string()], value: var("arr") }),
        stmt(StmtKind::Global { vars: vec!["g".to_string()] }),
        stmt(StmtKind::StaticVar { name: "sv".to_string(), init: int(1) }),
        stmt(StmtKind::ClassDecl {
            name: "C".to_string(),
            extends: None,
            implements: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            trait_uses: vec![trait_use.clone()],
            properties: vec![property.clone()],
            methods: vec![method.clone(), static_method.clone()],
            constants: vec![class_const.clone()],
        }),
        stmt(StmtKind::EnumDecl { name: "E".to_string(), backing_type: Some(TypeExpr::Int), cases: vec![EnumCaseDecl { name: "A".to_string(), value: Some(int(1)), span: sp(), attributes: Vec::new() }], implements: Vec::new(), methods: Vec::new(), constants: Vec::new() }),
        stmt(StmtKind::PackedClassDecl { name: "P".to_string(), fields: vec![PackedField { name: "x".to_string(), type_expr: TypeExpr::Int, span: sp() }] }),
        stmt(StmtKind::InterfaceDecl { name: "I".to_string(), extends: Vec::new(), properties: Vec::new(), methods: Vec::new(), constants: Vec::new() }),
        stmt(StmtKind::TraitDecl { name: "T".to_string(), trait_uses: Vec::new(), properties: Vec::new(), methods: vec![method], constants: vec![class_const] }),
        stmt(StmtKind::PropertyAssign { object: Box::new(object.clone()), property: "p".to_string(), value: int(1) }),
        stmt(StmtKind::StaticPropertyAssign { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string(), value: int(1) }),
        stmt(StmtKind::StaticPropertyArrayPush { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string(), value: int(1) }),
        stmt(StmtKind::StaticPropertyArrayAssign { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string(), index: int(0), value: int(1) }),
        stmt(StmtKind::PropertyArrayPush { object: Box::new(object.clone()), property: "p".to_string(), value: int(1) }),
        stmt(StmtKind::PropertyArrayAssign { object: Box::new(object), property: "p".to_string(), index: int(0), value: int(1) }),
        stmt(StmtKind::ExternFunctionDecl { name: "ef".to_string(), params: vec![ExternParam { name: "x".to_string(), c_type: CType::Int }], return_type: CType::Int, library: Some("c".to_string()) }),
        stmt(StmtKind::ExternClassDecl { name: "EC".to_string(), fields: vec![ExternField { name: "x".to_string(), c_type: CType::Int }] }),
        stmt(StmtKind::ExternGlobalDecl { name: "eg".to_string(), c_type: CType::Int }),
    ];

    for statement in program {
        let text = lower_program(vec![statement]);
        assert!(text.contains("function main"), "unexpected empty EIR: {text}");
    }
}

/// Constructs a concrete class method declaration for synthetic class-like AST nodes.
fn class_method(name: &str, is_static: bool) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body: vec![stmt(StmtKind::Return(Some(int(1))))],
        span: sp(),
        attributes: Vec::new(),
    }
}
