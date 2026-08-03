//! Purpose:
//! Implements fail-closed strict PHP equality for exact WASM value shapes and
//! the length-delimited byte-string helper used by `===` and `!==`.
//!
//! Called from:
//! - `crate::codegen_wasm::inst::lower_instruction()` for strict comparisons.
//! - `crate::codegen_wasm::plan::plan_module()` to register the string runtime.
//! - `crate::codegen_wasm::capability` to share the admitted value families.
//!
//! Key details:
//! - Type identity is based on exact PHP/EIR metadata, never `codegen_repr()`;
//!   resources must not collapse into integers for strict comparison.
//! - Strings are compared by length and raw bytes, including embedded NUL and
//!   invalid UTF-8; operands remain borrowed.
//! - A runtime-tagged `Mixed` cell is comparable against a CONCRETE value: the
//!   cell's tag decides the type, and the concrete side is never an array, so
//!   this never needs PHP's deep element-wise array identity. Two Mixed cells
//!   are refused for exactly that reason.
//! - Unions, tagged scalars, containers, callables, resources, pointers, and
//!   packed values remain outside this deliberately narrow batch.

use super::context::{FnCtx, Result};
use super::inst::{operand, store_result};
use super::wat::WatModule;
use super::WasmError;
use crate::ir::{Instruction, IrHeapKind, IrType, Op, Ownership};
use crate::types::PhpType;

/// Exact value families whose PHP strict identity is implemented by WASM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StrictValueKind {
    Int,
    Bool,
    Null,
    Float,
    Str,
    Object,
    /// A runtime-tagged `Mixed` cell, comparable against a CONCRETE side only.
    ///
    /// A builtin whose PHP result is `int|false` — `strpos` most visibly — hands back one of
    /// these, and the whole point of the idiom `strpos($h, $n) === false` is that it separates a
    /// match at offset 0 from a miss. Answering it needs the cell's runtime tag, not its storage.
    MixedCell,
}

/// Returns the runtime Mixed tag a concrete strict kind boxes under.
///
/// These mirror `inst::lower_mixed_box` exactly; a divergence would make `===` compare a value
/// against the wrong tag and answer false where PHP answers true.
fn mixed_tag_for(kind: StrictValueKind) -> Option<i64> {
    Some(match kind {
        StrictValueKind::Int => 0,
        StrictValueKind::Str => 1,
        StrictValueKind::Float => 2,
        StrictValueKind::Bool => 3,
        StrictValueKind::Object => 6,
        StrictValueKind::Null => 8,
        StrictValueKind::MixedCell => return None,
    })
}

/// Returns whether a strict comparison of these two kinds is lowered.
///
/// A `Mixed` cell is comparable against any concrete kind, because a tag mismatch settles the
/// answer and the concrete side is never an array — so this never needs PHP's deep element-wise
/// array identity. Two Mixed cells are NOT admitted for exactly that reason: both could be
/// arrays, and answering that correctly is a different problem.
pub(super) fn strict_pair_is_supported(lhs: StrictValueKind, rhs: StrictValueKind) -> bool {
    !(lhs == StrictValueKind::MixedCell && rhs == StrictValueKind::MixedCell)
}

/// Classifies one exact EIR/PHP/ownership shape for strict comparison.
pub(super) fn classify_strict_value(
    ir_type: IrType,
    php_type: &PhpType,
    ownership: Ownership,
) -> Option<StrictValueKind> {
    match (ir_type, php_type, ownership) {
        (IrType::I64, PhpType::Int, Ownership::NonHeap) => Some(StrictValueKind::Int),
        (
            IrType::I64,
            PhpType::Bool | PhpType::False,
            Ownership::NonHeap,
        ) => Some(StrictValueKind::Bool),
        (IrType::I64, PhpType::Void, Ownership::NonHeap) => Some(StrictValueKind::Null),
        (IrType::F64, PhpType::Float, Ownership::NonHeap) => Some(StrictValueKind::Float),
        (
            IrType::Str,
            PhpType::Str,
            Ownership::Owned
            | Ownership::Borrowed
            | Ownership::MaybeOwned
            | Ownership::Persistent,
        ) => Some(StrictValueKind::Str),
        (
            IrType::Heap(IrHeapKind::Object),
            PhpType::Object(_),
            Ownership::Owned
            | Ownership::Borrowed
            | Ownership::MaybeOwned
            | Ownership::Persistent,
        ) => Some(StrictValueKind::Object),
        (
            IrType::Heap(IrHeapKind::Mixed),
            _,
            Ownership::Owned
            | Ownership::Borrowed
            | Ownership::MaybeOwned
            | Ownership::Persistent,
        ) => Some(StrictValueKind::MixedCell),
        _ => None,
    }
}

