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

use crate::fast_hash::FastMap as HashMap;

use crate::codegen::platform::Target;
use crate::ir::print_module;
use crate::names::Name;
use crate::parser::ast::{
    ArrayEntry, BinOp, CallableTarget, CastType, CatchClause, ClassConst, ClassMethod, ClassProperty,
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
    let mut functions = HashMap::default();
    functions.insert(
        "f".to_string(),
        FunctionSig {
            params: vec![("x".to_string(), PhpType::Int)],
            param_type_exprs: vec![None],
            param_attributes: Vec::new(),
            defaults: vec![None],
            return_type: PhpType::Int,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![false],
            declared_params: vec![true],
            variadic: None,
            deprecation: None,
            is_generator: false,
        },
    );

    let mut classes = crate::fast_hash::FastMap::default();
    classes.insert("C".to_string(), class_info("C"));

    let mut extern_functions = HashMap::default();
    extern_functions.insert(
        "ef".to_string(),
        ExternFunctionSig {
            name: "ef".to_string(),
            params: vec![("x".to_string(), PhpType::Int)],
            abi_params: vec![crate::types::ExternAbiType::INT64],
            return_type: PhpType::Int,
            abi_return: crate::types::ExternAbiType::INT64,
            library: Some("c".to_string()),
        },
    );

    let mut extern_classes = HashMap::default();
    extern_classes.insert(
        "EC".to_string(),
        ExternClassInfo {
            name: "EC".to_string(),
            fields: Vec::new(),
            total_size: 0,
        },
    );

    let mut enums = HashMap::default();
    enums.insert("E".to_string(), EnumInfo { backing_type: None, cases: Vec::new() });

    let mut packed_classes = HashMap::default();
    packed_classes.insert(
        "P".to_string(),
        PackedClassInfo { fields: Vec::new(), total_size: 0, align: 1 },
    );

    CheckResult {
        requested_instantiations: Vec::new(),
        requested_class_instantiations: Vec::new(),
        requested_method_instantiations: Vec::new(),
        generic_method_sites: crate::fast_hash::FastMap::default(),
        generic_new_sites: Default::default(),
        generic_call_sites: HashMap::default(),
        global_env: Default::default(),
        functions,
        function_attribute_names: HashMap::default(),
        function_attribute_args: HashMap::default(),
        callable_param_sigs: HashMap::default(),
        return_alias_summaries: Default::default(),
        callable_return_sigs: HashMap::default(),
        callable_array_return_sigs: HashMap::default(),
        interfaces: HashMap::default(),
        classes,
        enums,
        packed_classes,
        extern_functions,
        extern_classes,
        extern_globals: HashMap::default(),
        required_libraries: Vec::new(),
        warnings: Vec::new(),
        throw_access_sites: HashMap::default(),
        builtin_call_types: HashMap::default(),
        closure_generator_spans: Default::default(),
        array_push_property_plans: HashMap::default(),
        member_receiver_types: HashMap::default(),
        flow_typed_property_accesses: HashMap::default(),
        boxed_reference_promotion_sites: HashMap::default(),
        buffer_read_sites: Default::default(),
        first_class_builtin_call_types: HashMap::default(),
        loop_storage_types: HashMap::default(),
        string_incdec_locals: Default::default(),
        by_ref_local_storage_types: HashMap::default(),
        ref_param_escaping_callees: Default::default(),
        property_bound_promotion_sites: Default::default(),
        by_ref_call_result_sites: Default::default(),
        bcmath_operator_sites: Default::default(),
        stdclass_by_ref_walk_sites: Default::default(),
        runtime_define_names: Default::default(),
        undefined_constant_reads: Default::default(),
        dynamic_ref_local_types: HashMap::default(),
        local_bind_kill_sites: Default::default(),
        local_ref_detach_sites: Default::default(),
        local_retype_sites: Default::default(),
        mixed_storage_store_sites: Default::default(),
        user_filters_reachable: true,
        user_filter_registered: None,
    }
}

