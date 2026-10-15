//! Purpose:
//! String builtins: search, trim, substr, pad, repeat, replace, split, wordwrap, comparisons, predicates and unary transforms.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.
//! - `crate::codegen_wasm::inst` calls `lower_unary_string`.

use super::*;

/// Lowers `strstr` in both its arities.
///
/// PHP's result is `string|false`, so the two outcomes are boxed under different Mixed tags the
/// way `strpos` boxes its own. The returned slice is a REGION of the haystack — from the match to
/// the end, or from the start up to the match when `$before_needle` is true — and boxing under
/// the string tag persists a copy, so pointing into the source is safe rather than aliasing it.
/// An empty needle matches at 0, which is why `strstr("abcdef", "")` is the whole string and its
/// `before` form is empty.
pub(super) fn lower_strstr(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let found = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    let hptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    let hlen = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {hlen}"), "spill haystack length");
    ctx.fb.ins(&format!("local.set {hptr}"), "spill haystack pointer");
    ctx.fb.ins(&format!("local.get {hptr}"), "haystack pointer");
    ctx.fb.ins(&format!("local.get {hlen}"), "haystack length");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins("i32.const 0", "strstr is case-sensitive");
    ctx.fb
        .ins("call $__rt_str_find", "first offset, or -1 when absent");
    ctx.fb
        .ins(&format!("local.set {found}"), "spill the scan result");

    ctx.fb.ins(&format!("local.get {found}"), "scan result");
    ctx.fb.ins("i64.const 0", "the absent sentinel is negative");
    ctx.fb.ins("i64.lt_s", "was the needle absent?");
    ctx.fb
        .ins("if (result i32)", "string|false travels as a Mixed cell");
    ctx.fb.ins("i64.const 3", "mixed tag (bool)");
    ctx.fb.ins("i64.const 0", "the value false");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb.ins("call $__rt_mixed_from_value", "box PHP's false");
    ctx.fb.ins("else", "the needle was found");
    ctx.fb.ins("i64.const 1", "mixed tag (string)");
    if inst.operands.len() == 3 {
        // `$before_needle` selects which side of the match survives, and it is a runtime value
        // rather than a literal, so both regions are computed and one is selected.
        ctx.fb.ins(&format!("local.get {hptr}"), "the haystack start");
        ctx.fb.ins("i64.extend_i32_u", "start pointer -> lo");
        ctx.fb.ins(&format!("local.get {hptr}"), "the haystack start");
        ctx.fb.ins("i64.extend_i32_u", "widen before adding the offset");
        ctx.fb.ins(&format!("local.get {found}"), "the match offset");
        ctx.fb.ins("i64.add", "pointer to the match");
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i64.const 0", "compare the flag against false");
        ctx.fb
            .ins("i64.ne", "a truthy flag selects the leading region");
        ctx.fb.ins("select", "which region's pointer");
        ctx.fb.ins(&format!("local.get {found}"), "bytes before the match");
        ctx.fb.ins(&format!("local.get {hlen}"), "haystack length");
        ctx.fb.ins(&format!("local.get {found}"), "the match offset");
        ctx.fb.ins("i64.sub", "bytes from the match to the end");
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins("i64.const 0", "compare the flag against false");
        ctx.fb
            .ins("i64.ne", "a truthy flag selects the leading region");
        ctx.fb.ins("select", "which region's length");
    } else {
        ctx.fb.ins(&format!("local.get {hptr}"), "the haystack start");
        ctx.fb.ins("i64.extend_i32_u", "widen before adding the offset");
        ctx.fb.ins(&format!("local.get {found}"), "the match offset");
        ctx.fb.ins("i64.add", "lo: pointer to the match");
        ctx.fb.ins(&format!("local.get {hlen}"), "haystack length");
        ctx.fb.ins(&format!("local.get {found}"), "the match offset");
        ctx.fb.ins("i64.sub", "hi: bytes from the match to the end");
    }
    ctx.fb
        .ins("call $__rt_mixed_from_value", "box the region (persists a copy)");
    ctx.fb.ins("end", "end string|false selection");
    store_result(ctx, inst)
}

