//! Purpose:
//! Unit regressions for target-aware userspace stream-wrapper adapter assembly.
//! Covers callback-slot shapes, untyped boxing, typed coercion, and Throwable paths.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - x86_64 generation is validated without requiring a local GNU assembler or linker.

use crate::codegen_support::platform::{Arch, Platform, Target};
use crate::ir::Module;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

use super::{
    emit_user_wrapper_adapter, type_contract, wrapper_runtime_arg_types,
};

/// Builds a minimal instance-method signature for adapter emission tests.
fn method_signature(params: Vec<(&str, PhpType)>, return_type: PhpType) -> FunctionSig {
    let len = params.len();
    FunctionSig {
        params: params
            .into_iter()
            .map(|(name, php_type)| (name.to_string(), php_type))
            .collect(),
        param_type_exprs: vec![None; len],
        param_attributes: vec![Vec::new(); len],
        defaults: vec![None; len],
        return_type,
        declared_return: false,
        by_ref_return: false,
        ref_params: vec![false; len],
        declared_params: vec![false; len],
        variadic: None,
        deprecation: None,
    }
}

/// Verifies the runtime callback matrix distinguishes string and integer wrapper slots.
#[test]
fn wrapper_runtime_arg_matrix_matches_stream_contracts() {
    assert_eq!(
        wrapper_runtime_arg_types(9, "Wrapper"),
        vec![
            PhpType::Object("Wrapper".to_string()),
            PhpType::Str,
            PhpType::Int,
        ]
    );
    assert_eq!(
        wrapper_runtime_arg_types(19, "Wrapper"),
        vec![
            PhpType::Object("Wrapper".to_string()),
            PhpType::Str,
            PhpType::Int,
        ]
    );
}

/// Verifies exact union arms suppress only the weak int conversions they supersede.
#[test]
fn weak_int_deprecation_sources_follow_exact_first_union_selection() {
    use crate::parser::ast::TypeExpr;

    assert_eq!(
        type_contract::weak_int_deprecation_sources(&TypeExpr::Int),
        (true, true)
    );
    assert_eq!(
        type_contract::weak_int_deprecation_sources(&TypeExpr::Union(vec![
            TypeExpr::Int,
            TypeExpr::Float,
        ])),
        (false, false)
    );
    assert_eq!(
        type_contract::weak_int_deprecation_sources(&TypeExpr::Union(vec![
            TypeExpr::Int,
            TypeExpr::Str,
        ])),
        (true, false)
    );
    assert_eq!(
        type_contract::weak_int_deprecation_sources(&TypeExpr::Union(vec![
            TypeExpr::Int,
            TypeExpr::Bool,
        ])),
        (true, true)
    );
}

/// Verifies x86_64 adapters box untyped arguments and normalize owned string returns.
#[test]
fn x86_untyped_wrapper_adapter_boxes_and_balances_mixed_temps() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let signature = method_signature(vec![("count", PhpType::Mixed)], PhpType::Str);

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        7,
        2,
        "stream_read",
        "Wrapper",
        &signature,
        &mut data,
    );
    let assembly = emitter.output();

    assert!(assembly.contains("_user_wrapper_adapter_7_stream_u_read:"));
    assert!(assembly.contains("call __rt_mixed_from_value"));
    assert!(assembly.contains("call _method_Wrapper_stream_u_read"));
    assert!(assembly.contains("call __rt_decref_mixed"));
    assert!(assembly.contains("call __rt_str_persist"));
    assert!(!assembly.contains("bl __rt_mixed_from_value"));
}

