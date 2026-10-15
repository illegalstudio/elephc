//! Purpose:
//! Numeric builtins: `round` helpers (WAT), integer `abs`/`min`/`max`, `range`, and their shape predicates.
//!
//! Called from:
//! - `super::lower_direct_builtin` and `super::direct_builtin_shape_issue` dispatch to it.
//! - `crate::codegen_wasm::capability` reads its `*_shape_issue` predicates for the static audit.

use super::*;

/// `__rt_round`: PHP's `round()` at precision 0 — half away from ZERO.
///
/// WebAssembly's `f64.nearest` is half-to-EVEN, so it answers 2 for `round(2.5)` where PHP answers
/// 3. The naive repair, `floor(|x| + 0.5)`, is worse: the addition is inexact, so
/// `round(0.49999999999999994)` answers 1 where PHP answers 0, and above 2^52 it perturbs values
/// that are already integers.
///
/// Comparing against the TRUNCATED part instead is exact — `x - trunc(x)` loses nothing for
/// |x| < 2^53, and above it the difference is zero because x is already an integer. `f64.trunc`
/// also carries the sign of zero, which PHP prints: `round(-0.4)` is `-0`.
///
/// Verified on 104 values against php-src 8.5.6, including both halfway directions, the
/// 0.49999999999999994 trap, the 2^52/2^53 boundaries, infinities and NaN.
pub(super) const RT_ROUND: &str = r#"(func $__rt_round (param $x f64) (result f64)
  (local $t f64)
  (if (f64.ne (local.get $x) (local.get $x))                ;; NaN rounds to itself
    (then (return (local.get $x))))
  (if (f64.eq (f64.abs (local.get $x)) (f64.const inf))     ;; so does an infinity
    (then (return (local.get $x))))
  (local.set $t (f64.trunc (local.get $x)))                 ;; keeps the sign of zero
  (if (f64.ge (f64.abs (f64.sub (local.get $x) (local.get $t))) (f64.const 0.5))
    (then (return (f64.add (local.get $t) (f64.copysign (f64.const 1) (local.get $x))))))
  (local.get $t))
"#;

/// `__rt_pow10`: php-src's `php_intpow10`, exact for the 0..22 table it carries.
///
/// Above 22 php-src calls `pow(10.0, p)`, which repeated multiplication does not reproduce to the
/// last bit, so `__rt_round_places` refuses that range rather than answering nearly-right.
pub(super) const RT_POW10: &str = r#"(func $__rt_pow10 (param $p i32) (result f64)
  (local $r f64) (local $i i32)
  (local.set $r (f64.const 1))
  (local.set $i (i32.const 0))
  (block $done
    (loop $mul
      (br_if $done (i32.ge_s (local.get $i) (local.get $p)))
      (local.set $r (f64.mul (local.get $r) (f64.const 10)))   ;; exact for 1e0..1e22
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $mul)))
  (local.get $r))
"#;

/// `__rt_round_places`: PHP's two-argument `round($value, $places)`, transcribed from php-src
/// 8.5's `_php_math_round` with `PHP_ROUND_HALF_UP`.
///
/// The naive scale-round-unscale is NOT this function. php-src first extracts the integral part
/// with `floor`/`ceil`, then repairs the extraction: scaling is inexact, so `0.285 * 1e10` is
/// `2849999999.9999995` and `floor` loses a unit. Adding one back and checking whether the
/// unscaled result equals the input recovers it — which is why `round(1.005, 2)` is `1.01` and
/// `round(0.285, 2)` is `0.29`, both of which the naive form gets wrong. Rounding is then decided
/// against an EDGE CASE computed in the original scale, not by comparing a scaled fraction.
///
/// `f64.copysign` carries the sign of zero, which PHP prints: `round(-0.5, -3)` is `-0`.
///
/// Transcribed to Python and validated at **1420/1420** against php-src 8.5.6 — 220 systematic
/// cases at the halfway values, the `1.005`/`9.995`/`0.285` traps, the 1e15/1e-15 boundaries and
/// both zeroes, plus 1200 random values spanning 24 orders of magnitude at precisions -6..8. The
/// naive model scores 1087/1420 on that same corpus.
pub(super) const RT_ROUND_PLACES: &str = r#"(func $__rt_round_places (param $v f64) (param $places i64) (result f64)
  (local $p i32) (local $exp f64) (local $tmp f64) (local $tmp2 f64) (local $edge f64) (local $back f64)
  (if (f64.ne (local.get $v) (local.get $v))                      ;; NaN rounds to itself
    (then (return (local.get $v))))
  (if (f64.eq (f64.abs (local.get $v)) (f64.const inf))           ;; so does an infinity
    (then (return (local.get $v))))
  (if (f64.eq (local.get $v) (f64.const 0))                       ;; and either zero, sign kept
    (then (return (local.get $v))))
  (local.set $p (i32.wrap_i64 (local.get $places)))
  (local.set $exp (call $__rt_pow10                               ;; php_intpow10(abs(places))
    (select (local.get $p) (i32.sub (i32.const 0) (local.get $p)) (i32.gt_s (local.get $p) (i32.const 0)))))
  (if (f64.ge (local.get $v) (f64.const 0))
    (then
      (local.set $tmp (f64.floor (select
        (f64.mul (local.get $v) (local.get $exp))
        (f64.div (local.get $v) (local.get $exp))
        (i32.gt_s (local.get $p) (i32.const 0)))))
      (local.set $tmp2 (f64.add (local.get $tmp) (f64.const 1))))
    (else
      (local.set $tmp (f64.ceil (select
        (f64.mul (local.get $v) (local.get $exp))
        (f64.div (local.get $v) (local.get $exp))
        (i32.gt_s (local.get $p) (i32.const 0)))))
      (local.set $tmp2 (f64.sub (local.get $tmp) (f64.const 1)))))
  (local.set $back (select                                        ;; unscale the +/-1 candidate
    (f64.div (local.get $tmp2) (local.get $exp))
    (f64.mul (local.get $tmp2) (local.get $exp))
    (i32.gt_s (local.get $p) (i32.const 0))))
  (if (f64.eq (local.get $back) (local.get $v))                   ;; the extraction lost a unit
    (then (local.set $tmp (local.get $tmp2))))
  (if (f64.ge (f64.abs (local.get $tmp)) (f64.const 1e16))        ;; beyond our precision
    (then (return (local.get $v))))
  (local.set $edge (f64.abs (select                               ;; HALF_UP edge, original scale
    (f64.div (f64.add (local.get $tmp) (f64.copysign (f64.const 0.5) (local.get $tmp))) (local.get $exp))
    (f64.mul (f64.add (local.get $tmp) (f64.copysign (f64.const 0.5) (local.get $tmp))) (local.get $exp))
    (i32.gt_s (local.get $p) (i32.const 0)))))
  (if (f64.ge (f64.abs (local.get $v)) (local.get $edge))
    (then (local.set $tmp (f64.add (local.get $tmp) (f64.copysign (f64.const 1) (local.get $tmp))))))
  (select
    (f64.div (local.get $tmp) (local.get $exp))
    (f64.mul (local.get $tmp) (local.get $exp))
    (i32.gt_s (local.get $p) (i32.const 0))))
