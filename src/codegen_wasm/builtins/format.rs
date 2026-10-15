//! Purpose:
//! The `sprintf` family: format parser, the `__rt_fmt_*` WAT helpers, `sprintf`/`printf`/`fprintf` lowering and `fscanf`.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.
//!
//! Key details:
//! - The audit and the emitter parse the same literal format, so a conversion the emitter cannot lower is refused before planning.

use super::*;

/// One piece of a `sprintf` format that has been resolved at COMPILE time.
///
/// The format is required to be a literal, so the parse happens once here rather than becoming a
/// format interpreter in the emitted module. That is the whole point of doing this in an AOT
/// compiler: a call whose format is known becomes a fixed sequence of appends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::codegen_wasm) enum FormatPiece {
    /// A contiguous run of the FORMAT's own bytes, given as an offset and length into it.
    ///
    /// Referencing the source rather than copying means the run needs no data segment of its
    /// own: the format literal is already laid out, so a run is a slice of it. `%%` is emitted
    /// as a one-byte run pointing at the first `%` of the pair, which is what keeps every run
    /// contiguous even though the pair collapses.
    Literal { offset: usize, length: usize },
    /// One conversion, with its argument already resolved to a zero-based index.
    Conversion {
        argument: usize,
        conversion: u8,
        left: bool,
        plus: bool,
        pad: u8,
        width: usize,
        precision: Option<usize>,
    },
}

/// Parses a `sprintf` format into pieces, or explains why this backend will not lower it.
///
/// The admitted subset is `%%`, `%d`, `%s` and `%f` with php-src's flags, width and precision.
/// Every rule here was measured against php-src 8.5.6 rather than taken from C:
///
/// - the LAST padding flag wins, so `%'x03d` pads with zeros and `%0'x3d` pads with `x`;
/// - `-` cancels a ZERO pad on `%d` and `%f` but NOT on `%s`, and never cancels an explicit
///   `'X` — `%-08d` is space-padded while `%-03s` is zero-padded;
/// - the space flag is IGNORED by PHP, unlike C.
///
/// `-` together with `0` on `%f` is refused rather than reproduced: php-src loses the precision
/// there (`%-08.2f` of 1.5 gives "1.500000"), which is a quirk this target should not bake in.
pub(in crate::codegen_wasm) fn parse_sprintf_format(
    format: &[u8],
    argument_count: usize,
) -> std::result::Result<Vec<FormatPiece>, String> {
    let mut pieces = Vec::new();
    let mut run_start = 0usize;
    let mut next_positional = 0usize;
    let mut i = 0usize;
    while i < format.len() {
        if format[i] != b'%' {
            i += 1;
            continue;
        }
        if i > run_start {
            pieces.push(FormatPiece::Literal {
                offset: run_start,
                length: i - run_start,
            });
        }
        let percent = i;
        i += 1;
        if i >= format.len() {
            return Err("format ends with a lone '%'".to_string());
        }
        if format[i] == b'%' {
            pieces.push(FormatPiece::Literal {
                offset: percent,
                length: 1,
            });
            i += 1;
            run_start = i;
            continue;
        }
        // An explicit `N$` argument selector, which may repeat an argument.
        let start = i;
        let mut digits = 0usize;
        while i < format.len() && format[i].is_ascii_digit() {
            digits = digits * 10 + usize::from(format[i] - b'0');
            i += 1;
        }
        let mut argument = None;
        if i > start && i < format.len() && format[i] == b'$' {
            if digits == 0 {
                return Err("argument numbers start at 1".to_string());
            }
            argument = Some(digits - 1);
            i += 1;
        } else {
            i = start;
        }

        let mut left = false;
        let mut plus = false;
        let mut pad = b' ';
        let mut zero_flag = false;
        loop {
            if i >= format.len() {
                return Err("format ends inside a conversion".to_string());
            }
            match format[i] {
                b'-' => left = true,
                b'+' => plus = true,
                b'0' => {
                    pad = b'0';
                    zero_flag = true;
                }
                b'\'' => {
                    i += 1;
                    if i >= format.len() {
                        return Err("a padding flag needs the character after it".to_string());
                    }
                    pad = format[i];
                    zero_flag = false;
                }
                _ => break,
            }
            i += 1;
        }

        let mut width = 0usize;
        while i < format.len() && format[i].is_ascii_digit() {
            width = width * 10 + usize::from(format[i] - b'0');
            i += 1;
        }
        let mut precision = None;
        if i < format.len() && format[i] == b'.' {
            i += 1;
            let mut value = 0usize;
            while i < format.len() && format[i].is_ascii_digit() {
                value = value * 10 + usize::from(format[i] - b'0');
                i += 1;
            }
            precision = Some(value);
        }
        if i >= format.len() {
            return Err("format ends before its conversion character".to_string());
        }
        let conversion = format[i];
        i += 1;
        if !matches!(conversion, b'd' | b's' | b'f' | b'x' | b'X' | b'b' | b'o') {
            return Err(format!(
                "conversion '%{}' is outside the lowered subset (%%, %d, %s, %f, %x, %X, %b, %o)",
                conversion as char
            ));
        }
        if left && zero_flag && conversion == b'f' {
            return Err(
                "'-' with '0' on %f loses the precision in php-src; refused rather than copied"
                    .to_string(),
            );
        }
        // Left-justifying cancels a zero pad on the numeric conversions only.
        if left && pad == b'0' && matches!(conversion, b'd' | b'f') {
            pad = b' ';
        }
        let argument = match argument {
            Some(explicit) => explicit,
            None => {
                let next = next_positional;
                next_positional += 1;
                next
            }
        };
        if argument >= argument_count {
            return Err(format!(
                "format wants argument #{} but {argument_count} were passed",
                argument + 1
            ));
        }
        run_start = i;
        pieces.push(FormatPiece::Conversion {
            argument,
            conversion,
            left,
            plus,
            pad,
            width,
            precision,
        });
    }
    if format.len() > run_start {
        pieces.push(FormatPiece::Literal {
            offset: run_start,
            length: format.len() - run_start,
        });
    }
    Ok(pieces)
}