/// Verifies dynamic callback returns are converted to each slot's fixed runtime ABI.
#[test]
fn x86_wrapper_adapter_normalizes_dynamic_return_contracts() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let dynamic_signature = method_signature(Vec::new(), PhpType::Mixed);

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        12,
        5,
        "stream_tell",
        "Wrapper",
        &dynamic_signature,
        &mut data,
    );
    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        12,
        8,
        "stream_stat",
        "Wrapper",
        &dynamic_signature,
        &mut data,
    );
    let assembly = emitter.output();

    assert!(assembly.contains("_user_wrapper_adapter_12_stream_u_tell_return_tag_0_invalid"));
    assert!(assembly.contains("mov rax, -1"));
    assert!(assembly.contains("_user_wrapper_adapter_12_stream_u_stat_return_stat_array"));
    assert!(assembly.contains("cmp r10, 4"));
    assert!(assembly.contains("cmp r10, 5"));
    assert!(assembly.contains("call __rt_decref_mixed"));
}

/// Verifies stream-eof adapters preserve the hidden strictness mode and exact warnings.
#[test]
fn x86_stream_eof_adapter_distinguishes_strict_and_lenient_results() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let dynamic_signature = method_signature(Vec::new(), PhpType::Mixed);

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        12,
        4,
        "stream_eof",
        "Wrapper",
        &dynamic_signature,
        &mut data,
    );
    let assembly = emitter.output();
    let literals = data.emit();

    assert!(assembly.contains("mov QWORD PTR [rbp - "));
    assert!(assembly.contains(", rsi"));
    assert!(assembly.contains("_return_eof_lenient"));
    assert!(assembly.contains("_return_eof_invalid"));
    assert!(assembly.contains("_class_name_entries"));
    assert!(assembly.contains("call __rt_diag_warning"));
    assert!(assembly.contains("call __rt_decref_mixed"));
    assert!(literals.contains(
        "Warning: feof(): Wrapper::stream_eof value must be of type bool, "
    ));
    assert!(literals.contains(" given"));
}

/// Verifies internal callback values receive warned, owned by-reference cells.
#[test]
fn x86_wrapper_adapter_materializes_by_ref_callback_values() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let mut signature = method_signature(vec![("count", PhpType::Mixed)], PhpType::Str);
    signature.ref_params = vec![true];

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        13,
        2,
        "stream_read",
        "Wrapper",
        &signature,
        &mut data,
    );
    let assembly = emitter.output();
    let literals = data.emit();

    assert!(assembly.contains("call __rt_diag_warning"));
    assert!(assembly.contains("call setjmp"));
    assert!(assembly.contains("call __rt_heap_alloc"));
    assert!(assembly.contains("call __rt_mixed_from_value"));
    assert!(assembly.contains("call __rt_decref_mixed"));
    assert!(assembly.contains("call __rt_heap_free"));
    assert!(assembly.contains("_user_wrapper_adapter_13_stream_u_read_callback_throw"));
    assert!(assembly.contains(
        "_user_wrapper_adapter_13_stream_u_read_callback_throw_not_installed"
    ));
    assert!(assembly.contains("jmp __rt_throw_current"));
    assert!(literals.contains(
        "Warning: Wrapper::stream_read(): Argument #1 ($count) must be passed by reference, \
value given"
    ));
}

/// Verifies the opened-path cell is passed directly and rejects non-nullable declarations.
#[test]
fn x86_wrapper_adapter_validates_opened_path_reference_type() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let mut signature = method_signature(
        vec![
            ("path", PhpType::Mixed),
            ("mode", PhpType::Mixed),
            ("options", PhpType::Mixed),
            ("openedPath", PhpType::Str),
        ],
        PhpType::Bool,
    );
    signature.ref_params = vec![false, false, false, true];
    signature.declared_params = vec![false, false, false, true];
    signature.param_type_exprs = vec![
        None,
        None,
        None,
        Some(crate::parser::ast::TypeExpr::Str),
    ];

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        14,
        0,
        "stream_open",
        "Wrapper",
        &signature,
        &mut data,
    );
    let assembly = emitter.output();
    let literals = data.emit();

    assert!(!literals.contains("Argument #4 ($openedPath) must be passed by reference"));
    assert!(literals.contains(
        "Wrapper::stream_open(): Argument #4 ($openedPath) must be of type string, null given"
    ));
    assert!(assembly.contains("_spl_type_error_class_id"));
    assert!(assembly.contains("call setjmp"));
    assert!(assembly.contains("call __rt_heap_free_safe"));
    assert!(assembly.contains("call __rt_object_free_deep"));
    assert!(
        assembly.find("call setjmp").unwrap()
            < assembly.find("_spl_type_error_class_id").unwrap()
    );
}