/// Constructs minimal class metadata with one instance and one static method.
fn class_info(_class_name: &str) -> ClassInfo {
    let method_sig = FunctionSig {
        params: Vec::new(),
        param_type_exprs: Vec::new(),
        param_attributes: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Int,
        declared_return: true,
        by_ref_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
        is_generator: false,
    };
    let mut methods = crate::fast_hash::FastMap::default();
    methods.insert("m".to_string(), method_sig.clone());
    let mut static_methods = crate::fast_hash::FastMap::default();
    static_methods.insert("sm".to_string(), method_sig);
    ClassInfo {
        class_id: 1,
        declaration_span: crate::span::Span::dummy(),
        doc_comment: None,
        is_internal: false,
        parent: None,
        is_abstract: false,
        is_final: false,
        is_readonly_class: false,
        allow_dynamic_properties: true,
        eval_property_storage: false,
        clone_override_property_storage: false,
        scope_dynamic_property_storage: false,
        constants: HashMap::default(),
        constant_order: Vec::new(),
        constant_deprecations: HashMap::default(),
        constant_types: HashMap::default(),
        constant_visibilities: Default::default(),
        final_constants: Default::default(),
        attribute_names: Vec::new(),
        attribute_args: Vec::new(),
        method_attribute_names: HashMap::default(),
        method_attribute_args: HashMap::default(),
        property_attribute_names: HashMap::default(),
        property_attribute_args: HashMap::default(),
        constant_attribute_names: HashMap::default(),
        constant_attribute_args: HashMap::default(),
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
        properties: Vec::new(),
        property_offsets: HashMap::default(),
        property_declaring_classes: HashMap::default(),
        defaults: Vec::new(),
        deferred_property_default_error: None,
        property_visibilities: HashMap::default(),
        property_set_visibilities: HashMap::default(),
        declared_properties: Default::default(),
        property_declared_slots: Vec::new(),
        final_properties: Default::default(),
        readonly_properties: Default::default(),
        lazily_unset_properties: Default::default(),
        backed_hooked_properties: Default::default(),
        reference_properties: Default::default(),
        owned_reference_properties: Default::default(),
        promoted_properties: Default::default(),
        property_reference_slots: Vec::new(),
        property_slot_declaring_classes: Vec::new(),
        property_owned_reference_slots: Vec::new(),
        abstract_properties: Default::default(),
        abstract_property_hooks: HashMap::default(),
        reflection_properties: Vec::new(),
        reflection_methods: Vec::new(),
        property_hooks: HashMap::default(),
        // `sp` backs the static-property statements below. The checker only lets an element
        // reference (`C::$sp[0] = &C::$sp[1]`) reach lowering for an array static property, so
        // the fixture declares one instead of leaving it to the untyped `Mixed` fallback.
        static_properties: vec![(
            "sp".to_string(),
            PhpType::AssocArray { key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed) },
        )],
        static_defaults: Vec::new(),
        static_property_declaring_classes: HashMap::default(),
        static_property_visibilities: HashMap::default(),
        declared_static_properties: Default::default(),
        final_static_properties: Default::default(),
        method_decls: Vec::new(),
        methods,
        static_methods,
        late_static_method_returns: Default::default(),
        late_static_static_method_returns: Default::default(),
        callable_method_return_sigs: HashMap::default(),
        callable_array_method_return_sigs: HashMap::default(),
        method_visibilities: crate::fast_hash::FastMap::default(),
        final_methods: Default::default(),
        method_declaring_classes: crate::fast_hash::FastMap::default(),
        method_impl_classes: crate::fast_hash::FastMap::default(),
        abstract_methods: Default::default(),
        vtable_methods: Vec::new(),
        vtable_slots: HashMap::default(),
        static_method_visibilities: crate::fast_hash::FastMap::default(),
        final_static_methods: Default::default(),
        static_method_declaring_classes: crate::fast_hash::FastMap::default(),
        static_method_impl_classes: crate::fast_hash::FastMap::default(),
        abstract_static_methods: Default::default(),
        static_vtable_methods: Vec::new(),
        static_vtable_slots: HashMap::default(),
        declared_interfaces: Vec::new(),
        interfaces: Vec::new(),
        constructor_param_to_prop: Vec::new(),
    }
}

