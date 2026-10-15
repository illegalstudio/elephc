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
        // The file family shares one contract table and one lowering, so it shares its
        // evidence: the round-trip test writes, reads back, and removes real files under a
        // preopened directory, which is the only way any of these can be exercised at all.
        RuntimeFnId::Fopen
        | RuntimeFnId::Fwrite
        | RuntimeFnId::Fread
        | RuntimeFnId::Fclose
        | RuntimeFnId::FileExists
        | RuntimeFnId::Unlink
        | RuntimeFnId::FileGetContents
        | RuntimeFnId::FilePutContents
        | RuntimeFnId::Feof
        | RuntimeFnId::Ftell
        | RuntimeFnId::Rewind
        | RuntimeFnId::Fseek
        | RuntimeFnId::StreamGetContents
        // `stream_get_line`'s helper and shape contract are complete, but the registry
        // still declares `returns: Str` where PHP answers `string|false`, so every call
        // site refuses today at the result-storage shape check. It lights up the moment
        // the registry contract widens — no wasm-side change needed then.
        | RuntimeFnId::StreamGetLine
        // `stream_get_meta_data` builds php's nine-key array from the per-fd record
        // `fopen` writes (mode, uri) plus the live eof flag and a seekability probe.
        | RuntimeFnId::StreamGetMetaData
        | RuntimeFnId::StreamCopyToStream
        // Not a file, but the same WASI forwarding through the same contract table:
        // `getenv` reads `environ_get` fresh on every call and answers `string|false`
        // — a boxed cell, since a missing name and an empty value are both zero-length.
        | RuntimeFnId::Fgetc
        | RuntimeFnId::Readfile
        | RuntimeFnId::Flock
        | RuntimeFnId::Tmpfile
        | RuntimeFnId::Getenv
        | RuntimeFnId::GetResourceType
        | RuntimeFnId::Define => (
            "codegen_wasm::files",
            "codegen_wasm::builtins::files::lower_file_builtin",
            &[
                "codegen_wasm::builtins::tests::file_builtins_pin_the_storage_their_helpers_read",
                "codegen::cli::test_cli_wasm_file_round_trip_matches_php",
                "codegen::cli::test_cli_wasm_reads_and_writes_an_in_memory_stream",
                "codegen::cli::test_cli_wasm_stream_extensions_match_php",
            ][..],
        ),
        // The four namespace questions share one closed-world answer and one lowering: the
        // checker requires a literal name in AOT mode, so this module — the whole program —
        // settles them at compile time.
        RuntimeFnId::ClassExists
        | RuntimeFnId::InterfaceExists
        | RuntimeFnId::TraitExists
        | RuntimeFnId::EnumExists
        | RuntimeFnId::FunctionExists => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::classes::lower_class_exists",
            &[
                "codegen_wasm::builtins::tests::the_exists_family_answers_from_the_modules_own_declarations",
                "codegen::cli::test_cli_wasm_answers_the_exists_family_from_its_own_declarations",
            ][..],
        ),
        // The three class relations are settled by the same closed world, into an assoc hash
        // rather than a bool. Their NAMES come from `ir::class_relations`, shared with the
        // native backend so the two backends cannot answer differently.
        RuntimeFnId::ClassImplements
        | RuntimeFnId::ClassParents
        | RuntimeFnId::ClassUses => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::classes::lower_class_relation",
            &[
                "codegen_wasm::builtins::tests::class_relations_answer_from_the_modules_own_declarations",
                "codegen::cli::test_cli_wasm_class_relations_and_countable_count_match_php",
            ][..],
        ),
        // Pure computation over bytes already in memory, and php's parser is STRICTER than a
        // tolerant reader — the strictness is the contract, not an implementation detail.
        RuntimeFnId::Long2ip
        | RuntimeFnId::Ip2long
        | RuntimeFnId::InetNtop
        | RuntimeFnId::InetPton => (
            "codegen_wasm::inet",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::ipv4_conversions_answer_phps_strict_parser",
                "codegen::cli::test_cli_wasm_ipv4_conversions_and_scalar_var_dump_match_php",
            ][..],
        ),
        // SCALARS only: an array or object keeps refusing, because a half-implemented walk
        // would print something that looks right.
        RuntimeFnId::VarDump => (
            "codegen_wasm::inet",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::var_dump_admits_only_renderable_values",
                "codegen::cli::test_cli_wasm_ipv4_conversions_and_scalar_var_dump_match_php",
            ][..],
        ),
        // The array rebuilds and the two mutators. Generic where they only MOVE elements —
        // a clone already settled the representation and the refcounts — and raw-i64-only
        // where an element has to be WRITTEN.
        RuntimeFnId::ArrayPad
        | RuntimeFnId::ArrayUnique
        | RuntimeFnId::ArrayColumn
        | RuntimeFnId::ArraySplice
        | RuntimeFnId::ArrayUnshift => (
            "codegen_wasm::arrays_ext",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::array_rebuilds_admit_only_the_storage_they_write",
                "codegen::cli::test_cli_wasm_array_rebuilds_match_php",
            ][..],
        ),
        // Every value the audit proved encodable. An object answers through `jsonSerialize` when
        // it implements `JsonSerializable`, and through its public properties otherwise.
        RuntimeFnId::JsonEncode => (
            "codegen_wasm::json",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::json_encode_admits_only_provably_encodable_values",
                "codegen::cli::test_cli_wasm_json_encode_matches_php",
            ][..],
        ),
        // The buffer stack. One interception point serves every stdout write, and the handler
        // path reaches user code through the callable ladder.
        RuntimeFnId::ObStart
        | RuntimeFnId::ObGetClean
        | RuntimeFnId::ObEndFlush
        | RuntimeFnId::ObEndClean
        | RuntimeFnId::ObGetStatus => (
            "codegen_wasm::output_buffer",
            "codegen_wasm::builtins::output::lower_output_buffer",
            &[
                "codegen_wasm::builtins::tests::output_buffering_admits_only_a_descriptor_handler",
                "codegen::cli::test_cli_wasm_output_buffering_matches_php",
            ][..],
        ),
        RuntimeFnId::Readline => (
            "codegen_wasm::runtime",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::readline_takes_a_string_prompt_and_answers_a_string",
                "codegen::cli::test_cli_wasm_readline_reads_a_line_without_its_newline",
            ][..],
        ),
        RuntimeFnId::GetClass => (
            "codegen_wasm::classes",
            "codegen_wasm::classes::lower_get_class",
            &["codegen_wasm::tests::get_class_object_returns_class_name"][..],
        ),
        RuntimeFnId::Opendir
        | RuntimeFnId::Readdir
        | RuntimeFnId::Rewinddir
        | RuntimeFnId::Closedir => (
            "codegen_wasm::dirs",
            "codegen_wasm::inst::lower_dir_call",
            &[
                "codegen_wasm::capability::tests::directory_iteration_admits_the_handle_shape_opendir_answers",
                "codegen::cli::test_cli_wasm_directory_iteration_matches_php",
            ][..],
        ),
        RuntimeFnId::ExtensionLoaded | RuntimeFnId::GetLoadedExtensions => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::extensions::lower_get_loaded_extensions",
            &[
                "codegen_wasm::builtins::tests::loaded_extension_set_is_this_target_s_own",
                "codegen::cli::test_cli_wasm_extension_loaded_set_is_honest",
            ][..],
        ),
        RuntimeFnId::Time | RuntimeFnId::Microtime => (
            "codegen_wasm::clock",
            "codegen_wasm::builtins::clock::lower_clock",
            &[
                "codegen_wasm::clock::tests::the_wall_clock_is_served_and_sleeping_is_not",
                "codegen::cli::test_cli_wasm_wall_clock_matches_php",
            ][..],
        ),
        RuntimeFnId::PregMatch => (
            "codegen_wasm::regex",
            "codegen_wasm::builtins::preg::lower_preg_match",
            &[
                "codegen_wasm::regex::tests::the_emitted_layout_is_what_the_vm_reads",
                "codegen_wasm::regex::tests::the_compiled_program_matches_what_php_matches",
                "codegen_wasm::regex::tests::unimplemented_constructs_are_refused_by_name",
                "codegen::cli::test_cli_wasm_preg_match_matches_php",
            ][..],
        ),
        RuntimeFnId::Gzdeflate
        | RuntimeFnId::Gzinflate
        | RuntimeFnId::Gzcompress
        | RuntimeFnId::Gzuncompress => (
            "codegen_wasm::zlib",
            "codegen_wasm::builtins::zlib::lower_zlib",
            &[
                "codegen_wasm::zlib::tests::the_deflate_tables_are_the_format_s",
                "codegen::cli::test_cli_wasm_deflate_reads_what_zlib_writes",
            ][..],
        ),
        RuntimeFnId::BcAdd
        | RuntimeFnId::BcMul
        | RuntimeFnId::BcDivmod
        | RuntimeFnId::BcScale => (
            "codegen_wasm::bcmath",
            "codegen_wasm::builtins::bcmath::lower_bcmath",
            &[
                "codegen_wasm::bcmath::tests::decimal_arithmetic_serves_exactly_what_is_written",
                "codegen::cli::test_cli_wasm_decimal_arithmetic_matches_php",
            ][..],
        ),
        RuntimeFnId::Fprintf => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::format::lower_fprintf",
            &[
                "codegen_wasm::scanf::tests::the_scan_format_admits_only_what_the_tokenizer_can_split",
                "codegen::cli::test_cli_wasm_formatted_stream_io_matches_php",
            ][..],
        ),
        RuntimeFnId::Fscanf => (
            "codegen_wasm::scanf",
            "codegen_wasm::builtins::format::lower_fscanf",
            &[
                "codegen_wasm::scanf::tests::the_scan_format_admits_only_what_the_tokenizer_can_split",
                "codegen::cli::test_cli_wasm_formatted_stream_io_matches_php",
            ][..],
        ),
        RuntimeFnId::StreamSetChunkSize => (
            "codegen_wasm::streamtune",
            "codegen_wasm::builtins::files::lower_stream_tuner",
            &[
                "codegen_wasm::streamtune::tests::the_tuners_answer_what_php_answers",
                "codegen_wasm::function::tests::every_catchable_builtin_is_listed_as_raising",
                "codegen::cli::test_cli_wasm_stream_tuners_match_php",
            ][..],
        ),
        RuntimeFnId::StreamSetReadBuffer | RuntimeFnId::StreamSetWriteBuffer => (
            "codegen_wasm::streamtune",
            "codegen_wasm::builtins::files::lower_stream_tuner",
            &[
                "codegen_wasm::streamtune::tests::the_tuners_answer_what_php_answers",
                "codegen::cli::test_cli_wasm_stream_tuners_match_php",
            ][..],
        ),
        RuntimeFnId::Getprotobyname | RuntimeFnId::Getprotobynumber => (
            "codegen_wasm::protoent",
            "codegen_wasm::builtins::network::lower_protocol_lookup",
            &[
                "codegen_wasm::protoent::tests::protocol_table_agrees_with_the_registry",
                "codegen::cli::test_cli_wasm_protocol_registry_matches_php",
            ][..],
        ),
        RuntimeFnId::GetDeclaredClasses | RuntimeFnId::SplClasses => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::classes::lower_declared_names",
            &[
                "codegen_wasm::builtins::tests::introspection_builtins_admit_only_the_objects_they_can_ask",
                "codegen::cli::test_cli_wasm_declared_class_lists_are_this_modules_own",
            ][..],
        ),
        RuntimeFnId::SplObjectId => (
            "codegen_wasm::handles",
            "codegen_wasm::builtins::classes::lower_spl_object_id",
            &[
                "codegen_wasm::builtins::tests::the_handle_pool_is_only_built_for_a_module_that_asks",
                "codegen::cli::test_cli_wasm_object_identity_matches_php",
            ][..],
        ),
        RuntimeFnId::IsA => (
            "codegen_wasm::classes",
            "codegen_wasm::classes::lower_object_is_a",
            &[
                "codegen_wasm::builtins::tests::introspection_builtins_admit_only_the_objects_they_can_ask",
                "codegen::cli::test_cli_wasm_object_identity_matches_php",
            ][..],
        ),
        RuntimeFnId::GetObjectVars => (
            "codegen_wasm::objvars",
            "codegen_wasm::inst::lower_get_object_vars",
            &[
                "codegen_wasm::capability::tests::get_object_vars_admits_only_the_projection_its_table_can_answer",
                "codegen::cli::test_cli_wasm_get_object_vars_matches_php",
            ][..],
        ),
        RuntimeFnId::ArrayMap => (
            "codegen_wasm::inst",
            "codegen_wasm::inst::lower_array_map",
            &["codegen_wasm::closures::tests::array_map_lowering_via_builtin_call_returns_4220"]
                [..],
        ),
        RuntimeFnId::Gettype => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::introspection::lower_gettype",
            &[
                "codegen_wasm::builtins::tests::gettype_admits_settled_boxed_and_resource_values",
                "codegen::cli::test_cli_wasm_gettype_names_every_type_the_way_php_does",
            ][..],
        ),
        RuntimeFnId::Round => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::direct_builtins_admit_only_the_storage_they_lower",
                "codegen::cli::test_cli_wasm_round_and_radix_conversions_match_php",
            ][..],
        ),
        RuntimeFnId::ArraySearch => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
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
        // The callback-free array-parity builtins share one runtime module and one CLI probe,
        // because what they have in common is the hash walk they are each a shape of.
        RuntimeFnId::ArrayKeyFirst
        | RuntimeFnId::ArrayKeyLast
        | RuntimeFnId::ArrayReplace
        | RuntimeFnId::ArrayDiffAssoc
        | RuntimeFnId::ArrayIntersectAssoc => (
            "codegen_wasm::arrays_parity",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::array_parity_builtins_take_the_hash_they_walk",
                "codegen::cli::test_cli_wasm_array_parity_matches_php",
            ][..],
        ),
        RuntimeFnId::Sort | RuntimeFnId::Rsort => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::scalar_sorts_admit_only_orderable_elements",
                "codegen::cli::test_cli_wasm_scalar_sorts_match_php",
            ][..],
        ),
        // The four associative sorts share one merge over the insertion-order chain, and differ
        // only in the mode word that picks the key or the value and the direction.
        RuntimeFnId::Ksort
        | RuntimeFnId::Krsort
        | RuntimeFnId::Asort
        | RuntimeFnId::Arsort => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::array_sort::lower_hash_sort",
            &[
                "codegen_wasm::builtins::tests::associative_sorts_take_a_hash_and_no_flag",
                "codegen::cli::test_cli_wasm_hash_sorts_match_php",
            ][..],
        ),
        RuntimeFnId::Range => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::range_admits_only_two_integer_bounds",
                "codegen::cli::test_cli_wasm_range_matches_php",
            ][..],
        ),
        RuntimeFnId::ArrayMerge => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::array_merge_admits_only_agreeing_element_storage",
                "codegen::cli::test_cli_wasm_array_merge_matches_php",
            ][..],
        ),
        RuntimeFnId::ArraySlice => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
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
        // Scalars, arrays, and hashes to any depth, php's measured byte format; an
        // object refuses statically and fails loudly (code 15) when it hides in a
        // Mixed container at runtime.
        RuntimeFnId::PrintR => (
            "codegen_wasm::runtime",
            "codegen_wasm::builtins::output::lower_print_r",
            &[
                "codegen_wasm::tests::print_r_renders_php_bytes_for_nested_containers",
                "codegen::cli::test_cli_wasm_print_r_matches_php",
            ][..],
        ),
        // Only the audited float-to-"integer" narrowing of a plain local; the shape check
        // (`settype_shape_issue`) refuses every other target type, source type, or origin.
        RuntimeFnId::Settype => (
            "codegen_wasm::inst",
            "codegen_wasm::inst::lower_settype",
            &[
                "codegen_wasm::tests::settype_float_to_integer_narrows_the_slot_in_place",
                "codegen::cli::test_cli_wasm_settype_float_to_integer_matches_php",
            ][..],
        ),
        RuntimeFnId::Abs | RuntimeFnId::Floor | RuntimeFnId::Ceil | RuntimeFnId::Sqrt
        | RuntimeFnId::Count | RuntimeFnId::ArrayIsList | RuntimeFnId::ArrayKeys
        | RuntimeFnId::ArrayValues | RuntimeFnId::InArray | RuntimeFnId::ArrayReverse
        | RuntimeFnId::ArraySum | RuntimeFnId::ArrayProduct | RuntimeFnId::Max
        | RuntimeFnId::Min | RuntimeFnId::Intdiv | RuntimeFnId::ArrayFill
        | RuntimeFnId::StrContains | RuntimeFnId::StrStartsWith
        | RuntimeFnId::StrEndsWith => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
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
            "codegen_wasm::builtins::direct::lower_direct_builtin",
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
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::string_shaping_builtins_admit_only_their_arities",
                "codegen::cli::test_cli_wasm_strpos_and_tagged_strict_equality_match_php",
                "codegen::cli::test_cli_wasm_strstr_matches_php",
                "codegen::cli::test_cli_wasm_strrpos_matches_php",
            ][..],
        ),
        RuntimeFnId::Chr | RuntimeFnId::Ord => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::chr_and_ord_admit_only_their_concrete_scalar",
                "codegen::cli::test_cli_wasm_chr_and_ord_match_php",
            ][..],
        ),
        RuntimeFnId::Base64Decode => (
            "codegen_wasm::builtins",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::base64_decode_and_parse_url_admit_only_the_calls_they_answer_exactly",
                "codegen::cli::test_cli_wasm_base64_and_url_coding_match_php",
            ][..],
        ),
        RuntimeFnId::ParseUrl => (
            "codegen_wasm::urls",
            "codegen_wasm::builtins::direct::lower_direct_builtin",
            &[
                "codegen_wasm::builtins::tests::base64_decode_and_parse_url_admit_only_the_calls_they_answer_exactly",
                "codegen::cli::test_cli_wasm_parse_url_matches_php_fixture_corpus",
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
                lowerer: "codegen_wasm::builtins::strings::lower_unary_string",
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
        Terminator::Throw { .. } => &[
            "codegen_wasm::function::tests::exception_ops_lower_to_core_wasm_forms",
            "codegen::cli::test_cli_wasm_uncaught_throw_emits_the_throw_terminator",
            "codegen::cli::test_cli_wasm_uncaught_exception_is_a_php_fatal",
        ],
        Terminator::Fatal { .. } => {
            &["codegen::cli::test_cli_wasm_unmatched_match_is_a_fatal_like_php"]
        }
        Terminator::GeneratorSuspend { .. } => &[],
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
            "throwable.initialize",
            Disposition::Missing,
            vec!["a builtin Throwable constructor over a non-compact receiver layout".to_string()],
            None,
            Some("runtime target added on main after this backend's branch point; WASM lowerer absent"),
        ),
        form(
            "exception.chain",
            Disposition::Supported,
            vec!["an exception thrown out of a `finally` entered with one already pending".to_string()],
            Some(SupportedEvidence {
                backend: "codegen_wasm::objects",
                lowerer: "codegen_wasm::objects::lower_exception_chain",
                tests: &[
                    "codegen_wasm::capability::tests::exception_chain_needs_one_previous_offset",
                    "codegen::cli::test_cli_wasm_finally_runs_on_every_exit",
                ],
            }),
            None,
        ),
        form(
            "array.unpack_to_hash",
            Disposition::Missing,
            vec!["`[...$boxed]` unpacking a boxed array into a literal".to_string()],
            None,
            Some("runtime target added on main after this backend's branch point; WASM lowerer absent"),
        ),
        form(
            "exception.guard_owned",
            Disposition::Supported,
            vec!["an owned temporary kept alive across a call that can throw".to_string()],
            Some(SupportedEvidence {
                backend: "codegen_wasm::inst",
                lowerer: "codegen_wasm::inst::lower_runtime_call",
                tests: &[
                    "codegen_wasm::capability::tests::exception_guards_admit_only_their_token_shapes",
                    "codegen::cli::test_cli_wasm_generator_arguments_outlive_the_call",
                ],
            }),
            None,
        ),
        form(
            "exception.unguard_owned",
            Disposition::Supported,
            vec!["the matching release of an exception-guarded temporary".to_string()],
            Some(SupportedEvidence {
                backend: "codegen_wasm::inst",
                lowerer: "codegen_wasm::inst::lower_runtime_call",
                tests: &[
                    "codegen_wasm::capability::tests::exception_guards_admit_only_their_token_shapes",
                    "codegen::cli::test_cli_wasm_generator_arguments_outlive_the_call",
                ],
            }),
            None,
        ),
        form(
            "exception.update_array_guard",
            Disposition::Supported,
            vec!["an exception guard refreshed after an append relocates an indexed array".to_string()],
            Some(SupportedEvidence {
                backend: "codegen_wasm::inst",
                lowerer: "codegen_wasm::inst::lower_runtime_call",
                tests: &["codegen_wasm::capability::tests::exception_guards_admit_only_their_token_shapes"],
            }),
            None,
        ),
        form(
            "exception.update_hash_guard",
            Disposition::Supported,
            vec!["an exception guard refreshed after an insertion relocates a hash".to_string()],
            Some(SupportedEvidence {
                backend: "codegen_wasm::inst",
                lowerer: "codegen_wasm::inst::lower_runtime_call",
                tests: &["codegen_wasm::capability::tests::exception_guards_admit_only_their_token_shapes"],
            }),
            None,
        ),
        form(
            "call_argument.validate_unpack",
            Disposition::Missing,
            vec!["`f(...$args)` validating the unpacked source".to_string()],
            None,
            Some("runtime target added on main after this backend's branch point; WASM lowerer absent"),
        ),
        form(
            "call_argument.collect_positionals",
            Disposition::Missing,
            vec!["`f(...$args)` collecting its positional entries".to_string()],
            None,
            Some("runtime target added on main after this backend's branch point; WASM lowerer absent"),
        ),
        form(
            "call_argument.collect_named",
            Disposition::Missing,
            vec!["`f(...$args)` collecting its string-keyed entries".to_string()],
            None,
            Some("runtime target added on main after this backend's branch point; WASM lowerer absent"),
        ),
        // Added upstream after this backend's last audit: the three boxed-Mixed cell
        // forms a nested write through a `mixed` container reaches, and the typed PCNTL
        // dispatch. Each is an ordinary PHP construct with no WASM lowerer, so each is a
        // reachable gap rather than an exclusion.
        form(
            "mixed_cell.promote_to_hash",
            Disposition::Missing,
            vec!["string-keyed write into a boxed `mixed` indexed array".to_string()],
            None,
            Some("ordinary PHP boxed-container promotion; WASM lowerer absent"),
        ),
        form(
            "mixed_cell.promote_attached_to_hash",
            Disposition::Missing,
            vec!["string-keyed write into a boxed `mixed` element fetched for write".to_string()],
            None,
            Some("ordinary PHP boxed-container promotion; WASM lowerer absent"),
        ),
        form(
            "mixed_cell.clone",
            Disposition::Missing,
            vec!["copy-on-write split of a stored boxed `mixed` container".to_string()],
            None,
            Some("ordinary PHP copy-on-write split; WASM lowerer absent"),
        ),
        form(
            "pcntl",
            Disposition::Missing,
            vec!["pcntl_fork(...), pcntl_waitpid(...), and the rest of ext/pcntl".to_string()],
            None,
            Some("process control has no WASI Preview 1 equivalent; WASM lowerer absent"),
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
            // The registry never names this form: it declares `RuntimeCallTarget::Function`
            // and `LoweringContext::emit_runtime_call` upgrades the call site to
            // `ProfiledFunction` for exactly these runtime functions, so the producers are
            // the PHP calls that reach that upgrade.
            vec![
                "function_exists(...)".to_string(),
                "is_callable(...)".to_string(),
                "ob_start(...)".to_string(),
                "a builtin handed a callable by its string name, such as usort($a, 'cmp')"
                    .to_string(),
                "the mb_* functions (mbstring runtime)".to_string(),
            ],
            Some(SupportedEvidence {
                backend: "codegen_wasm::inst",
                lowerer: "codegen_wasm::inst::lower_runtime_call",
                tests: &[
                    "codegen_wasm::tests::get_class_object_returns_class_name",
                    "codegen::cli::test_cli_wasm_profiled_runtime_functions_match_php",
                ],
            }),
            None,
        ),
    ]
}