/// Validates `strstr`: two strings, an optional bool, and PHP's `string|false` Mixed out.
pub(super) fn strstr_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 2 | 3) {
        return Some(format!(
            "expected a haystack, a needle and an optional flag, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index < 2 {
            (IrType::Str, PhpType::Str)
        } else {
            (IrType::I64, PhpType::Bool)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "strstr operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Mixed)
        || call.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "strstr result {:?}/{:?} is not the Mixed cell PHP's string|false needs",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Returns whether a unary string transform is lowered by this module.
///
/// Admitted: the exact same-length BYTE transforms, the re-encoders whose rules are pure byte
/// arithmetic (hex expansion, backslash escaping, line-break tagging), and base64 and url coding,
/// whose alphabets are contiguous enough to compute rather than tabulate. Still out are the html
/// entity decoders, which need a real named-entity table, and `hex2bin`, whose PHP result is
/// `string|false` rather than the `string` this family's signature promises.
pub(in crate::codegen_wasm) fn unary_string_is_supported(target: UnaryStringRuntime) -> bool {
    matches!(
        target,
        UnaryStringRuntime::StrToUpper
            | UnaryStringRuntime::StrToLower
            | UnaryStringRuntime::StrReverse
            | UnaryStringRuntime::BinToHex
            | UnaryStringRuntime::AddSlashes
            | UnaryStringRuntime::StripSlashes
            | UnaryStringRuntime::NlToBr
            | UnaryStringRuntime::UrlEncode
            | UnaryStringRuntime::RawUrlEncode
            | UnaryStringRuntime::UrlDecode
            | UnaryStringRuntime::RawUrlDecode
            | UnaryStringRuntime::Base64Encode
    )
}

/// Validates one unary string transform: a string in, a string out.
pub(in crate::codegen_wasm) fn unary_string_shape_issue(
    function: &Function,
    call: &Instruction,
    target: UnaryStringRuntime,
) -> Option<String> {
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one string operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("string operand is missing from the value table".to_string());
    };
    if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
        return Some(format!(
            "expected a string operand, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers one unary string transform to its byte-mapping helper.
pub(in crate::codegen_wasm) fn lower_unary_string(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: UnaryStringRuntime,
) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    match target {
        UnaryStringRuntime::StrToUpper => {
            ctx.fb.ins("i32.const 1", "map towards upper case");
            ctx.fb
                .ins("call $__rt_str_map_case", "ASCII-only case mapping");
        }
        UnaryStringRuntime::StrToLower => {
            ctx.fb.ins("i32.const 0", "map towards lower case");
            ctx.fb
                .ins("call $__rt_str_map_case", "ASCII-only case mapping");
        }
        UnaryStringRuntime::StrReverse => {
            ctx.fb
                .ins("call $__rt_str_reverse", "reverse the bytes");
        }
        UnaryStringRuntime::BinToHex => {
            ctx.fb
                .ins("call $__rt_str_bin2hex", "expand each byte to two hex digits");
        }
        UnaryStringRuntime::AddSlashes => {
            ctx.fb
                .ins("call $__rt_str_addslashes", "escape quotes, backslash and NUL");
        }
        UnaryStringRuntime::StripSlashes => {
            ctx.fb
                .ins("call $__rt_str_stripslashes", "remove one level of backslash escaping");
        }
        UnaryStringRuntime::NlToBr => {
            ctx.fb
                .ins("call $__rt_str_nl2br", "insert a break tag before each line break");
        }
        UnaryStringRuntime::UrlEncode => {
            ctx.fb.ins("i32.const 0", "urlencode folds a space to a plus");
            ctx.fb
                .ins("call $__rt_str_url_encode", "percent-encode the reserved bytes");
        }
        UnaryStringRuntime::RawUrlEncode => {
            ctx.fb.ins("i32.const 1", "rawurlencode keeps a tilde and encodes a space");
            ctx.fb
                .ins("call $__rt_str_url_encode", "percent-encode the reserved bytes");
        }
        UnaryStringRuntime::UrlDecode => {
            ctx.fb.ins("i32.const 1", "urldecode reads a plus as a space");
            ctx.fb
                .ins("call $__rt_str_url_decode", "percent-decode tolerantly");
        }
        UnaryStringRuntime::RawUrlDecode => {
            ctx.fb.ins("i32.const 0", "rawurldecode keeps a plus literal");
            ctx.fb
                .ins("call $__rt_str_url_decode", "percent-decode tolerantly");
        }
        UnaryStringRuntime::Base64Encode => {
            ctx.fb
                .ins("call $__rt_str_base64_encode", "encode with padding to a quartet");
        }
        other => {
            return Err(WasmError::Unsupported(format!(
                "unary string transform {:?}",
                other
            )))
        }
    }
    store_result(ctx, inst)
}

/// Validates `str_contains`, `str_starts_with` and `str_ends_with`: two strings in, a bool out.
/// Lowers `trim`, `ltrim` and `rtrim` through the shared end-stripping helper.
///
/// The one-argument form passes a `-1` charlist length, which the helper reads as PHP's default
/// set. That keeps the default out of the data segments of a module that never asks for it, and
/// keeps an explicitly EMPTY charlist — which strips nothing — distinguishable from it.
pub(super) fn lower_trim(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    if inst.operands.len() == 2 {
        ctx.emit_load_value(operand(inst, 1)?)?;
    } else {
        ctx.fb.ins("i32.const 0", "no charlist pointer");
        ctx.fb
            .ins("i64.const -1", "sentinel: PHP's default character set");
    }
    let mode = match target {
        RuntimeFnId::Ltrim => 1,
        RuntimeFnId::Rtrim => 2,
        _ => 3,
    };
    ctx.fb
        .ins(&format!("i32.const {mode}"), "which ends to strip");
    ctx.fb.ins("call $__rt_str_trim", "strip the selected ends");
    store_result(ctx, inst)
}

/// Lowers `substr()`: pushes the string, the start, and the optional length with a flag saying
/// whether one was written, then calls the runtime slice helper.
pub(super) fn lower_substr(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    emit_int_operand(ctx, inst, 1)?;
    if inst.operands.len() == 3 {
        emit_int_operand(ctx, inst, 2)?;
        ctx.fb.ins("i32.const 1", "an explicit length was written");
    } else {
        ctx.fb.ins("i64.const 0", "unused length");
        ctx.fb.ins("i32.const 0", "no length: run to the end");
    }
    ctx.fb.ins("call $__rt_str_substr", "own the selected bytes");
    store_result(ctx, inst)
}

/// Lowers `wordwrap` in its one- and two-argument forms.
///
/// The default break is a single newline and the default is NOT to cut long words, which together
/// select php-src's fast in-place path. A custom `$break` or `$cut_long_words` selects a different
/// algorithm in php-src — the general path can change the LENGTH, which this one never does — so
/// those arities are refused rather than approximated.
pub(super) fn lower_wordwrap(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 0)?)?;
    if inst.operands.len() >= 2 {
        ctx.emit_load_value(operand(inst, 1)?)?;
    } else {
        ctx.fb.ins("i64.const 75", "PHP's default width");
    }
    if inst.operands.len() <= 2 {
        // The one-byte, no-cut form rewrites in place: a space BECOMES the break, so the length
        // never changes and no buffer has to be built.
        ctx.fb.ins("call $__rt_wordwrap", "break lines at spaces, in place");
        return store_result(ctx, inst);
    }
    ctx.emit_load_value(operand(inst, 2)?)?;
    if inst.operands.len() == 4 {
        ctx.emit_load_value(operand(inst, 3)?)?;
        ctx.fb.ins("i32.wrap_i64", "the cut flag is a PHP bool");
    } else {
        ctx.fb.ins("i32.const 0", "no cut: a long word is left whole");
    }
    ctx.fb.ins(
        "call $__rt_wordwrap_general",
        "an arbitrary break, and cutting, both lengthen the text",
    );
    store_result(ctx, inst)
}

/// Validates `wordwrap`: a subject and an optional integer width, a string out.
pub(super) fn wordwrap_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 1 | 2 | 3 | 4) {
        return Some(format!(
            "expected a subject, an optional width, break and cut flag, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index == 0 || index == 2 {
            (IrType::Str, PhpType::Str)
        } else if index == 3 {
            (IrType::I64, PhpType::Bool)
        } else {
            (IrType::I64, PhpType::Int)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "wordwrap operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "wordwrap result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// The `__rt_fail` code PHP's non-positive-length `str_split` `ValueError` reports under.
pub(super) const STR_SPLIT_LENGTH_FAILURE_CODE: i32 = 14;

/// Lowers `str_split` in both arities.
///
/// The one-argument form's default really is a chunk of one rather than a different rule, so both
/// arities share the same lowering with a literal 1 supplied. A chunk length below one raises
/// php-src's ValueError.
pub(super) fn lower_str_split(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let chunk = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    if inst.operands.len() == 2 {
        ctx.emit_load_value(operand(inst, 1)?)?;
    } else {
        ctx.fb.ins("i64.const 1", "the default chunk is one byte");
    }
    ctx.fb.ins(&format!("local.set {chunk}"), "spill the chunk length");
    ctx.fb.ins(&format!("local.get {chunk}"), "chunk length");
    ctx.fb.ins("i64.const 1", "PHP's lower bound");
    ctx.fb.ins("i64.lt_s", "below one?");
    ctx.fb.ins("if", "str_split() rejects a non-positive chunk length");
    crate::codegen_wasm::inst::emit_runtime_failure(
        ctx,
        STR_SPLIT_LENGTH_FAILURE_CODE,
        "str_split() non-positive length",
    );
    ctx.fb.ins("end", "end chunk-length guard");
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.get {chunk}"), "the validated chunk length");
    ctx.fb.ins("call $__rt_str_split", "cut into a fresh indexed array");
    store_result(ctx, inst)
}

/// Validates `str_split`: a subject and an optional chunk length, an indexed string array out.
pub(super) fn str_split_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 1 | 2) {
        return Some(format!(
            "expected a subject and an optional length, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index == 0 {
            (IrType::Str, PhpType::Str)
        } else {
            (IrType::I64, PhpType::Int)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "str_split operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "str_split result {:?} is not an indexed array",
            call.result_type
        ));
    }
    if !matches!(&call.result_php_type, PhpType::Array(element) if **element == PhpType::Str) {
        return Some(format!(
            "str_split must produce exactly array<string>, got {:?}",
            call.result_php_type
        ));
    }
    None
}

/// Lowers `strrpos` in its two-argument form, boxing PHP's `int|false`.
///
/// Only the two-argument form is admitted. The three-argument form's `$offset` has a rule of its
/// own that is NOT the mirror of `strpos`'s — a negative offset there bounds where the match may
/// START, counted from the end — so it is refused rather than assumed symmetrical.
pub(super) fn lower_string_rsearch(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let found = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb
        .ins("call $__rt_str_rfind", "last offset, or -1 when absent");
    ctx.fb
        .ins(&format!("local.set {found}"), "spill the scan result");
    ctx.fb.ins(&format!("local.get {found}"), "scan result");
    ctx.fb.ins("i64.const 0", "the absent sentinel is negative");
    ctx.fb.ins("i64.lt_s", "was the needle absent?");
    ctx.fb.ins("if (result i32)", "int|false travels as a Mixed cell");
    ctx.fb.ins("i64.const 3", "mixed tag (bool)");
    ctx.fb.ins("i64.const 0", "the value false");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb.ins("call $__rt_mixed_from_value", "box PHP's false");
    ctx.fb.ins("else", "the needle was found");
    ctx.fb.ins("i64.const 0", "mixed tag (int)");
    ctx.fb.ins(&format!("local.get {found}"), "the byte offset");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb.ins("call $__rt_mixed_from_value", "box the offset");
    ctx.fb.ins("end", "end int|false selection");
    store_result(ctx, inst)
}

/// Lowers `strpos` in its two-argument form.
///
/// The scan helper takes a case-folding flag because `stripos`, `strrpos` and `strstr` all want
/// the same search with one knob changed; only `strpos` is registered as a distinct runtime
/// identity today, so this passes the case-sensitive setting.
///
/// PHP's result is `int|false`, which EIR carries as a `Mixed` cell, so the two outcomes are
/// boxed under different tags: an index under the int tag and a miss under the BOOL tag holding
/// zero, which is exactly `false`. Returning 0 under the int tag instead would make
/// `strpos($h, $n) === false` answer wrong for a match at the start — the classic PHP trap this
/// distinction exists to serve.
pub(super) fn lower_string_search(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let found = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(
        "i32.const 0",
        "strpos is case-sensitive",
    );
    ctx.fb
        .ins("call $__rt_str_find", "first offset, or -1 when absent");
    ctx.fb
        .ins(&format!("local.set {found}"), "spill the scan result");
    ctx.fb.ins(&format!("local.get {found}"), "scan result");
    ctx.fb.ins("i64.const 0", "the absent sentinel is negative");
    ctx.fb.ins("i64.lt_s", "was the needle absent?");
    ctx.fb.ins("if (result i32)", "int|false travels as a Mixed cell");
    ctx.fb.ins("i64.const 3", "mixed tag (bool)");
    ctx.fb.ins("i64.const 0", "the value false");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb
        .ins("call $__rt_mixed_from_value", "box PHP's false");
    ctx.fb.ins("else", "the needle was found");
    ctx.fb.ins("i64.const 0", "mixed tag (int)");
    ctx.fb.ins(&format!("local.get {found}"), "the byte offset");
    ctx.fb.ins("i64.const 0", "hi unused");
    ctx.fb.ins("call $__rt_mixed_from_value", "box the offset");
    ctx.fb.ins("end", "end int|false selection");
    store_result(ctx, inst)
}

/// Validates `strpos` and `stripos`: two strings in, PHP's `int|false` Mixed out.
///
/// Only the two-argument form is admitted. The three-argument form has to validate `$offset`
/// against the haystack and raise a `ValueError` naming the called function when it does not fit,
/// which is a different contract rather than a default, so it is refused rather than guessed.
pub(super) fn string_search_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [haystack, needle] = call.operands.as_slice() else {
        return Some(format!(
            "expected a haystack and a needle with no offset, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [haystack, needle] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "expected a string operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Heap(IrHeapKind::Mixed)
        || call.result_php_type.codegen_repr() != PhpType::Mixed
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the Mixed cell PHP's int|false needs",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers `str_replace` in its all-string, three-argument form.
///
/// PHP also accepts arrays for any of the three arguments and an optional by-reference `$count`,
/// which are different operations rather than defaults, so only the string form is admitted.
pub(super) fn lower_str_replace(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    ctx.emit_load_value(operand(inst, 2)?)?;
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb
        .ins("call $__rt_str_replace", "own the rewritten bytes");
    store_result(ctx, inst)
}

/// Validates `str_replace`: three strings in, a string out.
pub(super) fn str_replace_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [search, replace, subject] = call.operands.as_slice() else {
        return Some(format!(
            "expected a search, a replacement and a subject, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [search, replace, subject] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "str_replace takes strings, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "str_replace result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// The `__rt_fail` code PHP's empty-pad `str_pad` `ValueError` reports under.
pub(super) const STR_PAD_EMPTY_FAILURE_CODE: i32 = 12;

/// Lowers `str_pad` in its two- and three-argument forms.
///
/// PHP refuses an empty `$pad_string` — but ONLY when padding is actually needed. `str_pad("abc",
/// 2, "")` answers `"abc"` rather than raising, so the guard tests the target length as well as
/// the pad, and raises through the same path the other builtin ValueErrors take.
///
/// The four-argument form stays refused: it validates `$pad_type` eagerly, raising even when no
/// padding is needed and even when the pad string is also invalid, which is a different contract
/// with its own message rather than a default for this one.
pub(super) fn lower_str_pad(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let target = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    let sptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    let slen = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    let pptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    let plen = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);

    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb.ins(&format!("local.set {slen}"), "spill subject length");
    ctx.fb.ins(&format!("local.set {sptr}"), "spill subject pointer");
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb.ins(&format!("local.set {target}"), "spill the target length");
    if inst.operands.len() == 3 {
        ctx.emit_load_value(operand(inst, 2)?)?;
        ctx.fb.ins(&format!("local.set {plen}"), "spill pad length");
        ctx.fb.ins(&format!("local.set {pptr}"), "spill pad pointer");
    } else {
        // A null pad pointer means PHP's default single space. Synthesizing it in the helper
        // costs one branch and keeps a module that never pads from carrying a data segment.
        ctx.fb.ins("i32.const 0", "null pad pointer selects the default space");
        ctx.fb.ins(&format!("local.set {pptr}"), "spill pad pointer");
        ctx.fb.ins("i64.const 1", "the default pad is one byte");
        ctx.fb.ins(&format!("local.set {plen}"), "spill pad length");
    }

    if inst.operands.len() == 3 {
        ctx.fb.ins(&format!("local.get {plen}"), "pad length");
        ctx.fb.ins("i64.eqz", "an empty pad string?");
        ctx.fb.ins(&format!("local.get {target}"), "target length");
        ctx.fb.ins(&format!("local.get {slen}"), "subject length");
        ctx.fb.ins("i64.gt_s", "is padding actually needed?");
        ctx.fb.ins("i32.and", "PHP raises only when both hold");
        ctx.fb.ins("if", "str_pad() rejects an empty pad it would have to use");
        crate::codegen_wasm::inst::emit_runtime_failure(
            ctx,
            STR_PAD_EMPTY_FAILURE_CODE,
            "str_pad() empty pad string",
        );
        ctx.fb.ins("end", "end empty-pad guard");
    }

    ctx.fb.ins(&format!("local.get {sptr}"), "subject pointer");
    ctx.fb.ins(&format!("local.get {slen}"), "subject length");
    ctx.fb.ins(&format!("local.get {target}"), "target length");
    ctx.fb.ins(&format!("local.get {pptr}"), "pad pointer, or 0 for the default space");
    ctx.fb.ins(&format!("local.get {plen}"), "pad length");
    ctx.fb.ins("i32.const 1", "STR_PAD_RIGHT");
    ctx.fb.ins("call $__rt_str_pad", "own the padded bytes");
    store_result(ctx, inst)
}

/// Validates `str_pad`: a subject, an integer length, and optionally a pad string.
pub(super) fn str_pad_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 2 | 3) {
        return Some(format!(
            "expected a subject, a length and an optional pad, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index == 1 {
            (IrType::I64, PhpType::Int)
        } else {
            (IrType::Str, PhpType::Str)
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "str_pad operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "str_pad result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// The `__rt_fail` code PHP's negative-`str_repeat` `ValueError` reports under.
pub(super) const STR_REPEAT_NEGATIVE_FAILURE_CODE: i32 = 11;

/// Lowers `str_repeat`, refusing a negative count the way PHP does.
///
/// PHP does not clamp a negative `$times` to zero: it raises a `ValueError`, which an ordinary
/// `catch` receives. The guard therefore goes through the shared runtime-failure path so it is
/// RAISED where the module can catch it and reported as a fatal where it cannot — the same
/// treatment division by zero gets. The count is spilled to a local because the guard reads it
/// before the helper consumes it.
pub(super) fn lower_str_repeat(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let times = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(operand(inst, 1)?)?;
    ctx.fb
        .ins(&format!("local.set {times}"), "spill the repeat count");
    ctx.fb
        .ins(&format!("local.get {times}"), "repeat count");
    ctx.fb.ins("i64.const 0", "PHP's lower bound");
    ctx.fb.ins("i64.lt_s", "negative count?");
    ctx.fb.ins("if", "str_repeat() rejects a negative count");
    crate::codegen_wasm::inst::emit_runtime_failure(
        ctx,
        STR_REPEAT_NEGATIVE_FAILURE_CODE,
        "str_repeat() negative count",
    );
    ctx.fb.ins("end", "end negative-count guard");
    ctx.emit_load_value(operand(inst, 0)?)?;
    ctx.fb
        .ins(&format!("local.get {times}"), "the validated repeat count");
    ctx.fb
        .ins("call $__rt_str_repeat", "own the repeated bytes");
    store_result(ctx, inst)
}

/// Validates `str_repeat`: a string subject and an integer count.
pub(super) fn str_repeat_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [subject, times] = call.operands.as_slice() else {
        return Some(format!(
            "expected a subject and a count, got {} operands",
            call.operands.len()
        ));
    };
    for (operand, want_ir, want_php) in [
        (subject, IrType::Str, PhpType::Str),
        (times, IrType::I64, PhpType::Int),
    ] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "str_repeat operand is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "str_repeat result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `trim`, `ltrim` and `rtrim`: a string, and optionally a charlist string.
pub(super) fn trim_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    if !matches!(call.operands.len(), 1 | 2) {
        return Some(format!(
            "expected a subject and an optional charlist, got {} operands",
            call.operands.len()
        ));
    }
    for operand in &call.operands {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "expected a string operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `substr`: a string, an integer offset, and optionally an integer length.
///
/// An offset or length may also arrive BOXED, which is the commonest shape in real code and
/// used to be refused: `substr($s, $mixed)` reaches the call with the Mixed intact, since the
/// frontend materialises no cast for it. `emit_int_operand` coerces it with php-src's own
/// per-tag answers and diagnostics, so the boxed form is admitted here — but only in a module
/// that can raise them, since they go through WASI like every other diagnostic.
pub(super) fn substr_shape_issue(module: &Module, function: &Function, call: &Instruction) -> Option<String> {
    if !matches!(call.operands.len(), 2 | 3) {
        return Some(format!(
            "expected a subject, an offset and an optional length, got {} operands",
            call.operands.len()
        ));
    }
    for (index, operand) in call.operands.iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        let (want_ir, want_php) = if index == 0 {
            (IrType::Str, PhpType::Str)
        } else {
            (IrType::I64, PhpType::Int)
        };
        let coercible = index > 0
            && crate::codegen_wasm::capability::runtime_call_int_operand_coercion(function, call, index)
                .is_some()
            && module.functions.iter().any(|candidate| candidate.flags.is_main);
        if !coercible
            && (value.ir_type != want_ir || value.php_type.codegen_repr() != want_php)
        {
            return Some(format!(
                "substr operand {index} is {:?}/{:?}, expected {want_ir:?}/{want_php:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::Str
        || call.result_php_type.codegen_repr() != PhpType::Str
    {
        return Some(format!(
            "substr result {:?}/{:?} is not the expected Str/Str",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `strcmp` and `strcasecmp`: two strings in, an integer out.
pub(super) fn string_compare_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [left, right] = call.operands.as_slice() else {
        return Some(format!(
            "expected two string operands, got {}",
            call.operands.len()
        ));
    };
    for operand in [left, right] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "expected a string operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Int
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected I64/Int",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `chr` and `ord`: one concrete scalar in, the opposite one out.
///
/// A `mixed` argument is refused rather than coerced. PHP would juggle it, and juggling carries
/// its own per-tag diagnostics that this backend does not reproduce yet, so admitting one here
/// would answer confidently where PHP would have complained first.
pub(super) fn byte_conversion_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let (argument_ir, argument_php, result_ir, result_php) = if target == RuntimeFnId::Chr {
        (IrType::I64, PhpType::Int, IrType::Str, PhpType::Str)
    } else {
        (IrType::Str, PhpType::Str, IrType::I64, PhpType::Int)
    };
    let [operand] = call.operands.as_slice() else {
        return Some(format!(
            "expected one operand, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*operand) else {
        return Some("operand is missing from the value table".to_string());
    };
    if value.ir_type != argument_ir || value.php_type.codegen_repr() != argument_php {
        return Some(format!(
            "{target:?} operand {:?}/{:?} is not the expected {argument_ir:?}/{argument_php:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    if call.result.is_none()
        || call.result_type != result_ir
        || call.result_php_type.codegen_repr() != result_php
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected {result_ir:?}/{result_php:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Validates `str_contains`, `str_starts_with` and `str_ends_with`: two string operands in and a
/// bool out, returning the reason the call cannot be lowered or `None`.
pub(super) fn string_predicate_shape_issue(
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [haystack, needle] = call.operands.as_slice() else {
        return Some(format!(
            "expected a haystack and a needle, got {} operands",
            call.operands.len()
        ));
    };
    for operand in [haystack, needle] {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        if value.ir_type != IrType::Str || value.php_type.codegen_repr() != PhpType::Str {
            return Some(format!(
                "expected a string operand, got {:?}/{:?}",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != IrType::I64
        || call.result_php_type.codegen_repr() != PhpType::Bool
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected I64/Bool",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// Lowers the three PHP 8 substring predicates to a byte comparison.
///
/// `str_starts_with` and `str_ends_with` compare ONE region, so they check the needle fits and
/// then compare at offset zero or at `hlen - nlen`. `str_contains` scans every offset that
/// leaves room. An empty needle matches in all three, which is PHP's answer.
pub(super) fn lower_string_predicate(
    ctx: &mut FnCtx,
    inst: &Instruction,
    target: RuntimeFnId,
) -> Result<()> {
    let haystack = operand(inst, 0)?;
    let needle = operand(inst, 1)?;
    let (hptr, hlen) = match ctx.value_repr(haystack)?.clone() {
        crate::codegen_wasm::values::WasmRepr::Str { ptr, len } => (ptr, len),
        other => {
            return Err(WasmError::Unsupported(format!(
                "string predicate haystack is {:?}",
                other
            )))
        }
    };
    let (nptr, nlen) = match ctx.value_repr(needle)?.clone() {
        crate::codegen_wasm::values::WasmRepr::Str { ptr, len } => (ptr, len),
        other => {
            return Err(WasmError::Unsupported(format!(
                "string predicate needle is {:?}",
                other
            )))
        }
    };
    if target == RuntimeFnId::StrContains {
        ctx.fb.ins(&format!("local.get {}", hptr), "haystack pointer");
        ctx.fb.ins(&format!("local.get {}", hlen), "haystack length");
        ctx.fb.ins(&format!("local.get {}", nptr), "needle pointer");
        ctx.fb.ins(&format!("local.get {}", nlen), "needle length");
        ctx.fb
            .ins("call $__rt_str_contains", "scan every start offset");
        return store_result(ctx, inst);
    }
    // A needle longer than the haystack cannot match at any single offset, and the comparison
    // would read past the end, so the length check has to come first.
    ctx.fb.ins(&format!("local.get {}", nlen), "needle length");
    ctx.fb.ins(&format!("local.get {}", hlen), "haystack length");
    ctx.fb.ins("i64.gt_s", "does the needle overrun the haystack?");
    ctx.fb.ins("if (result i64)", "needle too long");
    ctx.fb.ins("i64.const 0", "an overrunning needle never matches");
    ctx.fb.ins("else", "the needle fits");
    ctx.fb.ins(&format!("local.get {}", hptr), "haystack pointer");
    ctx.fb.ins(&format!("local.get {}", nptr), "needle pointer");
    ctx.fb.ins(&format!("local.get {}", nlen), "needle length");
    if target == RuntimeFnId::StrStartsWith {
        ctx.fb.ins("i64.const 0", "compare at the start");
    } else {
        ctx.fb.ins(&format!("local.get {}", hlen), "haystack length");
        ctx.fb.ins(&format!("local.get {}", nlen), "needle length");
        ctx.fb.ins("i64.sub", "compare at the trailing region");
    }
    ctx.fb
        .ins("call $__rt_str_region_eq", "compare the one candidate region");
    ctx.fb.ins("end", "end needle-length guard");
    store_result(ctx, inst)
}