/// Lowers one synthetic program and returns printable EIR.
fn lower_program(program: Program) -> String {
    let target = Target::detect_host();
    let check_result = dummy_check_result();
    let catalog = crate::ir::SourceCatalog::from_units(["inc", "/included.php"].map(|path| {
        crate::resolver::SourceUnit {
            canonical_path: path.into(), mode: crate::source::SourceMode::Php,
            source: "<?php echo 'guard';".into(),
        }
    })).unwrap();
    let module = crate::ir_lower::lower_program_with_source_catalog(
        &program, &check_result, target, std::path::Path::new("synthetic.php"), false, catalog,
    )
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
        expr(ExprKind::ArrayLiteralMixed(vec![
            ArrayEntry::Keyed(str_lit("k"), int(1)),
            ArrayEntry::Spread(expr(ExprKind::ArrayLiteral(vec![int(2)]))),
            ArrayEntry::Value(int(3)),
        ])),
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
            variadic_by_ref: false,
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
        expr(ExprKind::ObjectClassName { object: Box::new(object.clone()) }),
        expr(ExprKind::ScopedConstantAccess { receiver: StaticReceiver::Named(name("C")), name: "K".to_string() }),
        expr(ExprKind::DynamicScopedConstantAccess { receiver: Box::new(object.clone()), name: "K".to_string() }),
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

/// Verifies canonical-path include markers lower to valid native guard instructions.
#[test]
fn lowers_include_source_path_markers() {
    let text = lower_program(vec![
        stmt(StmtKind::IncludeOnceMark { source_path: "/included.php".into() }),
        stmt(StmtKind::IncludeOnceGuard {
            source_path: "/included.php".into(),
            required: true,
            body: vec![stmt(StmtKind::Echo(str_lit("guard")))],
        }),
    ]);
    assert!(text.contains("include_once_mark"), "{text}");
    assert!(text.contains("include_once_guard"), "{text}");
    assert!(text.contains("source[0]"), "{text}");
}

#[test]
fn include_source_without_catalog_is_a_lowering_error() {
    let path = std::path::PathBuf::from("/not-catalogued.php");
    let program = vec![stmt(StmtKind::IncludeOnceMark { source_path: path.clone() })];
    let result = crate::ir_lower::lower_program(&program, &dummy_check_result(), Target::detect_host(), false);
    assert!(matches!(result, Err(crate::ir_lower::LoweringError::Validation(
        crate::ir::ValidationError::UnresolvedSource(found))) if found == path));
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
        is_promoted: false,
        default: Some(int(0)),
        span: sp(),
        attributes: Vec::new(),
    };
    let class_const = ClassConst {
        name: "K".to_string(),
        visibility: Visibility::Public,
        is_final: false,
        type_expr: None,
        value: int(1),
        span: sp(),
        attributes: Vec::new(),
    };
    let trait_use = TraitUse {
        trait_names: vec![name("T")],
        type_args: vec![Vec::new()],
        adaptations: Vec::new(),
        span: sp(),
    };
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
        stmt(StmtKind::IncludeOnceMark { source_path: "inc".into() }),
        stmt(StmtKind::IncludeOnceGuard { source_path: "inc".into(), required: true, body: vec![stmt(StmtKind::Echo(str_lit("guard")))] }),
        stmt(StmtKind::Throw(object.clone())),
        stmt(StmtKind::Synthetic(vec![stmt(StmtKind::Echo(str_lit("synthetic")))])),
        stmt(StmtKind::Try {
            try_body: vec![stmt(StmtKind::Echo(str_lit("try")))],
            catches: vec![CatchClause { exception_type_args: Vec::new(), exception_types: vec![name("Exception")], variable: Some("e".to_string()), body: vec![stmt(StmtKind::Echo(str_lit("catch")))] }],
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
            type_params: Vec::new(),
            params: vec![("x".to_string(), Some(TypeExpr::Int), None, false)],
            param_attributes: Vec::new(),
            variadic: None,
            variadic_by_ref: false,
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
            generics: None,
            name: "C".to_string(),
            doc_comment: None,
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
        stmt(StmtKind::EnumDecl { name: "E".to_string(), generics: None, backing_type: Some(TypeExpr::Int), cases: vec![EnumCaseDecl { name: "A".to_string(), value: Some(int(1)), span: sp(), attributes: Vec::new() }], implements: Vec::new(), trait_uses: Vec::new(), methods: Vec::new(), constants: Vec::new() }),
        stmt(StmtKind::PackedClassDecl { name: "P".to_string(), fields: vec![PackedField { name: "x".to_string(), type_expr: TypeExpr::Int, span: sp() }] }),
        stmt(StmtKind::InterfaceDecl { generics: None, name: "I".to_string(), extends: Vec::new(), properties: Vec::new(), methods: Vec::new(), constants: Vec::new() }),
        stmt(StmtKind::TraitDecl { name: "T".to_string(), generics: None, trait_uses: Vec::new(), properties: Vec::new(), methods: vec![method], constants: vec![class_const] }),
        stmt(StmtKind::PropertyAssign { object: Box::new(object.clone()), property: "p".to_string(), value: int(1) }),
        stmt(StmtKind::PropertyRefAssign { object: Box::new(object.clone()), property: "p".to_string(), source: Expr::new(ExprKind::PropertyAccess { object: Box::new(object.clone()), property: "p".to_string() }, sp()) }),
        stmt(StmtKind::StaticPropertyAssign { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string(), value: int(1) }),
        stmt(StmtKind::StaticPropertyArrayPush { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string(), value: int(1) }),
        stmt(StmtKind::StaticPropertyArrayAssign { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string(), index: int(0), value: int(1) }),
        stmt(StmtKind::StaticPropertyElementRefAssign { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string(), index: int(0), source: Expr::new(ExprKind::ArrayAccess { array: Box::new(Expr::new(ExprKind::StaticPropertyAccess { receiver: StaticReceiver::Named(name("C")), property: "sp".to_string() }, sp())), index: Box::new(int(1)) }, sp()) }),
        stmt(StmtKind::DynamicStaticPropertyWrite { receiver: StaticReceiver::Named(name("C")), property: Box::new(str_lit("sp")), index: None, append: false, value: int(1) }),
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
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body: vec![stmt(StmtKind::Return(Some(int(1))))],
        span: sp(),
        attributes: Vec::new(),
    }
}

/// Verifies the three `ArrayLiteralMixed` entry forms lower to the ops their order depends on.
///
/// The smoke list above only proves the node lowers at all. `["k" => 1, ...[2], 3]` is the shape
/// the ORDER matters for -- the spread's elements take the next free integer key where the
/// spread sits -- so each form has to emit its own op, in source order, into the one hash the
/// literal opens with. The bare value appends through a generic `runtime_call` rather than an
/// op of its own, so this also pins that the call takes the literal's hash as its receiver.
#[test]
fn lowers_a_mixed_array_literal_through_set_spread_and_append() {
    let text = lower_program(vec![stmt(StmtKind::Assign {
        name: "mixed".to_string(),
        value: expr(ExprKind::ArrayLiteralMixed(vec![
            ArrayEntry::Keyed(str_lit("k"), int(1)),
            ArrayEntry::Spread(expr(ExprKind::ArrayLiteral(vec![int(2)]))),
            ArrayEntry::Value(int(3)),
        ])),
    })]);

    let ops: Vec<&str> = text
        .lines()
        .filter_map(|line| {
            ["hash_new", "hash_set", "array_to_hash", "hash_spread", "runtime_call"]
                .into_iter()
                .find(|op| line.contains(op))
        })
        .collect();
    assert_eq!(
        ops,
        vec!["hash_new", "hash_set", "array_to_hash", "hash_spread", "runtime_call"],
        "unexpected mixed-literal EIR: {text}"
    );

    // The one hash every entry writes into: an indexed source is promoted to its own hash first
    // (`array_to_hash`), so "a hash exists" is not enough to prove the append went to the right
    // one.
    let hash = text
        .lines()
        .find_map(|line| line.split_once(": Heap(Hash)"))
        .map(|(value, _)| value.trim())
        .unwrap_or_else(|| panic!("no hash in mixed-literal EIR: {text}"));
    for op in ["hash_set", "hash_spread", "runtime_call"] {
        assert!(
            text.lines()
                .any(|line| line.trim().starts_with(&format!("{op} {hash} "))),
            "{op} does not write into the literal's own hash {hash}: {text}"
        );
    }
}
