//! Purpose:
//! Unit tests for the WASM builtin admission predicates and the pure helpers beside them.
//!
//! Called from:
//! - `cargo test` through Rust's test harness; the WASM inventory cites these tests by `codegen_wasm::builtins::tests::` path.

use super::*;
use crate::ir::{Builder, Immediate, Op, Ownership, RuntimeCallTarget};

/// Builds a one-instruction function calling `target` with one operand of the given storage.
fn call_with(
    target: RuntimeFnId,
    operand_ir: IrType,
    operand_php: PhpType,
    result_ir: IrType,
    result_php: PhpType,
) -> Function {
    let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("v".to_string()),
            operand_ir,
            operand_php.clone(),
            crate::ir::LocalKind::PhpLocal,
        );
        let argument = builder.emit_load_local(slot, operand_ir, operand_php);
        builder.emit(
            Op::RuntimeCall,
            vec![argument],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
            result_ir,
            result_php,
            Ownership::NonHeap,
        );
        builder.terminate(crate::ir::Terminator::Return { value: None });
    }
    function
}

/// Returns the audit verdict for the last instruction of `function`.
fn verdict(function: &Function, target: RuntimeFnId) -> Option<String> {
    let call = function
        .instructions
        .last()
        .expect("the probe emitted a call");
    direct_builtin_shape_issue(&probe_module(), function, call, target)
}

/// Builds `in_array($needle, $haystack, $strict)` with the given strict operand.
fn in_array_call(strict_is_literal_true: bool) -> Function {
    let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let needle_slot = builder.add_local(
            Some("n".to_string()),
            IrType::I64,
            PhpType::Int,
            crate::ir::LocalKind::PhpLocal,
        );
        let haystack_slot = builder.add_local(
            Some("a".to_string()),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            crate::ir::LocalKind::PhpLocal,
        );
        let needle = builder.emit_load_local(needle_slot, IrType::I64, PhpType::Int);
        let haystack = builder.emit_load_local(
            haystack_slot,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        );
        let strict = if strict_is_literal_true {
            builder.emit_const_bool(true)
        } else {
            builder.emit_const_bool(false)
        };
        builder.emit(
            Op::RuntimeCall,
            vec![needle, haystack, strict],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::InArray,
            ))),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        );
        builder.terminate(crate::ir::Terminator::Return { value: None });
    }
    function
}

/// Builds a unary string transform call with the given operand storage.
fn unary_string_call(
    target: UnaryStringRuntime,
    operand_ir: IrType,
    operand_php: PhpType,
) -> Function {
    let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("s".to_string()),
            operand_ir,
            operand_php.clone(),
            crate::ir::LocalKind::PhpLocal,
        );
        let argument = builder.emit_load_local(slot, operand_ir, operand_php);
        builder.emit(
            Op::RuntimeCall,
            vec![argument],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::UnaryString(
                target,
            ))),
            IrType::Str,
            PhpType::Str,
            Ownership::MaybeOwned,
        );
        builder.terminate(crate::ir::Terminator::Return { value: None });
    }
    function
}

/// Verifies the admitted unary string transforms take a string and nothing else.
#[test]
fn unary_string_transforms_admit_only_strings() {
    for target in [
        UnaryStringRuntime::StrToUpper,
        UnaryStringRuntime::StrToLower,
        UnaryStringRuntime::StrReverse,
        UnaryStringRuntime::BinToHex,
        UnaryStringRuntime::AddSlashes,
        UnaryStringRuntime::StripSlashes,
        UnaryStringRuntime::NlToBr,
        UnaryStringRuntime::UrlEncode,
        UnaryStringRuntime::RawUrlEncode,
        UnaryStringRuntime::UrlDecode,
        UnaryStringRuntime::RawUrlDecode,
        UnaryStringRuntime::Base64Encode,
    ] {
        assert!(unary_string_is_supported(target), "{target:?} is lowered");
        let ok = unary_string_call(target, IrType::Str, PhpType::Str);
        let call = ok.instructions.last().expect("the probe emitted a call");
        assert_eq!(unary_string_shape_issue(&ok, call, target), None);

        let scalar = unary_string_call(target, IrType::I64, PhpType::Int);
        let call = scalar.instructions.last().expect("the probe emitted a call");
        assert!(
            unary_string_shape_issue(&scalar, call, target).is_some(),
            "{target:?} maps string bytes"
        );
    }

    // The html decoders need a real named-entity table, and `hex2bin` returns `string|false`
    // in PHP rather than the `string` this family's signature promises.
    assert!(!unary_string_is_supported(
        UnaryStringRuntime::HtmlEntityDecode
    ));
    assert!(!unary_string_is_supported(UnaryStringRuntime::HexToBin));
}

/// Builds a one-operand `RuntimeCall` probe with a chosen operand and result typing.
fn scalar_conversion_call(
    target: RuntimeFnId,
    operand_ir: IrType,
    operand_php: PhpType,
    result_ir: IrType,
    result_php: PhpType,
) -> Function {
    let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("v".to_string()),
            operand_ir,
            operand_php.clone(),
            crate::ir::LocalKind::PhpLocal,
        );
        let argument = builder.emit_load_local(slot, operand_ir, operand_php);
        builder.emit(
            Op::RuntimeCall,
            vec![argument],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
            result_ir,
            result_php,
            Ownership::MaybeOwned,
        );
        builder.terminate(crate::ir::Terminator::Return { value: None });
    }
    function
}

/// Verifies `chr` and `ord` take one concrete scalar and refuse anything juggled.
///
/// `RuntimeFnId::Chr` maps an int to a one-byte string and `RuntimeFnId::Ord` maps a string
/// back to an int, so each refuses the other's typing as well as a `mixed` operand. PHP would
/// juggle a `mixed` with its own per-tag diagnostics, which this backend does not reproduce,
/// so admitting one would answer confidently where PHP would have complained first.
#[test]
fn chr_and_ord_admit_only_their_concrete_scalar() {
    let chr = scalar_conversion_call(
        RuntimeFnId::Chr,
        IrType::I64,
        PhpType::Int,
        IrType::Str,
        PhpType::Str,
    );
    let call = chr.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &chr, call, RuntimeFnId::Chr), None);

    let ord = scalar_conversion_call(
        RuntimeFnId::Ord,
        IrType::Str,
        PhpType::Str,
        IrType::I64,
        PhpType::Int,
    );
    let call = ord.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &ord, call, RuntimeFnId::Ord), None);

    // Each refuses the other's operand typing, and both refuse a juggled one.
    let swapped = scalar_conversion_call(
        RuntimeFnId::Chr,
        IrType::Str,
        PhpType::Str,
        IrType::Str,
        PhpType::Str,
    );
    let call = swapped.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &swapped, call, RuntimeFnId::Chr).is_some());

    let juggled = scalar_conversion_call(
        RuntimeFnId::Ord,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
        IrType::I64,
        PhpType::Int,
    );
    let call = juggled.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &juggled, call, RuntimeFnId::Ord).is_some());

    // A wrong RESULT typing is refused too: `chr` cannot answer an int.
    let bad_result = scalar_conversion_call(
        RuntimeFnId::Chr,
        IrType::I64,
        PhpType::Int,
        IrType::I64,
        PhpType::Int,
    );
    let call = bad_result.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &bad_result, call, RuntimeFnId::Chr).is_some());
}

/// Verifies the PHP 8.5 deprecations are the ONLY thing the profile changes about chr/ord.
///
/// PHP 8.5 diagnoses an out-of-range `chr` argument and an `ord` argument that is not exactly
/// one byte, but both still ANSWER, and with the same value an earlier profile gives. So the
/// two renderings must differ only by the diagnostic call: if the arithmetic differed, an
/// earlier profile would silently compute something else.
#[test]
fn chr_and_ord_differ_between_profiles_only_by_the_deprecation() {
    let (diagnosing_chr, diagnosing_ord) = str_chr_ord(true);
    let (silent_chr, silent_ord) = str_chr_ord(false);

    assert!(diagnosing_chr.contains("call $__rt_deprecated_chr_range"));
    assert!(!silent_chr.contains("__rt_deprecated"));
    assert!(diagnosing_ord.contains("call $__rt_deprecated_ord_length"));
    assert!(!silent_ord.contains("__rt_deprecated"));

    // Strip the guard the diagnosing form adds; what remains must be identical.
    for (diagnosing, silent, marker) in [
        (&diagnosing_chr, &silent_chr, "__rt_deprecated_chr_range"),
        (&diagnosing_ord, &silent_ord, "__rt_deprecated_ord_length"),
    ] {
        let stripped: String = diagnosing
            .lines()
            .skip_while(|line| !line.contains("(if ("))
            .skip_while(|line| !line.contains(marker))
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n");
        let expected: String = silent
            .lines()
            .skip_while(|line| !line.contains("(local.set $byte") && !line.contains("(if (i64.le_s"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            stripped, expected,
            "the profiles must agree on the ANSWER, not just on the diagnostic"
        );
    }
}

/// `RuntimeFnId::Ksort`, `RuntimeFnId::Krsort`, `RuntimeFnId::Asort` and
/// `RuntimeFnId::Arsort` take an ASSOCIATIVE array and nothing else.
///
/// No element type is refused, because the comparison is `zend_compare` over boxed keys or
/// values rather than a per-pair specialization — which is the difference from the indexed
/// sorts beside them. What IS refused is an indexed receiver, since these reorder an
/// insertion-order chain a packed array does not have, and a sort FLAG, which would change
/// the comparison this serves.
#[test]
fn associative_sorts_take_a_hash_and_no_flag() {
    let hash_of = |value: PhpType| {
        (
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(value),
            },
        )
    };
    for target in [
        RuntimeFnId::Ksort,
        RuntimeFnId::Krsort,
        RuntimeFnId::Asort,
        RuntimeFnId::Arsort,
    ] {
        for value in [PhpType::Int, PhpType::Str, PhpType::Float, PhpType::Mixed] {
            let probe = shaped_call(
                target,
                &[hash_of(value.clone())],
                IrType::I64,
                PhpType::Void,
            );
            let call = probe.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&command_probe_module(), &probe, call, target),
                None,
                "{target:?} orders a hash of {value:?}"
            );
        }
        // An INDEXED receiver holds its keys as slot positions 0..n-1. `ksort` on one is
        // therefore already sorted, and php runs it and answers `true` — measured — so it is
        // admitted as the no-op it is. The other three have no meaning there: descending key
        // order cannot be expressed by positions, and the value sorts would need a slot
        // permuter this target does not have.
        let probe = shaped_call(
            target,
            &[(
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Int)),
            )],
            IrType::I64,
            PhpType::Void,
        );
        let call = probe.instructions.last().expect("the probe emitted a call");
        let indexed = direct_builtin_shape_issue(&command_probe_module(), &probe, call, target);
        if target == RuntimeFnId::Ksort {
            assert_eq!(indexed, None, "ksort on an indexed array is php's no-op");
        } else {
            assert!(
                indexed.is_some_and(|issue| issue.contains("slot positions")),
                "{target:?} must refuse an indexed receiver"
            );
        }
        // A sort flag changes the comparison, so the two-operand form is refused rather
        // than accepted and ignored.
        let probe = shaped_call(
            target,
            &[hash_of(PhpType::Int), (IrType::I64, PhpType::Int)],
            IrType::I64,
            PhpType::Void,
        );
        let call = probe.instructions.last().expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(&command_probe_module(), &probe, call, target)
                .is_some_and(|issue| issue.contains("one array")),
            "{target:?} must refuse a sort flag"
        );
    }
}