/// Returns the shape predicates enforced before WAT staging.
pub(super) fn shape_predicates() -> Vec<ShapePredicate> {
    [
        "terminator_transfer_shape_issue",
        "call_operand_owner_shape_issue",
        "mixed_clone_shape_issue",
        "mixed_array_append_shape_issue",
        "prop_get_for_write_shape_issue",
        "widened_parameter_slot_issue",
        "type_predicate_shape_issue",
        "exception_guard_shape_issue",
        "mixed_property_read_named_issue",
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
        "interface_throwable_intrinsic_shape_issue",
        "local_transfer_shape_issue",
        "load_global_shape_issue",
        "store_ref_cell_shape_issue",
        "forward_transfer_shape_issue",
        "cast_shape_issue",
        "include_once_shape_issue",
        "function_variant_mark_shape_issue",
        "int_like_to_string_shape_issue",
        "strict_compare_shape_issue",
        "str_inc_dec_shape_issue",
        "truthiness_shape_issue",
        "empty_construct_shape_issue",
        "settype_shape_issue",
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
        "php_rel_cmp_shape_issue",
        "return_boundary_int_shape_issue",
        "release_unless_aliases_shape_issue",
        "zero_local_slot_shape_issue",
        "prop_initialized_shape_issue",
        "static_prop_initialized_shape_issue",
        "property_unset_shape_issue",
        "dynamic_property_shape_issue",
        "generator_method_shape_issue",
        "generator_send_argument_issue",
        "generator_throw_argument_issue",
        "generator_yield_shape_issue",
        "generator_yield_from_shape_issue",
        "direct_call_shape_issue",
        "boxed_static_default_issue",
        "by_ref_source_shape_issue",
        "by_ref_parameter_representation_issue",
        "method_call_shape_issue",
        "interface_method_call_shape_issue",
        "object_new_shape_issue",
        "property_get_shape_issue",
        "mixed_property_read_issue",
        "property_set_shape_issue",
        "static_method_call_shape_issue",
        "enum_static_intrinsic_shape_issue",
        "object_clone_shape_issue",
        "spaceship_shape_issue",
        "static_property_shape_issue",
        "static_local_shape_issue",
        "store_global_shape_issue",
        "scoped_constant_shape_issue",
        "method_signature_shape_issue",
        "method_body_signature_shape_issue",
        "method_body_argument_shape_issue",
        "direct_method_result_shape_issue",
        "mixed_method_issue",
        "runtime_function_shape_issue",
        "get_class_shape_issue",
        "get_object_vars_shape_issue",
        "opendir_shape_issue",
        "dir_handle_call_shape_issue",
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
        "array_callable_argument_container_issue",
        "throw_error_value_shape_issue",
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
        IrType::TaggedScalar => &[
            "codegen_wasm::values::tests::val_types_tagged_returns_i64_i32",
            "codegen::cli::test_cli_wasm_value_tests_and_suppression_match_php",
        ][..],
        IrType::Heap(IrHeapKind::Array)
        | IrType::Heap(IrHeapKind::Hash)
        | IrType::Heap(IrHeapKind::Iterable) => &[
            "codegen_wasm::values::tests::val_types_every_heap_kind_is_one_i32",
            "codegen::cli::test_cli_wasm_array_and_hash_storage_match_php",
        ][..],
        // No PHP program lowers to `Heap(Union)` on this target: the checker's union types
        // reach EIR as `Mixed` cells, so the storage mapping itself is the evidence.
        IrType::Heap(IrHeapKind::Union) => {
            &["codegen_wasm::values::tests::val_types_every_heap_kind_is_one_i32"][..]
        }
        IrType::Void => &[
            "codegen_wasm::values::tests::val_types_void_returns_empty",
            "codegen::cli::test_cli_wasm_scalar_operators_match_php",
        ][..],
        IrType::Heap(IrHeapKind::Buffer) => &[],
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
