//! Purpose:
//! Per-identity disposition classification for the WASM capability inventory.
//!
//! Called from:
//! - `super::build_report` to turn every enumerated EIR identity into one row
//!   with exactly one `Disposition`.
//!
//! Key details:
//! - Supported/missing reuses the exhaustive `codegen_wasm::capability`
//!   classifiers; `excluded` is reserved for native-only Elephc extensions
//!   (ptr, buffer, packed, native `extern`, native bridge/system-library
//!   requirements, and the web SAPI) with a stable contract and matching CLI
//!   or capability-audit diagnostic. Ordinary PHP without a WASM lowerer is
//!   `missing`, never silently `excluded`.
#![allow(dead_code)]

use super::evidence::{op_source_producers, op_supported_evidence};
use super::schema::{
    Disposition, ExecutionMode, Exclusion, InventoryRow, ShapePredicate, SupportedEvidence,
};
use crate::builtins::registry;
use crate::builtins::semantics::BuiltinLowering;
use crate::codegen_wasm::capability::{
    runtime_function_is_supported, terminator_is_supported, terminator_name,
    unary_string_name,
};
use crate::ir::{
    IrHeapKind, IrType, Op, RuntimeCallTarget, RuntimeFnId, Terminator,
    UnaryStringRuntime,
};

/// Returns the public compilation modes that can reach the WASM capability
/// audit, whether the row is ultimately lowered or rejected.
fn public_wasm_modes() -> Vec<&'static str> {
    vec!["command", "npm"]
}

/// Returns every checked-in PHP builtin whose registry-owned lowering produces
/// the requested runtime function.
fn runtime_fn_producers(id: RuntimeFnId) -> Vec<String> {
    registry::names()
        .filter_map(registry::lookup)
        .filter(|definition| !definition.spec.internal)
        .filter_map(|definition| match definition.spec.semantics.lowering {
            BuiltinLowering::Runtime(
                RuntimeCallTarget::Function(target)
                | RuntimeCallTarget::ProfiledFunction { target, .. },
            ) if target == id => Some(format!("{}(...)", definition.name)),
            _ => None,
        })
        .collect()
}

/// Returns every checked-in PHP builtin whose registry-owned lowering produces
/// the requested unary-string runtime target.
fn unary_string_producers(target: UnaryStringRuntime) -> Vec<String> {
    registry::names()
        .filter_map(registry::lookup)
        .filter(|definition| !definition.spec.internal)
        .filter_map(|definition| match definition.spec.semantics.lowering {
            BuiltinLowering::Runtime(RuntimeCallTarget::UnaryString(candidate))
                if candidate == target =>
            {
                Some(format!("{}(...)", definition.name))
            }
            _ => None,
        })
        .collect()
}

/// Returns exact source spellings for native-only opcodes excluded from WASM.
fn excluded_op_producers(op: Op) -> Vec<String> {
    let source = match op {
        Op::PtrCast => "ptr_cast<T>(...)",
        Op::PtrRead => "ptr_read*($pointer)",
        Op::PtrWrite => "ptr_write*($pointer, $value)",
        Op::PtrReadString => "ptr_read_string($pointer, $length)",
        Op::PtrWriteString => "ptr_write_string($pointer, $value)",
        Op::PtrOffset => "ptr_offset($pointer, $offset)",
        Op::PtrCheckNonnull => "pointer dereference guard",
        Op::BufferNew => "buffer<T>(...) construction",
        Op::BufferLen => "buffer length access",
        Op::BufferGet => "buffer element read",
        Op::BufferSet => "buffer element write",
        Op::BufferFree => "buffer_free($buffer)",
        Op::PackedFieldGet => "packed-class field read",
        Op::PackedFieldSet => "packed-class field write",
        Op::ExternCall => "extern function call",
        Op::ExternGlobalLoad => "extern global read",
        Op::ExternGlobalStore => "extern global write",
        _ => return Vec::new(),
    };
    vec![source.to_string()]
}