/// Lowers exact strict equality or inequality without PHP coercion.
pub(super) fn lower_strict_compare(ctx: &mut FnCtx, inst: &Instruction) -> Result<()> {
    let lhs = operand(inst, 0)?;
    let rhs = operand(inst, 1)?;
    let lhs_value = ctx
        .function
        .value(lhs)
        .ok_or_else(|| WasmError::Unsupported(format!("missing strict lhs {:?}", lhs)))?;
    let rhs_value = ctx
        .function
        .value(rhs)
        .ok_or_else(|| WasmError::Unsupported(format!("missing strict rhs {:?}", rhs)))?;
    let lhs_kind = classify_strict_value(
        lhs_value.ir_type,
        &lhs_value.php_type,
        lhs_value.ownership,
    )
    .ok_or_else(|| {
        WasmError::Unsupported(format!(
            "strict lhs shape {:?}/{:?}/{:?}",
            lhs_value.ir_type, lhs_value.php_type, lhs_value.ownership
        ))
    })?;
    let rhs_kind = classify_strict_value(
        rhs_value.ir_type,
        &rhs_value.php_type,
        rhs_value.ownership,
    )
    .ok_or_else(|| {
        WasmError::Unsupported(format!(
            "strict rhs shape {:?}/{:?}/{:?}",
            rhs_value.ir_type, rhs_value.php_type, rhs_value.ownership
        ))
    })?;
    let negated = inst.op == Op::StrictNotEq;

    // A runtime-tagged cell against a concrete value: the cell's TAG decides the type, so this
    // cannot take the "different kinds are unequal" shortcut below — a Mixed holding an int is
    // strictly equal to an int.
    if lhs_kind == StrictValueKind::MixedCell || rhs_kind == StrictValueKind::MixedCell {
        if lhs_kind == rhs_kind {
            return Err(WasmError::Unsupported(
                "strict comparison of two Mixed cells".to_string(),
            ));
        }
        let (cell, concrete, concrete_kind) = if lhs_kind == StrictValueKind::MixedCell {
            (lhs, rhs, rhs_kind)
        } else {
            (rhs, lhs, lhs_kind)
        };
        let tag = mixed_tag_for(concrete_kind).ok_or_else(|| {
            WasmError::Unsupported("strict comparison against an untagged shape".to_string())
        })?;
        ctx.emit_load_value(cell)?;
        ctx.fb
            .ins(&format!("i64.const {tag}"), "the concrete side's runtime tag");
        match concrete_kind {
            StrictValueKind::Null => {
                ctx.fb.ins("i64.const 0", "null carries no payload");
                ctx.fb.ins("i64.const 0", "hi unused");
            }
            StrictValueKind::Float => {
                ctx.emit_load_value(concrete)?;
                ctx.fb
                    .ins("i64.reinterpret_f64", "float bits -> lo");
                ctx.fb.ins("i64.const 0", "hi unused");
            }
            StrictValueKind::Str => {
                ctx.emit_load_value(concrete)?;
                // The loaded string is (ptr, len); the pointer has to widen under the length.
                ctx.fb.ins("call $__rt_str_pair_to_mixed_args", "(ptr,len) -> (lo,hi)");
            }
            StrictValueKind::Object => {
                ctx.emit_load_value(concrete)?;
                ctx.fb.ins("i64.extend_i32_u", "object pointer -> lo");
                ctx.fb.ins("i64.const 0", "hi unused");
            }
            _ => {
                ctx.emit_load_value(concrete)?;
                ctx.fb.ins("i64.const 0", "hi unused");
            }
        }
        ctx.fb.ins(
            "call $__rt_strict_mixed_scalar",
            "compare a tagged cell against a concrete value",
        );
        return finish_i32_boolean(ctx, inst, negated);
    }

    if lhs_kind != rhs_kind {
        ctx.fb.ins(
            if negated {
                "i64.const 1"
            } else {
                "i64.const 0"
            },
            "different exact PHP types compare strictly unequal",
        );
        return store_result(ctx, inst);
    }

    match lhs_kind {
        StrictValueKind::Int | StrictValueKind::Bool | StrictValueKind::Null => {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                if negated { "i64.ne" } else { "i64.eq" },
                "strict integer-backed equality",
            );
            finish_i32_boolean(ctx, inst, false)
        }
        StrictValueKind::Float => {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                if negated { "f64.ne" } else { "f64.eq" },
                "strict PHP float equality",
            );
            finish_i32_boolean(ctx, inst, false)
        }
        StrictValueKind::Str => {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                "call $__rt_strict_str_eq",
                "compare strict strings by length and bytes",
            );
            finish_i32_boolean(ctx, inst, negated)
        }
        StrictValueKind::Object => {
            ctx.emit_load_value(lhs)?;
            ctx.emit_load_value(rhs)?;
            ctx.fb.ins(
                if negated { "i32.ne" } else { "i32.eq" },
                "compare strict object identity",
            );
            finish_i32_boolean(ctx, inst, false)
        }
        // Handled above: a Mixed cell never reaches the same-kind path, because a pair of them
        // is refused and a mixed/concrete pair returns before this match.
        StrictValueKind::MixedCell => Err(WasmError::Unsupported(
            "strict comparison of two Mixed cells".to_string(),
        )),
    }
}

