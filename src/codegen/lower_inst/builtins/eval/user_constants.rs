//! Purpose:
//! Converts AOT user-declared global constant values into eval bridge metadata.
//!
//! Called from:
//! - `super::context_registration` while seeding a freshly created eval context.
//!
//! Key details:
//! - The accepted expression surface is the one
//!   `crate::codegen::lower_inst::core_builtins::constants::emit_boxed_constant_value`
//!   accepts, with the same keys, duplicates, and nested element types, bounded
//!   by `MAX_NATIVE_DEFAULT_CONSTANT_DEPTH`. The native materializer has no such
//!   depth cap, so a constant nested deeper than the limit is seeded natively and
//!   skipped here; every constant eval can read is one native
//!   `get_defined_constants()` can materialize, but not the reverse.
//! - Nested elements are materialized by AOT with a plain `int` declared type, so a
//!   nested value never carries the resource tag; only a top-level scalar can.
//! - Array metadata reuses the native callable array-default value/key encoding so
//!   there is no second compound codec on either side of the bridge.

use super::*;

/// One AOT user-declared global constant shape the eval bridge can be seeded with.
pub(super) enum EvalNativeUserConstantValue {
    /// A scalar constant encoded with the shared global-constant kind/payload ABI.
    Scalar {
        kind: i64,
        word: i64,
        string_value: Option<String>,
    },
    /// A recursively nested array constant with explicit PHP-normalized keys.
    Array(Vec<EvalNativeCallableArrayDefaultElement>),
}

/// Converts one prescanned user constant into eval bridge metadata.
///
/// Returns `None` for any value AOT itself cannot materialize (unfolded expressions,
/// object constants, float array keys) and for an array nested deeper than
/// `MAX_NATIVE_DEFAULT_CONSTANT_DEPTH`, which the native materializer accepts but this
/// metadata encoding does not. Such a constant stays invisible to eval rather than
/// failing the compile, because the registration runs for every `eval()` call site while
/// `get_defined_constants()` is the only native surface that rejects the value.
pub(super) fn eval_native_user_constant_value(
    value: &ExprKind,
    ty: &PhpType,
) -> Option<EvalNativeUserConstantValue> {
    if let Some(elements) = eval_native_user_constant_array(value, 0) {
        return Some(EvalNativeUserConstantValue::Array(elements));
    }
    let (kind, word, string_value) = eval_native_global_constant_abi_value(value, ty).ok()?;
    Some(EvalNativeUserConstantValue::Scalar {
        kind,
        word,
        string_value,
    })
}

/// Converts one array constant literal into ordered element metadata.
///
/// Indexed literals receive the same explicit `0..n` keys AOT emits, and associative
/// literals keep their declared key order, so duplicate keys collapse identically on both
/// sides: the last occurrence wins at the position of the first one.
fn eval_native_user_constant_array(
    value: &ExprKind,
    depth: usize,
) -> Option<Vec<EvalNativeCallableArrayDefaultElement>> {
    if depth > MAX_NATIVE_DEFAULT_CONSTANT_DEPTH {
        return None;
    }
    match value {
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                eval_native_user_constant_element(
                    &ExprKind::IntLiteral(index as i64),
                    &item.kind,
                    depth,
                )
            })
            .collect(),
        ExprKind::ArrayLiteralAssoc(items) => items
            .iter()
            .map(|(key, item)| eval_native_user_constant_element(&key.kind, &item.kind, depth))
            .collect(),
        _ => None,
    }
}

/// Converts one keyed array element into eval bridge metadata.
fn eval_native_user_constant_element(
    key: &ExprKind,
    value: &ExprKind,
    depth: usize,
) -> Option<EvalNativeCallableArrayDefaultElement> {
    Some(EvalNativeCallableArrayDefaultElement {
        key: Some(eval_native_user_constant_key(key)?),
        default: eval_native_user_constant_element_value(value, depth + 1)?,
    })
}