/// Returns the exclusion contract for a native-only `Op` variant, if any.
pub(super) fn op_exclusion(op: Op) -> Option<Exclusion> {
    match op {
        Op::PtrCast
        | Op::PtrRead
        | Op::PtrWrite
        | Op::PtrReadString
        | Op::PtrWriteString
        | Op::PtrOffset
        | Op::PtrCheckNonnull => Some(Exclusion {
            category: "native-ffi-ptr",
            reason: "elephc-only raw pointer extension; not PHP-visible",
            owner: "wasm-backend",
            removal_gate: "a WASM linear-memory pointer ABI with bounds-checked lowering",
            diagnostic: format!("unsupported op {}", op.name()),
        }),
        Op::BufferNew
        | Op::BufferLen
        | Op::BufferGet
        | Op::BufferSet
        | Op::BufferFree => Some(Exclusion {
            category: "native-buffer",
            reason: "elephc-only `buffer<T>` extension; not PHP-visible",
            owner: "wasm-backend",
            removal_gate: "a WASM buffer lowering over linear memory",
            diagnostic: format!("unsupported op {}", op.name()),
        }),
        Op::PackedFieldGet | Op::PackedFieldSet => Some(Exclusion {
            category: "native-packed",
            reason: "elephc-only `packed class` extension; not PHP-visible",
            owner: "wasm-backend",
            removal_gate: "a WASM packed-field storage lowering",
            diagnostic: format!("unsupported op {}", op.name()),
        }),
        Op::ExternCall | Op::ExternGlobalLoad | Op::ExternGlobalStore => Some(Exclusion {
            category: "native-extern",
            reason: "native `extern` FFI requiring host linker libraries",
            owner: "wasm-backend",
            removal_gate: "a WASM component-model import surface and prelude rewriting",
            diagnostic: format!("unsupported op {}", op.name()),
        }),
        _ => None,
    }
}


/// Returns the exclusion contract for an Elephc-only runtime identity, if any.
pub(super) fn runtime_fn_exclusion(id: RuntimeFnId) -> Option<Exclusion> {
    if matches!(id, RuntimeFnId::Header | RuntimeFnId::HttpResponseCode) {
        return Some(Exclusion {
            category: "web-sapi",
            reason: "HTTP/web SAPI builtin requiring the --web server entry point",
            owner: "wasm-backend",
            removal_gate: "a WASI HTTP/component-model server surface",
            diagnostic: format!("unsupported runtime function {}", id.as_eir()),
        });
    }
    if matches!(
        id,
        RuntimeFnId::ElephcPtrIsNull
            | RuntimeFnId::ElephcPtrReadString
            | RuntimeFnId::ElephcPtrWriteString
            | RuntimeFnId::BufferFree
            | RuntimeFnId::BufferLen
            | RuntimeFnId::Ptr
            | RuntimeFnId::PtrGet
            | RuntimeFnId::PtrIsNull
            | RuntimeFnId::PtrNull
            | RuntimeFnId::PtrOffset
            | RuntimeFnId::PtrRead8
            | RuntimeFnId::PtrRead16
            | RuntimeFnId::PtrRead32
            | RuntimeFnId::PtrReadString
            | RuntimeFnId::PtrSet
            | RuntimeFnId::PtrSizeof
            | RuntimeFnId::PtrWrite8
            | RuntimeFnId::PtrWrite16
            | RuntimeFnId::PtrWrite32
            | RuntimeFnId::PtrWriteString
            | RuntimeFnId::ZvalFree
            | RuntimeFnId::ZvalPack
            | RuntimeFnId::ZvalType
            | RuntimeFnId::ZvalUnpack
            | RuntimeFnId::ClassAttributeArgs
            | RuntimeFnId::ClassAttributeNames
            | RuntimeFnId::ClassGetAttributes
    ) {
        return Some(Exclusion {
            category: "elephc-native-extension",
            reason: "Elephc-only native pointer/buffer/zval/attribute extension; not PHP-visible",
            owner: "wasm-backend",
            removal_gate: "an explicit WASM extension ABI with bounds-checked linear-memory semantics",
            diagnostic: format!("unsupported runtime function {}", id.as_eir()),
        });
    }
    None
}