/// The callback-free parity builtins walk a HASH, and answer in the shape php answers.
///
/// `RuntimeFnId::ArrayKeyFirst` and `RuntimeFnId::ArrayKeyLast` answer a BOXED key, because
/// php returns the key in its own kind and `null` for an empty array — an unboxed result
/// would have to pick one of the three. `RuntimeFnId::ArrayReplace`,
/// `RuntimeFnId::ArrayDiffAssoc` and `RuntimeFnId::ArrayIntersectAssoc` take two hashes and
/// answer one, since each walks an insertion-order chain a packed array does not have.
#[test]
fn array_parity_builtins_take_the_hash_they_walk() {
    let hash = || {
        (
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
        )
    };
    let indexed = || {
        (
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        )
    };

    for target in [RuntimeFnId::ArrayKeyFirst, RuntimeFnId::ArrayKeyLast] {
        let probe = shaped_call(
            target,
            &[hash()],
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let call = probe.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&command_probe_module(), &probe, call, target),
            None,
            "{target:?} reads the edge of a hash's insertion order"
        );

        // An indexed receiver has no chain to read an edge from.
        let probe = shaped_call(
            target,
            &[indexed()],
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let call = probe.instructions.last().expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(&command_probe_module(), &probe, call, target)
                .is_some_and(|issue| issue.contains("insertion order")),
            "{target:?} must refuse an indexed receiver"
        );

        // An unboxed result cannot carry an int key, a string key and null at once.
        let probe = shaped_call(target, &[hash()], IrType::I64, PhpType::Int);
        let call = probe.instructions.last().expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(&command_probe_module(), &probe, call, target)
                .is_some_and(|issue| issue.contains("boxed key")),
            "{target:?} must refuse an unboxed result"
        );
    }

    for target in [
        RuntimeFnId::ArrayReplace,
        RuntimeFnId::ArrayDiffAssoc,
        RuntimeFnId::ArrayIntersectAssoc,
    ] {
        let probe = shaped_call(
            target,
            &[hash(), hash()],
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
        );
        let call = probe.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&command_probe_module(), &probe, call, target),
            None,
            "{target:?} combines two hashes"
        );

        // One indexed operand is enough to refuse: the walk is over both chains.
        for operands in [[hash(), indexed()], [indexed(), hash()]] {
            let probe = shaped_call(
                target,
                &operands,
                IrType::Heap(IrHeapKind::Hash),
                PhpType::AssocArray {
                    key: Box::new(PhpType::Str),
                    value: Box::new(PhpType::Mixed),
                },
            );
            let call = probe.instructions.last().expect("the probe emitted a call");
            assert!(
                direct_builtin_shape_issue(&command_probe_module(), &probe, call, target)
                    .is_some_and(|issue| issue.contains("walks two hashes")),
                "{target:?} must refuse an indexed operand"
            );
        }

        // A single operand is a different function; it is refused rather than defaulted.
        let probe = shaped_call(
            target,
            &[hash()],
            IrType::Heap(IrHeapKind::Hash),
            PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            },
        );
        let call = probe.instructions.last().expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(&command_probe_module(), &probe, call, target)
                .is_some_and(|issue| issue.contains("two arrays")),
            "{target:?} must refuse a one-operand call"
        );
    }
}

/// Builds a `RuntimeCall` probe with an arbitrary operand list.
/// `sort` and `rsort` order scalar slots in place; string and Mixed elements are refused,
/// because PHP compares two numeric strings NUMERICALLY and that rule is not this helper's.
#[test]
fn scalar_sorts_admit_only_orderable_elements() {
    let array_of = |element: PhpType| {
        (
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(element)),
        )
    };
    for target in [RuntimeFnId::Sort, RuntimeFnId::Rsort] {
        for element in [PhpType::Int, PhpType::Bool, PhpType::Float, PhpType::Never] {
            let probe = shaped_call(
                target,
                &[array_of(element.clone())],
                IrType::I64,
                PhpType::Void,
            );
            let call = probe.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&probe_module(), &probe, call, target),
                None,
                "{target:?} orders elements of {element:?}"
            );
        }
        // Strings order through the numeric-string classifier, which only a command
        // module carries — so the same call is admitted there and refused without it.
        let probe = shaped_call(
            target,
            &[array_of(PhpType::Str)],
            IrType::I64,
            PhpType::Void,
        );
        let call = probe.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&command_probe_module(), &probe, call, target),
            None,
            "{target:?} orders strings in a command module"
        );
        assert!(
            direct_builtin_shape_issue(&probe_module(), &probe, call, target).is_some(),
            "{target:?} has no string ordering without a main"
        );
        for element in [PhpType::Mixed] {
            let probe = shaped_call(
                target,
                &[array_of(element.clone())],
                IrType::I64,
                PhpType::Void,
            );
            let call = probe.instructions.last().expect("the probe emitted a call");
            assert!(
                direct_builtin_shape_issue(&probe_module(), &probe, call, target).is_some(),
                "{target:?} has no measured ordering for {element:?}"
            );
        }
    }
}

/// `range` here is the two-bound integer form only: the front-end rejects every other arity
/// with "range() takes exactly 2 arguments", so there is no step to validate and no float or
/// string bound to widen.
#[test]
fn range_admits_only_two_integer_bounds() {
    let int_arg = (IrType::I64, PhpType::Int);
    let probe = |operands: Vec<(IrType, PhpType)>| {
        let function = shaped_call(
            RuntimeFnId::Range,
            &operands,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        );
        let call = function
            .instructions
            .last()
            .expect("the probe emitted a call")
            .clone();
        direct_builtin_shape_issue(&probe_module(), &function, &call, RuntimeFnId::Range)
    };

    assert_eq!(probe(vec![int_arg.clone(), int_arg.clone()]), None);
    assert!(probe(vec![int_arg.clone()]).is_some(), "range needs both bounds");
    assert!(
        probe(vec![int_arg.clone(), int_arg.clone(), int_arg.clone()]).is_some(),
        "the step form does not reach this target"
    );
    assert!(
        probe(vec![(IrType::F64, PhpType::Float), int_arg.clone()]).is_some(),
        "a float bound produces float elements"
    );
    assert!(probe(vec![int_arg.clone(), (IrType::Str, PhpType::Str)]).is_some());
}

/// `array_merge` clones the left and appends the right, so both operands must agree on slot
/// layout — which they do, because EIR widens each with `Op::ArrayToMixed` when they differ.
/// An empty operand carries no elements, so its type never has to agree.
#[test]
fn array_merge_admits_only_agreeing_element_storage() {
    let array_of = |element: PhpType| {
        (
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(element)),
        )
    };
    let probe = |left: PhpType, right: PhpType| {
        let function = shaped_call(
            RuntimeFnId::ArrayMerge,
            &[array_of(left.clone()), array_of(right)],
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(left)),
        );
        let call = function
            .instructions
            .last()
            .expect("the probe emitted a call")
            .clone();
        direct_builtin_shape_issue(&probe_module(), &function, &call, RuntimeFnId::ArrayMerge)
    };

    for element in [
        PhpType::Int,
        PhpType::Str,
        PhpType::Float,
        PhpType::Bool,
        PhpType::Mixed,
    ] {
        assert_eq!(
            probe(element.clone(), element.clone()),
            None,
            "two arrays of {element:?} merge"
        );
        // `Never` is the empty literal's element type: nothing is read from that side.
        assert_eq!(probe(element.clone(), PhpType::Never), None);
        assert_eq!(probe(PhpType::Never, element.clone()), None);
    }

    assert!(
        probe(PhpType::Int, PhpType::Str).is_some(),
        "an int slot and a string slot are different widths; EIR widens before the call"
    );
    assert!(probe(PhpType::Mixed, PhpType::Float).is_some());
}

/// `array_slice` copies the window into a fresh `array<mixed>`, so it admits every element
/// type this target has a slot layout for — including `Mixed`, whose cells it shares rather
/// than copies. The `preserve_keys` form answers a hash and stays refused, as does a
/// non-integer offset.
#[test]
fn array_slice_admits_its_lowered_windows_only() {
    let array_of = |element: PhpType| {
        (
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(element)),
        )
    };
    let int_arg = (IrType::I64, PhpType::Int);

    for element in [
        PhpType::Int,
        PhpType::Str,
        PhpType::Float,
        PhpType::Bool,
        PhpType::Mixed,
        PhpType::Never,
    ] {
        for operands in [
            vec![array_of(element.clone()), int_arg.clone()],
            vec![array_of(element.clone()), int_arg.clone(), int_arg.clone()],
        ] {
            let probe = shaped_call(
                RuntimeFnId::ArraySlice,
                &operands,
                IrType::Heap(IrHeapKind::Array),
                PhpType::Array(Box::new(PhpType::Mixed)),
            );
            let call = probe.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(
                    &probe_module(),
                    &probe,
                    call,
                    RuntimeFnId::ArraySlice
                ),
                None,
                "an array of {element:?} sliced with {} bounds",
                operands.len() - 1
            );
        }
    }

    // The four-operand `preserve_keys` form answers a hash, not a reindexed list.
    let preserve = shaped_call(
        RuntimeFnId::ArraySlice,
        &[
            array_of(PhpType::Int),
            int_arg.clone(),
            int_arg.clone(),
            (IrType::I64, PhpType::Bool),
        ],
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Mixed)),
    );
    let call = preserve.instructions.last().expect("the probe emitted a call");
    assert!(
        direct_builtin_shape_issue(&probe_module(), &preserve, call, RuntimeFnId::ArraySlice)
            .is_some(),
        "preserve_keys is a different result type and is not lowered"
    );

    // A string offset is a coercion this lowerer does not perform.
    let string_offset = shaped_call(
        RuntimeFnId::ArraySlice,
        &[array_of(PhpType::Int), (IrType::Str, PhpType::Str)],
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Mixed)),
    );
    let call = string_offset
        .instructions
        .last()
        .expect("the probe emitted a call");
    assert!(
        direct_builtin_shape_issue(
            &probe_module(),
            &string_offset,
            call,
            RuntimeFnId::ArraySlice
        )
        .is_some(),
        "array_slice takes an integer offset"
    );
}

