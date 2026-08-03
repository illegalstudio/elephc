//! Purpose:
//! Producer/test evidence tables for supported `Op` variants in the WASM
//! capability inventory.
//!
//! Called from:
//! - `super::classify::op_row` to attach backend lowerer and test evidence to
//!   each supported `Op` row.
//!
//! Key details:
//! - Evidence is grouped by lowering family so the supported `Op` variants
//!   share compact, maintainable records instead of one row per producer.
#![allow(dead_code)]

use super::schema::SupportedEvidence;
use crate::codegen_wasm::capability::op_is_supported;
use crate::ir::Op;

/// Returns the evidence record for a supported `Op` variant, if it is supported.
pub(super) fn op_supported_evidence(op: Op) -> Option<SupportedEvidence> {
    if !op_is_supported(op) {
        return None;
    }
    let group = op_evidence_group(op);
    let mut evidence = supported_evidence_for_group(group);
    evidence.lowerer = op_lowerer(op);
    evidence.tests = op_tests(op);
    Some(evidence)
}

/// Returns PHP-source producer descriptions for one opcode without inventing
/// a generic fallback when the source mapping has not been audited yet.
pub(super) fn op_source_producers(op: Op) -> &'static [&'static str] {
    match op {
        Op::ConstI64 => &["integer literal"],
        Op::ConstF64 => &["float literal"],
        Op::ConstStr => &["string literal"],
        Op::ConstNull => &["null literal"],
        Op::ConstBool => &["true/false literal"],
        Op::LoadLocal => &["local variable read"],
        Op::StoreLocal => &["local variable assignment"],
        Op::UnsetLocal => &["unset($local)"],
        Op::LoadRefCell => &["read through a PHP reference"],
        Op::StoreRefCell => &["assignment through a PHP reference"],
        Op::PromoteLocalRefCell => &["first by-reference use of a local"],
        Op::AliasLocalRefCell => &["reference assignment (`$a =& $b`)"],
        Op::ReleaseLocalRefCell => &["scope exit for a referenced local"],
        Op::LoadGlobal => &["superglobal/global variable read"],
        Op::IAdd | Op::ICheckedAdd => &["integer `+`"],
        Op::ISub | Op::ICheckedSub => &["integer `-`"],
        Op::IMul | Op::ICheckedMul => &["integer `*`"],
        Op::MixedNumericBinop => &["`+`, `-`, or `*` on operands typed only at runtime"],
        Op::IDiv => &["integer operands to PHP `/`"],
        Op::ISDiv => &["integer `intdiv()`"],
        Op::ISMod => &["integer `%`"],
        Op::INeg => &["unary integer `-`"],
        Op::IBitAnd => &["integer `&`"],
        Op::IBitOr => &["integer `|`"],
        Op::IBitXor => &["integer `^`"],
        Op::IBitNot => &["integer `~`"],
        Op::IShl => &["integer `<<`"],
        Op::IShrA => &["integer `>>`"],
        Op::FAdd => &["float `+`"],
        Op::FSub => &["float `-`"],
        Op::FMul => &["float `*`"],
        Op::FDiv => &["float `/`"],
        Op::FNeg => &["unary float `-`"],
        Op::ICmp => &["integer comparison"],
        Op::LooseEq => &["loose equality (`==`)"],
        Op::LooseNotEq => &["loose inequality (`!=`, `<>`)"],
        Op::FCmp => &["float comparison"],
        Op::StrictEq => &["strict equality (`===`)", "`match` arm selection"],
        Op::StrictNotEq => &["strict inequality (`!==`)"],
        Op::IsNull => &["null comparison or `is_null()` lowering"],
        Op::IsTruthy => &["condition/boolean-context truthiness"],
        Op::InstanceOf => &["`instanceof` with a statically resolved class"],
        Op::IToF => &["implicit integer-to-float representation conversion"],
        Op::IToStr => &["integer or boolean string coercion"],
        Op::Cast => &["explicit or compiler-required scalar cast"],
        Op::MixedBox => &["boxing a concrete PHP value into Mixed"],
        Op::MixedTagOf => &["runtime type inspection of a Mixed value"],
        Op::StrConcat => &["string concatenation (`.`)"],
        Op::StrLen => &["`strlen()`"],
        Op::StrPersist => &["returning a string from a function", "storing a computed string"],
        Op::ConcatReset => &["reset of a compiler-managed concatenation chain"],
        Op::ArrayNew => &["indexed array literal"],
        Op::ArrayLen => &["`count()` on an indexed array"],
        Op::ArrayGet => &["indexed array offset read"],
        Op::ArrayGetSilent => &["silent indexed array offset read"],
        Op::ArraySet => &["indexed array offset assignment"],
        Op::ArrayPush => &["indexed array append (`$array[] = ...`)"],
        Op::ArrayToHash => &["indexed-to-associative array promotion"],
        Op::ArrayUnion => &["indexed array union (`+`)"],
        Op::HashNew => &["associative array literal"],
        Op::HashGet => &["associative array offset read"],
        Op::HashGetSilent => &["silent associative array offset read"],
        Op::HashSet => &["associative array offset assignment"],
        Op::HashUnset => &["unset of an associative array key"],
        Op::HashIsset => &["isset($h[k]) over an associative array"],
        Op::HashAppend => &["append to an associative array"],
        Op::HashUnion => &["associative array union (`+`)"],
        Op::ArrayHashUnion => &["indexed-left/associative-right array union"],
        Op::HashArrayUnion => &["associative-left/indexed-right array union"],
        Op::IterStart => &["`foreach` initialization"],
        Op::IterCurrentKey => &["`foreach` key binding"],
        Op::IterCurrentValue => &["`foreach` value binding"],
        Op::IterCurrentValueRef => &["by-reference `foreach` value binding"],
        Op::IterNext => &["`foreach` loop advance"],
        Op::IterEnd => &["`foreach` loop completion"],
        Op::ObjectNew => &["object construction (`new ClassName(...)`)"],
        Op::PropGet => &["object property read"],
        Op::PropSet => &["object property assignment"],
        Op::NullsafePropGet => &["nullsafe property read (`?->property`)"],
        Op::MethodCall => &["instance method call"],
        Op::StaticMethodCall => &["static method call"],
        Op::NullsafeMethodCall => &["nullsafe method call (`?->method(...)`)"],
        Op::InstanceOfDynamic => &["`instanceof` with a runtime target"],
        Op::Call => &["user-defined function call"],
        Op::LanguageConstructCall => &["`exit`/`die` language construct"],
        Op::RuntimeCall => &["registry-backed PHP builtin call"],
        Op::ClosureNew => &["closure literal"],
        Op::ClosureCapture => &["closure `use (...)` capture"],
        Op::ClosureCall => &["closure invocation"],
        Op::FirstClassCallableNew => &["first-class callable syntax (`foo(...)`)"],
        Op::CallableDescriptorInvoke => &["runtime callable invocation"],
        Op::EchoValue => &["`echo`"],
        Op::PrintValue => &["`print`"],
        Op::Warn => &["PHP warning emitted by an admitted operation"],
        Op::ThrowError => &["PHP fatal `Error` emitted by an admitted operation"],
        Op::Acquire => &["compiler-inserted retain of a refcounted PHP value"],
        Op::Release => &["compiler-inserted release of a refcounted PHP value"],
        Op::GcCollect => &["the cycle-collection safe point unset(...) emits"],
        Op::LoadStaticProperty => &["Class::$prop read"],
        Op::ScopedConstantGet => &["Enum::Case read"],
        Op::StoreStaticProperty => &["Class::$prop = ... assignment"],
        Op::Move => &["compiler-inserted ownership move"],
        Op::Borrow => &["compiler-inserted ownership borrow"],
        Op::Nop => &["compiler-inserted no-op"],
        Op::TryPushHandler => &["entering a `try` block"],
        Op::TryPopHandler => &["leaving a `try` block"],
        Op::ThrowException => &["`throw`"],
        Op::ThrowErrorValue => &["`throw` of an `Error` instance"],
        Op::CatchCurrent => &["matching a `catch` clause against the exception in flight"],
        Op::CatchBind => &["binding the caught exception to a `catch` variable"],
        _ => &[],
    }
}