/// Returns the evidence record for a supported `RuntimeFnId`, if it is supported.
pub(super) fn runtime_fn_supported_evidence(id: RuntimeFnId) -> Option<SupportedEvidence> {
    if !runtime_function_is_supported(id) {
        return None;
    }
    let (backend, lowerer, tests) = match id {
        RuntimeFnId::GetClass => (
            "codegen_wasm::classes",
            "codegen_wasm::classes::lower_get_class",
            &["codegen_wasm::tests::get_class_object_returns_class_name"][..],
        ),
        RuntimeFnId::ArrayMap => (
            "codegen_wasm::inst",
            "codegen_wasm::inst::lower_array_map",
            &["codegen_wasm::closures::tests::array_map_lowering_via_builtin_call_returns_4220"]
                [..],
        ),
        RuntimeFnId::Round => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::direct_builtins_admit_only_the_storage_they_lower",
                "codegen::cli::test_cli_wasm_round_and_radix_conversions_match_php",
            ][..],
        ),
        RuntimeFnId::ArraySearch => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::in_array_admits_only_the_pairs_whose_rule_was_measured",
                "codegen::cli::test_cli_wasm_array_search_matches_php",
            ][..],
        ),
        RuntimeFnId::ArrayKeyExists => (
            "codegen_wasm::inst_hash",
            "codegen_wasm::inst_hash::lower_array_key_exists",
            &[
                "codegen_wasm::tests::hash_isset_and_array_key_exists_lower",
                "codegen::cli::test_cli_wasm_assoc_foreach_and_key_tests_match_php",
            ][..],
        ),
        RuntimeFnId::Sort | RuntimeFnId::Rsort => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::scalar_sorts_admit_only_orderable_elements",
                "codegen::cli::test_cli_wasm_scalar_sorts_match_php",
            ][..],
        ),
        RuntimeFnId::Range => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::range_admits_only_two_integer_bounds",
                "codegen::cli::test_cli_wasm_range_matches_php",
            ][..],
        ),
        RuntimeFnId::ArrayMerge => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::array_merge_admits_only_agreeing_element_storage",
                "codegen::cli::test_cli_wasm_array_merge_matches_php",
            ][..],
        ),
        RuntimeFnId::ArraySlice => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::array_slice_admits_its_lowered_windows_only",
                "codegen::cli::test_cli_wasm_array_slice_matches_php",
            ][..],
        ),
        RuntimeFnId::Usort => (
            "codegen_wasm::inst",
            "codegen_wasm::inst::lower_user_sort",
            &["codegen_wasm::closures::tests::usort_lowering_writes_back_to_local"][..],
        ),
        RuntimeFnId::ArrayReduce => (
            "codegen_wasm::inst",
            "codegen_wasm::inst::lower_array_reduce",
            &["codegen_wasm::closures::tests::array_reduce_lowering_boxes_mixed_result"][..],
        ),
        RuntimeFnId::Abs | RuntimeFnId::Floor | RuntimeFnId::Ceil | RuntimeFnId::Sqrt
        | RuntimeFnId::Count | RuntimeFnId::ArrayIsList | RuntimeFnId::ArrayKeys
        | RuntimeFnId::ArrayValues | RuntimeFnId::InArray | RuntimeFnId::ArrayReverse
        | RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct | RuntimeFnId::Max
        | RuntimeFnId::Min | RuntimeFnId::Intdiv | RuntimeFnId::ArrayFill
        | RuntimeFnId::StrContains | RuntimeFnId::StrStartsWith
        | RuntimeFnId::StrEndsWith => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::direct_builtins_admit_only_the_storage_they_lower",
                "codegen_wasm::builtins::tests::in_array_admits_only_the_pairs_whose_rule_was_measured",
                "codegen::cli::test_cli_wasm_in_array_matches_php",
                "codegen::cli::test_cli_wasm_direct_builtins_match_php",
            ][..],
        ),
        RuntimeFnId::Ucfirst
        | RuntimeFnId::Lcfirst
        | RuntimeFnId::Ucwords
        | RuntimeFnId::Strcmp
        | RuntimeFnId::Strcasecmp
        | RuntimeFnId::Trim
        | RuntimeFnId::Ltrim
        | RuntimeFnId::Rtrim
        | RuntimeFnId::Substr
        | RuntimeFnId::StrRepeat
        | RuntimeFnId::StrPad
        | RuntimeFnId::StrReplace
        | RuntimeFnId::Crc32
        | RuntimeFnId::Sha1
        | RuntimeFnId::Md5
        | RuntimeFnId::Htmlspecialchars
        | RuntimeFnId::Implode
        | RuntimeFnId::Explode
        | RuntimeFnId::StrSplit
        | RuntimeFnId::Wordwrap
        | RuntimeFnId::Sprintf
        | RuntimeFnId::Printf => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::string_shaping_builtins_admit_only_their_arities",
                "codegen::cli::test_cli_wasm_string_shaping_builtins_match_php",
                "codegen::cli::test_cli_wasm_str_repeat_matches_php_and_raises_its_value_error",
                "codegen::cli::test_cli_wasm_str_pad_matches_php",
                "codegen::cli::test_cli_wasm_str_replace_and_crc32_match_php",
                "codegen::cli::test_cli_wasm_sha1_matches_php",
                "codegen::cli::test_cli_wasm_md5_matches_php",
                "codegen::cli::test_cli_wasm_htmlspecialchars_matches_php",
                "codegen::cli::test_cli_wasm_implode_matches_php",
                "codegen::cli::test_cli_wasm_explode_matches_php",
                "codegen::cli::test_cli_wasm_str_split_matches_php",
                "codegen::cli::test_cli_wasm_wordwrap_matches_php",
                "codegen_wasm::builtins::tests::sprintf_format_parser_follows_php_flag_rules",
                "codegen::cli::test_cli_wasm_sprintf_matches_php",
                "codegen::cli::test_cli_wasm_sprintf_float_matches_php",
                "codegen::cli::test_cli_wasm_printf_matches_php",
            ][..],
        ),
        RuntimeFnId::Strpos | RuntimeFnId::Strrpos | RuntimeFnId::Strstr => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::string_shaping_builtins_admit_only_their_arities",
                "codegen::cli::test_cli_wasm_strpos_and_tagged_strict_equality_match_php",
                "codegen::cli::test_cli_wasm_strstr_matches_php",
                "codegen::cli::test_cli_wasm_strrpos_matches_php",
            ][..],
        ),
        RuntimeFnId::Chr | RuntimeFnId::Ord => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::chr_and_ord_admit_only_their_concrete_scalar",
                "codegen::cli::test_cli_wasm_chr_and_ord_match_php",
            ][..],
        ),
        _ => return None,
    };
    Some(SupportedEvidence {
        backend,
        lowerer,
        tests,
    })
}