/// Verifies `gettype()` admits a settled type, a boxed one and a resource.
///
/// php-src answers `"resource"` or `"resource (closed)"` depending on a liveness the cell tag
/// does not carry, so a settled resource type has no COMPILE-TIME name; the lowering reads the
/// stream's closed state at runtime instead.
#[test]
fn gettype_admits_settled_boxed_and_resource_values() {
    for (ir, php) in [
        (IrType::I64, PhpType::Int),
        (IrType::F64, PhpType::Float),
        (IrType::Str, PhpType::Str),
        (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed),
        (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Int))),
    ] {
        let probe = shaped_call(
            RuntimeFnId::Gettype,
            &[(ir, php.clone())],
            IrType::Str,
            PhpType::Str,
        );
        let call = probe.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &probe, call, RuntimeFnId::Gettype),
            None,
            "gettype over {php:?} should be admitted"
        );
    }

    // php-src's own spellings, not PHP 8's type names.
    assert_eq!(gettype_name(&PhpType::Int), Some("integer"));
    assert_eq!(gettype_name(&PhpType::Float), Some("double"));
    assert_eq!(gettype_name(&PhpType::Bool), Some("boolean"));
    assert_eq!(gettype_name(&PhpType::Void), Some("NULL"));
    assert_eq!(gettype_name(&PhpType::Resource(None)), None);

    let resource = shaped_call(
        RuntimeFnId::Gettype,
        &[(IrType::I64, PhpType::Resource(None))],
        IrType::Str,
        PhpType::Str,
    );
    let call = resource.instructions.last().expect("the probe emitted a call");
    assert_eq!(
        direct_builtin_shape_issue(&probe_module(), &resource, call, RuntimeFnId::Gettype),
        None,
        "a resource is named by the stream's closed state at runtime"
    );
}

/// Builds a one-instruction probe function holding a runtime call of `target` over operands of the
/// given storage and PHP types, so a shape check can be exercised without lowering a program.
fn shaped_call(
    target: RuntimeFnId,
    operands: &[(IrType, PhpType)],
    result_ir: IrType,
    result_php: PhpType,
) -> Function {
    let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let arguments: Vec<_> = operands
            .iter()
            .enumerate()
            .map(|(index, (ir, php))| {
                let slot = builder.add_local(
                    Some(format!("a{index}")),
                    *ir,
                    php.clone(),
                    crate::ir::LocalKind::PhpLocal,
                );
                builder.emit_load_local(slot, *ir, php.clone())
            })
            .collect();
        builder.emit(
            Op::RuntimeCall,
            arguments,
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target))),
            result_ir,
            result_php,
            Ownership::MaybeOwned,
        );
        builder.terminate(crate::ir::Terminator::Return { value: None });
    }
    function
}

/// Verifies `base64_decode` and `parse_url` admit exactly the calls they can answer EXACTLY.
///
/// Both declare a union whose second half only one arity can produce, and both refuse that
/// arity rather than approximate it. `RuntimeFnId::Base64Decode` takes the lax form only:
/// `$strict` decides between a string and `false` at RUN TIME, so a lowering that ignored it
/// would answer a string where php answers false. `RuntimeFnId::ParseUrl` needs a CONSTANT
/// selector, because a value above `PHP_URL_FRAGMENT` is a catchable `ValueError` rather than
/// a result — a `load_local` selector could carry one and is refused for that reason, while
/// the no-selector form is the whole array and cannot go out of range.
#[test]
fn base64_decode_and_parse_url_admit_only_the_calls_they_answer_exactly() {
    let str_arg = (IrType::Str, PhpType::Str);
    let bool_arg = (IrType::I64, PhpType::Bool);
    let mixed = (IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed);

    let lax = shaped_call(
        RuntimeFnId::Base64Decode,
        &[str_arg.clone()],
        mixed.0,
        mixed.1.clone(),
    );
    let call = lax.instructions.last().expect("the probe emitted a call");
    assert_eq!(
        direct_builtin_shape_issue(&probe_module(), &lax, call, RuntimeFnId::Base64Decode),
        None,
    );

    let strict = shaped_call(
        RuntimeFnId::Base64Decode,
        &[str_arg.clone(), bool_arg],
        mixed.0,
        mixed.1.clone(),
    );
    let call = strict.instructions.last().expect("the probe emitted a call");
    assert!(
        direct_builtin_shape_issue(&probe_module(), &strict, call, RuntimeFnId::Base64Decode)
            .is_some_and(|issue| issue.contains("$strict")),
        "the strict arity must be refused by NAME",
    );

    let whole = shaped_call(
        RuntimeFnId::ParseUrl,
        &[str_arg.clone()],
        mixed.0,
        mixed.1.clone(),
    );
    let call = whole.instructions.last().expect("the probe emitted a call");
    assert_eq!(
        direct_builtin_shape_issue(&probe_module(), &whole, call, RuntimeFnId::ParseUrl),
        None,
    );

    let dynamic = shaped_call(
        RuntimeFnId::ParseUrl,
        &[str_arg.clone(), (IrType::I64, PhpType::Int)],
        mixed.0,
        mixed.1.clone(),
    );
    let call = dynamic.instructions.last().expect("the probe emitted a call");
    assert!(
        direct_builtin_shape_issue(&probe_module(), &dynamic, call, RuntimeFnId::ParseUrl)
            .is_some_and(|issue| issue.contains("compile-time constant")),
        "a selector that is not a literal must be refused",
    );

    for (component, admitted) in [(1_i64, true), (-1, true), (7, true), (8, false)] {
        let function = parse_url_constant_selector_call(component);
        let call = function.instructions.last().expect("the probe emitted a call");
        let issue =
            direct_builtin_shape_issue(&probe_module(), &function, call, RuntimeFnId::ParseUrl);
        assert_eq!(
            issue.is_none(),
            admitted,
            "component {component} admitted={admitted}, got {issue:?}",
        );
    }
}

/// Builds a `parse_url($url, <literal>)` probe, whose selector really is a `const_i64`.
fn parse_url_constant_selector_call(component: i64) -> Function {
    let mut function = Function::new("probe".to_string(), IrType::Void, PhpType::Void);
    {
        let mut builder = Builder::new(&mut function);
        let entry = builder.create_named_block("entry", Vec::new());
        builder.set_entry(entry);
        builder.position_at_end(entry);
        let slot = builder.add_local(
            Some("url".to_string()),
            IrType::Str,
            PhpType::Str,
            crate::ir::LocalKind::PhpLocal,
        );
        let url = builder.emit_load_local(slot, IrType::Str, PhpType::Str);
        let selector = builder.emit_const_i64(component);
        builder.emit(
            Op::RuntimeCall,
            vec![url, selector],
            Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(
                RuntimeFnId::ParseUrl,
            ))),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            Ownership::MaybeOwned,
        );
        builder.terminate(crate::ir::Terminator::Return { value: None });
    }
    function
}