/// `__rt_fmt_pad`: places a rendered body inside its field width, sign included.
///
/// The two padding characters behave differently, which is measured php-src behaviour rather
/// than a choice: ZEROS go AFTER the sign — `%05d` of -7 is `-0007` — while spaces and an
/// explicit `'X` go BEFORE it, so `%'x8.2f` of -1.5 is `xxx-1.50`. `$sign` is 0 when there is
/// none, otherwise the character to emit.
pub(super) const RT_FMT_PAD: &str = r#"(func $__rt_fmt_pad (param $bptr i32) (param $blen i64) (param $sign i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (local $signed i64)                                             ;; body length plus any sign
  (local $total i64)                                              ;; final field length
  (local $fill i64)                                               ;; padding characters to add
  (local $out i32)                                                ;; owned result block
  (local $w i32)                                                  ;; write cursor
  (local $i i64)                                                  ;; copy cursor
  (local.set $signed (i64.add (local.get $blen)
    (i64.extend_i32_u (i32.ne (local.get $sign) (i32.const 0)))))
  (local.set $total (local.get $signed))
  (if (i64.gt_s (local.get $width) (local.get $total))
    (then (local.set $total (local.get $width))))                 ;; a shorter width never truncates
  (local.set $fill (i64.sub (local.get $total) (local.get $signed)))
  (local.set $out (call $__rt_str_alloc (local.get $total)))
  (local.set $w (i32.const 0))
  (if (i32.and (i32.eqz (local.get $left)) (i32.ne (local.get $pad) (i32.const 48)))
    (then                                                         ;; spaces and 'X precede the sign
      (block $e1 (loop $l1
        (br_if $e1 (i64.le_s (local.get $fill) (i64.const 0)))
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $pad))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))
        (local.set $fill (i64.sub (local.get $fill) (i64.const 1)))
        (br $l1)))))
  (if (i32.ne (local.get $sign) (i32.const 0))
    (then
      (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $sign))
      (local.set $w (i32.add (local.get $w) (i32.const 1)))))
  (if (i32.and (i32.eqz (local.get $left)) (i32.eq (local.get $pad) (i32.const 48)))
    (then                                                         ;; zeros follow the sign
      (block $e2 (loop $l2
        (br_if $e2 (i64.le_s (local.get $fill) (i64.const 0)))
        (i32.store8 (i32.add (local.get $out) (local.get $w)) (i32.const 48))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))
        (local.set $fill (i64.sub (local.get $fill) (i64.const 1)))
        (br $l2)))))
  (local.set $i (i64.const 0))
  (block $e3 (loop $l3
    (br_if $e3 (i64.ge_s (local.get $i) (local.get $blen)))
    (i32.store8 (i32.add (local.get $out) (local.get $w))
      (i32.load8_u (i32.add (local.get $bptr) (i32.wrap_i64 (local.get $i)))))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $l3)))
  (block $e4 (loop $l4                                            ;; left-justified padding trails
    (br_if $e4 (i64.le_s (local.get $fill) (i64.const 0)))
    (i32.store8 (i32.add (local.get $out) (local.get $w)) (local.get $pad))
    (local.set $w (i32.add (local.get $w) (i32.const 1)))
    (local.set $fill (i64.sub (local.get $fill) (i64.const 1)))
    (br $l4)))
  (local.get $out) (i64.extend_i32_u (local.get $w)))
"#;

/// `__rt_fmt_int`: renders `%d` — decimal digits, an optional sign, then the field.
///
/// The digits are produced without a sign so `__rt_fmt_pad` can place it, which is what makes
/// `%05d` of -7 come out as `-0007` rather than `000-7`. `PHP_INT_MIN` is negated in UNSIGNED
/// space, where its magnitude is representable.
pub(super) const RT_FMT_INT: &str = r#"(func $__rt_fmt_int (param $value i64) (param $plus i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (local $mag i64)                                                ;; magnitude, sign removed
  (local $sign i32)                                               ;; the sign character, or 0
  (local $digits i32)                                             ;; scratch for the digits
  (local $out i32)                                                ;; the padded result block
  (local $outlen i64)                                             ;; its length
  (local $n i32)                                                  ;; digit count
  (local $w i32)                                                  ;; back-to-front cursor
  (local.set $sign (i32.const 0))
  (if (i64.lt_s (local.get $value) (i64.const 0))
    (then
      (local.set $sign (i32.const 45))                             ;; '-'
      (local.set $mag (i64.sub (i64.const 0) (local.get $value)))) ;; wraps only at PHP_INT_MIN,
    (else                                                          ;; whose unsigned form is right
      (local.set $mag (local.get $value))
      (if (local.get $plus) (then (local.set $sign (i32.const 43)))))) ;; '+'
  (local.set $digits (call $__rt_str_alloc (i64.const 24)))       ;; an i64 needs at most 20
  (local.set $w (i32.const 24))
  (block $end (loop $emit
    (local.set $w (i32.sub (local.get $w) (i32.const 1)))
    (i32.store8 (i32.add (local.get $digits) (local.get $w))
      (i32.add (i32.const 48)
        (i32.wrap_i64 (i64.rem_u (local.get $mag) (i64.const 10)))))
    (local.set $mag (i64.div_u (local.get $mag) (i64.const 10)))
    (br_if $end (i64.eqz (local.get $mag)))                       ;; zero still emits one digit
    (br $emit)))
  (local.set $n (i32.sub (i32.const 24) (local.get $w)))
  (call $__rt_fmt_pad (i32.add (local.get $digits) (local.get $w))
    (i64.extend_i32_u (local.get $n)) (local.get $sign)
    (local.get $width) (local.get $pad) (local.get $left))
  ;; Length first: it is the TOP of the two results, and popping them the other way round put
  ;; an i64 into the pointer local and failed module validation.
  (local.set $outlen)
  (local.set $out)
  ;; `__rt_fmt_pad` allocated its own block and copied the digits into it, so the 24-byte
  ;; scratch this reserved has no other owner. Not freeing it leaked 65 bytes per call, on
  ;; every `%d` — measured as `sprintf("%d", $i)` in a loop growing from 3 pages to 80 over
  ;; 80000 iterations while the same loop with a `%s` stayed flat.
  (call $__rt_heap_free_safe (local.get $digits))
  (local.get $out)
  (local.get $outlen))