/// Classifies one `Op` into an inventory row with exactly one disposition.
pub(super) fn op_row(op: Op) -> InventoryRow {
    let name = op.name().to_string();
    if let Some(exclusion) = op_exclusion(op) {
        return InventoryRow {
            name,
            family: "op",
            enum_name: "Op",
            disposition: Disposition::Excluded,
            producers: excluded_op_producers(op),
            execution_modes: public_wasm_modes(),
            evidence_gaps: Vec::new(),
            supported: None,
            excluded: Some(exclusion),
            missing: None,
        };
    }
    if let Some(evidence) = op_supported_evidence(op) {
        return InventoryRow {
            name,
            family: "op",
            enum_name: "Op",
            disposition: Disposition::Supported,
            producers: op_source_producers(op)
                .iter()
                .map(|producer| (*producer).to_string())
                .collect(),
            execution_modes: public_wasm_modes(),
            evidence_gaps: Vec::new(),
            supported: Some(evidence),
            excluded: None,
            missing: None,
        };
    }
    InventoryRow {
        name,
        family: "op",
        enum_name: "Op",
        disposition: Disposition::Missing,
        producers: op_source_producers(op)
            .iter()
            .map(|producer| (*producer).to_string())
            .collect(),
        execution_modes: public_wasm_modes(),
        evidence_gaps: Vec::new(),
        supported: None,
        excluded: None,
        missing: Some("ordinary PHP reachable from the public frontend; WASM lowerer absent"),
    }
}