/// Returns tests that actually emit and lower the requested opcode.
fn op_tests(op: Op) -> &'static [&'static str] {
    match op {
        Op::ConstI64 => &["codegen_wasm::tests::echo_integers_writes_to_stdout"],
        Op::ConstF64 => &["codegen_wasm::tests::echo_float_writes_to_stdout"],
        Op::ConstStr => &["codegen_wasm::tests::echo_string_literal_writes_to_stdout"],
        Op::ConstBool => &["codegen_wasm::tests::echo_booleans_writes_to_stdout"],
        Op::LoadLocal | Op::StoreLocal | Op::PromoteLocalRefCell => {
            &["codegen_wasm::tests::ref_cell_promotion_is_runtime_idempotent_across_branches"]
        }
        Op::LoadRefCell | Op::Acquire | Op::Release => {
            &["codegen_wasm::tests::acquired_ref_cell_return_survives_owner_epilogue"]
        }
        Op::GcCollect => &[
            "codegen_wasm::gc::tests::collect_cycles_reclaims_a_two_block_cycle",
            "codegen::cli::test_cli_wasm_unset_collects_reference_cycles",
        ],
        Op::LoadStaticProperty | Op::StoreStaticProperty => &[
            "codegen_wasm::statics::tests::inherited_statics_share_one_slot",
            "codegen::cli::test_cli_wasm_static_properties_match_php",
        ],
        Op::ScopedConstantGet => &[
            "codegen_wasm::statics::tests::enum_cases_get_one_singleton_slot_each",
            "codegen::cli::test_cli_wasm_enums_match_php",
        ],
        Op::StoreRefCell | Op::AliasLocalRefCell => {
            &["codegen_wasm::tests::ref_cell_alias_string_store_e2e"]
        }
        Op::LoadGlobal => &["codegen_wasm::tests::argc_reports_argument_count"],
        Op::IAdd | Op::ISub | Op::IMul | Op::IBitAnd | Op::ISDiv | Op::ISMod => {
            &["codegen_wasm::tests::int_arithmetic_invokes_correctly"]
        }
        Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul => {
            &["codegen_wasm::tests::checked_integer_arithmetic_promotes_overflow_to_float"]
        }
        Op::MixedNumericBinop => &[
            "codegen_wasm::tests::mixed_numeric_binop_matches_php_result_typing",
            "codegen::cli::test_cli_wasm_mixed_numeric_arithmetic_matches_php",
            "codegen::cli::test_cli_wasm_mixed_numeric_string_diagnostics",
        ],
        Op::IShl | Op::IShrA => {
            &["codegen_wasm::tests::integer_shifts_match_php_at_word_boundaries"]
        }
        Op::IDiv => &["codegen_wasm::tests::php_division_returns_float"],
        Op::ICmp => &["codegen_wasm::tests::int_compare_invokes_correctly"],
        Op::LooseEq | Op::LooseNotEq => &[
            "codegen_wasm::capability::tests::loose_equality_admits_only_measured_pairs",
            "codegen::cli::test_cli_wasm_loose_equality_matches_php",
        ],
        Op::StrictEq | Op::StrictNotEq => {
            &[
                "codegen_wasm::strict::tests::strict_scalar_equality_opcodes_lower_and_run",
                "codegen::cli::test_cli_wasm_strict_equality_executes_supported_profiles",
            ]
        }
        Op::IToStr => {
            &["codegen_wasm::tests::integer_and_boolean_to_string_lowering_matches_php"]
        }
        Op::IsNull => {
            &["codegen_wasm::tests::integer_null_sentinel_value_is_not_misclassified"]
        }
        Op::FMul => &["codegen_wasm::tests::closure_capture_float_e2e"],
        Op::MixedBox => &["codegen_wasm::tests::echo_mixed_float_writes_to_stdout"],
        Op::MixedTagOf => {
            &["codegen_wasm::tests::checked_integer_arithmetic_promotes_overflow_to_float"]
        }
        Op::StrConcat => &["codegen_wasm::tests::chained_concat_echoes_correctly"],
        Op::StrLen => &["codegen_wasm::tests::strlen_of_literal_invokes_correctly"],
        Op::StrPersist => &[
            "codegen_wasm::tests::str_persist_copies_a_literal_into_owned_heap_bytes",
            "codegen::cli::test_cli_wasm_chr_and_ord_match_php",
        ],
        Op::ArrayNew | Op::ArrayPush | Op::ArrayGet => {
            &["codegen_wasm::tests::array_new_push_get_lowers"]
        }
        Op::ArrayLen => &["codegen_wasm::tests::array_len_lowers"],
        Op::ArraySet => &["codegen_wasm::tests::array_set_overwrite_lowers"],
        Op::ArrayUnion => &["codegen_wasm::tests::array_union_lowers"],
        Op::ArrayHashUnion => &["codegen_wasm::tests::array_hash_union_lowers"],
        Op::HashArrayUnion => &["codegen_wasm::tests::hash_array_union_lowers"],
        Op::HashNew | Op::HashGet | Op::HashSet => {
            &["codegen_wasm::tests::hash_set_get_int_lowers"]
        }
        Op::HashUnion => &["codegen_wasm::tests::hash_union_left_wins_lowers"],
        Op::HashUnset => &["codegen_wasm::tests::hash_unset_removes_element_lowers"],
        Op::HashIsset => &[
            "codegen_wasm::tests::hash_isset_and_array_key_exists_lower",
            "codegen::cli::test_cli_wasm_assoc_foreach_and_key_tests_match_php",
        ],
        Op::IterStart
        | Op::IterCurrentKey
        | Op::IterCurrentValue
        | Op::IterNext => &[
            "codegen_wasm::tests::foreach_echoes_indexed_int_array",
            "codegen_wasm::tests::foreach_hash_string_keys",
        ],
        Op::IterCurrentValueRef => &["codegen_wasm::tests::ref_cell_foreach_ref_int_e2e"],
        Op::ObjectNew | Op::PropGet | Op::PropSet => {
            &["codegen_wasm::tests::object_prop_set_overwrites"]
        }
        Op::MethodCall => &["codegen_wasm::tests::method_direct_call_returns_value"],
        Op::StaticMethodCall => {
            &["codegen_wasm::tests::static_method_named_direct_call"]
        }
        Op::NullsafeMethodCall => {
            &["codegen_wasm::tests::nullsafe_on_object_receiver_dispatches"]
        }
        Op::NullsafePropGet => {
            &["codegen_wasm::tests::nullsafe_prop_get_object_reads_dyn_prop"]
        }
        Op::InstanceOf => &["codegen_wasm::tests::instanceof_union_receiver_returns_true"],
        Op::InstanceOfDynamic => &[
            "codegen_wasm::tests::instanceof_dynamic_string_target_matches",
            "codegen_wasm::tests::instanceof_dynamic_object_target_matches",
        ],
        Op::Call => &[
            "codegen_wasm::tests::ref_cell_promotion_is_runtime_idempotent_across_branches",
            "codegen_wasm::tests::acquired_ref_cell_return_survives_owner_epilogue",
        ],
        Op::RuntimeCall => &[
            "codegen_wasm::closures::tests::array_map_lowering_via_builtin_call_returns_4220",
            "codegen_wasm::tests::get_class_object_returns_class_name",
        ],
        Op::LanguageConstructCall => {
            &["codegen_wasm::tests::exit_with_code_sets_process_status"]
        }
        Op::ClosureNew | Op::ClosureCall => {
            &["codegen_wasm::tests::closure_capture_float_e2e"]
        }
        Op::FirstClassCallableNew => {
            &["codegen_wasm::tests::first_class_callable_free_fn_e2e"]
        }
        Op::EchoValue => &[
            "codegen_wasm::tests::echo_integers_writes_to_stdout",
            "codegen_wasm::tests::echo_float_writes_to_stdout",
            "codegen_wasm::tests::echo_string_literal_writes_to_stdout",
            "codegen_wasm::tests::echo_booleans_writes_to_stdout",
        ],
        Op::TryPushHandler
        | Op::TryPopHandler
        | Op::ThrowException
        | Op::ThrowErrorValue
        | Op::CatchCurrent
        | Op::CatchBind => &[
            "codegen_wasm::function::tests::exception_ops_lower_to_core_wasm_forms",
            "codegen::cli::test_cli_wasm_try_catch_lowers_to_core_exception_forms",
            "codegen::cli::test_cli_wasm_try_catch_dispatch_matches_php",
            "codegen::cli::test_cli_wasm_uncaught_exception_is_a_php_fatal",
        ],
        _ => &[],
    }
}