"#;

/// `__rt_fmt_uint_radix`: renders `%x`, `%X`, `%b` and `%o` — the value as UNSIGNED, in `$radix`.
///
/// PHP reads the argument as an unsigned 64-bit word for these, so `-1` prints as
/// `ffffffffffffffff` and no sign is ever emitted, whatever the `+` flag says. Measured across
/// 0, 1, 255, 4095, -1, -255 and both i64 extremes in all four conversions.
///
/// 64 binary digits is the widest any radix produces, so the scratch is sized for that.
pub(super) const RT_FMT_UINT_RADIX: &str = r#"(func $__rt_fmt_uint_radix (param $value i64) (param $radix i64) (param $upper i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (local $digits i32)
  (local $n i32)
  (local $w i32)
  (local $d i32)
  (local.set $digits (call $__rt_str_alloc (i64.const 64)))       ;; %b of -1 is 64 digits
  (local.set $w (i32.const 64))
  (block $end (loop $emit
    (local.set $w (i32.sub (local.get $w) (i32.const 1)))
    (local.set $d (i32.wrap_i64 (i64.rem_u (local.get $value) (local.get $radix))))
    (if (i32.lt_u (local.get $d) (i32.const 10))
      (then (local.set $d (i32.add (local.get $d) (i32.const 48))))         ;; '0'..'9'
      (else (local.set $d (i32.add (local.get $d)
              (select (i32.const 55) (i32.const 87) (local.get $upper)))))) ;; 'A'-10 or 'a'-10
    (i32.store8 (i32.add (local.get $digits) (local.get $w)) (local.get $d))
    (local.set $value (i64.div_u (local.get $value) (local.get $radix)))
    (br_if $end (i64.eqz (local.get $value)))                     ;; zero still emits one digit
    (br $emit)))
  (local.set $n (i32.sub (i32.const 64) (local.get $w)))
  (call $__rt_fmt_pad (i32.add (local.get $digits) (local.get $w))
    (i64.extend_i32_u (local.get $n)) (i32.const 0)               ;; never a sign character
    (local.get $width) (local.get $pad) (local.get $left)))
"#;

/// `__rt_fmt_str`: renders `%s` — the string, truncated by any precision, then the field.
///
/// A precision TRUNCATES rather than pads, so `%.2s` of "abcdef" is "ab"; `$has_prec` tells an
/// absent precision from an explicit `.0`, which yields the empty string.
pub(super) const RT_FMT_STR: &str = r#"(func $__rt_fmt_str (param $ptr i32) (param $len i64) (param $prec i64) (param $has_prec i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (if (i32.and (local.get $has_prec) (i64.lt_s (local.get $prec) (local.get $len)))
    (then (local.set $len (local.get $prec))))                    ;; precision cuts, never extends
  (call $__rt_fmt_pad (local.get $ptr) (local.get $len) (i32.const 0)
    (local.get $width) (local.get $pad) (local.get $left)))
"#;

/// Recovers a `sprintf` format's literal bytes, or `None` when it is not a literal.
///
/// The whole design rests on this: a known format is parsed once at compile time, so the module
/// carries a fixed sequence of appends instead of a format interpreter. A computed format has no
/// such sequence and is refused.
pub(in crate::codegen_wasm) fn sprintf_literal_format(
    function: &Function,
    module: &Module,
    value: crate::ir::ValueId,
) -> Option<Vec<u8>> {
    let defined = function.value(value)?;
    let crate::ir::ValueDef::Instruction { inst, .. } = defined.def else {
        return None;
    };
    let defining = function.instruction(inst)?;
    if defining.op != Op::ConstStr {
        return None;
    }
    let Some(Immediate::Data(id)) = defining.immediate else {
        return None;
    };
    let interned = module.data.strings.get(id.as_raw() as usize)?;
    Some(crate::string_bytes::literal_bytes(interned))
}

/// Resolves the data segment the literal format was laid out in.
pub(super) fn sprintf_format_segment(ctx: &FnCtx, value: crate::ir::ValueId) -> Result<(u32, u32)> {
    let defined = ctx
        .function
        .value(value)
        .ok_or_else(|| WasmError::Unsupported("sprintf format value is missing".to_string()))?;
    let crate::ir::ValueDef::Instruction { inst, .. } = defined.def else {
        return Err(WasmError::Unsupported(
            "sprintf format is not a literal".to_string(),
        ));
    };
    let defining = ctx.function.instruction(inst).ok_or_else(|| {
        WasmError::Unsupported("sprintf format has no defining instruction".to_string())
    })?;
    let Some(Immediate::Data(id)) = defining.immediate else {
        return Err(WasmError::Unsupported(
            "sprintf format is not a data literal".to_string(),
        ));
    };
    ctx.str_literals
        .get(id.as_raw() as usize)
        .copied()
        .ok_or_else(|| WasmError::Unsupported("sprintf format has no data segment".to_string()))
}

/// Lowers `sprintf` with a literal format into a fixed sequence of appends.
pub(super) fn lower_sprintf(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    emit_formatted_string(ctx, inst, 0)?;
    store_result(ctx, inst)
}

/// Lowers `printf`: the same formatting, written to stdout, answering the BYTE count.
///
/// PHP returns the number of BYTES written rather than characters, so `printf("h\xc3\xa9")`
/// answers 3. The length is already on the stack from the shared builder, which is why this is
/// the same work as `sprintf` plus one write.
pub(super) fn lower_printf(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let length = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    emit_formatted_string(ctx, inst, 0)?;
    ctx.fb
        .ins(&format!("local.tee {length}"), "keep the byte count to answer with");
    ctx.fb.ins("call $__rt_echo_str", "write the formatted bytes");
    ctx.fb
        .ins(&format!("local.get {length}"), "PHP answers the byte count");
    store_result(ctx, inst)
}