/// Classifies one `RuntimeFnId` into an inventory row with exactly one disposition.
pub(super) fn runtime_fn_row(id: RuntimeFnId) -> InventoryRow {
    let name = id.as_eir().to_string();
    let producers = runtime_fn_producers(id);
    if let Some(exclusion) = runtime_fn_exclusion(id) {
        return InventoryRow {
            name,
            family: "runtime_fn",
            enum_name: "RuntimeFnId",
            disposition: Disposition::Excluded,
            producers,
            execution_modes: public_wasm_modes(),
            evidence_gaps: Vec::new(),
            supported: None,
            excluded: Some(exclusion),
            missing: None,
        };
    }
    if let Some(evidence) = runtime_fn_supported_evidence(id) {
        return InventoryRow {
            name,
            family: "runtime_fn",
            enum_name: "RuntimeFnId",
            disposition: Disposition::Supported,
            producers,
            execution_modes: public_wasm_modes(),
            evidence_gaps: Vec::new(),
            supported: Some(evidence),
            excluded: None,
            missing: None,
        };
    }
    InventoryRow {
        name,
        family: "runtime_fn",
        enum_name: "RuntimeFnId",
        disposition: Disposition::Missing,
        producers,
        execution_modes: public_wasm_modes(),
        evidence_gaps: Vec::new(),
        supported: None,
        excluded: None,
        missing: Some("ordinary PHP builtin reachable from the public frontend; WASM lowerer absent"),
    }
}


/// Classifies one `UnaryStringRuntime` into an inventory row with exactly one disposition.
pub(super) fn unary_string_row(target: UnaryStringRuntime) -> InventoryRow {
    if crate::codegen_wasm::builtins::unary_string_is_supported(target) {
        let mut row = InventoryRow {
            name: unary_string_name(target).to_string(),
            family: "unary_string",
            enum_name: "UnaryStringRuntime",
            disposition: Disposition::Supported,
            producers: unary_string_producers(target),
            execution_modes: public_wasm_modes(),
            evidence_gaps: Vec::new(),
            supported: Some(SupportedEvidence {
                backend: "codegen_wasm::builtins",
                lowerer: "codegen_wasm::builtins::lower_unary_string",
                tests: &[
                    "codegen_wasm::builtins::tests::unary_string_transforms_admit_only_strings",
                    "codegen::cli::test_cli_wasm_unary_string_transforms_match_php",
                ],
            }),
            excluded: None,
            missing: None,
        };
        row.evidence_gaps = Vec::new();
        return row;
    }
    InventoryRow {
        name: unary_string_name(target).to_string(),
        family: "unary_string",
        enum_name: "UnaryStringRuntime",
        disposition: Disposition::Missing,
        producers: unary_string_producers(target),
        execution_modes: public_wasm_modes(),
        evidence_gaps: Vec::new(),
        supported: None,
        excluded: None,
        missing: Some(
            "ordinary PHP string transform reachable from the public frontend; WASM lowerer absent",
        ),
    }
}

/// Returns the PHP/control-flow producer represented by one terminator form.
fn terminator_producers(terminator: &Terminator) -> Vec<String> {
    let producer = match terminator {
        Terminator::Br { .. } => "unconditional control-flow edge",
        Terminator::CondBr { .. } => "if/loop conditional control flow",
        Terminator::Switch { .. } => "switch/match dispatch",
        Terminator::Return { .. } => "return statement or implicit function return",
        Terminator::Throw { .. } => "throw expression",
        Terminator::Fatal { .. } => "fatal PHP runtime error",
        Terminator::GeneratorSuspend { .. } => "yield/yield from suspension",
        Terminator::Unreachable => "statically unreachable CFG tail",
    };
    vec![producer.to_string()]
}

/// Returns tests that lower and validate the exact terminator form.
fn terminator_tests(terminator: &Terminator) -> &'static [&'static str] {
    match terminator {
        Terminator::Br { .. } => {
            &["codegen_wasm::tests::br_with_args_lowers_to_valid_wasm"]
        }
        Terminator::CondBr { .. } => {
            &["codegen_wasm::tests::main_condbr_lowers_to_valid_wasm"]
        }
        Terminator::Switch { .. } => {
            &["codegen_wasm::tests::switch_lowers_to_valid_wasm"]
        }
        Terminator::Return { .. } => {
            &["codegen_wasm::tests::echo_integers_writes_to_stdout"]
        }
        Terminator::Unreachable => {
            &["codegen_wasm::tests::main_command_has_a_complete_unreachable_inventory"]
        }
        Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::GeneratorSuspend { .. } => &[],
    }
}