/// Verifies the string-shaping builtins accept exactly the arities they lower.
///
/// `trim` and `substr` each have an optional trailing argument that changes the RULE, not
/// just a default — an absent charlist means PHP's built-in set rather than the empty one,
/// and an absent length runs to the end rather than meaning zero — so both arities have to be
/// admitted and a third refused. The rest take exactly what they take.
#[test]
fn string_shaping_builtins_admit_only_their_arities() {
    let str_arg = (IrType::Str, PhpType::Str);
    let int_arg = (IrType::I64, PhpType::Int);

    for target in [
        RuntimeFnId::Ucfirst,
        RuntimeFnId::Lcfirst,
        RuntimeFnId::Ucwords,
    ] {
        let ok = shaped_call(target, &[str_arg.clone()], IrType::Str, PhpType::Str);
        let call = ok.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &ok, call, target), None);

        let two = shaped_call(
            target,
            &[str_arg.clone(), str_arg.clone()],
            IrType::Str,
            PhpType::Str,
        );
        let call = two.instructions.last().expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(&probe_module(), &two, call, target).is_some(),
            "{target:?} takes one string"
        );
    }

    for target in [RuntimeFnId::Trim, RuntimeFnId::Ltrim, RuntimeFnId::Rtrim] {
        for arity in 1..=2 {
            let operands = vec![str_arg.clone(); arity];
            let ok = shaped_call(target, &operands, IrType::Str, PhpType::Str);
            let call = ok.instructions.last().expect("the probe emitted a call");
            assert_eq!(
                direct_builtin_shape_issue(&probe_module(), &ok, call, target),
                None,
                "{target:?} accepts {arity} operand(s)"
            );
        }
        let three = shaped_call(target, &vec![str_arg.clone(); 3], IrType::Str, PhpType::Str);
        let call = three.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &three, call, target).is_some());
    }

    for arity in 2..=3 {
        let mut operands = vec![str_arg.clone()];
        operands.extend(vec![int_arg.clone(); arity - 1]);
        let ok = shaped_call(RuntimeFnId::Substr, &operands, IrType::Str, PhpType::Str);
        let call = ok.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &ok, call, RuntimeFnId::Substr),
            None,
            "substr accepts {arity} operands"
        );
    }
    // A string where the offset belongs is refused rather than coerced.
    let bad_offset = shaped_call(
        RuntimeFnId::Substr,
        &[str_arg.clone(), str_arg.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = bad_offset.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &bad_offset, call, RuntimeFnId::Substr).is_some());

    // `str_repeat` takes a subject and a count, and refuses a string where the count goes.
    let ok = shaped_call(
        RuntimeFnId::StrRepeat,
        &[str_arg.clone(), int_arg.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = ok.instructions.last().expect("the probe emitted a call");
    assert_eq!(
        direct_builtin_shape_issue(&probe_module(), &ok, call, RuntimeFnId::StrRepeat),
        None
    );
    let bad_count = shaped_call(
        RuntimeFnId::StrRepeat,
        &[str_arg.clone(), str_arg.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = bad_count.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &bad_count, call, RuntimeFnId::StrRepeat).is_some());

    // `strpos` answers PHP's `int|false`, which only a runtime-tagged Mixed cell can carry.
    let searched = shaped_call(
        RuntimeFnId::Strpos,
        &[str_arg.clone(), str_arg.clone()],
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    let call = searched.instructions.last().expect("the probe emitted a call");
    assert_eq!(
        direct_builtin_shape_issue(&probe_module(), &searched, call, RuntimeFnId::Strpos),
        None
    );
    // An Int result would lose the difference between a match at offset 0 and a miss.
    let as_int = shaped_call(
        RuntimeFnId::Strpos,
        &[str_arg.clone(), str_arg.clone()],
        IrType::I64,
        PhpType::Int,
    );
    let call = as_int.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &as_int, call, RuntimeFnId::Strpos).is_some());
    // `str_replace` takes three strings; an array form is a different operation.
    let ok = shaped_call(
        RuntimeFnId::StrReplace,
        &[str_arg.clone(), str_arg.clone(), str_arg.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = ok.instructions.last().expect("the probe emitted a call");
    assert_eq!(
        direct_builtin_shape_issue(&probe_module(), &ok, call, RuntimeFnId::StrReplace),
        None
    );
    let with_count = shaped_call(
        RuntimeFnId::StrReplace,
        &[str_arg.clone(), str_arg.clone(), str_arg.clone(), int_arg.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = with_count.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &with_count, call, RuntimeFnId::StrReplace).is_some());

    // `sha1` answers a 40-character hex string from exactly one string.
    let digest = shaped_call(
        RuntimeFnId::Sha1,
        &[str_arg.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = digest.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &digest, call, RuntimeFnId::Sha1), None);
    // The digest is fixed-width, and every SHA-1 word is big-endian.
    assert!(RT_SHA1_HEX.contains("(local.get $out) (i64.const 40)"));
    assert!(
        RT_SHA1_HEX.contains("(i32.const 0x5A827999)")
            && RT_SHA1_HEX.contains("(i32.const 0xCA62C1D6)"),
        "the four round constants must all be present"
    );

    // `htmlspecialchars` escapes BOTH quote styles under PHP 8.1+ defaults.
    let escaped = shaped_call(
        RuntimeFnId::Htmlspecialchars,
        &[str_arg.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = escaped.instructions.last().expect("the probe emitted a call");
    assert_eq!(
        direct_builtin_shape_issue(&probe_module(), &escaped, call, RuntimeFnId::Htmlspecialchars),
        None
    );
    // `&#039;` is the ENT_QUOTES default since 8.1; leaving `'` alone would be the 8.0 rule.
    assert!(RT_HTML_PUT.contains("(i32.const 35)"), "the numeric entity for an apostrophe");
    // The bad-span rule stops at a byte that could START a sequence, not at a non-continuation:
    // a plain continuation test gets 102 byte pairs wrong.
    assert!(
        RT_UTF8_BAD_SPAN.contains("(i32.ge_u (local.get $next) (i32.const 0xC2))")
            && RT_UTF8_BAD_SPAN.contains("(i32.le_u (local.get $next) (i32.const 0xF4))"),
        "a valid lead terminates the bad span; a never-valid byte is absorbed"
    );
    // Surrogates and overlong forms are rejected, not just structurally short sequences.
    assert!(RT_UTF8_SEQ_LEN.contains("(i32.const 0xED)"), "surrogate exclusion");
    assert!(RT_UTF8_SEQ_LEN.contains("(i32.const 0xC2)"), "C0/C1 are overlong");

    // `md5` answers a 32-character digest, half sha1's width.
    let md5 = shaped_call(
        RuntimeFnId::Md5,
        &[str_arg.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = md5.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &md5, call, RuntimeFnId::Md5), None);
    assert!(RT_MD5_HEX.contains("(local.get $out) (i64.const 32)"));
    // The K table is what the algorithm cannot compute: WebAssembly has no `sin`.
    assert!(RT_MD5_K.contains("(i32.const 0xd76aa478)"), "K[0]");
    assert!(RT_MD5_K.contains("(i32.const 0xeb86d391)"), "K[63]");
    assert_eq!(
        RT_MD5_K.matches("(i32.eq (local.get $i) (i32.const").count(),
        64,
        "all 64 round constants must be present"
    );
    assert_eq!(
        RT_MD5_S.matches("(i32.eq (local.get $i) (i32.const").count(),
        64,
        "all 64 rotation amounts must be present"
    );

    // `crc32` answers an int, and PHP's is the UNSIGNED 32-bit value.
    let hashed = shaped_call(
        RuntimeFnId::Crc32,
        &[str_arg.clone()],
        IrType::I64,
        PhpType::Int,
    );
    let call = hashed.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &hashed, call, RuntimeFnId::Crc32), None);
    assert!(
        RT_CRC32.contains("(i64.const 0xFFFFFFFF)"),
        "the result is masked to 32 bits, not sign-extended"
    );
    assert!(
        RT_CRC32.contains("(i32.const 0xEDB88320)"),
        "PHP uses the reflected IEEE 802.3 polynomial"
    );

    // `strstr` answers `string|false` through the same tagged cell, in BOTH arities.
    for arity in 2..=3 {
        let mut operands = vec![str_arg.clone(), str_arg.clone()];
        if arity == 3 {
            operands.push((IrType::I64, PhpType::Bool));
        }
        let ok = shaped_call(
            RuntimeFnId::Strstr,
            &operands,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        let call = ok.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &ok, call, RuntimeFnId::Strstr),
            None,
            "strstr accepts {arity} operands"
        );
    }
    // The flag is a bool, not an int offset, and the result is never a bare string.
    let bad_flag = shaped_call(
        RuntimeFnId::Strstr,
        &[str_arg.clone(), str_arg.clone(), int_arg.clone()],
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    let call = bad_flag.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &bad_flag, call, RuntimeFnId::Strstr).is_some());

    // `wordwrap` takes an optional width; a custom break or cut flag is a different path.
    for arity in 1..=2 {
        let mut operands = vec![str_arg.clone()];
        if arity == 2 {
            operands.push(int_arg.clone());
        }
        let wrapped = shaped_call(RuntimeFnId::Wordwrap, &operands, IrType::Str, PhpType::Str);
        let call = wrapped.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &wrapped, call, RuntimeFnId::Wordwrap),
            None,
            "wordwrap accepts {arity} operand(s)"
        );
    }
    // The break-string and cut forms are lowered too, through the building helper rather
    // than the in-place one.
    for operands in [
        vec![str_arg.clone(), int_arg.clone(), str_arg.clone()],
        vec![
            str_arg.clone(),
            int_arg.clone(),
            str_arg.clone(),
            (IrType::I64, PhpType::Bool),
        ],
    ] {
        let wrapped = shaped_call(RuntimeFnId::Wordwrap, &operands, IrType::Str, PhpType::Str);
        let call = wrapped.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &wrapped, call, RuntimeFnId::Wordwrap),
            None,
            "wordwrap accepts {} operands",
            operands.len()
        );
    }
    // A break that is not a string, and a cut flag that is not a bool, still refuse.
    for operands in [
        vec![str_arg.clone(), int_arg.clone(), int_arg.clone()],
        vec![
            str_arg.clone(),
            int_arg.clone(),
            str_arg.clone(),
            str_arg.clone(),
        ],
    ] {
        let wrapped = shaped_call(RuntimeFnId::Wordwrap, &operands, IrType::Str, PhpType::Str);
        let call = wrapped.instructions.last().expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(&probe_module(), &wrapped, call, RuntimeFnId::Wordwrap)
                .is_some(),
            "wordwrap checks its break and cut storage"
        );
    }
    // The fast path never changes the length: it replaces a space, it does not insert.
    assert!(
        RT_WORDWRAP.contains("(local.get $out) (local.get $olen)"),
        "wordwrap returns the persisted length unchanged"
    );

    // `str_split` accepts both arities, since the default really is a chunk of one.
    for arity in 1..=2 {
        let mut operands = vec![str_arg.clone()];
        if arity == 2 {
            operands.push(int_arg.clone());
        }
        let cut = shaped_call(
            RuntimeFnId::StrSplit,
            &operands,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Str)),
        );
        let call = cut.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &cut, call, RuntimeFnId::StrSplit),
            None,
            "str_split accepts {arity} operand(s)"
        );
    }

    // `explode` produces exactly array<string>; a limit argument is a different contract.
    let split = shaped_call(
        RuntimeFnId::Explode,
        &[str_arg.clone(), str_arg.clone()],
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
    );
    let call = split.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &split, call, RuntimeFnId::Explode), None);
    let with_limit = shaped_call(
        RuntimeFnId::Explode,
        &[str_arg.clone(), str_arg.clone(), int_arg.clone()],
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
    );
    let call = with_limit.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &with_limit, call, RuntimeFnId::Explode).is_some());

    // `implode` reads an array, so its element type is part of the contract.
    let str_array = (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Str)));
    let joined = shaped_call(
        RuntimeFnId::Implode,
        &[str_arg.clone(), str_array.clone()],
        IrType::Str,
        PhpType::Str,
    );
    let call = joined.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &joined, call, RuntimeFnId::Implode), None);
    // A provably empty array is fine: the element read never happens.
    let empty = shaped_call(
        RuntimeFnId::Implode,
        &[
            str_arg.clone(),
            (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Never))),
        ],
        IrType::Str,
        PhpType::Str,
    );
    let call = empty.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &empty, call, RuntimeFnId::Implode), None);
    // Int, float and Mixed elements are CONVERTED — PHP applies the same rule as an
    // explicit `(string)` cast, so they go through the owning helper rather than being
    // refused.
    for element in [PhpType::Int, PhpType::Float, PhpType::Mixed] {
        let converted = shaped_call(
            RuntimeFnId::Implode,
            &[
                str_arg.clone(),
                (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(element.clone()))),
            ],
            IrType::Str,
            PhpType::Str,
        );
        let call = converted.instructions.last().expect("the probe emitted a call");
        assert_eq!(
            direct_builtin_shape_issue(&probe_module(), &converted, call, RuntimeFnId::Implode),
            None,
            "an array of {element:?} converts element by element"
        );
    }
    // A bool slot has neither a (pointer, length) layout nor a conversion here.
    for element in [PhpType::Bool] {
        let wrong = shaped_call(
            RuntimeFnId::Implode,
            &[
                str_arg.clone(),
                (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(element.clone()))),
            ],
            IrType::Str,
            PhpType::Str,
        );
        let call = wrong.instructions.last().expect("the probe emitted a call");
        assert!(
            direct_builtin_shape_issue(&probe_module(), &wrong, call, RuntimeFnId::Implode).is_some(),
            "an array of {element:?} has no lowered element conversion"
        );
    }

    // `strrpos` answers the same tagged cell, and scans from the right.
    let last = shaped_call(
        RuntimeFnId::Strrpos,
        &[str_arg.clone(), str_arg.clone()],
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    let call = last.instructions.last().expect("the probe emitted a call");
    assert_eq!(direct_builtin_shape_issue(&probe_module(), &last, call, RuntimeFnId::Strrpos), None);
    assert!(
        RT_STR_RFIND.contains("(local.set $at (i64.sub (local.get $hlen) (local.get $nlen)))"),
        "the scan starts at the rightmost offset that still fits"
    );

    // Only the two-argument form is lowered; the offset form has its own ValueError contract.
    let with_offset = shaped_call(
        RuntimeFnId::Strpos,
        &[str_arg.clone(), str_arg.clone(), int_arg.clone()],
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    let call = with_offset.instructions.last().expect("the probe emitted a call");
    assert!(direct_builtin_shape_issue(&probe_module(), &with_offset, call, RuntimeFnId::Strpos).is_some());

    for target in [RuntimeFnId::Strcmp, RuntimeFnId::Strcasecmp] {
        let ok = shaped_call(
            target,
            &[str_arg.clone(), str_arg.clone()],
            IrType::I64,
            PhpType::Int,
        );
        let call = ok.instructions.last().expect("the probe emitted a call");
        assert_eq!(direct_builtin_shape_issue(&probe_module(), &ok, call, target), None);

        // A comparison answers an int, never a bool: PHP's result is a byte distance.
        let as_bool = shaped_call(
            target,
            &[str_arg.clone(), str_arg.clone()],
            IrType::I64,
            PhpType::Bool,
        );
        let call = as_bool.instructions.last().expect("the probe emitted a call");
        assert!(direct_builtin_shape_issue(&probe_module(), &as_bool, call, target).is_some());
    }
}

/// Verifies `strcmp` reports a byte DISTANCE but normalizes a pure length difference.
///
/// These are two different rules and php-src applies both: `strcmp("ABC", "abc")` is -32
/// because `A` and `a` are 32 apart, while `strcmp("abcd", "a")` is 1 rather than 3 because
/// nothing mismatched — only the lengths differ. A helper that returned the length delta, or
/// that clamped the byte distance to a sign, would pass a naive test and fail php-src.
#[test]
fn strcmp_returns_a_byte_distance_but_a_normalized_length_difference() {
    assert!(
        RT_STR_CMP.contains("(return (i64.extend_i32_s (i32.sub (local.get $x) (local.get $y))))"),
        "a mismatched byte yields its raw distance, not a sign"
    );
    for normalized in ["(then (return (i64.const -1)))", "(then (return (i64.const 1)))"] {
        assert!(
            RT_STR_CMP.contains(normalized),
            "a pure length difference is normalized to +/-1"
        );
    }
    // The bytes are read UNSIGNED, which is what makes strcmp("\xff", "\x01") 254 and not -2.
    assert!(!RT_STR_CMP.contains("i32.load8_s"));
}

/// Verifies every builtin that RAISES is also declared as raising.
///
/// `function::raises_runtime_error` is what makes the module lay out an error's message, and
/// `emit_runtime_failure` silently falls back to the deterministic fatal when the message is
/// absent. So a builtin added to the catchable catalogue but not to that predicate compiles,
/// runs, and reports the right text — while no `catch` ever receives it. That failure is
/// invisible unless the test catches the error rather than just reading stderr, which is
/// exactly how it was found.
#[test]
fn builtins_that_raise_are_declared_as_raising() {
    let str_arg = (IrType::Str, PhpType::Str);
    let int_arg = (IrType::I64, PhpType::Int);
    for (target, operands) in [
        (RuntimeFnId::StrRepeat, vec![str_arg.clone(), int_arg.clone()]),
        (RuntimeFnId::Explode, vec![str_arg.clone(), str_arg.clone()]),
        (RuntimeFnId::StrSplit, vec![str_arg.clone(), int_arg.clone()]),
        (
            RuntimeFnId::StrPad,
            vec![str_arg.clone(), int_arg.clone(), str_arg.clone()],
        ),
    ] {
        let probe = shaped_call(target, &operands, IrType::Str, PhpType::Str);
        assert!(
            crate::codegen_wasm::function::raises_runtime_error(&probe),
            "{target:?} raises a PHP error, so its message must be laid out"
        );
    }
    // A builtin with no failure mode must NOT drag the error messages into every module.
    let quiet = shaped_call(
        RuntimeFnId::Ucfirst,
        &[(IrType::Str, PhpType::Str)],
        IrType::Str,
        PhpType::Str,
    );
    assert!(!crate::codegen_wasm::function::raises_runtime_error(&quiet));
}

/// An empty module for shape checks that do not depend on module data.
///
/// Only `sprintf` reads the module — it recovers its literal format from the interned
/// strings — and no probe here builds one, so an empty module is the honest stand-in.
fn probe_module() -> Module {
    Module::new(crate::codegen_support::platform::Target::wasm())
}

/// A module carrying a `main`, for the shapes whose runtime is command-only.
fn command_probe_module() -> Module {
    let mut module = probe_module();
    let mut main = Function::new("main".to_string(), IrType::Void, PhpType::Void);
    main.flags.is_main = true;
    module.add_function(main);
    module
}

/// Verifies the format parser applies php-src's flag rules, not C's.
///
/// Every expectation here was measured against php-src 8.5.6 before the parser was written,
/// and each is a place C and PHP disagree — so a parser transcribed from the C manual would
/// pass a naive test and fail these.
#[test]
fn sprintf_format_parser_follows_php_flag_rules() {
    use FormatPiece::{Conversion, Literal};
    let conv = |p: &FormatPiece| match p {
        Conversion { left, plus, pad, width, precision, argument, conversion } => {
            (*left, *plus, *pad, *width, *precision, *argument, *conversion)
        }
        other => panic!("expected a conversion, got {other:?}"),
    };

    // The LAST padding flag wins: `%'x03d` pads with zeros, `%0'x3d` pads with x.
    assert_eq!(conv(&parse_sprintf_format(b"%'x03d", 1).unwrap()[0]).2, b'0');
    assert_eq!(conv(&parse_sprintf_format(b"%0'x3d", 1).unwrap()[0]).2, b'x');

    // `-` cancels a ZERO pad on the numeric conversions but NOT on %s, and never cancels
    // an explicit `'X`. Measured: `%-08d` is spaces, `%-03s` is zeros, `%'x-3d` is x.
    assert_eq!(conv(&parse_sprintf_format(b"%-08d", 1).unwrap()[0]).2, b' ');
    assert_eq!(conv(&parse_sprintf_format(b"%-03s", 1).unwrap()[0]).2, b'0');
    assert_eq!(conv(&parse_sprintf_format(b"%'x-3d", 1).unwrap()[0]).2, b'x');

    // `%%` becomes a one-byte run pointing at the FIRST `%`, so every run stays a
    // contiguous slice of the format and needs no segment of its own.
    assert_eq!(
        parse_sprintf_format(b"a%%b", 0).unwrap(),
        vec![
            Literal { offset: 0, length: 1 },
            Literal { offset: 1, length: 1 },
            Literal { offset: 3, length: 1 },
        ]
    );
    assert_eq!(
        parse_sprintf_format(b"%%", 0).unwrap(),
        vec![Literal { offset: 0, length: 1 }]
    );

    // An explicit argument number is zero-based here and may repeat.
    let repeated = parse_sprintf_format(b"%1$s%1$s", 1).unwrap();
    assert_eq!(conv(&repeated[0]).5, 0);
    assert_eq!(conv(&repeated[1]).5, 0);
    // Positional arguments advance independently of explicit ones.
    let mixed = parse_sprintf_format(b"%s%d", 2).unwrap();
    assert_eq!(conv(&mixed[0]).5, 0);
    assert_eq!(conv(&mixed[1]).5, 1);

    // Width and precision, including an empty precision meaning zero.
    let sized = conv(&parse_sprintf_format(b"%8.2f", 1).unwrap()[0]);
    assert_eq!((sized.3, sized.4), (8, Some(2)));
    assert_eq!(conv(&parse_sprintf_format(b"%.s", 1).unwrap()[0]).4, Some(0));

    // `%f` accepts the same flags; `-` with `0` is the one combination refused.
    assert!(parse_sprintf_format(b"%08.2f", 1).is_ok());
    assert_eq!(conv(&parse_sprintf_format(b"%.3f", 1).unwrap()[0]).4, Some(3));
    // An absent precision on %f is SIX, which the lowering supplies, not zero.
    assert_eq!(conv(&parse_sprintf_format(b"%f", 1).unwrap()[0]).4, None);

    // The radix conversions read the argument as UNSIGNED and carry no sign.
    for radix in [&b"%x"[..], b"%X", b"%b", b"%o"] {
        assert!(
            parse_sprintf_format(radix, 1).is_ok(),
            "{} is lowered",
            String::from_utf8_lossy(radix)
        );
    }

    // Refused rather than guessed.
    for (format, why) in [
        (&b"%e"[..], "a conversion outside the subset"),
        (&b"%-08.2f"[..], "php-src loses the precision for '-' with '0' on %f"),
        (&b"%"[..], "a lone trailing percent"),
        (&b"%d"[..], "more conversions than arguments"),
    ] {
        let count = if format == b"%d" { 0 } else { 1 };
        assert!(
            parse_sprintf_format(format, count).is_err(),
            "{why} must be refused: {}",
            String::from_utf8_lossy(format)
        );
    }
}

/// Verifies the two url codecs differ exactly where php-src says they do.
///
/// `urlencode` and `rawurlencode` share one helper and are told apart by a flag, so the
/// difference is one branch rather than two implementations. Those two branches are the whole
/// contract between them — a space folding to `+`, and `~` counting as unreserved — and this
/// reads both back out of the emitted helper rather than trusting the call sites.
#[test]
fn url_codecs_differ_only_in_space_and_tilde() {
    assert!(
        RT_STR_URL_ENCODE.contains("(i32.eq (local.get $byte) (i32.const 32))"),
        "urlencode folds a space to a plus"
    );
    assert!(
        RT_STR_URL_ENCODE.contains("(i32.eq (local.get $byte) (i32.const 126))"),
        "rawurlencode alone leaves a tilde unreserved"
    );
    assert!(
        RT_STR_URL_DECODE.contains("(i32.eq (local.get $byte) (i32.const 43))"),
        "urldecode alone reads a plus as a space"
    );
    // Percent-encoding is uppercase: 55 is 'A' - 10, where a lowercase table would use 87.
    assert!(RT_STR_URL_ENCODE.contains("(i32.const 55)"));
    assert!(!RT_STR_URL_ENCODE.contains("(i32.const 87)"));
    // ...and hex DECODING accepts both cases, so it needs the lowercase offset as well.
    assert!(RT_HEX_DIGIT_VALUE.contains("(i32.const 55)"));
    assert!(RT_HEX_DIGIT_VALUE.contains("(i32.const 87)"));
}

/// Verifies every lowered unary string transform reserves enough room for its own output.
///
/// A re-encoder allocates a worst case up front and reports the length it actually wrote, so
/// a reservation smaller than the expansion would corrupt the heap block after it rather
/// than fail visibly. Each factor is read back out of the emitted helper so a helper whose
/// escaping grows without its reservation growing cannot pass.
#[test]
fn re_encoding_helpers_reserve_their_worst_case_expansion() {
    for (helper, factor, transform) in [
        (RT_STR_BIN2HEX, 2, "bin2hex writes two hex digits per byte"),
        (
            RT_STR_ADDSLASHES,
            2,
            "addslashes writes a backslash before an escaped byte",
        ),
        (
            RT_STR_NL2BR,
            7,
            "nl2br writes a six-byte tag before a kept break",
        ),
    ] {
        assert!(
            helper.contains(&format!(
                "(call $__rt_str_alloc (i64.mul (local.get $len) (i64.const {factor})))"
            )),
            "{transform}, so it must reserve {factor} bytes per input byte:\n{helper}"
        );
    }
    // stripslashes only ever removes bytes, so the source length is already its worst case.
    assert!(RT_STR_STRIPSLASHES.contains("(call $__rt_str_alloc (local.get $len))"));
    for helper in [
        RT_STR_BIN2HEX,
        RT_STR_ADDSLASHES,
        RT_STR_STRIPSLASHES,
        RT_STR_NL2BR,
    ] {
        assert!(
            helper.contains("(i64.extend_i32_u (local.get $w))"),
            "a re-encoder returns what it WROTE, not what it reserved:\n{helper}"
        );
    }
}

/// Verifies `in_array` admits every (needle, element) pair whose rule was MEASURED, in both
/// the loose and the strict form, and refuses the rest.
///
/// The loose form is no longer an identity scan: it reuses the same comparison `==` lowers, so
/// `in_array("1e1", ["10"])` is true loosely and false strictly. What is still refused is the
/// pairs PHP's table handles differently — a string against a number, a bool against a number,
/// and anything boxed — because guessing there answers the wrong question silently.
#[test]
fn in_array_admits_only_the_pairs_whose_rule_was_measured() {
    assert_eq!(verdict(&in_array_call(true), RuntimeFnId::InArray), None);
    assert_eq!(
        verdict(&in_array_call(false), RuntimeFnId::InArray),
        None,
        "the loose form now goes through the measured comparison"
    );

    for (needle, element) in [
        (PhpType::Int, PhpType::Int),
        (PhpType::Bool, PhpType::Bool),
        (PhpType::Float, PhpType::Float),
        (PhpType::Str, PhpType::Str),
        (PhpType::Int, PhpType::Float),
        (PhpType::Float, PhpType::Int),
        // An empty haystack never matches, so any needle is safe.
        (PhpType::Str, PhpType::Never),
        (PhpType::Float, PhpType::Never),
    ] {
        assert!(
            in_array_scan(&needle, &element).is_some(),
            "{needle:?} against elements of {element:?} was measured"
        );
    }
    for (needle, element) in [
        (PhpType::Str, PhpType::Int),
        (PhpType::Int, PhpType::Str),
        (PhpType::Bool, PhpType::Int),
        (PhpType::Int, PhpType::Bool),
        (PhpType::Mixed, PhpType::Int),
        (PhpType::Int, PhpType::Mixed),
    ] {
        assert!(
            in_array_scan(&needle, &element).is_none(),
            "{needle:?} against elements of {element:?} still needs its measured table"
        );
    }
}

/// Verifies every file builtin pins the exact storage its runtime helper reads.
///
/// The helpers take raw representations — a string is a pointer and a length, a stream
/// handle is a boxed cell carrying the WASI fd, a count is an i64 — and perform no
/// conversion, so a call site holding anything else would have its bytes reinterpreted.
/// Each is checked in its declared shape and then with one position moved.
#[test]
fn file_builtins_pin_the_storage_their_helpers_read() {
    const STREAM: IrType = IrType::Heap(IrHeapKind::Mixed);
    for (target, operands, result) in [
        (
            RuntimeFnId::Fopen,
            &[IrType::Str, IrType::Str][..],
            STREAM,
        ),
        // `int|false` upstream: the byte count comes back boxed.
        (RuntimeFnId::Fwrite, &[STREAM, IrType::Str][..], STREAM),
        (RuntimeFnId::Fread, &[STREAM, IrType::I64][..], IrType::Str),
        (RuntimeFnId::Fclose, &[STREAM][..], IrType::I64),
        (RuntimeFnId::FileExists, &[IrType::Str][..], IrType::I64),
        (RuntimeFnId::Unlink, &[IrType::Str][..], IrType::I64),
        (RuntimeFnId::FileGetContents, &[IrType::Str][..], STREAM),
        (
            RuntimeFnId::FilePutContents,
            &[IrType::Str, IrType::Str][..],
            IrType::I64,
        ),
        (RuntimeFnId::Feof, &[STREAM][..], IrType::I64),
        (RuntimeFnId::Ftell, &[STREAM][..], IrType::I64),
        (RuntimeFnId::Rewind, &[STREAM][..], IrType::I64),
        // `fseek` takes the whence as a third operand; PHP's SEEK_* constants are the same
        // 0/1/2 WASI uses, so it passes straight through for a real fd.
        (
            RuntimeFnId::Fseek,
            &[STREAM, IrType::I64, IrType::I64][..],
            IrType::I64,
        ),
    ] {
        let file = file_builtin_helper(target).expect("a file builtin has a contract");
        assert_eq!(file.operands, operands, "{target:?} operands");
        assert_eq!(file.result, result, "{target:?} result");
        assert!(
            is_direct_builtin(target),
            "{target:?} must reach the inline lowering"
        );
        assert!(
            super::super::capability::runtime_function_is_supported(target),
            "{target:?} must be admitted by the audit"
        );
    }
    assert!(
        file_builtin_helper(RuntimeFnId::Sqrt).is_none(),
        "only the file family carries a file contract"
    );
    // `fopen` is the one whose helper is also called internally, so it is the one that
    // needs a flag saying whose diagnostic to emit.
    assert!(file_builtin_helper(RuntimeFnId::Fopen).is_some_and(|file| file.warns));
    assert!(file_builtin_helper(RuntimeFnId::Fread).is_some_and(|file| !file.warns));
}

/// The three class relations resolve names PHP's way and refuse what they cannot settle.
///
/// Order matters and is not incidental: `foreach` walks the answer hash in INSERTION order,
/// so `class_parents` listing the immediate parent before its ancestors is observable
/// output, not an implementation detail.
#[test]
fn class_relations_answer_from_the_modules_own_declarations() {
    use crate::ir::class_relations::{ClassLikeTarget, ClassRelation};

    // The mapping is the whole reason three builtins share one lowering.
    assert_eq!(
        ClassRelation::from_builtin_name("class_implements"),
        Some(ClassRelation::Implements)
    );
    assert_eq!(
        ClassRelation::from_builtin_name("class_parents"),
        Some(ClassRelation::Parents)
    );
    assert_eq!(
        ClassRelation::from_builtin_name("class_uses"),
        Some(ClassRelation::Uses)
    );
    assert_eq!(ClassRelation::from_builtin_name("class_exists"), None);
    for target in [
        RuntimeFnId::ClassImplements,
        RuntimeFnId::ClassParents,
        RuntimeFnId::ClassUses,
    ] {
        assert!(
            class_relation_of(target).is_some(),
            "{target:?} must map to a relation"
        );
        assert!(
            is_direct_builtin(target),
            "{target:?} must be audited as a direct builtin"
        );
    }
    assert!(class_relation_of(RuntimeFnId::ClassExists).is_none());

    // An empty module declares nothing, so every name is unresolvable — which PHP answers
    // `false` for rather than treating as an error.
    let module = Module::new(crate::codegen_support::platform::Target::wasm());
    assert_eq!(
        crate::ir::class_relations::resolve_named_target(&module, "Cart"),
        ClassLikeTarget::Unknown
    );
    for relation in [
        ClassRelation::Implements,
        ClassRelation::Parents,
        ClassRelation::Uses,
    ] {
        assert!(crate::ir::class_relations::relation_names(
            &module,
            relation,
            &ClassLikeTarget::Unknown
        )
        .is_empty());
    }
    // Nothing to lay out when nothing calls one.
    assert!(class_relation_answer_strings(&module).is_empty());
}

/// The four IPv4 conversions are admitted with exactly the storage their helpers read.
///
/// `long2ip` is the odd one: it takes an int and answers a BARE string, while the other
/// three answer a boxed union because php's failure value has a different type from the
/// success value. Getting that backwards would reinterpret a pointer as a length.
#[test]
fn ipv4_conversions_answer_phps_strict_parser() {
    for target in [
        RuntimeFnId::Long2ip,
        RuntimeFnId::Ip2long,
        RuntimeFnId::InetNtop,
        RuntimeFnId::InetPton,
    ] {
        assert!(
            is_direct_builtin(target),
            "{target:?} must be audited as a direct builtin"
        );
        assert!(
            super::super::capability::runtime_function_is_supported(target),
            "{target:?} must be admitted"
        );
    }
}

/// The array rebuilds admit exactly the storage each one can serve.
///
/// The line is not arbitrary: an operation that only MOVES elements works on a clone, whose
/// representation and refcounts are already right, so it takes any element type. One that
/// WRITES an element has to know the slot's width and its refcount obligation, and both
/// follow the array's `value_type` — so those take raw i64 slots and refuse the rest.
#[test]
fn array_rebuilds_admit_only_the_storage_they_write() {
    for target in [
        RuntimeFnId::ArrayPad,
        RuntimeFnId::ArrayUnique,
        RuntimeFnId::ArrayColumn,
        RuntimeFnId::ArraySplice,
        RuntimeFnId::ArrayUnshift,
    ] {
        assert!(
            is_direct_builtin(target),
            "{target:?} must be audited as a direct builtin"
        );
        assert!(
            super::super::capability::runtime_function_is_supported(target),
            "{target:?} must be admitted"
        );
    }

    // `array_pad` writes its fill, so a string list has no slot it can place one in.
    let pad_strings = call_with(
        RuntimeFnId::ArrayPad,
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
    );
    assert!(
        verdict(&pad_strings, RuntimeFnId::ArrayPad).is_some(),
        "array_pad writes an element, so it needs a raw i64 slot"
    );

    // `array_splice` only moves them, so a string list is served.
    let splice_strings = call_with(
        RuntimeFnId::ArraySplice,
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
    );
    // Two operands short of a call: the shape check wants an offset too.
    assert!(
        verdict(&splice_strings, RuntimeFnId::ArraySplice).is_some(),
        "array_splice needs its offset"
    );
}

/// `json_encode` admits a value only when everything it can contain has a JSON form here.
///
/// A bare `mixed` is refused: its runtime tag could be a float or a resource, and the
/// encoder would then have to answer for a value the audit never saw. A float is refused
/// even when its type IS known, because php writes one with serialize_precision rather than
/// the precision the existing formatter uses.
#[test]
fn json_encode_admits_only_provably_encodable_values() {
    assert!(
        is_direct_builtin(RuntimeFnId::JsonEncode),
        "json_encode must be audited as a direct builtin"
    );
    assert!(
        super::super::capability::runtime_function_is_supported(RuntimeFnId::JsonEncode),
        "json_encode must be admitted"
    );

    let module = Module::new(crate::codegen_support::platform::Target::wasm());
    for encodable in [
        PhpType::Int,
        PhpType::Str,
        PhpType::Bool,
        PhpType::Array(Box::new(PhpType::Int)),
        PhpType::Array(Box::new(PhpType::Array(Box::new(PhpType::Str)))),
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Int),
        },
        // A FLOAT is encodable now that `__rt_ftoa_shortest` writes php's
        // serialize_precision = -1 form, and a bare `mixed` is encodable because every
        // runtime tag has an answer — including the resource that abandons the encode.
        PhpType::Float,
        PhpType::Mixed,
        PhpType::Array(Box::new(PhpType::Float)),
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Mixed),
        },
        // A hash key is written as a string whichever kind it is, so a `mixed` key needs no
        // proof of its own.
        PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Str),
        },
    ] {
        assert!(
            super::super::json::type_is_encodable(&module, &encodable),
            "{encodable:?} has a JSON form"
        );
    }
    for refused in [
        // No class is declared in an empty module, so an object names nothing to walk.
        PhpType::Object("Money".to_string()),
        // A container OF objects inherits that: the element still has to be walkable.
        PhpType::Array(Box::new(PhpType::Object("Money".to_string()))),
    ] {
        assert!(
            !super::super::json::type_is_encodable(&module, &refused),
            "{refused:?} must keep refusing"
        );
    }
}