/// Validates `fscanf($stream, $format)`: a stream and a LITERAL format in, an array out.
///
/// The by-reference form — `fscanf($f, "%d", $out)` — is a different function: it assigns
/// through its extra arguments and answers a COUNT rather than the fields. Neither backend
/// serves it, so a call that passes one is refused rather than answered with the fields.
pub(super) fn fscanf_shape_issue(function: &Function, module: &Module, call: &Instruction) -> Option<String> {
    let [handle, format] = call.operands.as_slice() else {
        return Some(format!(
            "fscanf takes a stream and a format, got {} operands{}",
            call.operands.len(),
            if call.operands.len() > 2 {
                " — the by-reference form assigns through them and answers a count"
            } else {
                ""
            }
        ));
    };
    let Some(handle) = function.value(*handle) else {
        return Some("stream operand is missing from the value table".to_string());
    };
    let boxed = handle.ir_type == IrType::Heap(IrHeapKind::Mixed);
    let predefined =
        handle.ir_type == IrType::I64 && matches!(handle.php_type, PhpType::Resource(_));
    if !boxed && !predefined {
        return Some(format!(
            "fscanf takes a stream handle, not {:?}/{:?}",
            handle.ir_type,
            handle.php_type.codegen_repr()
        ));
    }
    let Some(format) = sprintf_literal_format(function, module, *format) else {
        return Some(
            "only a LITERAL format is scanned; a computed one would need a format interpreter \
             at runtime, which this target does not carry"
                .to_string(),
        );
    };
    let Ok(format) = std::str::from_utf8(&format) else {
        return Some("the format is not valid UTF-8".to_string());
    };
    if let Err(why) = crate::codegen_wasm::scanf::parse_scan_format(format) {
        return Some(format!("fscanf format: {why}"));
    }
    if call.result.is_none() || call.result_type != IrType::Heap(IrHeapKind::Array) {
        return Some(format!(
            "fscanf answers array<string>, not {:?}",
            call.result_type
        ));
    }
    None
}

/// Lowers `fscanf($stream, $format)`: read one line, split it, render each field.
///
/// The format is settled when the module is built, so this emits one token-and-render step per
/// conversion rather than a runtime format cursor. Only `%s` is lowered: php types `%d` and `%f`
/// fields as int and float, which this builtin's `array<string>` type cannot hold, so
/// `parse_scan_format` refuses those formats by name. See `scanf`'s module comment.
pub(super) fn lower_fscanf(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    use crate::codegen_wasm::wat::ValType;

    let format_value = operand(inst, 1)?;
    let format = sprintf_literal_format(ctx.function, ctx.module, format_value)
        .ok_or_else(|| WasmError::Unsupported("fscanf format is not a literal".to_string()))?;
    let format = std::str::from_utf8(&format)
        .map_err(|_| WasmError::Unsupported("fscanf format is not valid UTF-8".to_string()))?;
    let conversions = crate::codegen_wasm::scanf::parse_scan_format(format)
        .map_err(|why| WasmError::Unsupported(format!("fscanf format: {why}")))?;

    let handle = operand(inst, 0)?;
    let boxed = ctx
        .function
        .value(handle)
        .is_some_and(|value| value.ir_type == IrType::Heap(IrHeapKind::Mixed));
    let fd = ctx.fresh_temp(ValType::I32);
    ctx.emit_load_value(handle)?;
    if boxed {
        ctx.fb.ins("call $__rt_stream_fd", "unbox the stream");
    } else {
        ctx.fb.ins("call $__rt_handle_fd", "a predefined stream IS its fd");
    }
    ctx.fb.ins(&format!("local.set {fd}"), "the fd or memstream");

    let line = ctx.fresh_temp(ValType::I32);
    let line_len = ctx.fresh_temp(ValType::I32);
    let ok = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.get {fd}"), "the stream to read");
    ctx.fb
        .ins("call $__rt_stream_read_line", "one line, owned, newline stripped");
    ctx.fb.ins(&format!("local.set {ok}"), "was there a line at all?");
    ctx.fb.ins(&format!("local.set {line_len}"), "its length");
    ctx.fb.ins(&format!("local.set {line}"), "its bytes");

    // The array is built empty and filled only when a line came back, so end of input answers
    // the empty array php's `false` stands in for here.
    let array = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(
        &format!(
            "(local.set {array} (call $__rt_array_new (i64.const {}) (i64.const 16)))",
            conversions.len()
        ),
        "one string slot per conversion",
    );

    let pos = ctx.fresh_temp(ValType::I32);
    let tok = ctx.fresh_temp(ValType::I32);
    let tok_len = ctx.fresh_temp(ValType::I32);
    ctx.fb.ins(&format!("local.get {ok}"), "a line was read?");
    ctx.fb.ins("if", "fill the fields only when there was a line");
    ctx.fb.ins(&format!("(local.set {pos} (i32.const 0))"), "start of line");
    for conversion in &conversions {
        ctx.fb.ins(&format!("local.get {line}"), "the line");
        ctx.fb.ins(&format!("local.get {line_len}"), "its length");
        ctx.fb.ins(&format!("local.get {pos}"), "the cursor");
        ctx.fb
            .ins("call $__rt_scan_token", "the next whitespace-delimited run");
        ctx.fb.ins(&format!("local.set {pos}"), "advance the cursor");
        ctx.fb.ins(&format!("local.set {tok_len}"), "the token's length");
        ctx.fb.ins(&format!("local.set {tok}"), "the token's bytes");
        ctx.fb.ins(&format!("local.get {array}"), "the fields so far");
        match conversion {
            crate::codegen_wasm::scanf::ScanConversion::Str => {
                ctx.fb.ins(&format!("local.get {tok}"), "the token itself");
                ctx.fb.ins(
                    &format!("(i64.extend_i32_u (local.get {tok_len}))"),
                    "its length",
                );
            }
        }
        // The arm above leaves (pointer, length) on top of the array pushed before it, which is
        // exactly `__rt_array_push_str`'s argument order. A field the line did not supply is the
        // empty string where php answers null; see `scanf`'s module comment.
        ctx.fb.ins("call $__rt_array_push_str", "append the field");
        ctx.fb.ins(
            &format!("local.set {array}"),
            "keep the array the push answered",
        );
    }
    ctx.fb.ins("end", "end the has-a-line guard");

    ctx.fb.ins(&format!("local.get {line}"), "the line block");
    ctx.fb.ins("call $__rt_heap_free", "the fields own their own copies now");
    ctx.fb.ins(&format!("local.get {array}"), "the finished fields");
    store_result(ctx, inst)
}