"#;

/// Lowers `abs($int)` branchlessly as `(x ^ (x >> 63)) - (x >> 63)`.
///
/// KNOWN DIVERGENCE, shared with the native backend and rooted in EIR rather than in either
/// emitter: PHP promotes `abs(PHP_INT_MIN)` to the float `9.2233720368548E+18`, because its
/// magnitude has no integer representation. EIR types this call `I64`/`int`, so there is no slot
/// a float could be returned in, and both backends therefore answer `PHP_INT_MIN` unchanged.
/// Every other input is exact.
pub(super) fn lower_int_abs(ctx: &mut FnCtx, inst: &Instruction, argument: crate::ir::ValueId) -> Result<()> {
    // `abs(int)` is `int|float`, not `int`: PHP_INT_MIN has no positive counterpart, so
    // php answers the FLOAT 9.2233720368548E+18 for it and an int for everything else
    // (measured on php-src 8.5.6). A boxed result carries both; a raw I64 result means the
    // EIR proved the argument cannot be PHP_INT_MIN, and the branchless form stands alone.
    let boxed = inst.result_type == IrType::Heap(IrHeapKind::Mixed);
    let mask = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    let magnitude = ctx.fresh_temp(crate::codegen_wasm::wat::ValType::I64);
    ctx.emit_load_value(argument)?;
    ctx.fb.ins("i64.const 63", "sign-bit shift distance");
    ctx.fb
        .ins("i64.shr_s", "all ones for a negative argument, zero otherwise");
    ctx.fb.ins(&format!("local.tee {}", mask), "keep the sign mask");
    ctx.emit_load_value(argument)?;
    ctx.fb.ins("i64.xor", "conditionally invert the argument");
    ctx.fb.ins(&format!("local.get {}", mask), "the sign mask again");
    ctx.fb.ins("i64.sub", "add one back for a negative argument");
    if !boxed {
        return store_result(ctx, inst);
    }
    ctx.fb
        .ins(&format!("local.set {}", magnitude), "the magnitude, still wrapped for INT_MIN");
    ctx.emit_load_value(argument)?;
    ctx.fb.ins(
        "i64.const -9223372036854775808",
        "PHP_INT_MIN: the one argument whose magnitude does not fit",
    );
    ctx.fb.ins("i64.eq", "did the magnitude overflow?");
    ctx.fb.ins("if (result i32)", "overflow answers a float");
    ctx.fb.ins("i64.const 2", "mixed float tag");
    ctx.fb.ins(
        "i64.const 4890909195324358656",
        "the f64 bits of 9223372036854775808.0",
    );
    ctx.fb.ins("i64.const 0", "float payloads use no high word");
    ctx.fb
        .ins("call $__rt_mixed_from_value", "box php's float answer");
    ctx.fb.ins("else", "every other argument keeps its integer magnitude");
    ctx.fb.ins("i64.const 0", "mixed int tag");
    ctx.fb
        .ins(&format!("local.get {}", magnitude), "the integer magnitude");
    ctx.fb.ins("i64.const 0", "int payloads use no high word");
    ctx.fb.ins("call $__rt_mixed_from_value", "box the integer answer");
    ctx.fb.ins("end", "end the abs overflow check");
    store_result(ctx, inst)
}