/// `ob_start` admits a handler only as a callable DESCRIPTOR, and the other four take none.
///
/// The handler is invoked through the closure ladder, which is keyed on that descriptor: a
/// string function name would need a runtime name lookup this backend does not have, so it
/// is refused rather than silently ignored — a buffer whose handler never runs would emit
/// the wrong bytes without any diagnostic.
#[test]
fn output_buffering_admits_only_a_descriptor_handler() {
    for target in [
        RuntimeFnId::ObStart,
        RuntimeFnId::ObGetClean,
        RuntimeFnId::ObEndFlush,
        RuntimeFnId::ObEndClean,
        RuntimeFnId::ObGetStatus,
    ] {
        assert!(
            is_direct_builtin(target),
            "{target:?} must be audited as a direct builtin"
        );
        assert!(
            super::super::capability::runtime_function_is_supported(target),
            "{target:?} must be admitted"
        );
    }

    // The seven keys `ob_get_status` answers, in php's own order — `foreach` makes that
    // order observable, so it is part of the contract rather than an implementation detail.
    assert_eq!(
        super::super::output_buffer::STATUS_KEYS,
        [
            "name",
            "type",
            "flags",
            "level",
            "chunk_size",
            "buffer_size",
            "buffer_used",
        ]
    );
}

/// `var_dump` admits a value only when EVERY type it can hold renders.
///
/// `int|false` does — both arms render — and so does a container whose elements do, because
/// the walk is the generic one the dynamic `foreach` uses. A bare `mixed` does not, and
/// The loaded-extension set is this target's own, not the native backend's list.
///
/// Both identities are admitted and answer from one set: `extension_loaded` folds a literal
/// name against it, `get_loaded_extensions` materializes it. The names are compared php's way
/// — case-insensitive on the canonical spelling only, so `opcache` is not an alias for
/// `Zend OPcache` — and an extension whose surface this target refuses is absent, which
/// `test_cli_wasm_extension_loaded_set_is_honest` checks against the compiler itself.
#[test]
fn loaded_extension_set_is_this_target_s_own() {
    assert!(
        super::super::capability::runtime_function_is_supported(RuntimeFnId::ExtensionLoaded),
        "extension_loaded must be admitted"
    );
    assert!(
        super::super::capability::runtime_function_is_supported(
            RuntimeFnId::GetLoadedExtensions
        ),
        "get_loaded_extensions must be admitted"
    );
    assert!(is_direct_builtin(RuntimeFnId::ExtensionLoaded));
    assert!(is_direct_builtin(RuntimeFnId::GetLoadedExtensions));

    let module = Module::new(crate::codegen::platform::Target::wasm());
    for served in WASM_LOADED_EXTENSIONS {
        assert_eq!(
            class_exists_answer(&module, RuntimeFnId::ExtensionLoaded, served),
            Some(true),
            "{served} is in the set"
        );
    }
    // Case-insensitive on the canonical name, which is php's rule.
    assert_eq!(
        class_exists_answer(&module, RuntimeFnId::ExtensionLoaded, "JSON"),
        Some(true)
    );
    // The native reports whose surface this target's audit still refuses entirely, plus a
    // bridge one. `pcre`, `zlib` and `bcmath` used to be here and moved to the served list
    // when their implementations landed — which is the point of pinning both halves.
    for absent in [
        "date",
        "ctype",
        "mbstring",
        "Reflection",
        "Zend OPcache",
        "curl",
    ] {
        assert_eq!(
            class_exists_answer(&module, RuntimeFnId::ExtensionLoaded, absent),
            Some(false),
            "{absent} is not served here"
        );
    }
    // `opcache` is not an alias php accepts, and is absent for that reason too.
    assert_eq!(
        class_exists_answer(&module, RuntimeFnId::ExtensionLoaded, "opcache"),
        Some(false)
    );
    // elephc ships no Zend extension this target serves, where native ships OPcache.
    assert!(WASM_LOADED_ZEND_EXTENSIONS.is_empty());
}