/// Lowers `fprintf`: the same formatting as `sprintf`, written to a stream.
///
/// php answers the BYTE count written, which `__rt_fwrite` already reports — so this is
/// `sprintf` plus one write, exactly as `printf` is, and the only difference between the two is
/// where the bytes go and that the format sits one operand later.
pub(super) fn lower_fprintf(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let target = operand(inst, 0)?;
    let boxed = ctx
        .function
        .value(target)
        .is_some_and(|value| value.ir_type == IrType::Heap(IrHeapKind::Mixed));
    let fd = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    ctx.emit_load_value(target)?;
    if boxed {
        ctx.fb.ins("call $__rt_stream_fd", "unbox the stream");
    } else {
        // `STDOUT` and friends arrive as the fd itself, widened to i64 by the value table.
        ctx.fb.ins("call $__rt_handle_fd", "a predefined stream IS its fd");
    }
    ctx.fb.ins(&format!("local.set {fd}"), "the fd or memstream");
    // The formatter leaves (pointer, length); `__rt_fwrite` wants (fd, pointer, length), so the
    // fd has to go under them — hence the spill rather than loading it inline.
    ctx.fb.ins(&format!("local.get {fd}"), "write target");
    emit_formatted_string(ctx, inst, 1)?;
    // A read-only in-memory stream refuses the write with -1, yet php's `fprintf` still answers
    // the length it formatted (measured: `fprintf(fopen("php://memory", "r"), "xy")` is 2).
    let len = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    let written = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.fb.ins(&format!("local.tee {len}"), "the formatted length");
    ctx.fb.ins("drop", "keep it in the local");
    ctx.fb.ins(&format!("local.get {len}"), "the length again, as the write wants it");
    ctx.fb
        .ins("call $__rt_fwrite", "write the formatted bytes, answering the count");
    ctx.fb.ins(&format!("local.set {written}"), "the count written");
    ctx.fb.ins(&format!("local.get {len}"), "the formatted length, if the write was refused");
    ctx.fb.ins(&format!("local.get {written}"), "the count otherwise");
    ctx.fb.ins(&format!("local.get {written}"), "the count");
    ctx.fb.ins("i64.const 0", "no write is negative");
    ctx.fb.ins("i64.lt_s", "was the write refused?");
    ctx.fb.ins("select", "answer the length for a refused write");
    store_result(ctx, inst)
}