/// Converts one nested constant element value into eval bridge metadata.
///
/// This mirrors AOT's nested materialization, which always passes `int` as the declared
/// type, so an integer element is an integer and never a resource handle.
fn eval_native_user_constant_element_value(
    value: &ExprKind,
    depth: usize,
) -> Option<EvalNativeCallableDefault> {
    if depth > MAX_NATIVE_DEFAULT_CONSTANT_DEPTH {
        return None;
    }
    match value {
        ExprKind::Null => Some(EvalNativeCallableDefault::Scalar {
            kind: NATIVE_DEFAULT_NULL,
            payload: 0,
        }),
        ExprKind::BoolLiteral(value) => Some(eval_native_bool_default(*value)),
        ExprKind::IntLiteral(value) => Some(eval_native_int_default(*value)),
        ExprKind::FloatLiteral(value) => Some(eval_native_float_default(*value)),
        ExprKind::StringLiteral(value) => {
            Some(EvalNativeCallableDefault::String(value.clone()))
        }
        // AOT negates with wrapping semantics, so `-PHP_INT_MIN` stays `PHP_INT_MIN`
        // here too instead of dropping the whole constant.
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(eval_native_int_default(value.wrapping_neg())),
            ExprKind::FloatLiteral(value) => Some(eval_native_float_default(-value)),
            _ => None,
        },
        ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_) => {
            eval_native_user_constant_array(value, depth).map(EvalNativeCallableDefault::Array)
        }
        _ => None,
    }
}