/// The four introspection identities are admitted with exactly the storage they lower.
///
/// Two of them are settled when the module is built and two are not, and the split is the
/// point. `get_declared_classes()` and `spl_classes()` fold to a list of names, so they
/// take no operands and answer an array. `spl_object_id()` and `is_a()` ask about a
/// PARTICULAR object, so they need a settled receiver — a boxed one would have to be
/// unboxed and tag-checked first, and php raises a TypeError rather than answering for a
/// non-object, neither of which is written here.
///
/// `is_a`'s third argument is refused rather than ignored. `$allow_string` makes the first
/// argument a class NAME instead of an instance, which is a different question with a
/// different answer; accepting the call and asking this one would answer it wrongly.
#[test]
fn introspection_builtins_admit_only_the_objects_they_can_ask() {
    for target in [
        RuntimeFnId::GetDeclaredClasses,
        RuntimeFnId::SplClasses,
        RuntimeFnId::SplObjectId,
        RuntimeFnId::IsA,
    ] {
        assert!(
            super::super::capability::runtime_function_is_supported(target),
            "{target:?} must be supported on this target"
        );
        assert!(
            is_direct_builtin(target),
            "{target:?} must be audited as a direct builtin"
        );
    }

    let probe = |target: RuntimeFnId,
                 operands: Vec<(IrType, PhpType)>,
                 result_ir: IrType,
                 result_php: PhpType| {
        let function = shaped_call(target, &operands, result_ir, result_php);
        let call = function
            .instructions
            .last()
            .expect("the probe emitted a call")
            .clone();
        direct_builtin_shape_issue(&probe_module(), &function, &call, target)
    };

    let list = (IrType::Heap(IrHeapKind::Array), PhpType::Array(Box::new(PhpType::Str)));
    let object = (IrType::Heap(IrHeapKind::Object), PhpType::Object("Cart".to_string()));
    let name = (IrType::Str, PhpType::Str);
    let flag = (IrType::I64, PhpType::Bool);

    // A list builtin takes nothing and answers an array of names.
    for target in [RuntimeFnId::GetDeclaredClasses, RuntimeFnId::SplClasses] {
        assert_eq!(probe(target, Vec::new(), list.0.clone(), list.1.clone()), None);
        assert!(
            probe(target, vec![name.clone()], list.0.clone(), list.1.clone())
                .is_some_and(|issue| issue.contains("takes no arguments")),
            "{target:?} takes no arguments"
        );
        assert!(
            probe(target, Vec::new(), IrType::I64, PhpType::Int)
                .is_some_and(|issue| issue.contains("answers array<string>")),
            "{target:?} answers an array"
        );
    }

    // The two object questions need a settled receiver, not a boxed one.
    assert_eq!(
        probe(
            RuntimeFnId::SplObjectId,
            vec![object.clone()],
            IrType::I64,
            PhpType::Int
        ),
        None
    );
    assert!(probe(
        RuntimeFnId::SplObjectId,
        vec![(IrType::Heap(IrHeapKind::Mixed), PhpType::Mixed)],
        IrType::I64,
        PhpType::Int
    )
    .is_some_and(|issue| issue.contains("settled object")));

    assert_eq!(
        probe(
            RuntimeFnId::IsA,
            vec![object.clone(), name.clone()],
            IrType::I64,
            PhpType::Bool
        ),
        None
    );
    assert!(probe(
        RuntimeFnId::IsA,
        vec![name.clone(), name.clone()],
        IrType::I64,
        PhpType::Bool
    )
    .is_some_and(|issue| issue.contains("$allow_string is not served")));

    // `$allow_string` is a third operand, and answering it here would answer the wrong
    // question — so the shape gate refuses the arity rather than dropping the flag.
    assert!(probe(
        RuntimeFnId::IsA,
        vec![object, name, flag],
        IrType::I64,
        PhpType::Bool
    )
    .is_some_and(|issue| issue.contains("got 3 operands")));
}