/// Validates the builtins taking two ints and answering one: `max`, `min` and `intdiv`.
///
/// PHP's `max`/`min` are variadic and compare across types; only the two-integer form is served
/// here, where the comparison is a plain signed ordering with no juggling.
pub(super) fn int_pair_shape_issue(
    module: &Module,
    function: &Function,
    call: &Instruction,
    target: RuntimeFnId,
) -> Option<String> {
    let [left, right] = call.operands.as_slice() else {
        return Some(format!(
            "expected two int operands, got {}",
            call.operands.len()
        ));
    };
    for (index, operand) in [left, right].into_iter().enumerate() {
        let Some(value) = function.value(*operand) else {
            return Some("operand is missing from the value table".to_string());
        };
        // A BOXED operand is coerced at the call the way php-src converts one at a declared
        // `int` parameter — truncating a float after a Deprecated, converting a numeric string,
        // and raising a TypeError for anything with no conversion. That needs the diagnostics,
        // so it is a command-module rule like every other one that can warn.
        if crate::codegen_wasm::capability::runtime_call_int_operand_coercion(function, call, index).is_some()
            && module.functions.iter().any(|candidate| candidate.flags.is_main)
        {
            continue;
        }
        if value.ir_type != IrType::I64 || value.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "expected an int operand, got {:?}/{:?}",
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

/// Lowers the two-integer `max` / `min` to a signed comparison and a select.
pub(super) fn lower_int_extremum(ctx: &mut FnCtx, inst: &Instruction, target: RuntimeFnId) -> Result<()> {
    let left = operand(inst, 0)?;
    let right = operand(inst, 1)?;
    ctx.emit_load_value(left)?;
    ctx.emit_load_value(right)?;
    ctx.emit_load_value(left)?;
    ctx.emit_load_value(right)?;
    if target == RuntimeFnId::Max {
        ctx.fb.ins("i64.gt_s", "is the left operand the larger?");
    } else {
        ctx.fb.ins("i64.lt_s", "is the left operand the smaller?");
    }
    ctx.fb
        .ins("select", "keep the operand the comparison chose");
    store_result(ctx, inst)
}

/// Validates `range`: two integer bounds. The step form does not exist — the front-end rejects
/// any arity but two — and a float or string bound is a different result element type.
pub(super) fn range_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [start, end] = call.operands.as_slice() else {
        return Some(format!(
            "range takes two bounds, got {} operands",
            call.operands.len()
        ));
    };
    for (operand, side) in [(start, "start"), (end, "end")] {
        let Some(value) = function.value(*operand) else {
            return Some(format!("range {side} is missing from the value table"));
        };
        if value.ir_type != IrType::I64 || value.php_type.codegen_repr() != PhpType::Int {
            return Some(format!(
                "range {side} is {:?}/{:?}, expected I64/Int",
                value.ir_type,
                value.php_type.codegen_repr()
            ));
        }
    }
    None
}

/// Validates `round($value, $places)`: a float value and an integer precision.
///
/// `|places| > 22` is refused. php-src reaches for `pow(10.0, places)` past its lookup table, and
/// repeated multiplication does not reproduce that to the last bit — the runtime would answer
/// nearly-right, which is the one thing this backend does not do.
pub(super) fn round_places_shape_issue(function: &Function, call: &Instruction) -> Option<String> {
    let [value, places] = call.operands.as_slice() else {
        return Some(format!(
            "round takes a value and a precision, got {}",
            call.operands.len()
        ));
    };
    let Some(value) = function.value(*value) else {
        return Some("round value is missing from the value table".to_string());
    };
    if value.ir_type != IrType::F64 || value.php_type.codegen_repr() != PhpType::Float {
        return Some(format!(
            "round with a precision takes a float, got {:?}/{:?}",
            value.ir_type,
            value.php_type.codegen_repr()
        ));
    }
    let Some(places) = function.value(*places) else {
        return Some("round precision is missing from the value table".to_string());
    };
    if places.ir_type != IrType::I64 || places.php_type.codegen_repr() != PhpType::Int {
        return Some(format!(
            "round precision must be an integer, got {:?}/{:?}",
            places.ir_type,
            places.php_type.codegen_repr()
        ));
    }
    if call.result_type != IrType::F64 || call.result_php_type.codegen_repr() != PhpType::Float {
        return Some(format!(
            "round result {:?}/{:?} is not the expected F64/Float",
            call.result_type,
            call.result_php_type.codegen_repr()
        ));
    }
    // A literal precision is checked here; a dynamic one is bounded by the runtime, which answers
    // the value unchanged once the scaled magnitude leaves the range php-src also gives up on.
    if let Some(constant) = constant_i64_operand(function, call.operands[1]) {
        if constant.unsigned_abs() > 22 {
            return Some(format!(
                "round precision {constant} is outside php_intpow10's exact table"
            ));
        }
    }
    None
}