/// Converts an i32 comparison result into the EIR i64 boolean representation.
fn finish_i32_boolean(ctx: &mut FnCtx, inst: &Instruction, negate: bool) -> Result<()> {
    if negate {
        ctx.fb.ins("i32.eqz", "invert strict string equality");
    }
    ctx.fb.ins("i64.extend_i32_u", "strict bool i32 -> i64");
    store_result(ctx, inst)
}

const RT_STRICT_STR_EQ: &str = r#"(func $__rt_strict_str_eq
  (param $ap i32) (param $al i64) (param $bp i32) (param $bl i64) (result i32)
  (local $i i64)
  (if (i64.ne (local.get $al) (local.get $bl))
    (then (return (i32.const 0))))
  (loop $scan
    (if (i64.ge_u (local.get $i) (local.get $al))
      (then (return (i32.const 1))))
    (if
      (i32.ne
        (i32.load8_u
          (i32.add (local.get $ap) (i32.wrap_i64 (local.get $i))))
        (i32.load8_u
          (i32.add (local.get $bp) (i32.wrap_i64 (local.get $i)))))
      (then (return (i32.const 0))))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))
    (br $scan))
  (i32.const 0))
"#;

/// Registers the borrowed, length-delimited strict string comparison runtime.
/// `__rt_str_pair_to_mixed_args`: reshapes a loaded `(ptr, len)` string into `(lo, hi)`.
///
/// A string reaches the stack as an i32 pointer under an i64 length, which is the wrong order
/// and the wrong width for the `(lo, hi)` pair the Mixed comparison takes. Doing the swap in a
/// helper keeps the caller from needing two scratch locals at every comparison site.
const RT_STR_PAIR_TO_MIXED_ARGS: &str = r#"(func $__rt_str_pair_to_mixed_args (param $ptr i32) (param $len i64) (result i64) (result i64)
  (i64.extend_i32_u (local.get $ptr))                             ;; lo: the pointer, widened
  (local.get $len))                                               ;; hi: the length
"#;