/// Classifies one terminator kind into an inventory row with exactly one disposition.
pub(super) fn terminator_row(terminator: &Terminator) -> InventoryRow {
    let name = terminator_name(terminator).to_string();
    if terminator_is_supported(terminator) {
        InventoryRow {
            name,
            family: "terminator",
            enum_name: "Terminator",
            disposition: Disposition::Supported,
            producers: terminator_producers(terminator),
            execution_modes: public_wasm_modes(),
            evidence_gaps: Vec::new(),
            supported: Some(SupportedEvidence {
                backend: "codegen_wasm::function",
                lowerer: "codegen_wasm::function::lower_terminator",
                tests: terminator_tests(terminator),
            }),
            excluded: None,
            missing: None,
        }
    } else {
        InventoryRow {
            name,
            family: "terminator",
            enum_name: "Terminator",
            disposition: Disposition::Missing,
            producers: terminator_producers(terminator),
            execution_modes: public_wasm_modes(),
            evidence_gaps: Vec::new(),
            supported: None,
            excluded: None,
            missing: Some("ordinary PHP control-flow terminator; WASM lowerer absent"),
        }
    }
}


/// Classifies the four `RuntimeCallTarget` forms into inventory rows.
pub(super) fn runtime_call_target_rows() -> Vec<InventoryRow> {
    let form = |name: &'static str,
                disposition: Disposition,
                producers: Vec<String>,
                supported: Option<SupportedEvidence>,
                missing: Option<&'static str>| InventoryRow {
        name: name.to_string(),
        family: "runtime_call_target",
        enum_name: "RuntimeCallTarget",
        disposition,
        producers,
        execution_modes: public_wasm_modes(),
        evidence_gaps: Vec::new(),
        supported,
        excluded: None,
        missing,
    };
    vec![
        form(
            "array.fetch_for_write",
            Disposition::Missing,
            vec!["nested array write through an intermediate offset".to_string()],
            None,
            Some(
                "ordinary PHP nested-array write helper; WASM lowerer absent",
            ),
        ),
        form(
            "unary_string",
            Disposition::Missing,
            registry::names()
                .filter_map(registry::lookup)
                .filter(|definition| !definition.spec.internal)
                .filter(|definition| {
                    matches!(
                        definition.spec.semantics.lowering,
                        BuiltinLowering::Runtime(RuntimeCallTarget::UnaryString(_))
                    )
                })
                .map(|definition| format!("{}(...)", definition.name))
                .collect(),
            None,
            Some(
                "ordinary PHP string transform dispatch form; WASM lowerer absent (see unary_string family)",
            ),
        ),
        form(
            "function",
            Disposition::Supported,
            registry::names()
                .filter_map(registry::lookup)
                .filter(|definition| !definition.spec.internal)
                .filter(|definition| {
                    matches!(
                        definition.spec.semantics.lowering,
                        BuiltinLowering::Runtime(RuntimeCallTarget::Function(_))
                    )
                })
                .map(|definition| format!("{}(...)", definition.name))
                .collect(),
            Some(SupportedEvidence {
                backend: "codegen_wasm::inst",
                lowerer: "codegen_wasm::inst::lower_runtime_call",
                tests: &[
                    "codegen_wasm::closures::tests::array_map_lowering_via_builtin_call_returns_4220",
                ],
            }),
            None,
        ),
        form(
            "profiled_function",
            Disposition::Supported,
            registry::names()
                .filter_map(registry::lookup)
                .filter(|definition| !definition.spec.internal)
                .filter(|definition| {
                    matches!(
                        definition.spec.semantics.lowering,
                        BuiltinLowering::Runtime(RuntimeCallTarget::ProfiledFunction { .. })
                    )
                })
                .map(|definition| format!("{}(...)", definition.name))
                .collect(),
            Some(SupportedEvidence {
                backend: "codegen_wasm::inst",
                lowerer: "codegen_wasm::inst::lower_runtime_call",
                tests: &["codegen_wasm::tests::get_class_object_returns_class_name"],
            }),
            None,
        ),
    ]
}