/// Returns the exact active dispatch lowerer for one admitted opcode.
fn op_lowerer(op: Op) -> &'static str {
    match op {
        Op::ConstI64 => "codegen_wasm::inst::lower_const_i64",
        Op::ConstF64 => "codegen_wasm::inst::lower_const_f64",
        Op::ConstBool => "codegen_wasm::inst::lower_const_bool",
        Op::ConstNull => "codegen_wasm::inst::lower_const_null",
        Op::ConstStr => "codegen_wasm::inst::lower_const_str",
        Op::StrLen => "codegen_wasm::inst::lower_strlen",
        Op::StrPersist => "codegen_wasm::inst::lower_str_persist",
        Op::StrConcat => "codegen_wasm::inst::lower_str_concat",
        Op::Nop => "codegen_wasm::inst::lower_nop",
        Op::ConcatReset => "codegen_wasm::inst::lower_concat_reset",
        Op::LoadLocal => "codegen_wasm::inst::lower_load_local",
        Op::StoreLocal => "codegen_wasm::inst::lower_store_local",
        Op::UnsetLocal => "codegen_wasm::inst::lower_unset_local",
        Op::IAdd | Op::ISub | Op::IMul | Op::IBitAnd | Op::IBitOr | Op::IBitXor => {
            "codegen_wasm::inst::lower_int_binop"
        }
        Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul => {
            "codegen_wasm::inst::lower_checked_int_binop"
        }
        Op::MixedNumericBinop => "codegen_wasm::inst::lower_mixed_numeric_binop",
        Op::IShl | Op::IShrA => "codegen_wasm::inst::lower_int_shift",
        Op::ISDiv => "codegen_wasm::inst::lower_signed_int_div",
        Op::ISMod => "codegen_wasm::inst::lower_signed_int_mod",
        Op::INeg => "codegen_wasm::inst::lower_int_neg",
        Op::IBitNot => "codegen_wasm::inst::lower_int_bitnot",
        Op::IDiv => "codegen_wasm::inst::lower_int_div_to_float",
        Op::FAdd | Op::FSub | Op::FMul => "codegen_wasm::inst::lower_float_binop",
        Op::FDiv => "codegen_wasm::inst::lower_float_div",
        Op::FNeg => "codegen_wasm::inst::lower_float_neg",
        Op::ICmp => "codegen_wasm::inst::lower_int_cmp",
        Op::FCmp => "codegen_wasm::inst::lower_float_cmp",
        Op::StrictEq | Op::StrictNotEq => "codegen_wasm::strict::lower_strict_compare",
        Op::IToF => "codegen_wasm::inst::lower_itof",
        Op::IToStr => "codegen_wasm::inst::lower_int_like_to_string",
        Op::Cast => "codegen_wasm::inst::lower_cast",
        Op::IsTruthy => "codegen_wasm::inst::lower_is_truthy",
        Op::IsNull => "codegen_wasm::inst::lower_is_null",
        Op::Call => "codegen_wasm::inst::lower_call",
        Op::LoadGlobal => "codegen_wasm::inst::lower_load_global",
        Op::RuntimeCall => "codegen_wasm::inst::lower_runtime_call",
        Op::LanguageConstructCall => "codegen_wasm::inst::lower_language_construct_call",
        Op::EchoValue | Op::PrintValue => "codegen_wasm::inst::lower_echo",
        Op::Warn => "codegen_wasm::inst::lower_array_offset_on_null_warning",
        Op::ThrowError => "codegen_wasm::inst::lower_method_call_on_null_error",
        Op::Acquire => "codegen_wasm::inst::lower_acquire",
        Op::Release => "codegen_wasm::inst::lower_release",
        Op::GcCollect => "codegen_wasm::inst::lower_gc_collect",
        Op::LoadStaticProperty => "codegen_wasm::inst::lower_load_static_property",
        Op::ScopedConstantGet => "codegen_wasm::inst::lower_scoped_constant_get",
        Op::StoreStaticProperty => "codegen_wasm::inst::lower_store_static_property",
        Op::Move | Op::Borrow => "codegen_wasm::inst::lower_forward",
        Op::ArrayNew => "codegen_wasm::inst::lower_array_new",
        Op::ArrayLen => "codegen_wasm::inst::lower_array_len",
        Op::ArrayGet | Op::ArrayGetSilent => "codegen_wasm::inst::lower_array_get",
        Op::ArrayPush => "codegen_wasm::inst::lower_array_push",
        Op::ArraySet => "codegen_wasm::inst::lower_array_set",
        Op::ArrayToHash => "codegen_wasm::inst_hash::lower_array_to_hash",
        Op::HashNew => "codegen_wasm::inst_hash::lower_hash_new",
        Op::HashGet | Op::HashGetSilent => "codegen_wasm::inst_hash::lower_hash_get",
        Op::HashSet => "codegen_wasm::inst_hash::lower_hash_set",
        Op::HashUnset => "codegen_wasm::inst_hash::lower_hash_unset",
        Op::HashIsset => "codegen_wasm::inst_hash::lower_hash_isset",
        Op::HashAppend => "codegen_wasm::inst_hash::lower_hash_append",
        Op::HashUnion => "codegen_wasm::inst_hash::lower_hash_union",
        Op::ArrayUnion => "codegen_wasm::inst_hash::lower_array_union",
        Op::ArrayHashUnion => "codegen_wasm::inst_hash::lower_array_hash_union",
        Op::HashArrayUnion => "codegen_wasm::inst_hash::lower_hash_array_union",
        Op::MixedBox => "codegen_wasm::inst::lower_mixed_box",
        Op::MixedTagOf => "codegen_wasm::inst::lower_mixed_tag_of",
        Op::IterStart => "codegen_wasm::inst::lower_iter_start",
        Op::IterNext => "codegen_wasm::inst::lower_iter_next",
        Op::IterCurrentKey => "codegen_wasm::inst::lower_iter_current_key",
        Op::IterCurrentValue => "codegen_wasm::inst::lower_iter_current_value",
        Op::IterEnd => "codegen_wasm::inst::lower_instruction(no-op)",
        Op::ObjectNew => "codegen_wasm::objects::lower_object_new",
        Op::PropGet => "codegen_wasm::objects::lower_prop_get",
        Op::PropSet => "codegen_wasm::objects::lower_prop_set",
        Op::MethodCall => "codegen_wasm::methods::lower_method_call",
        Op::StaticMethodCall => "codegen_wasm::methods::lower_static_method_call",
        Op::NullsafeMethodCall => "codegen_wasm::methods::lower_nullsafe_method_call",
        Op::NullsafePropGet => "codegen_wasm::objects::lower_nullsafe_prop_get",
        Op::InstanceOf => "codegen_wasm::classes::lower_instanceof",
        Op::InstanceOfDynamic => "codegen_wasm::classes::lower_instanceof_dynamic",
        Op::ClosureNew => "codegen_wasm::closures::lower_closure_new",
        Op::ClosureCall => "codegen_wasm::closures::lower_closure_call",
        Op::ClosureCapture => "codegen_wasm::closures::lower_closure_capture",
        Op::FirstClassCallableNew => "codegen_wasm::closures::lower_first_class_callable_new",
        Op::CallableDescriptorInvoke => {
            "codegen_wasm::closures::lower_callable_descriptor_invoke"
        }
        Op::LoadRefCell => "codegen_wasm::refcell::lower_load_ref_cell",
        Op::StoreRefCell => "codegen_wasm::refcell::lower_store_ref_cell",
        Op::PromoteLocalRefCell => "codegen_wasm::refcell::lower_promote_local_ref_cell",
        Op::AliasLocalRefCell => "codegen_wasm::refcell::lower_alias_local_ref_cell",
        Op::ReleaseLocalRefCell => "codegen_wasm::refcell::lower_release_local_ref_cell",
        Op::IterCurrentValueRef => "codegen_wasm::refcell::lower_iter_current_value_ref",
        Op::ArrayToMixed => "codegen_wasm::inst::lower_array_to_mixed",
        Op::LooseEq | Op::LooseNotEq => "codegen_wasm::inst::lower_loose_eq",
        Op::TryPushHandler => "codegen_wasm::inst::lower_try_push_handler",
        Op::TryPopHandler => "codegen_wasm::inst::lower_try_pop_handler",
        Op::ThrowException | Op::ThrowErrorValue => "codegen_wasm::inst::lower_throw",
        Op::CatchCurrent => "codegen_wasm::inst::lower_catch_current",
        Op::CatchBind => "codegen_wasm::inst::lower_catch_bind",
        _ => panic!(
            "supported opcode {} lacks an exact WASM lowerer inventory entry",
            op.name()
        ),
    }
}