/// Builds the formatted string, leaving `(pointer, length)` on the stack.
///
/// `base` is the operand index the FORMAT sits at, so the conversions read their arguments from
/// `base + 1` onward. `sprintf` and `printf` pass 0; `fprintf` passes 1, because its first
/// operand is the stream. Threading an index rather than copying the builder is what keeps the
/// three sharing one format parser and one set of `__rt_fmt_*` helpers.
pub(super) fn emit_formatted_string(ctx: &mut FnCtx, inst: &Instruction, base: usize) -> Result<()> {
    let format_value = operand(inst, base)?;
    let format = sprintf_literal_format(ctx.function, ctx.module, format_value)
        .ok_or_else(|| WasmError::Unsupported("sprintf format is not a literal".to_string()))?;
    let pieces = parse_sprintf_format(&format, inst.operands.len() - base - 1)
        .map_err(|why| WasmError::Unsupported(format!("sprintf format: {why}")))?;
    // Every literal run is a slice of the format's own data segment, so its address is that
    // segment's base plus the run's offset. No run needs a segment of its own.
    let format_base = sprintf_format_segment(ctx, format_value)?.0;

    let acc_ptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    let acc_len = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    // Start from the empty string so a format with no pieces still yields one, and so every
    // piece below is an ordinary append rather than a special first case.
    ctx.fb.ins("i32.const 0", "empty accumulator pointer");
    ctx.fb.ins(&format!("local.set {acc_ptr}"), "accumulator");
    ctx.fb.ins("i64.const 0", "empty accumulator length");
    ctx.fb.ins(&format!("local.set {acc_len}"), "accumulator length");

    // Reused across pieces: the rendered piece, and the concat's answer beside it.
    let piece_ptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    let piece_len = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    let joined_ptr = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I32);
    let joined_len = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);

    for piece in &pieces {
        match piece {
            FormatPiece::Literal { offset, length } => {
                ctx.fb.ins(
                    &format!("i32.const {}", format_base + *offset as u32),
                    "a run of the format's own bytes",
                );
                ctx.fb.ins(&format!("i64.const {length}"), "its byte length");
            }
            FormatPiece::Conversion {
                argument,
                conversion,
                left,
                plus,
                pad,
                width,
                precision,
            } => {
                let value = operand(inst, base + argument + 1)?;
                match conversion {
                    b'd' => {
                        ctx.emit_load_value(value)?;
                        ctx.fb.ins(
                            &format!("i32.const {}", i32::from(*plus)),
                            "does a positive value carry a plus?",
                        );
                        ctx.fb.ins(&format!("i64.const {width}"), "field width");
                        ctx.fb.ins(&format!("i32.const {pad}"), "padding character");
                        ctx.fb
                            .ins(&format!("i32.const {}", i32::from(*left)), "left-justified?");
                        ctx.fb.ins("call $__rt_fmt_int", "render %d");
                    }
                    b'x' | b'X' | b'b' | b'o' => {
                        // PHP reads these as UNSIGNED, so no sign is ever emitted and the `+`
                        // flag does not reach them.
                        let (radix, upper) = match conversion {
                            b'x' => (16, 0),
                            b'X' => (16, 1),
                            b'b' => (2, 0),
                            _ => (8, 0),
                        };
                        ctx.emit_load_value(value)?;
                        ctx.fb.ins(&format!("i64.const {radix}"), "radix");
                        ctx.fb
                            .ins(&format!("i32.const {upper}"), "uppercase alphabet?");
                        ctx.fb.ins(&format!("i64.const {width}"), "field width");
                        ctx.fb.ins(&format!("i32.const {pad}"), "padding character");
                        ctx.fb
                            .ins(&format!("i32.const {}", i32::from(*left)), "left-justified?");
                        ctx.fb
                            .ins("call $__rt_fmt_uint_radix", "render an unsigned radix field");
                    }
                    b's' => {
                        ctx.emit_load_value(value)?;
                        ctx.fb.ins(
                            &format!("i64.const {}", precision.unwrap_or(0)),
                            "precision, when one was written",
                        );
                        ctx.fb.ins(
                            &format!("i32.const {}", i32::from(precision.is_some())),
                            "an absent precision is not a zero one",
                        );
                        ctx.fb.ins(&format!("i64.const {width}"), "field width");
                        ctx.fb.ins(&format!("i32.const {pad}"), "padding character");
                        ctx.fb
                            .ins(&format!("i32.const {}", i32::from(*left)), "left-justified?");
                        ctx.fb.ins("call $__rt_fmt_str", "render %s");
                    }
                    b'f' => {
                        ctx.emit_load_value(value)?;
                        ctx.fb
                            .ins("i64.reinterpret_f64", "the exact bits are what gets rounded");
                        ctx.fb.ins(
                            &format!("i64.const {}", precision.unwrap_or(6)),
                            "an absent precision is PHP's six, not zero",
                        );
                        ctx.fb.ins(
                            &format!("i32.const {}", i32::from(*plus)),
                            "does a positive value carry a plus?",
                        );
                        ctx.fb.ins(&format!("i64.const {width}"), "field width");
                        ctx.fb.ins(&format!("i32.const {pad}"), "padding character");
                        ctx.fb
                            .ins(&format!("i32.const {}", i32::from(*left)), "left-justified?");
                        ctx.fb.ins("call $__rt_fmt_float", "render %f");
                    }
                    other => {
                        return Err(WasmError::Unsupported(format!(
                            "sprintf conversion %{}",
                            *other as char
                        )))
                    }
                }
            }
        }
        // The piece and the accumulator are parked before the concat so both can be freed
        // after it: `__rt_concat` ALLOCATES and copies, so neither input has another owner.
        // Every conversion's piece is a fresh block — each `__rt_fmt_*` ends in
        // `__rt_fmt_pad`, which always allocates and never hands back its input — while a
        // literal run is a slice of the format's own data segment and the initial accumulator
        // is the null pointer, and `__rt_heap_free_safe` is a no-op for both of those.
        //
        // Leaving them leaked once per conversion: `sprintf("%d", $i)` in a loop grew from 3
        // pages to 80 over 80000 iterations. Freeing the digit scratch inside `__rt_fmt_int`
        // took that to 31; this takes it to 3.
        ctx.fb.ins(&format!("local.set {piece_len}"), "this piece's length");
        ctx.fb.ins(&format!("local.set {piece_ptr}"), "this piece's bytes");
        ctx.fb.ins(&format!("local.get {acc_ptr}"), "accumulated so far");
        ctx.fb.ins(&format!("local.get {acc_len}"), "accumulated length");
        ctx.fb.ins(&format!("local.get {piece_ptr}"), "this piece's bytes");
        ctx.fb.ins(&format!("local.get {piece_len}"), "this piece's length");
        ctx.fb.ins("call $__rt_concat", "append this piece");
        // The joined result needs temps of its OWN: reusing the piece's would overwrite the
        // pointer before it could be freed, which is the half-fix this replaces — freeing the
        // accumulator alone took `sprintf("%d", $i)` from 65 bytes a call to 25, not to zero.
        ctx.fb.ins(&format!("local.set {joined_len}"), "the joined length");
        ctx.fb.ins(&format!("local.set {joined_ptr}"), "the joined bytes");
        ctx.fb.ins(
            &format!("(call $__rt_heap_free_safe (local.get {acc_ptr}))"),
            "the accumulator the concat copied out of",
        );
        ctx.fb.ins(
            &format!("(call $__rt_heap_free_safe (local.get {piece_ptr}))"),
            "and the piece it copied out of",
        );
        ctx.fb.ins(&format!("local.get {joined_ptr}"), "the joined bytes");
        ctx.fb.ins(&format!("local.set {acc_ptr}"), "new accumulator");
        ctx.fb.ins(&format!("local.get {joined_len}"), "the joined length");
        ctx.fb.ins(&format!("local.set {acc_len}"), "new length");
    }
    ctx.fb.ins(&format!("local.get {acc_ptr}"), "the formatted string");
    ctx.fb.ins(&format!("local.get {acc_len}"), "its length");
    Ok(())
}