/// `__rt_strict_mixed_scalar`: PHP's `===` between a tagged Mixed cell and a concrete value.
///
/// The cell's runtime TAG decides the type, so a tag mismatch is the answer — which is also what
/// makes this correct without implementing PHP's deep array identity: the concrete side is never
/// an array, so a cell holding one simply mismatches. Floats compare as FLOATS rather than as
/// bits, because `NAN === NAN` is false and `0.0 === -0.0` is true, and neither survives a bit
/// comparison. Objects compare by pointer identity, which is what PHP's `===` means for them.
///
/// Null compares on its TAG alone. PHP has exactly one null, and this backend does not agree with
/// itself on a payload for it: an unboxed null literal carries the `0x7fff_ffff_ffff_fffe`
/// sentinel while an absent cell unboxes to zero. Comparing payloads there answered false for
/// `$mixed === null` depending only on how the null had arrived.
const RT_STRICT_MIXED_SCALAR: &str = r#"(func $__rt_strict_mixed_scalar (param $cell i32) (param $want i64) (param $lo i64) (param $hi i64) (result i32)
  (local $tag i64)                                                ;; the cell's runtime tag
  (local $clo i64)                                                ;; the cell's payload
  (local $chi i64)                                                ;; the cell's second word
  ;; Walk nested (tag 7) cells inline rather than calling `__rt_mixed_unbox`: this helper ships
  ;; with the strict runtime, which a module carries without necessarily carrying the Mixed one.
  (if (i32.eqz (local.get $cell))
    (then (local.set $tag (i64.const 8)))                         ;; an absent cell is PHP's null
    (else
      (block $found (loop $walk
        (local.set $tag (i64.load (local.get $cell)))
        (br_if $found (i64.ne (local.get $tag) (i64.const 7)))    ;; not a forwarding cell
        (local.set $cell (i32.wrap_i64 (i64.load (i32.add (local.get $cell) (i32.const 8)))))
        (if (i32.eqz (local.get $cell))
          (then (local.set $tag (i64.const 8)) (br $found)))      ;; a nested null is still null
        (br $walk)))
      (if (i32.eqz (local.get $cell))
        (then (local.set $tag (i64.const 8)))
        (else
          (local.set $clo (i64.load (i32.add (local.get $cell) (i32.const 8))))
          (local.set $chi (i64.load (i32.add (local.get $cell) (i32.const 16))))))))
  (if (i64.ne (local.get $tag) (local.get $want))
    (then (return (i32.const 0))))                                ;; a different type is never identical
  (if (i64.eq (local.get $tag) (i64.const 8))                     ;; null: the tag IS the whole value
    (then (return (i32.const 1))))                                ;; PHP has exactly one null
  (if (i64.eq (local.get $tag) (i64.const 1))                     ;; string: compare the bytes
    (then (return (call $__rt_strict_str_eq
      (i32.wrap_i64 (local.get $clo)) (local.get $chi)
      (i32.wrap_i64 (local.get $lo)) (local.get $hi)))))
  (if (i64.eq (local.get $tag) (i64.const 2))                     ;; float: compare as a float
    (then (return (f64.eq (f64.reinterpret_i64 (local.get $clo))
                          (f64.reinterpret_i64 (local.get $lo))))))
  (i64.eq (local.get $clo) (local.get $lo)))                      ;; int, bool, object identity
"#;