/// Verifies x86_64 typed adapters persist scalar string conversions and emit TypeError paths.
#[test]
fn x86_typed_wrapper_adapter_coerces_and_rejects_declared_parameters() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let mut string_signature = method_signature(vec![("count", PhpType::Str)], PhpType::Str);
    string_signature.declared_params = vec![true];
    string_signature.param_type_exprs = vec![Some(crate::parser::ast::TypeExpr::Str)];

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        8,
        2,
        "stream_read",
        "Wrapper",
        &string_signature,
        &mut data,
    );
    let string_assembly = emitter.output();
    assert!(string_assembly.contains("call __rt_itoa"));
    assert!(string_assembly.contains("call __rt_str_persist"));
    assert!(string_assembly.contains("call __rt_heap_free_safe"));

    let mut reject_emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut reject_data = crate::codegen::data_section::DataSection::new();
    let mut array_signature = method_signature(
        vec![("count", PhpType::Array(Box::new(PhpType::Mixed)))],
        PhpType::Str,
    );
    array_signature.declared_params = vec![true];
    array_signature.param_type_exprs = vec![Some(crate::parser::ast::TypeExpr::Named(
        crate::names::Name::unqualified("array"),
    ))];
    emit_user_wrapper_adapter(
        &module,
        &mut reject_emitter,
        "Wrapper",
        9,
        2,
        "stream_read",
        "Wrapper",
        &array_signature,
        &mut reject_data,
    );
    let reject_assembly = reject_emitter.output();
    let reject_literals = reject_data.emit();
    assert!(reject_assembly.contains("_spl_type_error_class_id"));
    assert!(reject_assembly.contains("jmp __rt_throw_current"));
    assert!(reject_literals.contains(
        "Wrapper::stream_read(): Argument #1 ($count) must be of type array, int given"
    ));
}

/// Verifies x86_64 metadata adapters dispatch boxed values through composite type contracts.
#[test]
fn x86_wrapper_adapter_enforces_dynamic_union_contracts() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let mut signature = method_signature(
        vec![
            ("path", PhpType::Str),
            ("option", PhpType::Int),
            (
                "value",
                PhpType::Union(vec![PhpType::Int, PhpType::Float, PhpType::Bool]),
            ),
        ],
        PhpType::Bool,
    );
    signature.declared_params = vec![true, true, true];
    signature.param_type_exprs = vec![
        Some(crate::parser::ast::TypeExpr::Str),
        Some(crate::parser::ast::TypeExpr::Int),
        Some(crate::parser::ast::TypeExpr::Union(vec![
            crate::parser::ast::TypeExpr::Int,
            crate::parser::ast::TypeExpr::Float,
            crate::parser::ast::TypeExpr::Bool,
        ])),
    ];

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        15,
        14,
        "stream_metadata",
        "Wrapper",
        &signature,
        &mut data,
    );
    let assembly = emitter.output();
    let literals = data.emit();

    assert!(assembly.contains(
        "_user_wrapper_adapter_15_stream_u_metadata_arg_3_dynamic_type_tag_1"
    ));
    assert!(assembly.contains("call __rt_str_numeric_union_kind"));
    assert!(assembly.contains("_numeric_union_bool"));
    assert!(assembly.contains("call __rt_decref_mixed"));
    assert!(assembly.contains("call __rt_decref_any"));
    assert!(assembly.contains("jmp __rt_throw_current"));
    assert!(
        !literals.contains("Implicit conversion"),
        "an exact float arm must prevent weak conversion deprecations"
    );
}