/// Returns the shape predicates enforced before WAT staging.
pub(super) fn shape_predicates() -> Vec<ShapePredicate> {
    [
        "terminator_transfer_shape_issue",
        "method_call_on_null_error_shape_issue",
        "array_offset_on_null_warning_shape_issue",
        "unset_owned_temp_shape_issue",
        "first_class_callable_new_shape_issue",
        "checked_int_binop_shape_issue",
        "value_transfer_shape_issue",
        "call_result_shape_issue",
        "property_write_shape_issue",
        "throwable_constructor_shape_issue",
        "throwable_intrinsic_shape_issue",
        "local_transfer_shape_issue",
        "load_global_shape_issue",
        "store_ref_cell_shape_issue",
        "forward_transfer_shape_issue",
        "cast_shape_issue",
        "int_like_to_string_shape_issue",
        "strict_compare_shape_issue",
        "truthiness_shape_issue",
        "array_store_shape_issue",
        "iter_start_shape_issue",
        "iter_current_value_ref_shape_issue",
        "array_get_shape_issue",
        "hash_get_shape_issue",
        "hash_key_diagnostic_issue",
        "hash_store_value_diagnostic_issue",
        "array_to_hash_shape_issue",
        "array_to_mixed_shape_issue",
        "loose_eq_shape_issue",
        "direct_call_shape_issue",
        "by_ref_source_shape_issue",
        "method_call_shape_issue",
        "object_new_shape_issue",
        "property_get_shape_issue",
        "property_set_shape_issue",
        "static_method_call_shape_issue",
        "static_property_shape_issue",
        "scoped_constant_shape_issue",
        "method_signature_shape_issue",
        "method_body_signature_shape_issue",
        "method_body_argument_shape_issue",
        "direct_method_result_shape_issue",
        "mixed_method_issue",
        "runtime_function_shape_issue",
        "get_class_shape_issue",
        "array_map_shape_issue",
        "usort_shape_issue",
        "array_reduce_shape_issue",
        "closure_call_shape_issue",
        "closure_new_by_ref_capture_issue",
        "callable_argument_contract_issue",
        "callable_result_contract_issue",
        "callable_wrapper_issue",
        "callable_wrapper_signature_issue",
        "callable_descriptor_invoke_shape_issue",
        "closure_result_shape_issue",
        "iterator_alias_mutation_issue",
    ]
    .into_iter()
    .map(|name| ShapePredicate {
        name,
        disposition: "enforced",
    })
    .collect()
}