/// Converts one constant array key into eval bridge metadata.
///
/// Mirrors AOT's `emit_constant_array_key`: integer-like keys become integer keys, `null`
/// becomes the empty string key, and string keys go through PHP's numeric-string collapsing
/// exactly as the `__rt_hash_normalize_key` runtime helper does. Float keys are rejected
/// because AOT rejects them too.
fn eval_native_user_constant_key(key: &ExprKind) -> Option<EvalNativeCallableArrayDefaultKey> {
    match key {
        ExprKind::IntLiteral(value) => Some(EvalNativeCallableArrayDefaultKey::Int(*value)),
        ExprKind::BoolLiteral(value) => {
            Some(EvalNativeCallableArrayDefaultKey::Int(i64::from(*value)))
        }
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(EvalNativeCallableArrayDefaultKey::Int(
                value.wrapping_neg(),
            )),
            _ => None,
        },
        ExprKind::StringLiteral(value) => eval_native_string_array_default_key(value),
        ExprKind::Null => Some(EvalNativeCallableArrayDefaultKey::String(String::new())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::codegen::shared_helper::helper_function;
    use crate::codegen::shared_state::SharedCodegenState;
    use crate::codegen::{data_section::DataSection, emit::Emitter, frame};

    const SUPPORTED_TARGETS: [&str; 5] = [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ];

    /// Builds a module with one seeded user scalar, one seeded user array, and one Core name.
    ///
    /// `PHP_INT_SIZE` is present in the prescanned constant inventory but absent from
    /// `user_defined_constants`, which is exactly how a predefined constant reaches codegen,
    /// so it must not produce a user registration call.
    fn user_constant_module(target: Target) -> Module {
        let mut module = Module::new(target);
        module.global_constants.insert(
            "USER_TEXT".to_string(),
            (ExprKind::StringLiteral("seeded".to_string()), PhpType::Str),
        );
        module.global_constants.insert(
            "USER_LIST".to_string(),
            (
                ExprKind::ArrayLiteral(vec![Expr::int_lit(1), Expr::int_lit(2)]),
                PhpType::Int,
            ),
        );
        module.global_constants.insert(
            "PHP_INT_SIZE".to_string(),
            (ExprKind::IntLiteral(8), PhpType::Int),
        );
        module.user_defined_constants = vec!["USER_LIST".to_string(), "USER_TEXT".to_string()];
        module
    }

    /// Emits the user-constant seeding sequence for one target and returns its assembly.
    fn user_constant_registration_asm(target: Target) -> String {
        let module = user_constant_module(target);
        let function = helper_function("user_constant_registration_probe", PhpType::Void);
        let layout = frame::layout_for_function(&function, target, false, false, false);
        let mut emitter = Emitter::new(target);
        let mut data = DataSection::new();
        let mut shared = SharedCodegenState::default();
        {
            let mut ctx = FunctionContext::new(
                &module,
                &function,
                &mut emitter,
                &mut data,
                &mut shared,
                layout,
                false,
                false,
                false,
                None,
            );
            register_eval_native_user_constants(&mut ctx, EVAL_CONTEXT_HANDLE_OFFSET);
        }
        emitter.output()
    }

    /// Returns the encoded array-spec byte length the array registration must pass as its length.
    fn user_list_spec_len() -> usize {
        let value = ExprKind::ArrayLiteral(vec![Expr::int_lit(1), Expr::int_lit(2)]);
        match eval_native_user_constant_value(&value, &PhpType::Int) {
            Some(EvalNativeUserConstantValue::Array(elements)) => {
                encode_eval_native_array_default_elements(&elements).len()
            }
            _ => panic!("an indexed literal constant must encode as array metadata"),
        }
    }

    /// Counts assembly lines that are exactly the given instruction, ignoring indentation.
    ///
    /// Exact line equality matters here: the array registration symbol has the scalar
    /// registration symbol as a prefix, so a `contains` check cannot tell them apart.
    fn exact_instruction_count(asm: &str, instruction: &str) -> usize {
        asm.lines().filter(|line| line.trim() == instruction).count()
    }

    /// Returns the zero-based line index of the first assembly line equal to `instruction`.
    fn instruction_line(asm: &str, instruction: &str) -> Option<usize> {
        asm.lines().position(|line| line.trim() == instruction)
    }

    /// Every supported target emits both new registration symbols with target-correct ABI args.
    #[test]
    fn user_constant_registration_emits_target_aware_calls_on_every_target() {
        for name in SUPPORTED_TARGETS {
            let target = Target::parse(name).unwrap();
            let asm = user_constant_registration_asm(target);
            let call = match target.arch {
                Arch::AArch64 => "bl",
                Arch::X86_64 => "call",
            };
            let scalar_call = format!(
                "{call} {}",
                target.extern_symbol("__elephc_eval_register_native_user_constant")
            );
            let array_call = format!(
                "{call} {}",
                target.extern_symbol("__elephc_eval_register_native_user_constant_array")
            );
            let core_call = format!(
                "{call} {}",
                target.extern_symbol("__elephc_eval_register_native_global_constant")
            );

            // Exactly one call each: the Core-only `PHP_INT_SIZE` produced neither.
            assert_eq!(exact_instruction_count(&asm, &scalar_call), 1, "{name}:\n{asm}");
            assert_eq!(exact_instruction_count(&asm, &array_call), 1, "{name}:\n{asm}");
            assert_eq!(exact_instruction_count(&asm, &core_call), 0, "{name}:\n{asm}");

            // Seeding order is the sorted constant name order, so `USER_LIST` (the array)
            // is registered before `USER_TEXT` (the scalar).
            let array_at = instruction_line(&asm, &array_call)
                .unwrap_or_else(|| panic!("{name}: missing array call\n{asm}"));
            let scalar_at = instruction_line(&asm, &scalar_call)
                .unwrap_or_else(|| panic!("{name}: missing scalar call\n{asm}"));
            assert!(array_at < scalar_at, "{name}:\n{asm}");
        }
    }

    /// Registration arguments land in the integer argument registers each target's ABI defines.
    #[test]
    fn user_constant_registration_uses_each_target_argument_registers() {
        let spec_len = user_list_spec_len();
        for name in SUPPORTED_TARGETS {
            let target = Target::parse(name).unwrap();
            let asm = user_constant_registration_asm(target);
            // Both seeded names are nine bytes long, so the shared name-length argument is
            // the same immediate for the scalar and the array registration.
            let expected = match target.arch {
                Arch::AArch64 => vec![
                    "mov x2, #9".to_string(),
                    "mov x3, #4".to_string(),
                    "mov x5, #6".to_string(),
                    format!("mov x4, #{spec_len}"),
                ],
                Arch::X86_64 => vec![
                    "mov rdx, 9".to_string(),
                    "mov rcx, 4".to_string(),
                    "mov r9, 6".to_string(),
                    format!("mov r8, {spec_len}"),
                ],
            };
            for instruction in &expected {
                assert!(
                    exact_instruction_count(&asm, instruction) >= 1,
                    "{name}: missing {instruction}\n{asm}"
                );
            }
            // The name length is loaded once per registration call.
            assert_eq!(exact_instruction_count(&asm, &expected[0]), 2, "{name}:\n{asm}");
        }
    }
}