/// Verifies x86_64 string callback preflight emits the exact lossy-int diagnostic path.
#[test]
fn x86_wrapper_adapter_emits_static_string_to_int_deprecation() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let mut signature =
        method_signature(vec![("data", PhpType::Int)], PhpType::Int);
    signature.declared_params = vec![true];
    signature.param_type_exprs = vec![Some(crate::parser::ast::TypeExpr::Int)];

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        22,
        3,
        "stream_write",
        "Wrapper",
        &signature,
        &mut data,
    );
    let assembly = emitter.output();
    let literals = data.emit();

    assert!(assembly.contains("call __rt_str_numeric_union_kind"));
    assert!(assembly.contains("call __rt_str_to_number"));
    assert!(assembly.contains("cvttsd2si r10, xmm0"));
    assert!(assembly.contains("call __rt_diag_warning"));
    assert!(literals.contains(
        "Deprecated: Implicit conversion from float-string \\\""
    ));
    assert!(literals.contains("\\\" to int loses precision\\n"));
}

/// Verifies x86_64 boxed unions inspect both lossy float and float-string sources.
#[test]
fn x86_wrapper_adapter_emits_dynamic_int_union_deprecations() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let mut signature = method_signature(
        vec![
            ("path", PhpType::Str),
            ("option", PhpType::Int),
            ("value", PhpType::Union(vec![PhpType::Int, PhpType::Bool])),
        ],
        PhpType::Bool,
    );
    signature.declared_params = vec![true, true, true];
    signature.param_type_exprs = vec![
        Some(crate::parser::ast::TypeExpr::Str),
        Some(crate::parser::ast::TypeExpr::Int),
        Some(crate::parser::ast::TypeExpr::Union(vec![
            crate::parser::ast::TypeExpr::Int,
            crate::parser::ast::TypeExpr::Bool,
        ])),
    ];

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        23,
        14,
        "stream_metadata",
        "Wrapper",
        &signature,
        &mut data,
    );
    let assembly = emitter.output();
    let literals = data.emit();

    assert!(assembly.contains(
        "_user_wrapper_adapter_23_stream_u_metadata_arg_3_dynamic_float_int_deprecation"
    ));
    assert!(assembly.contains(
        "_user_wrapper_adapter_23_stream_u_metadata_arg_3_dynamic_string_int_deprecation"
    ));
    assert!(assembly.contains("movq xmm0, rdi"));
    assert!(assembly.contains("call __rt_ftoa"));
    assert!(literals.contains(
        "Deprecated: Implicit conversion from float "
    ));
    assert!(literals.contains(
        "Deprecated: Implicit conversion from float-string \\\""
    ));
}

/// Verifies x86_64 adapters call synthetic default thunks and release owned results.
#[test]
fn x86_wrapper_adapter_calls_and_releases_default_thunks() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let mut signature = method_signature(
        vec![
            ("path", PhpType::Str),
            ("mode", PhpType::Str),
            ("options", PhpType::Int),
            ("openedPath", PhpType::Mixed),
            ("extra", PhpType::Array(Box::new(PhpType::Mixed))),
        ],
        PhpType::Bool,
    );
    signature.defaults[4] = Some(Expr::new(
        ExprKind::ArrayLiteral(Vec::new()),
        Span::dummy(),
    ));

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        16,
        0,
        "stream_open",
        "Wrapper",
        &signature,
        &mut data,
    );
    let assembly = emitter.output();
    let thunk_name = crate::codegen_support::runtime::user_wrapper_default_thunk_name(
        16,
        "stream_open",
        5,
    );
    let thunk_symbol = crate::names::function_symbol(&thunk_name);

    assert!(assembly.contains(&format!("call {thunk_symbol}")));
    assert!(assembly.contains("call __rt_decref_array"));
    assert!(assembly.contains("_user_wrapper_adapter_16_stream_u_open_callback_throw"));
}