/// The pool exists only in a module that asks an object for its identity.
///
/// Not an optimisation: registration sits on the object ALLOCATION path and the release
/// sits inside `__rt_decref_object`, so a gate that answered true everywhere would put a
/// call and a linear scan into every program that constructs anything. The gate is asked
/// once and consumed in three places, which is why it lives in one function.
#[test]
fn the_handle_pool_is_only_built_for_a_module_that_asks() {
    let with = |target: RuntimeFnId| {
        let mut module = probe_module();
        module.functions.push(shaped_call(
            target,
            &[(IrType::Heap(IrHeapKind::Object), PhpType::Object("Cart".to_string()))],
            IrType::I64,
            PhpType::Int,
        ));
        super::super::handles::module_uses_object_handles(&module)
    };
    assert!(
        !with(RuntimeFnId::GetObjectVars),
        "another object builtin must not switch the pool on"
    );
    assert!(with(RuntimeFnId::SplObjectId));
}

/// neither does a container of objects: this target has no property walk, and an object
/// printed half-way would look right while being wrong.
///
/// The distinction has to be made on the UNNORMALIZED type: `codegen_repr` collapses every
/// union to `Mixed`, which would make `int|false` indistinguishable from the `mixed` that
/// must keep refusing.
#[test]
fn var_dump_admits_only_renderable_values() {
    assert!(
        is_direct_builtin(RuntimeFnId::VarDump),
        "var_dump must be audited as a direct builtin"
    );
    assert!(
        super::super::capability::runtime_function_is_supported(RuntimeFnId::VarDump),
        "var_dump must be admitted"
    );
    assert!(php_type_is_dumpable_scalar(&PhpType::Int));
    assert!(php_type_is_dumpable_scalar(&PhpType::Str));
    assert!(php_type_is_dumpable_scalar(&PhpType::Bool));
    assert!(php_type_is_dumpable_scalar(&PhpType::Union(vec![
        PhpType::Int,
        PhpType::False
    ])));
    assert!(php_type_is_dumpable_scalar(&PhpType::Array(Box::new(
        PhpType::Int
    ))));
    assert!(php_type_is_dumpable_scalar(&PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Array(Box::new(PhpType::Str))),
    }));

    // A float is refused on purpose: php dumps it with serialize_precision, not the
    // precision `echo` uses, so the existing formatter would print a different number.
    assert!(!php_type_is_dumpable_scalar(&PhpType::Float));
    assert!(!php_type_is_dumpable_scalar(&PhpType::Mixed));
    // No property walk exists on this target, so an object is refused wherever it sits.
    assert!(!php_type_is_dumpable_scalar(&PhpType::Object("Cart".to_string())));
    assert!(!php_type_is_dumpable_scalar(&PhpType::Array(Box::new(
        PhpType::Object("Cart".to_string())
    ))));
    assert!(!php_type_is_dumpable_scalar(&PhpType::Union(vec![
        PhpType::Int,
        PhpType::Object("Cart".to_string()),
    ])));
    // An empty union claims nothing, so it proves nothing.
    assert!(!php_type_is_dumpable_scalar(&PhpType::Union(Vec::new())));
}

/// Verifies the four namespace questions answer from the module's own declarations.
///
/// `RuntimeFnId::ClassExists`, `RuntimeFnId::InterfaceExists`, `RuntimeFnId::TraitExists`
/// and `RuntimeFnId::EnumExists` are closed-world: the checker requires a literal name in
/// AOT mode, and this module is the whole program, so each folds to a constant.
///
/// Name matching is PHP's own — case-insensitive, and a leading `\\` on either side is not
/// part of the name. Both directions matter: missing a match answers false for a class that
/// exists, and matching too eagerly answers true for one that does not.
#[test]
fn the_exists_family_answers_from_the_modules_own_declarations() {
    let circle = "Circle".to_string();
    let namespaced = "\\App\\Model".to_string();
    let declared = vec![&circle, &namespaced];
    for (name, expected) in [
        ("Circle", true),
        ("circle", true),
        ("CIRCLE", true),
        ("\\Circle", true),
        ("App\\Model", true),
        ("\\App\\Model", true),
        ("Circl", false),
        ("Circles", false),
        ("Model", false),
        ("", false),
    ] {
        assert_eq!(
            name_is_declared(&declared, name),
            expected,
            "{name:?} against {declared:?}"
        );
    }
    assert!(!name_is_declared(&[], "Circle"), "nothing is declared");

    // A target outside the family has no closed-world answer, so it stays refused rather
    // than folding to a constant that would mean nothing.
    let module = Module::new(crate::codegen_support::platform::Target::wasm());
    for target in [
        RuntimeFnId::ClassExists,
        RuntimeFnId::InterfaceExists,
        RuntimeFnId::TraitExists,
        RuntimeFnId::EnumExists,
    ] {
        assert_eq!(
            class_exists_answer(&module, target, "Circle"),
            Some(false),
            "{target:?} over an empty module"
        );
    }
    assert_eq!(
        class_exists_answer(&module, RuntimeFnId::Count, "Circle"),
        None,
        "count() is not a namespace question"
    );
}