/// Maps a supported `Op` to its shared evidence-group key.
pub(super) fn op_evidence_group(op: Op) -> &'static str {
    match op {
        Op::ConstI64 | Op::ConstF64 | Op::ConstStr | Op::ConstNull | Op::ConstBool => "const",
        Op::LoadLocal | Op::StoreLocal | Op::UnsetLocal => "transfer_local",
        Op::LoadRefCell
        | Op::StoreRefCell
        | Op::PromoteLocalRefCell
        | Op::AliasLocalRefCell
        | Op::ReleaseLocalRefCell => "transfer_refcell",
        Op::LoadGlobal => "transfer_global_load",
        Op::IAdd
        | Op::ISub
        | Op::IMul
        | Op::ICheckedAdd
        | Op::ICheckedSub
        | Op::ICheckedMul
        | Op::IDiv
        | Op::ISDiv
        | Op::ISMod
        | Op::INeg
        | Op::IBitAnd
        | Op::IBitOr
        | Op::IBitXor
        | Op::IBitNot
        | Op::IShl
        | Op::IShrA => "scalar_int",
        Op::FAdd | Op::FSub | Op::FMul | Op::FDiv | Op::FNeg => "float",
        Op::ICmp
        | Op::FCmp
        | Op::StrictEq
        | Op::StrictNotEq
        | Op::LooseEq
        | Op::LooseNotEq
        | Op::IsNull
        | Op::IsTruthy => "compare",
        Op::InstanceOf => "instanceof",
        Op::IToF => "itof",
        Op::IToStr => "string",
        Op::Cast => "cast",
        Op::MixedBox
        | Op::MixedTagOf
        | Op::MixedUnbox
        | Op::ArrayToMixed
        | Op::MixedNumericBinop
        | Op::HashToMixed => "mixed",
        Op::StrConcat | Op::ConcatReset | Op::StrLen | Op::StrPersist => "string",
        Op::ArrayNew
        | Op::ArrayLen
        | Op::ArrayGet
        | Op::ArrayGetSilent
        | Op::ArraySet
        | Op::ArrayPush
        | Op::ArrayUnion
        | Op::ArrayIsset
        | Op::ArrayToHash => "array_indexed",
        Op::HashNew
        | Op::HashGet
        | Op::HashGetSilent
        | Op::HashSet
        | Op::HashUnset
        | Op::HashAppend
        | Op::HashUnion
        | Op::ArrayHashUnion
        | Op::HashArrayUnion
        | Op::HashLen
        | Op::HashIsset => "hash",
        Op::IterStart
        | Op::IterCurrentKey
        | Op::IterCurrentValue
        | Op::IterCurrentValueRef
        | Op::IterNext
        | Op::IterEnd => "iter",
        Op::ObjectNew | Op::PropGet | Op::PropSet | Op::NullsafePropGet => "object",
        Op::MethodCall | Op::NullsafeMethodCall | Op::StaticMethodCall => "method",
        Op::InstanceOfDynamic => "instanceof_dynamic",
        Op::Call | Op::LanguageConstructCall | Op::RuntimeCall => "call",
        Op::ClosureNew | Op::ClosureCapture | Op::ClosureCall
        | Op::FirstClassCallableNew
        | Op::CallableDescriptorInvoke => "closure",
        Op::EchoValue | Op::PrintValue | Op::WriteStrStdout => "echo",
        Op::Warn => "warn",
        Op::ThrowError => "throw_error",
        Op::Acquire | Op::Release | Op::Move | Op::Borrow | Op::Nop | Op::GcCollect => "ownership",
        Op::LoadStaticProperty | Op::StoreStaticProperty | Op::ScopedConstantGet => "object",
        Op::TryPushHandler
        | Op::TryPopHandler
        | Op::ThrowException
        | Op::ThrowErrorValue
        | Op::CatchCurrent
        | Op::CatchBind => "exception",
        _ => "other",
    }
}