/// Verifies x86_64 adapters pack variadics and emit the exact required-extra throwable.
#[test]
fn x86_wrapper_adapter_packs_variadics_and_checks_required_arity() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut variadic_emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut variadic_data = crate::codegen::data_section::DataSection::new();
    let mut variadic_signature = method_signature(
        vec![
            ("path", PhpType::Mixed),
            ("arguments", PhpType::Array(Box::new(PhpType::Mixed))),
        ],
        PhpType::Bool,
    );
    variadic_signature.variadic = Some("arguments".to_string());

    emit_user_wrapper_adapter(
        &module,
        &mut variadic_emitter,
        "Wrapper",
        10,
        0,
        "stream_open",
        "Wrapper",
        &variadic_signature,
        &mut variadic_data,
    );
    let variadic_assembly = variadic_emitter.output();
    assert!(variadic_assembly.contains("call __rt_array_new"));
    assert!(variadic_assembly.contains("call __rt_mixed_from_value"));
    assert!(variadic_assembly.contains("call __rt_decref_array"));

    let mut arity_emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut arity_data = crate::codegen::data_section::DataSection::new();
    let arity_signature = method_signature(
        vec![
            ("path", PhpType::Mixed),
            ("mode", PhpType::Mixed),
            ("options", PhpType::Mixed),
            ("openedPath", PhpType::Mixed),
            ("required", PhpType::Mixed),
        ],
        PhpType::Bool,
    );
    emit_user_wrapper_adapter(
        &module,
        &mut arity_emitter,
        "Wrapper",
        11,
        0,
        "stream_open",
        "Wrapper",
        &arity_signature,
        &mut arity_data,
    );
    let arity_assembly = arity_emitter.output();
    let arity_literals = arity_data.emit();
    assert!(arity_assembly.contains("_spl_argument_count_error_class_id"));
    assert!(arity_assembly.contains("jmp __rt_throw_current"));
    assert!(arity_literals.contains(
        "Too few arguments to function Wrapper::stream_open(), 4 passed and exactly 5 expected"
    ));
}

/// Verifies x86_64 adapters materialize each by-reference variadic element as an alias cell.
#[test]
fn x86_wrapper_adapter_materializes_by_ref_variadic_element_cells() {
    let module = Module::new(Target::new(Platform::Linux, Arch::X86_64));
    let mut emitter = crate::codegen_support::emit::Emitter::new(module.target);
    let mut data = crate::codegen::data_section::DataSection::new();
    let mut signature = method_signature(
        vec![
            ("path", PhpType::Mixed),
            ("arguments", PhpType::Array(Box::new(PhpType::Mixed))),
        ],
        PhpType::Bool,
    );
    signature.param_type_exprs = vec![
        None,
        Some(crate::parser::ast::TypeExpr::Str),
    ];
    signature.ref_params = vec![false, true];
    signature.declared_params = vec![false, true];
    signature.variadic = Some("arguments".to_string());

    emit_user_wrapper_adapter(
        &module,
        &mut emitter,
        "Wrapper",
        21,
        0,
        "stream_open",
        "Wrapper",
        &signature,
        &mut data,
    );
    let assembly = emitter.output();
    let literals = data.emit();

    assert!(assembly.contains("wrapper_variadic_ref_cell"));
    assert!(assembly.contains("call __rt_array_new"));
    assert!(assembly.contains("call __rt_mixed_from_value"));
    assert!(assembly.contains("call __rt_decref_array"));
    assert!(assembly.contains("call __rt_decref_mixed"));
    assert!(assembly.contains("call __rt_heap_free"));
    assert!(assembly.contains(
        "_user_wrapper_adapter_21_stream_u_open_throw_cleanup_done_3"
    ));
    assert!(literals.contains(
        "Wrapper::stream_open(): Argument #2 must be passed by reference, value given"
    ));
    assert!(literals.contains(
        "Wrapper::stream_open(): Argument #3 must be passed by reference, value given"
    ));
    assert!(!literals.contains(
        "Wrapper::stream_open(): Argument #4 must be passed by reference"
    ));
}