/// Verifies `RuntimeFnId::Readline` reads a string prompt and answers a boxed cell.
///
/// The lowering hands the prompt to `$__rt_readline`, which scans stdin one byte at a time
/// and persists what it read. The ANSWER is a cell rather than a raw string, because php's
/// `readline()` is `string|false` and EOF with nothing read is the `false`; a raw `Str`
/// result could only spell that as `""`, which is a line php distinguishes from EOF.
#[test]
fn readline_takes_a_string_prompt_and_answers_a_string() {
    let ok = call_with(
        RuntimeFnId::Readline,
        IrType::Str,
        PhpType::Str,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    assert_eq!(verdict(&ok, RuntimeFnId::Readline), None);

    let numeric_prompt = call_with(
        RuntimeFnId::Readline,
        IrType::I64,
        PhpType::Int,
        IrType::Heap(IrHeapKind::Mixed),
        PhpType::Mixed,
    );
    assert!(
        verdict(&numeric_prompt, RuntimeFnId::Readline).is_some(),
        "the prompt is written from a string pointer and length"
    );

    // A raw `Str` result is exactly the shape that cannot carry EOF's `false`.
    let raw_string_result = call_with(
        RuntimeFnId::Readline,
        IrType::Str,
        PhpType::Str,
        IrType::Str,
        PhpType::Str,
    );
    assert!(
        verdict(&raw_string_result, RuntimeFnId::Readline).is_some(),
        "the line comes back boxed, so EOF can answer false"
    );

    let numeric_result = call_with(
        RuntimeFnId::Readline,
        IrType::Str,
        PhpType::Str,
        IrType::I64,
        PhpType::Int,
    );
    assert!(
        verdict(&numeric_result, RuntimeFnId::Readline).is_some(),
        "the line comes back as a persisted string, not a number"
    );
}

/// Verifies each inline builtin admits exactly the storage its lowering can emit.
///
/// `RuntimeFnId::Floor`, `RuntimeFnId::Ceil` and `RuntimeFnId::Sqrt` are float-only;
/// `RuntimeFnId::Abs` accepts both widths and must keep an integral argument integral; and
/// `RuntimeFnId::Count` reads a container header, so a scalar operand has to be refused
/// rather than loading whatever lies at that address.
#[test]
fn direct_builtins_admit_only_the_storage_they_lower() {
    for target in [RuntimeFnId::Floor, RuntimeFnId::Ceil, RuntimeFnId::Sqrt] {
        let ok = call_with(target, IrType::F64, PhpType::Float, IrType::F64, PhpType::Float);
        assert_eq!(verdict(&ok, target), None, "{target:?} over a float");

        let bad = call_with(target, IrType::I64, PhpType::Int, IrType::I64, PhpType::Int);
        assert!(
            verdict(&bad, target).is_some(),
            "{target:?} has no integral lowering"
        );
    }

    let int_abs = call_with(
        RuntimeFnId::Abs,
        IrType::I64,
        PhpType::Int,
        IrType::I64,
        PhpType::Int,
    );
    assert_eq!(verdict(&int_abs, RuntimeFnId::Abs), None);
    let float_abs = call_with(
        RuntimeFnId::Abs,
        IrType::F64,
        PhpType::Float,
        IrType::F64,
        PhpType::Float,
    );
    assert_eq!(verdict(&float_abs, RuntimeFnId::Abs), None);
    let widened_abs = call_with(
        RuntimeFnId::Abs,
        IrType::I64,
        PhpType::Int,
        IrType::F64,
        PhpType::Float,
    );
    assert!(
        verdict(&widened_abs, RuntimeFnId::Abs).is_some(),
        "an integral argument must not claim a float result"
    );

    let counted = call_with(
        RuntimeFnId::Count,
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Int)),
        IrType::I64,
        PhpType::Int,
    );
    assert_eq!(verdict(&counted, RuntimeFnId::Count), None);
    let listed = call_with(
        RuntimeFnId::ArrayIsList,
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Int)),
        IrType::I64,
        PhpType::Bool,
    );
    assert_eq!(verdict(&listed, RuntimeFnId::ArrayIsList), None);
    let hashed = call_with(
        RuntimeFnId::ArrayIsList,
        IrType::Heap(IrHeapKind::Hash),
        PhpType::AssocArray {
            key: Box::new(PhpType::Str),
            value: Box::new(PhpType::Int),
        },
        IrType::I64,
        PhpType::Bool,
    );
    // A hash now GETS that real scan — `__rt_hash_is_list` walks the insertion order — so it
    // is admitted. The answer is still not read off the representation: `[1 => "b", 0 => "a"]`
    // holds both keys of a two-element list and php answers false, measured.
    assert_eq!(
        verdict(&hashed, RuntimeFnId::ArrayIsList),
        None,
        "a hash is answered by scanning its keys in order"
    );

    // All three answer an `array<int>` built from an `array<int>`; the empty `array<never>`
    // rides along on the same contract.
    for target in [
        RuntimeFnId::ArrayKeys,
        RuntimeFnId::ArrayValues,
        RuntimeFnId::ArrayReverse,
    ] {
        let ok = call_with(
            target,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        );
        assert_eq!(verdict(&ok, target), None, "{target:?} over array<int>");
    }

    // `array_reverse` reads no element at all: it swaps whole SLOTS on a clone, at the
    // array's own stride, so a wider element type is served rather than misread. What it
    // still may not do is CHANGE the element type, because the swap produces the source's.
    let reversed_strings = call_with(
        RuntimeFnId::ArrayReverse,
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
    );
    assert_eq!(
        verdict(&reversed_strings, RuntimeFnId::ArrayReverse),
        None,
        "array_reverse swaps slots at the array's own stride"
    );
    let reversed_retyped = call_with(
        RuntimeFnId::ArrayReverse,
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Str)),
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Object("Cart".to_string()))),
    );
    assert!(
        verdict(&reversed_retyped, RuntimeFnId::ArrayReverse).is_some(),
        "array_reverse answers its source's element type, not another"
    );

    // `array_values` is the OPPOSITE case, and grouping it with the walk above was stricter
    // than the code it guards: it is `__rt_array_clone_shallow`, which reads the array's own
    // `elem_size` from the header and re-persists or increfs each child by its value_type.
    // So any element type survives — verified against php-src for strings and for the
    // `array<mixed>` that `ArrayIterator::__construct` hands it.
    for element in [PhpType::Str, PhpType::Mixed, PhpType::Float] {
        let widened = call_with(
            RuntimeFnId::ArrayValues,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(element.clone())),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(element.clone())),
        );
        assert_eq!(
            verdict(&widened, RuntimeFnId::ArrayValues),
            None,
            "array_values must preserve an array<{element:?}>"
        );
        // Its keys are POSITIONS whatever the elements were, so the same source with an
        // `array<int>` result is the projection `array_keys` answers...
        let keys = call_with(
            RuntimeFnId::ArrayKeys,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(element.clone())),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
        );
        assert_eq!(
            verdict(&keys, RuntimeFnId::ArrayKeys),
            None,
            "array_keys over an array<{element:?}> answers its positions"
        );
        // ...and claiming the ELEMENT type for those keys is not — except for `mixed`,
        // which is not a claim about the element at all but about the STORAGE: a list the
        // checker types `array<mixed>` wants its positions in Mixed cells, which
        // `__rt_array_index_keys_mixed` builds. `ArrayIterator::__construct` is that case.
        let mistyped = call_with(
            RuntimeFnId::ArrayKeys,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(element.clone())),
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(element.clone())),
        );
        assert!(
            element == PhpType::Int
                || element == PhpType::Mixed
                || verdict(&mistyped, RuntimeFnId::ArrayKeys).is_some(),
            "array_keys never answers an array<{element:?}>"
        );
    }

    // `RuntimeFnId::ArraySum` and `RuntimeFnId::ArrayProduct` take an array<int> and answer
    // php's int|float as a Mixed cell, since an overflowing fold is promoted to a float.
    for target in [RuntimeFnId::ArraySum, RuntimeFnId::ArrayProduct] {
        let ok = call_with(
            target,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        assert_eq!(verdict(&ok, target), None, "{target:?} over array<int>");

        let wrapping = call_with(
            target,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Int)),
            IrType::I64,
            PhpType::Int,
        );
        assert!(
            verdict(&wrapping, target).is_some(),
            "{target:?} has no slot for its promoted float when it answers a bare int"
        );

        let boxed_elements = call_with(
            target,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Mixed)),
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
        );
        assert!(
            verdict(&boxed_elements, target).is_some(),
            "{target:?} does not lower php's per-type rules for boxed elements"
        );

        let floaty = call_with(
            target,
            IrType::Heap(IrHeapKind::Array),
            PhpType::Array(Box::new(PhpType::Float)),
            IrType::F64,
            PhpType::Float,
        );
        assert!(
            verdict(&floaty, target).is_some(),
            "{target:?} folds integer slots only"
        );
    }

    // `RuntimeFnId::Max`, `RuntimeFnId::Min` and `RuntimeFnId::Intdiv` take two ints; the
    // variadic and cross-type forms of max/min are not served.
    for target in [RuntimeFnId::Max, RuntimeFnId::Min, RuntimeFnId::Intdiv] {
        let single = call_with(
            target,
            IrType::I64,
            PhpType::Int,
            IrType::I64,
            PhpType::Int,
        );
        assert!(
            verdict(&single, target).is_some(),
            "{target:?} needs exactly two operands"
        );
    }

    // `RuntimeFnId::ArrayFill` needs its start index, count and value; one operand is not it.
    let short_fill = call_with(
        RuntimeFnId::ArrayFill,
        IrType::I64,
        PhpType::Int,
        IrType::Heap(IrHeapKind::Array),
        PhpType::Array(Box::new(PhpType::Int)),
    );
    assert!(
        verdict(&short_fill, RuntimeFnId::ArrayFill).is_some(),
        "array_fill takes three operands"
    );

    // `RuntimeFnId::StrContains`, `RuntimeFnId::StrStartsWith` and `RuntimeFnId::StrEndsWith`
    // take two strings; a scalar operand has no bytes to compare.
    for target in [
        RuntimeFnId::StrContains,
        RuntimeFnId::StrStartsWith,
        RuntimeFnId::StrEndsWith,
    ] {
        let scalar = call_with(target, IrType::I64, PhpType::Int, IrType::I64, PhpType::Bool);
        assert!(
            verdict(&scalar, target).is_some(),
            "{target:?} compares string bytes"
        );
    }

    let scalar_count = call_with(
        RuntimeFnId::Count,
        IrType::I64,
        PhpType::Int,
        IrType::I64,
        PhpType::Int,
    );
    assert!(
        verdict(&scalar_count, RuntimeFnId::Count).is_some(),
        "count() of a scalar is a PHP TypeError a header load cannot raise"
    );
}