/// Returns one inventory row for a concrete EIR storage type.
pub(super) fn ir_type_row(ir_type: IrType) -> InventoryRow {
    let producers = match ir_type {
        IrType::I64 => vec!["int/bool/callable/pointer/resource storage".to_string()],
        IrType::F64 => vec!["float storage".to_string()],
        IrType::Str => vec!["string storage".to_string()],
        IrType::TaggedScalar => vec!["tagged nullable-scalar storage".to_string()],
        IrType::Heap(IrHeapKind::Array) => vec!["indexed array storage".to_string()],
        IrType::Heap(IrHeapKind::Hash) => vec!["associative array storage".to_string()],
        IrType::Heap(IrHeapKind::Object) => vec!["object/packed-object storage".to_string()],
        IrType::Heap(IrHeapKind::Mixed) => vec!["mixed value storage".to_string()],
        IrType::Heap(IrHeapKind::Iterable) => vec!["iterable value storage".to_string()],
        IrType::Heap(IrHeapKind::Union) => vec!["runtime union storage".to_string()],
        IrType::Heap(IrHeapKind::Buffer) => vec!["buffer<T> storage".to_string()],
        IrType::Void => vec!["void/never result storage".to_string()],
    };
    if ir_type == IrType::Heap(IrHeapKind::Buffer) {
        return InventoryRow {
            name: ir_type.as_eir(),
            family: "ir_type",
            enum_name: "IrType",
            disposition: Disposition::Excluded,
            producers,
            execution_modes: public_wasm_modes(),
            evidence_gaps: Vec::new(),
            supported: None,
            excluded: Some(Exclusion {
                category: "native-buffer",
                reason: "elephc-only `buffer<T>` storage is not admitted by the WASM ABI",
                owner: "wasm-backend",
                removal_gate: "a WASM buffer lowering over linear memory",
                diagnostic: "unsupported storage type Heap(Buffer)".to_string(),
            }),
            missing: None,
        };
    }
    let tests = match ir_type {
        IrType::I64 => &["codegen_wasm::tests::echo_integers_writes_to_stdout"][..],
        IrType::F64 => &["codegen_wasm::tests::echo_float_writes_to_stdout"][..],
        IrType::Str => &["codegen_wasm::tests::echo_string_literal_writes_to_stdout"][..],
        IrType::Heap(IrHeapKind::Object) => {
            &["codegen_wasm::tests::get_class_object_returns_class_name"][..]
        }
        IrType::Heap(IrHeapKind::Mixed) => {
            &["codegen_wasm::tests::echo_mixed_float_writes_to_stdout"][..]
        }
        IrType::TaggedScalar
        | IrType::Heap(IrHeapKind::Array)
        | IrType::Heap(IrHeapKind::Hash)
        | IrType::Heap(IrHeapKind::Iterable)
        | IrType::Heap(IrHeapKind::Union)
        | IrType::Heap(IrHeapKind::Buffer)
        | IrType::Void => &[],
    };
    InventoryRow {
        name: ir_type.as_eir(),
        family: "ir_type",
        enum_name: "IrType",
        disposition: Disposition::Supported,
        producers,
        execution_modes: public_wasm_modes(),
        evidence_gaps: Vec::new(),
        supported: Some(SupportedEvidence {
            backend: "codegen_wasm::values",
            lowerer: "codegen_wasm::values::WasmRepr::val_types",
            tests,
        }),
        excluded: None,
        missing: None,
    }
}

/// Returns every concrete EIR storage form, expanding `Heap` by heap subkind.
pub(super) fn ir_type_representatives() -> Vec<IrType> {
    vec![
        IrType::I64,
        IrType::F64,
        IrType::Str,
        IrType::TaggedScalar,
        IrType::Heap(IrHeapKind::Array),
        IrType::Heap(IrHeapKind::Hash),
        IrType::Heap(IrHeapKind::Object),
        IrType::Heap(IrHeapKind::Mixed),
        IrType::Heap(IrHeapKind::Iterable),
        IrType::Heap(IrHeapKind::Union),
        IrType::Heap(IrHeapKind::Buffer),
        IrType::Void,
    ]
}


/// Returns the public execution modes that can reach the WASM backend.
pub(super) fn execution_modes() -> Vec<ExecutionMode> {
    vec![
        ExecutionMode {
            mode: "command",
            reachable: true,
        },
        ExecutionMode {
            mode: "npm",
            reachable: true,
        },
    ]
}


/// Returns one representative `Terminator` per variant kind for enumeration.
pub(super) fn terminator_representatives() -> Vec<Terminator> {
    use crate::ir::{BlockId, DataId, ValueId};
    let dummy_block = BlockId::from_raw(0);
    let dummy_value = ValueId::from_raw(0);
    let dummy_data = DataId::from_raw(0);
    vec![
        Terminator::Br {
            target: dummy_block,
            args: Vec::new(),
        },
        Terminator::CondBr {
            cond: dummy_value,
            then_target: dummy_block,
            then_args: Vec::new(),
            else_target: dummy_block,
            else_args: Vec::new(),
        },
        Terminator::Switch {
            scrutinee: dummy_value,
            cases: Vec::new(),
            default: dummy_block,
            default_args: Vec::new(),
        },
        Terminator::Return { value: None },
        Terminator::Throw { value: dummy_value },
        Terminator::Fatal { message: dummy_data },
        Terminator::GeneratorSuspend {
            key: None,
            value: None,
            resume: dummy_block,
            resume_args: Vec::new(),
        },
        Terminator::Unreachable,
    ]
}