/// Shared backend metadata for one lowering family.
struct EvidenceGroup {
    backend: &'static str,
}

/// Returns the shared backend record for a supported-group key.
fn evidence_for_group(group: &'static str) -> EvidenceGroup {
    let backend = match group {
        "const"
        | "transfer_local"
        | "transfer_global_load"
        | "scalar_int"
        | "float"
        | "compare"
        | "itof"
        | "cast"
        | "mixed"
        | "string"
        | "array_indexed"
        | "iter"
        | "call"
        | "echo"
        | "warn"
        | "throw_error"
        | "exception"
        | "ownership" => "codegen_wasm::inst",
        "transfer_refcell" => "codegen_wasm::refcell",
        "hash" => "codegen_wasm::inst_hash",
        "instanceof" | "instanceof_dynamic" => "codegen_wasm::classes",
        "object" => "codegen_wasm::objects",
        "method" => "codegen_wasm::methods",
        "closure" => "codegen_wasm::closures",
        "other" => "",
        unknown => panic!("unknown WASM inventory evidence group {unknown:?}"),
    };
    EvidenceGroup { backend }
}

/// Returns the supported row evidence after exact lowerer/test overrides.
fn supported_evidence_for_group(group: &'static str) -> SupportedEvidence {
    let group = evidence_for_group(group);
    assert!(
        !group.backend.is_empty(),
        "supported opcode is missing an audited evidence group"
    );
    SupportedEvidence {
        backend: group.backend,
        lowerer: "",
        tests: &[],
    }
}