/// Validates `sprintf`: a LITERAL format whose conversions match the argument types.
pub(super) fn sprintf_shape_issue(
    function: &Function,
    module: &Module,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    // `fprintf` puts the stream first, so every operand index below shifts by one.
    let base = usize::from(target == RuntimeFnId::Fprintf);
    let Some(format_value) = call.operands.get(base) else {
        return Some("sprintf needs a format".to_string());
    };
    let Some(format) = sprintf_literal_format(function, module, *format_value) else {
        return Some(
            "only a LITERAL format is lowered; a computed one would need a format interpreter"
                .to_string(),
        );
    };
    let pieces = match parse_sprintf_format(&format, call.operands.len() - base - 1) {
        Ok(pieces) => pieces,
        Err(why) => return Some(format!("sprintf format: {why}")),
    };
    for piece in &pieces {
        let FormatPiece::Conversion {
            argument,
            conversion,
            ..
        } = piece
        else {
            continue;
        };
        let Some(value) = function.value(call.operands[base + argument + 1]) else {
            return Some("argument is missing from the value table".to_string());
        };
        let (want_ir, want_php) = match conversion {
            b'd' | b'x' | b'X' | b'b' | b'o' => (IrType::I64, PhpType::Int),
            b'f' => (IrType::F64, PhpType::Float),
            _ => (IrType::Str, PhpType::Str),
        };
        // PHP coerces an argument to the conversion's type; that coercion carries its own
        // diagnostics, so an exact match is required rather than converted here.
        if value.ir_type != want_ir || value.php_type.codegen_repr() != want_php {
            return Some(format!(
                "%{} wants {want_ir:?}/{want_php:?}, argument #{} is {:?}/{:?}",
                *conversion as char,
                argument + 1,
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    // `printf` and `fprintf` write the string and answer its BYTE count; `sprintf` answers
    // the string itself.
    let (want_ir, want_php) = if matches!(target, RuntimeFnId::Printf | RuntimeFnId::Fprintf) {
        (IrType::I64, PhpType::Int)
    } else {
        (IrType::Str, PhpType::Str)
    };
    if target == RuntimeFnId::Fprintf {
        let Some(handle) = call.operands.first().and_then(|value| function.value(*value)) else {
            return Some("fprintf needs a stream".to_string());
        };
        // Two shapes reach here: `fopen` answers a BOXED resource, while `STDOUT`/`STDERR`/
        // `STDIN` arrive as the fd itself under a `Resource` php type. Both are streams.
        let boxed = handle.ir_type == IrType::Heap(IrHeapKind::Mixed);
        let predefined =
            handle.ir_type == IrType::I64 && matches!(handle.php_type, PhpType::Resource(_));
        if !boxed && !predefined {
            return Some(format!(
                "fprintf takes a stream handle, not {:?}/{:?}",
                handle.ir_type,
                handle.php_type.codegen_repr()
            ));
        }
    }
    if call.result.is_none()
        || call.result_type != want_ir
        || call.result_php_type.codegen_repr() != want_php
    {
        return Some(format!(
            "{target:?} result {:?}/{:?} is not the expected {want_ir:?}/{want_php:?}",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    None
}

/// `__rt_fmt_float`: renders `%f` from the EXACT decimal digits of the double.
///
/// PHP's `%f` rounds the exact binary value with ties-to-even — `%.0f` of 0.5 is 0 and of 2.5 is
/// 2 — which is C's rule and NOT `number_format`'s. `number_format` rounds the shortest decimal
/// that round-trips, so `number_format(2.675, 2)` is 2.68 while `sprintf("%.2f", 2.675)` is 2.67.
/// Working from `__rt_f64_digits`' exact digits is what makes the difference reproducible.
///
/// Non-finite values IGNORE the field entirely: `%08.2f` of INF is `INF`, not `00000INF`, and the
/// `+` flag does not reach them either — measured. A true zero drops its sign, so `-0.0` prints
/// `0.00`, while a negative value that merely ROUNDS to zero keeps it and prints `-0.00`.
pub(super) const RT_FMT_FLOAT: &str = r#"(func $__rt_fmt_float (param $bits i64) (param $prec i64) (param $plus i32) (param $width i64) (param $pad i32) (param $left i32) (result i32) (result i64)
  (local $scratch i32) (local $big i32) (local $dbuf i32) (local $kept i32) (local $body i32)
  (local $sign i32) (local $class i32) (local $digptr i32) (local $ndigits i32) (local $p i32)
  (local $keep i64) (local $klen i64) (local $i i64) (local $first i32) (local $up i32)
  (local $rest i64) (local $carry i64) (local $intlen i64) (local $w i32) (local $signch i32)
  (local.set $scratch (call $__rt_heap_alloc
    (i32.add (i32.const 2112) (i32.wrap_i64 (i64.mul (local.get $prec) (i64.const 2))))))
  (local.set $big (local.get $scratch))                           ;; 80 limbs, must start zeroed
  (local.set $dbuf (i32.add (local.get $scratch) (i32.const 640)))   ;; 792 bytes: see __rt_f64_digits
  (local.set $kept (i32.add (local.get $scratch) (i32.const 1440)))  ;; past dbuf's 640+792
  (local.set $i (i64.const 0))
  (block $ze (loop $zl                                            ;; zero the bignum limbs
    (br_if $ze (i64.ge_s (local.get $i) (i64.const 640)))
    (i32.store8 (i32.add (local.get $big) (i32.wrap_i64 (local.get $i))) (i32.const 0))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $zl)))
  (call $__rt_f64_digits (local.get $bits) (local.get $big) (i32.const 80)
        (local.get $dbuf) (i32.const 792))
  (local.set $p) (local.set $ndigits) (local.set $digptr)
  (local.set $class) (local.set $sign)
  (if (i32.eq (local.get $class) (i32.const 1))                   ;; infinity ignores the field
    (then
      (local.set $body (call $__rt_str_alloc (i64.const 4)))
      (local.set $w (i32.const 0))
      (if (local.get $sign)
        (then
          (i32.store8 (local.get $body) (i32.const 45))
          (local.set $w (i32.const 1))))
      (i32.store8 (i32.add (local.get $body) (local.get $w)) (i32.const 73))          ;; I
      (i32.store8 offset=1 (i32.add (local.get $body) (local.get $w)) (i32.const 78)) ;; N
      (i32.store8 offset=2 (i32.add (local.get $body) (local.get $w)) (i32.const 70)) ;; F
      (return (local.get $body) (i64.extend_i32_u (i32.add (local.get $w) (i32.const 3))))))
  (if (i32.eq (local.get $class) (i32.const 2))                   ;; PHP spells it NaN, not NAN
    (then
      (local.set $body (call $__rt_str_alloc (i64.const 3)))
      (i32.store8 (local.get $body) (i32.const 78))               ;; N
      (i32.store8 offset=1 (local.get $body) (i32.const 97))      ;; a
      (i32.store8 offset=2 (local.get $body) (i32.const 78))      ;; N
      (return (local.get $body) (i64.const 3))))
  (if (i32.eq (local.get $class) (i32.const 3))
    (then                                                          ;; a true zero has no digits
      (local.set $ndigits (i32.const 0))
      (local.set $p (i32.const 0))
      (local.set $sign (i32.const 0))))                            ;; ...and drops its sign
  (local.set $keep (i64.add (i64.sub (i64.extend_i32_s (local.get $ndigits))
                                     (i64.extend_i32_s (local.get $p)))
                            (local.get $prec)))
  (local.set $klen (local.get $keep))
  (if (i64.lt_s (local.get $klen) (i64.const 0))
    (then (local.set $klen (i64.const 0))))
  (local.set $i (i64.const 0))
  (block $ce (loop $cl                                            ;; the kept prefix, zero-extended
    (br_if $ce (i64.ge_s (local.get $i) (local.get $klen)))
    (i32.store8 (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i)))
      (select
        (i32.load8_u (i32.add (local.get $digptr) (i32.wrap_i64 (local.get $i))))
        (i32.const 48)
        (i64.lt_s (local.get $i) (i64.extend_i32_s (local.get $ndigits)))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $cl)))
  (local.set $up (i32.const 0))
  (if (i32.and (i64.ge_s (local.get $keep) (i64.const 0))
               (i64.lt_s (local.get $keep) (i64.extend_i32_s (local.get $ndigits))))
    (then
      (local.set $first (i32.sub
        (i32.load8_u (i32.add (local.get $digptr) (i32.wrap_i64 (local.get $keep))))
        (i32.const 48)))
      (local.set $rest (i64.add (local.get $keep) (i64.const 1)))
      (local.set $carry (i64.const 0))                            ;; reused: any non-zero tail?
      (block $re (loop $rl
        (br_if $re (i64.ge_s (local.get $rest) (i64.extend_i32_s (local.get $ndigits))))
        (if (i32.ne (i32.load8_u (i32.add (local.get $digptr) (i32.wrap_i64 (local.get $rest))))
                    (i32.const 48))
          (then (local.set $carry (i64.const 1)) (br $re)))
        (local.set $rest (i64.add (local.get $rest) (i64.const 1)))
        (br $rl)))
      (if (i32.gt_u (local.get $first) (i32.const 5))
        (then (local.set $up (i32.const 1))))
      (if (i32.eq (local.get $first) (i32.const 5))
        (then (if (i64.ne (local.get $carry) (i64.const 0))
          (then (local.set $up (i32.const 1)))
          (else                                                    ;; an exact tie rounds to EVEN
            (if (i64.gt_s (local.get $klen) (i64.const 0))
              (then (local.set $up (i32.and (i32.const 1)
                (i32.sub (i32.load8_u (i32.add (local.get $kept)
                  (i32.wrap_i64 (i64.sub (local.get $klen) (i64.const 1))))) (i32.const 48))))))))))))
  (if (local.get $up)
    (then
      (local.set $i (local.get $klen))
      (block $ue (loop $ul                                        ;; propagate the carry leftwards
        (br_if $ue (i64.le_s (local.get $i) (i64.const 0)))
        (local.set $i (i64.sub (local.get $i) (i64.const 1)))
        (if (i32.eq (i32.load8_u (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i))))
                    (i32.const 57))                               ;; '9' wraps and carries on
          (then (i32.store8 (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i))) (i32.const 48)))
          (else
            (i32.store8 (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i)))
              (i32.add (i32.load8_u (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i))))
                       (i32.const 1)))
            (local.set $up (i32.const 0))                         ;; absorbed
            (br $ue)))
        (br $ul)))
      (if (local.get $up)
        (then                                                     ;; every digit was 9: prepend 1
          (local.set $i (local.get $klen))
          (block $se (loop $sl
            (br_if $se (i64.le_s (local.get $i) (i64.const 0)))
            (i32.store8 (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i)))
              (i32.load8_u (i32.add (local.get $kept)
                (i32.wrap_i64 (i64.sub (local.get $i) (i64.const 1))))))
            (local.set $i (i64.sub (local.get $i) (i64.const 1)))
            (br $sl)))
          (i32.store8 (local.get $kept) (i32.const 49))            ;; '1'
          (local.set $klen (i64.add (local.get $klen) (i64.const 1)))
          (local.set $keep (i64.add (local.get $keep) (i64.const 1)))))))
  (local.set $intlen (i64.sub (local.get $keep) (local.get $prec)))
  (local.set $body (call $__rt_str_alloc
    (i64.add (i64.add (local.get $klen) (local.get $prec)) (i64.const 8))))
  (local.set $w (i32.const 0))
  (if (i64.le_s (local.get $intlen) (i64.const 0))
    (then                                                          ;; no integer digits: a bare 0
      (i32.store8 (local.get $body) (i32.const 48))
      (local.set $w (i32.const 1)))
    (else
      (local.set $i (i64.const 0))
      (block $ie (loop $il
        (br_if $ie (i64.ge_s (local.get $i) (local.get $intlen)))
        (i32.store8 (i32.add (local.get $body) (local.get $w))
          (i32.load8_u (i32.add (local.get $kept) (i32.wrap_i64 (local.get $i)))))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $il)))))
  (if (i64.gt_s (local.get $prec) (i64.const 0))
    (then
      (i32.store8 (i32.add (local.get $body) (local.get $w)) (i32.const 46))   ;; '.'
      (local.set $w (i32.add (local.get $w) (i32.const 1)))
      (local.set $i (i64.const 0))
      (block $fe (loop $fl
        (br_if $fe (i64.ge_s (local.get $i) (local.get $prec)))
        ;; the fraction starts at intlen; a negative intlen means leading zeros first
        (i32.store8 (i32.add (local.get $body) (local.get $w))
          (select (i32.const 48)
            (select
              (i32.load8_u (i32.add (local.get $kept)
                (i32.wrap_i64 (i64.add (local.get $intlen) (local.get $i)))))
              (i32.const 48)
              (i64.lt_s (i64.add (local.get $intlen) (local.get $i)) (local.get $klen)))
            (i64.lt_s (i64.add (local.get $intlen) (local.get $i)) (i64.const 0))))
        (local.set $w (i32.add (local.get $w) (i32.const 1)))
        (local.set $i (i64.add (local.get $i) (i64.const 1)))
        (br $fl)))))
  (call $__rt_heap_free (local.get $scratch))
  (local.set $signch (i32.const 0))
  (if (local.get $sign)
    (then (local.set $signch (i32.const 45)))
    (else (if (local.get $plus) (then (local.set $signch (i32.const 43))))))
  (call $__rt_fmt_pad (local.get $body) (i64.extend_i32_u (local.get $w))
    (local.get $signch) (local.get $width) (local.get $pad) (local.get $left)))
"#;