pub(super) fn emit_strict_runtime(module: &mut WatModule) {
    module.add_raw_func(RT_STRICT_STR_EQ);
    module.add_raw_func(RT_STR_PAIR_TO_MIXED_ARGS);
    module.add_raw_func(RT_STRICT_MIXED_SCALAR);
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! WAT validation and Wasmer regressions for strict binary string equality.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Raw data segments exercise empty, prefix, embedded-NUL, and invalid
    //!   UTF-8 bytes without passing through Rust or PHP string decoding.

    use super::*;
    use crate::codegen::Emit;
    use crate::codegen_wasm::wat::DataSegment;
    use crate::ir::{Builder, Function, Module, Terminator};
    use crate::codegen_support::platform::Target;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

    /// Returns whether the Wasmer CLI is available for runtime execution.
    fn wasmer_available() -> bool {
        std::process::Command::new("wasmer")
            .arg("--version")
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// Verifies the Mixed tags this module compares against match what boxing writes.
    ///
    /// `===` against a runtime-tagged cell decides the TYPE from the cell's tag, so these
    /// constants have to be the same ones `inst::lower_mixed_box` stamps. A drift would make a
    /// comparison answer false for a value PHP calls identical, silently and only at runtime.
    #[test]
    fn concrete_strict_kinds_map_to_the_tags_boxing_writes() {
        for (kind, tag) in [
            (StrictValueKind::Int, 0),
            (StrictValueKind::Str, 1),
            (StrictValueKind::Float, 2),
            (StrictValueKind::Bool, 3),
            (StrictValueKind::Object, 6),
            (StrictValueKind::Null, 8),
        ] {
            assert_eq!(mixed_tag_for(kind), Some(tag), "{kind:?} boxes under tag {tag}");
        }
        // A cell has no tag of its own to compare against: it IS the tagged side.
        assert_eq!(mixed_tag_for(StrictValueKind::MixedCell), None);

        let boxing = include_str!("inst.rs");
        for (tag, comment) in [
            (2, "mixed tag (float)"),
            (1, "mixed tag (string)"),
        ] {
            assert!(
                boxing.contains(&format!("i64.const {tag}\", \"{comment}")),
                "boxing must still stamp tag {tag} for {comment}"
            );
        }
    }

    /// Verifies a pair of Mixed cells is refused while a Mixed/concrete pair is admitted.
    ///
    /// Two cells could both hold arrays, whose PHP identity is a deep element-wise comparison
    /// this does not implement. One cell against a concrete value never can, because the
    /// concrete side is never an array — a tag mismatch settles it.
    #[test]
    fn two_mixed_cells_are_refused_but_a_concrete_side_is_not() {
        assert!(!strict_pair_is_supported(
            StrictValueKind::MixedCell,
            StrictValueKind::MixedCell
        ));
        for concrete in [
            StrictValueKind::Int,
            StrictValueKind::Bool,
            StrictValueKind::Null,
            StrictValueKind::Float,
            StrictValueKind::Str,
            StrictValueKind::Object,
        ] {
            assert!(strict_pair_is_supported(StrictValueKind::MixedCell, concrete));
            assert!(strict_pair_is_supported(concrete, StrictValueKind::MixedCell));
        }
    }

    /// Verifies the tagged comparison treats floats and null the way PHP does, not the way bits do.
    ///
    /// `NAN === NAN` is false and `0.0 === -0.0` is true, so a float has to be compared as a
    /// float; comparing the payload words would get both backwards. Null compares on its tag
    /// alone because this backend represents an unboxed null literal with a sentinel and an
    /// absent cell with zero, so a payload comparison would depend on how the null arrived.
    #[test]
    fn tagged_comparison_uses_float_semantics_and_a_tag_only_null() {
        assert!(
            RT_STRICT_MIXED_SCALAR.contains("f64.eq (f64.reinterpret_i64 (local.get $clo))"),
            "a float must compare as a float, not as its bits"
        );
        assert!(
            RT_STRICT_MIXED_SCALAR
                .contains("(if (i64.eq (local.get $tag) (i64.const 8))                     ;; null: the tag IS the whole value"),
            "null compares on its tag alone"
        );
        // Self-contained: this ships with the strict runtime, which a module carries without
        // necessarily carrying the Mixed one. The name may appear in a comment explaining why
        // the walk is inline; what must not appear is a CALL to it.
        assert!(!RT_STRICT_MIXED_SCALAR.contains("call $__rt_mixed_unbox"));
    }

    /// Builds, validates, and invokes the strict-string runtime driver.
    fn run_strict_string_driver() -> Option<String> {
        let mut module = WatModule::new();
        module.set_memory(1, Some("memory"));
        emit_strict_runtime(&mut module);
        for (offset, bytes) in [
            (32, vec![b'a', 0, b'b', 0xff]),
            (64, vec![b'a', 0, b'b', 0xff]),
            (96, vec![b'a', 0, b'b']),
            (128, vec![b'a', 0, b'c', 0xff]),
        ] {
            module.add_data(DataSegment { offset, bytes });
        }
        module.add_raw_func(
            r#"(func $t (export "t") (result i32)
  (call $__rt_strict_str_eq (i32.const 0) (i64.const 0) (i32.const 0) (i64.const 0))
  (i32.const 1000)
  i32.mul
  (call $__rt_strict_str_eq (i32.const 32) (i64.const 4) (i32.const 64) (i64.const 4))
  (i32.const 100)
  i32.mul
  i32.add
  (call $__rt_strict_str_eq (i32.const 32) (i64.const 4) (i32.const 96) (i64.const 3))
  i32.eqz
  (i32.const 10)
  i32.mul
  i32.add
  (call $__rt_strict_str_eq (i32.const 32) (i64.const 4) (i32.const 128) (i64.const 4))
  i32.eqz
  i32.add)
"#,
        );
        let wat = module.render();
        let bytes =
            ::wat::parse_str(&wat).unwrap_or_else(|error| panic!("WAT failed: {error}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|error| panic!("WASM validation failed: {error}\n{wat}"));
        if !wasmer_available() {
            return None;
        }

        let sequence = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "elephc_wasm_strict_{}_{}",
            std::process::id(),
            sequence
        ));
        std::fs::create_dir_all(&dir).expect("strict temp directory");
        let path = dir.join("strict.wasm");
        std::fs::write(&path, bytes).expect("strict wasm artifact");
        let output = std::process::Command::new("wasmer")
            .arg("run")
            .arg("--invoke")
            .arg("t")
            .arg(&path)
            .output()
            .expect("invoke strict wasm driver");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            output.status.success(),
            "strict driver failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Verifies both strict opcodes pass capability planning, assemble, validate,
    /// and execute with the EIR i64 boolean result convention.
    #[test]
    fn strict_scalar_equality_opcodes_lower_and_run() {
        let mut module = Module::new(Target::wasm());
        let delimiter = module.data.intern_string(",");
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);

            for (op, lhs, rhs) in [
                (Op::StrictEq, 7, 7),
                (Op::StrictNotEq, 7, 8),
                (Op::StrictEq, 7, 8),
                (Op::StrictNotEq, 7, 7),
            ] {
                let lhs = builder.emit_const_i64(lhs);
                let rhs = builder.emit_const_i64(rhs);
                let result = builder
                    .emit(
                        op,
                        vec![lhs, rhs],
                        None,
                        IrType::I64,
                        PhpType::Bool,
                        Ownership::NonHeap,
                    )
                    .expect("strict scalar result");
                let _ = builder.emit(
                    Op::EchoValue,
                    vec![result],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
                let delimiter = builder.emit_const_str(delimiter);
                let _ = builder.emit(
                    Op::EchoValue,
                    vec![delimiter],
                    None,
                    IrType::Void,
                    PhpType::Void,
                    Ownership::NonHeap,
                );
            }
            builder.terminate(Terminator::Return { value: None });
        }
        module.add_function(function);

        let wat = crate::codegen_wasm::generate(&module, Emit::Executable)
            .expect("strict scalar module");
        let bytes =
            ::wat::parse_str(&wat).unwrap_or_else(|error| panic!("WAT failed: {error}\n{wat}"));
        wasmparser::validate(&bytes)
            .unwrap_or_else(|error| panic!("WASM validation failed: {error}\n{wat}"));
        if !wasmer_available() {
            return;
        }

        let sequence = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "elephc_wasm_strict_eir_{}_{}",
            std::process::id(),
            sequence
        ));
        std::fs::create_dir_all(&dir).expect("strict EIR temp directory");
        let path = dir.join("strict-eir.wasm");
        std::fs::write(&path, bytes).expect("strict EIR wasm artifact");
        let output = std::process::Command::new("wasmer")
            .arg("run")
            .arg(&path)
            .output()
            .expect("run strict EIR module");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            output.status.success(),
            "strict EIR module failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stdout), "1,1,,,");
        assert!(output.stderr.is_empty());
    }

    /// Verifies strict strings compare by exact length and raw byte content.
    #[test]
    fn strict_binary_string_equality_is_length_delimited() {
        if let Some(output) = run_strict_string_driver() {
            assert_eq!(output, "1111");
        }
    }
}
