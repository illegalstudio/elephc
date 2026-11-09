//! Purpose:
//! Converts supported literal property defaults into backend-native values.
//! Keeps EIR object and static-property initialization on the same subset.
//!
//! Called from:
//! - `crate::codegen::block_emit` static-property initialization.
//! - `crate::codegen::lower_inst::objects` object allocation.
//!
//! Key details:
//! - This is intentionally narrower than full PHP expression lowering: only
//!   scalar, string, null, indexed-array literals, empty object-typed indexed
//!   arrays, and associative-array literals (empty, positional, or with constant
//!   integer/string keys) land here. Array elements and associative values are
//!   themselves scalar/string/null or nested array literals, in either spelling
//!   and to any depth; each nested container is allocated with, and owned by, the
//!   container enclosing it.
//!   Constant keys keep the ordinary PHP key normalization rules.
//! - The declared PHP type selects the storage shape, and slot-shape arms must precede the
//!   generic `Mixed`/`Union(_)` boxing arms. A null-capable int slot (`?int` under
//!   `NullRepr::Tagged`) is an inline two-word `{payload, tag}` TaggedScalar, so it takes
//!   `TaggedInt`/`TaggedNull` and never a boxed Mixed pointer.
//! - Every string (value or key) is emitted through `string_bytes::literal_bytes`, never
//!   `str::as_bytes`: a byte above 0x7F travels through the AST in its private-use marker form,
//!   and its UTF-8 spelling is three different bytes. The intl-normalizer polyfill keys a static
//!   property with `"\xC0"`..`"\xF0"`; emitted as markers, no UTF-8 lead byte found its entry and
//!   its decompose loop advanced by nothing, forever.

use crate::codegen::platform::Arch;
use crate::codegen::{
    abi, emit_box_current_value_as_mixed, emit_release_pushed_refcounted_temp_after_array_push,
    runtime_value_tag,
};
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

use super::context::FunctionContext;
use super::{CodegenIrError, Result};

#[cfg(test)]
mod tests;

/// Literal default value that the EIR backend can write directly.
#[derive(Clone)]
pub(crate) enum LiteralDefaultValue {
    /// Some default of the class has no literal form here (an array of enum cases, say): after
    /// the literal ones are written, the class's `_class_propinit_<id>` thunk runs and assigns
    /// every default the way compiled PHP would. Always the last entry of its list.
    InitThunk {
        class_id: u64,
    },
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Null,
    NullSentinel,
    TaggedNull,
    TaggedInt(i64),
    BoxedNull,
    BoxedInt(i64),
    BoxedBool(bool),
    BoxedFloat(f64),
    BoxedStr(String),
    Array {
        elem_type: PhpType,
        elements: Vec<LiteralArrayElement>,
    },
    AssocArray {
        value_type: PhpType,
        entries: Vec<LiteralAssocEntry>,
    },
    EmptyAssocArray {
        value_type: PhpType,
    },
    /// An indexed-array literal stored into a `mixed`/union slot, boxed into a Mixed cell.
    ///
    /// `class T { public mixed $m = []; }` had no default form at all and was refused outright,
    /// on the STATIC path as much as the dynamic one — which is what made `ReflectionClass`,
    /// whose `$__constants` is exactly this shape, unallocatable by the AOT `new $c` path.
    BoxedArray {
        elem_type: PhpType,
        elements: Vec<LiteralArrayElement>,
    },
    /// An ASSOCIATIVE array literal stored into a `mixed`/union slot, boxed into a Mixed cell.
    ///
    /// The `BoxedArray` sibling covers the positional spelling; this covers the keyed one.
    /// `class C { public ?array $x = ["k" => 1]; }` had no default form and was refused outright
    /// with `object_new for default value of property $x with PHP type Union([Array(Mixed),
    /// Void])` -- the same message the positional spelling used to produce (issue #688).
    /// The fresh hash owner transfers into the cell rather than being retained by it.
    BoxedAssocArray {
        value_type: PhpType,
        entries: Vec<LiteralAssocEntry>,
    },
    /// An enum case (`public $s = Status::On;`, `public Level $l = Level::Low;`): the case's
    /// singleton, read when the default is written. The slot takes its own reference; `boxed`
    /// wraps it for a `mixed`/union slot.
    ///
    /// Recognized by `enum_case_literal_default`, which resolves the receiver against the
    /// module (so `self::`/aliases work and a non-enum `Foo::BAR` is left to report unsupported
    /// rather than loading a symbol that does not exist).
    EnumCase {
        enum_name: String,
        case_name: String,
        boxed: bool,
    },
}

/// Returns the enum-case default form for a default naming an enum case, when the slot can hold
/// the case object: the enum's own type, or `mixed`/a union.
pub(crate) fn enum_case_literal_default(
    module: &crate::ir::Module,
    class_name: &str,
    php_type: &PhpType,
    expr: &ExprKind,
) -> Option<LiteralDefaultValue> {
    let (enum_name, case_name) =
        crate::codegen::eval_class_constant_helpers::resolve_class_like_enum_case(
            module, class_name, expr,
        )?;
    let boxed = match php_type.codegen_repr() {
        PhpType::Object(_) => false,
        PhpType::Mixed | PhpType::Union(_) => true,
        _ => return None,
    };
    Some(LiteralDefaultValue::EnumCase {
        enum_name,
        case_name,
        boxed,
    })
}

/// Literal indexed-array element that can be materialized without evaluating code.
///
/// The two container forms make this recursive, which is what lets a default nest:
/// `public array $x = [[1], [2]];` is an outer literal whose elements are themselves literals.
/// Each container element allocates its own storage at emit time and hands that storage to the
/// enclosing container, so the whole tree is owned by its root and released exactly once with
/// it (issue #1052).
#[derive(Clone)]
pub(crate) enum LiteralArrayElement {
    Int(i64),
    Bool(bool),
    Float(f64),
    Str(String),
    Null,
    /// A nested indexed-array literal, materialized into its own array.
    Array {
        elem_type: PhpType,
        elements: Vec<LiteralArrayElement>,
    },
    /// A nested associative-array literal, materialized into its own hash.
    AssocArray {
        value_type: PhpType,
        entries: Vec<LiteralAssocEntry>,
    },
}

impl LiteralArrayElement {
    /// Returns true for the forms that materialize into their own freshly allocated container.
    ///
    /// Such a value arrives owned, so the enclosing container must TRANSFER that reference
    /// rather than add one; a scalar element has no reference to transfer.
    fn is_owned_container(&self) -> bool {
        matches!(
            self,
            LiteralArrayElement::Array { .. } | LiteralArrayElement::AssocArray { .. }
        )
    }
}

/// Literal associative-array key that can be materialized without evaluating code. Positional
/// literals stored into hash slots use integer keys; `ArrayLiteralAssoc` keys may be integer or
/// string. String keys are normalized at emit time so PHP numeric-string keys become integer keys.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum LiteralArrayKey {
    Int(i64),
    Str(String),
}

/// One literal associative-array entry: a constant key paired with a constant value, ready to be
/// inserted into a freshly allocated hash without evaluating any runtime expression.
#[derive(Clone)]
pub(crate) struct LiteralAssocEntry {
    key: LiteralArrayKey,
    value: LiteralArrayElement,
}

/// Replaces global-constant references throughout a literal default with registered values.
///
/// Class constants have already been resolved by the frontend. This pass handles module global
/// constants, including nested array keys and values, and leaves cycles unresolved so the caller
/// reports the normal unsupported-default diagnostic.
pub(crate) fn resolve_literal_default_global_constants(
    expr: &ExprKind,
    global_constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
) -> ExprKind {
    /// Recursively resolves constant references inside one literal expression.
    fn resolve(
        expr: &ExprKind,
        global_constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
        visiting: &mut crate::fast_hash::FastSet<String>,
    ) -> ExprKind {
        match expr {
            ExprKind::ConstRef(name) => {
                resolve_name(name.as_str(), expr, global_constants, visiting)
            }
            ExprKind::Variable(name) => resolve_name(name, expr, global_constants, visiting),
            ExprKind::ArrayLiteral(items) => ExprKind::ArrayLiteral(
                items
                    .iter()
                    .map(|item| {
                        Expr::new(
                            resolve(&item.kind, global_constants, visiting),
                            item.span,
                        )
                    })
                    .collect(),
            ),
            ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
                items
                    .iter()
                    .map(|(key, value)| {
                        (
                            Expr::new(
                                resolve(&key.kind, global_constants, visiting),
                                key.span,
                            ),
                            Expr::new(
                                resolve(&value.kind, global_constants, visiting),
                                value.span,
                            ),
                        )
                    })
                    .collect(),
            ),
            // `JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE`: resolved operand by operand and
            // folded by `literal_default_value`.
            ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
                left: Box::new(Expr::new(resolve(&left.kind, global_constants, visiting), left.span)),
                op: op.clone(),
                right: Box::new(Expr::new(
                    resolve(&right.kind, global_constants, visiting),
                    right.span,
                )),
            },
            ExprKind::Negate(inner) => ExprKind::Negate(Box::new(Expr::new(
                resolve(&inner.kind, global_constants, visiting),
                inner.span,
            ))),
            ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(Expr::new(
                resolve(&inner.kind, global_constants, visiting),
                inner.span,
            ))),
            other => other.clone(),
        }
    }

    /// Resolves one named constant while guarding the recursive lookup against cycles.
    fn resolve_name(
        name: &str,
        original: &ExprKind,
        global_constants: &crate::fast_hash::FastMap<String, (ExprKind, PhpType)>,
        visiting: &mut crate::fast_hash::FastSet<String>,
    ) -> ExprKind {
        let key = name.trim_start_matches('\\');
        let Some((resolved, _)) = global_constants
            .get(name)
            .or_else(|| global_constants.get(key))
        else {
            return original.clone();
        };
        if !visiting.insert(key.to_string()) {
            return original.clone();
        }
        let result = resolve(resolved, global_constants, visiting);
        visiting.remove(key);
        result
    }

    resolve(expr, global_constants, &mut crate::fast_hash::FastSet::default())
}

/// Converts a supported default expression into a direct storage value.
pub(crate) fn literal_default_value(
    context: &str,
    php_type: &PhpType,
    expr: &ExprKind,
    op_name: &str,
) -> Result<LiteralDefaultValue> {
    // A constant EXPRESSION over integers is as literal as its value: php evaluates
    // `self::CHECK_A | self::CHECK_B` once, when the class is linked. By the time a default gets
    // here its class and global constants are already substituted, so what is left to do is the
    // arithmetic itself.
    if matches!(expr, ExprKind::BinaryOp { .. } | ExprKind::BitNot(_)) {
        if let Some(value) = fold_int_constant_expr(expr) {
            return literal_default_value(context, php_type, &ExprKind::IntLiteral(value), op_name);
        }
    }
    if let Some(folded) = fold_named_class_constant(expr) {
        return literal_default_value(context, php_type, &folded, op_name);
    }
    if let Some(folded) = crate::types::fold_spreadless_mixed_array_literal(expr, crate::codegen_support::compile_php_version()) {
        return literal_default_value(context, php_type, &folded, op_name);
    }
    if php_type.coerces_integer_to_float() {
        let integer = match expr {
            ExprKind::IntLiteral(value) => Some(*value),
            ExprKind::Negate(inner) => match inner.kind {
                ExprKind::IntLiteral(value) => value.checked_neg(),
                _ => None,
            },
            _ => None,
        };
        if let Some(value) = integer {
            return literal_default_value(context, php_type, &ExprKind::FloatLiteral(value as f64), op_name);
        }
    }
    if let Some(folded) = fold_array_union_literal(expr) {
        return literal_default_value(context, php_type, &folded, op_name);
    }
    match (php_type, expr) {
        (PhpType::Int, ExprKind::IntLiteral(value)) => Ok(LiteralDefaultValue::Int(*value)),
        (PhpType::Int, ExprKind::Negate(inner)) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(LiteralDefaultValue::Int)
                .ok_or_else(|| unsupported_literal_default(context, php_type, op_name)),
            _ => Err(unsupported_literal_default(context, php_type, op_name)),
        },
        (PhpType::Bool, ExprKind::BoolLiteral(value)) => Ok(LiteralDefaultValue::Bool(*value)),
        (PhpType::Float, ExprKind::FloatLiteral(value)) => Ok(LiteralDefaultValue::Float(*value)),
        (PhpType::Float, ExprKind::IntLiteral(value)) => Ok(LiteralDefaultValue::Float(*value as f64)),
        (PhpType::Float, ExprKind::Negate(inner)) => match &inner.kind {
            ExprKind::FloatLiteral(value) => Ok(LiteralDefaultValue::Float(-value)),
            ExprKind::IntLiteral(value) => Ok(LiteralDefaultValue::Float(-(*value as f64))),
            _ => Err(unsupported_literal_default(context, php_type, op_name)),
        },
        (PhpType::Str, ExprKind::StringLiteral(value)) => Ok(LiteralDefaultValue::Str(value.clone())),
        // A null-capable int slot (`?int` / `int|null` under `NullRepr::Tagged`) is stored inline
        // as the two-word `{payload, tag}` TaggedScalar, never as a pointer to a boxed Mixed cell.
        // These arms must stay ahead of the `Mixed | Union(_)` boxing arms below: an `int|null`
        // property type also matches `Union(_)`, and boxing it would write a Mixed pointer into the
        // payload word while the tag word still reads "int", so the reader hands back the pointer
        // as an integer.
        (php_type, ExprKind::IntLiteral(value))
            if php_type.codegen_repr() == PhpType::TaggedScalar =>
        {
            Ok(LiteralDefaultValue::TaggedInt(*value))
        }
        (php_type, ExprKind::Negate(inner)) if php_type.codegen_repr() == PhpType::TaggedScalar => {
            match &inner.kind {
                ExprKind::IntLiteral(value) => value
                    .checked_neg()
                    .map(LiteralDefaultValue::TaggedInt)
                    .ok_or_else(|| unsupported_literal_default(context, php_type, op_name)),
                _ => Err(unsupported_literal_default(context, php_type, op_name)),
            }
        }
        (php_type, ExprKind::Null) if php_type.codegen_repr() == PhpType::TaggedScalar => {
            Ok(LiteralDefaultValue::TaggedNull)
        }
        // A `?ptr<T>` slot holds a raw pointer word whose null is address 0, so its null
        // default is the zero word. This arm must precede the `Union(_)` boxing arms: boxing
        // would store a non-zero Mixed cell address that every later `=== null` reads as set.
        (php_type, ExprKind::Null) if crate::types::is_nullable_pointer_type(php_type) => {
            Ok(LiteralDefaultValue::Null)
        }
        (PhpType::Mixed | PhpType::Union(_), ExprKind::StringLiteral(value)) => {
            Ok(LiteralDefaultValue::BoxedStr(value.clone()))
        }
        // A scalar literal default for a `mixed`/union slot is boxed into a tagged Mixed cell,
        // mirroring the string and null cases above.
        (PhpType::Mixed | PhpType::Union(_), ExprKind::IntLiteral(value)) => {
            Ok(LiteralDefaultValue::BoxedInt(*value))
        }
        (PhpType::Mixed | PhpType::Union(_), ExprKind::BoolLiteral(value)) => {
            Ok(LiteralDefaultValue::BoxedBool(*value))
        }
        (PhpType::Mixed | PhpType::Union(_), ExprKind::FloatLiteral(value)) => {
            Ok(LiteralDefaultValue::BoxedFloat(*value))
        }
        (PhpType::Mixed | PhpType::Union(_), ExprKind::Negate(inner)) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(LiteralDefaultValue::BoxedInt)
                .ok_or_else(|| unsupported_literal_default(context, php_type, op_name)),
            ExprKind::FloatLiteral(value) => Ok(LiteralDefaultValue::BoxedFloat(-value)),
            _ => Err(unsupported_literal_default(context, php_type, op_name)),
        },
        (PhpType::Mixed | PhpType::Union(_), ExprKind::Null) => Ok(LiteralDefaultValue::BoxedNull),
        // An array literal in a `mixed`/union slot boxes the array the way the scalar arms above
        // box their payloads — `emit_box_current_value_as_mixed` already handles `Array` on both
        // architectures, so only the recognition was missing. Elements are typed `Mixed` because
        // the slot is: a later `$t->m[] = "s"` must not find an `array<int>` underneath.
        (PhpType::Mixed | PhpType::Union(_), ExprKind::ArrayLiteral(items)) => {
            let elem_type = PhpType::Mixed;
            let elements = items
                .iter()
                .map(|item| literal_array_element(context, &elem_type, &item.kind, op_name))
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::BoxedArray {
                elem_type,
                elements,
            })
        }
        // The keyed spelling of the arm above. PHP has no separate associative array type, so
        // `["k" => 1]` in a `?array` slot is the same default as `[1, 2]` is; only the storage
        // the literal needs differs, and hash storage is what a string key requires. Values are
        // typed `Mixed` for the same reason the positional elements are: the slot is `mixed`, so
        // a later write of any type into the hash must not find a narrower value type underneath.
        (PhpType::Mixed | PhpType::Union(_), ExprKind::ArrayLiteralAssoc(items)) => {
            let value_type = PhpType::Mixed;
            let entries = literal_assoc_entries(context, &value_type, items, op_name)?;
            Ok(LiteralDefaultValue::BoxedAssocArray { value_type, entries })
        }
        (PhpType::Void | PhpType::Never, ExprKind::Null) => Ok(LiteralDefaultValue::NullSentinel),
        (PhpType::Void | PhpType::Never, _) => Ok(LiteralDefaultValue::NullSentinel),
        (PhpType::Object(_), ExprKind::Null) => Ok(LiteralDefaultValue::Null),
        (php_type, ExprKind::Null) if php_type.codegen_repr().is_refcounted() => {
            Ok(LiteralDefaultValue::Null)
        }
        (PhpType::AssocArray { value, .. }, ExprKind::ArrayLiteral(items)) => {
            let value_type = value.as_ref().codegen_repr();
            if items.is_empty() {
                return Ok(LiteralDefaultValue::EmptyAssocArray { value_type });
            }
            // A positional literal stored into hash storage takes PHP's implicit 0,1,2,... keys.
            let entries = items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    Ok(LiteralAssocEntry {
                        key: LiteralArrayKey::Int(index as i64),
                        value: literal_array_element(context, &value_type, &item.kind, op_name)?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::AssocArray {
                value_type,
                entries,
            })
        }
        (PhpType::AssocArray { value, .. }, ExprKind::ArrayLiteralAssoc(items)) => {
            let value_type = value.as_ref().codegen_repr();
            let entries = literal_assoc_entries(context, &value_type, items, op_name)?;
            Ok(LiteralDefaultValue::AssocArray {
                value_type,
                entries,
            })
        }
        (PhpType::Iterable, ExprKind::ArrayLiteral(items)) => {
            let elem_type = PhpType::Mixed;
            let elements = items
                .iter()
                .map(|item| literal_array_element(context, &elem_type, &item.kind, op_name))
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::Array {
                elem_type,
                elements,
            })
        }
        (PhpType::Iterable, ExprKind::ArrayLiteralAssoc(items)) => {
            let value_type = PhpType::Mixed;
            let entries = items
                .iter()
                .map(|(key, value_expr)| {
                    Ok(LiteralAssocEntry {
                        key: literal_array_key(context, &key.kind, op_name)?,
                        value: literal_array_element(
                            context,
                            &value_type,
                            &value_expr.kind,
                            op_name,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::AssocArray {
                value_type,
                entries,
            })
        }
        (PhpType::Array(elem_type), ExprKind::ArrayLiteral(items)) => {
            let elem_type = elem_type.codegen_repr();
            let elements = items
                .iter()
                .map(|item| literal_array_element(context, &elem_type, &item.kind, op_name))
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::Array {
                elem_type,
                elements,
            })
        }
        // A KEYED literal in a slot whose static type says indexed. The two disagree, and the
        // literal is the one that knows: a `['kernel.secret' => false, …]` default is a hash at
        // run time whatever the inferred storage says, and every read of it goes through the
        // runtime heap kind. Building the hash is what PHP does; refusing to build it made
        // Symfony's generated container uncompilable over `$loadedDynamicParameters`.
        //
        // Restricted to a `mixed` element, which is the only case where the reads are already
        // dynamic. A precise element type would mean the storage really is indexed and the
        // disagreement is somewhere else, where a silent hash would be the wrong answer.
        (PhpType::Array(elem_type), ExprKind::ArrayLiteralAssoc(items))
            if elem_type.codegen_repr() == PhpType::Mixed =>
        {
            let value_type = PhpType::Mixed;
            let entries = items
                .iter()
                .map(|(key, value_expr)| {
                    Ok(LiteralAssocEntry {
                        key: literal_array_key(context, &key.kind, op_name)?,
                        value: literal_array_element(
                            context,
                            &value_type,
                            &value_expr.kind,
                            op_name,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(LiteralDefaultValue::AssocArray {
                value_type,
                entries,
            })
        }
        _ => Err(unsupported_literal_default(context, php_type, op_name)),
    }
}

/// Emits a string literal default into the canonical string result registers.
pub(crate) fn emit_string_literal_default_to_result(
    ctx: &mut FunctionContext<'_>,
    value: &str,
) {
    let bytes = elephc_builtin_contract::string_literal::literal_bytes(value);
    emit_string_bytes_default_to_result(ctx, &bytes);
}

/// Emits raw PHP bytes, including symbol-name metadata that has no literal escape markers.
pub(crate) fn emit_string_bytes_default_to_result(
    ctx: &mut FunctionContext<'_>, value: &[u8],
) {
    let (label, len) = ctx.data.add_string(value);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Emits a string literal default boxed as a Mixed value.
pub(crate) fn emit_boxed_string_literal_default_to_result(
    ctx: &mut FunctionContext<'_>,
    value: &str,
) {
    emit_string_literal_default_to_result(ctx, value);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
}

/// Emits a boxed integer literal default into the canonical result register: loads the immediate
/// into the integer result register, then boxes it as a `Mixed` cell (runtime tag 0).
pub(crate) fn emit_boxed_int_literal_to_result(ctx: &mut FunctionContext<'_>, value: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
}

/// Emits a boxed boolean literal default into the canonical result register: materializes `0`/`1`
/// in the integer result register, then boxes it as a `Mixed` cell (runtime tag 3).
pub(crate) fn emit_boxed_bool_literal_to_result(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
}

/// Emits a boxed float literal default into the canonical result register: loads the literal from
/// the `.rodata` float pool into the float result register, then boxes it as a `Mixed` cell (tag 2).
pub(crate) fn emit_boxed_float_literal_to_result(ctx: &mut FunctionContext<'_>, value: f64) {
    let label = ctx.data.add_float(value);
    let scratch = abi::symbol_scratch_reg(ctx.emitter);
    let float_reg = abi::float_result_reg(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, scratch, &label);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(                                            // load the boxed float default through the symbol scratch register
                &format!("ldr {}, [{}]", float_reg, scratch)
            );
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(                                            // load the boxed float default through the symbol scratch register
                &format!("movsd {}, QWORD PTR [{}]", float_reg, scratch)
            );
        }
    }
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
}

/// Emits an empty associative-array literal default into the canonical result register.
pub(crate) fn emit_empty_assoc_array_literal_to_result(
    ctx: &mut FunctionContext<'_>,
    value_type: &PhpType,
) {
    let capacity_reg = abi::int_arg_reg_name(ctx.emitter.target, 0);
    let value_tag_reg = abi::int_arg_reg_name(ctx.emitter.target, 1);
    abi::emit_load_int_immediate(ctx.emitter, capacity_reg, 16);
    abi::emit_load_int_immediate(
        ctx.emitter,
        value_tag_reg,
        crate::codegen::runtime_value_tag(&value_type.codegen_repr()) as i64,
    );
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
}

/// Emits a boxed PHP null default into the canonical result register.
/// Loads an enum case's singleton into the integer result register with a reference of its own
/// (boxed for a `mixed` slot), materializing the case on first use.
pub(crate) fn emit_enum_case_default_to_result(
    ctx: &mut FunctionContext<'_>,
    enum_name: &str,
    case_name: &str,
    boxed: bool,
) {
    crate::codegen::enum_singletons::emit_lazy_case_load(ctx, enum_name, case_name);
    let case_type = PhpType::Object(enum_name.to_string());
    abi::emit_incref_if_refcounted(ctx.emitter, &case_type);
    if boxed {
        crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &case_type);
    }
}

pub(crate) fn emit_boxed_null_literal_to_result(ctx: &mut FunctionContext<'_>) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
}

/// Emits PHP null as an inline tagged scalar literal default.
pub(crate) fn emit_tagged_null_literal_to_result(ctx: &mut FunctionContext<'_>) {
    crate::codegen::sentinels::emit_tagged_scalar_null(ctx.emitter);
}

/// Emits a non-null integer literal default as an inline tagged scalar: the immediate lands in
/// the integer result register (the payload word) and the int runtime tag in the adjacent tag
/// register, which is exactly the `{payload, tag}` pair a `?int` slot is read back through.
pub(crate) fn emit_tagged_int_literal_to_result(ctx: &mut FunctionContext<'_>, value: i64) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    crate::codegen::sentinels::emit_tagged_scalar_from_int_result(ctx.emitter);
}

/// Emits an indexed-array literal default into the canonical result register.
pub(super) fn emit_array_literal_default_to_result(
    ctx: &mut FunctionContext<'_>,
    elem_type: &PhpType,
    elements: &[LiteralArrayElement],
) -> Result<()> {
    emit_array_literal_allocation(ctx, elem_type, elements.len())?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    for element in elements {
        let value_type = emit_array_element_value(ctx, element)?;
        append_array_literal_element(
            ctx,
            elem_type,
            &value_type,
            element.is_owned_container(),
        )?;
    }
    abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    Ok(())
}

/// Emits an associative-array literal default into the canonical result register. Allocates an
/// empty hash, then inserts each constant key/value entry through `__rt_hash_set`. The hash pointer
/// flows through the result register across insertions because the runtime may reallocate it on
/// growth. For each entry the normalized key is staged on the temporary stack while the value is
/// materialized, so value-staging calls (string persistence, key normalization) cannot clobber it.
pub(crate) fn emit_assoc_array_literal_default_to_result(
    ctx: &mut FunctionContext<'_>,
    value_type: &PhpType,
    entries: &[LiteralAssocEntry],
) -> Result<()> {
    emit_empty_assoc_array_literal_to_result(ctx, value_type);
    for entry in entries {
        abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                materialize_assoc_literal_key_aarch64(ctx, &entry.key);
                abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
                let actual_value_type = emit_array_element_value(ctx, &entry.value)?;
                materialize_assoc_literal_value(ctx, value_type, &actual_value_type)?;
                abi::emit_pop_reg_pair(ctx.emitter, "x1", "x2");
                abi::emit_pop_reg(ctx.emitter, "x0");
                abi::emit_load_int_immediate(
                    ctx.emitter,
                    "x5",
                    assoc_literal_value_tag(value_type, &actual_value_type),
                );
            }
            Arch::X86_64 => {
                materialize_assoc_literal_key_x86_64(ctx, &entry.key);
                abi::emit_push_reg_pair(ctx.emitter, "rsi", "rdx");
                let actual_value_type = emit_array_element_value(ctx, &entry.value)?;
                materialize_assoc_literal_value(ctx, value_type, &actual_value_type)?;
                abi::emit_pop_reg_pair(ctx.emitter, "rsi", "rdx");
                abi::emit_pop_reg(ctx.emitter, "rdi");
                abi::emit_load_int_immediate(
                    ctx.emitter,
                    "r9",
                    assoc_literal_value_tag(value_type, &actual_value_type),
                );
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    }
    Ok(())
}

/// Transfers a freshly materialized associative default into its owning Mixed cell.
///
/// The OWNED boxer, for the reason the `BoxedArray` arms give: the literal allocated the hash
/// and the box takes its own reference, so the plain boxer would retain without releasing and
/// leak one block per object.
pub(crate) fn emit_boxed_assoc_array_literal_to_result(
    ctx: &mut FunctionContext<'_>,
    value_type: &PhpType,
    entries: &[LiteralAssocEntry],
) -> Result<()> {
    emit_assoc_array_literal_default_to_result(ctx, value_type, entries)?;
    crate::codegen::emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(value_type.clone()),
        },
    );
    Ok(())
}

/// Materializes a constant associative-array key into the AArch64 hash key registers `x1`/`x2`.
/// Integer keys load directly with the integer-key sentinel in `x2`; string keys load the literal
/// pointer/length and run `__rt_hash_normalize_key`, so PHP numeric-string keys collapse to integer
/// keys while genuine string keys keep `x1`/`x2` as pointer/length.
fn materialize_assoc_literal_key_aarch64(ctx: &mut FunctionContext<'_>, key: &LiteralArrayKey) {
    match key {
        LiteralArrayKey::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, "x1", *value);
            abi::emit_load_int_immediate(ctx.emitter, "x2", -1);
        }
        LiteralArrayKey::Str(value) => {
            let bytes = elephc_builtin_contract::string_literal::literal_bytes(value);
            let (label, len) = ctx.data.add_string(&bytes);
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
        }
    }
}

/// Materializes a constant associative-array key into the x86_64 hash key registers `rsi`/`rdx`.
/// Integer keys load directly with the integer-key sentinel in `rdx`; string keys load the literal
/// pointer/length into `rax`/`rdx` (the `__rt_hash_normalize_key` input registers), normalize
/// (result in `rax`/`rdx`), and move the normalized low word into `rsi` for the `__rt_hash_set` ABI.
fn materialize_assoc_literal_key_x86_64(ctx: &mut FunctionContext<'_>, key: &LiteralArrayKey) {
    match key {
        LiteralArrayKey::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, "rsi", *value);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", -1);
        }
        LiteralArrayKey::Str(value) => {
            let bytes = elephc_builtin_contract::string_literal::literal_bytes(value);
            let (label, len) = ctx.data.add_string(&bytes);
            abi::emit_symbol_address(ctx.emitter, "rax", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", len as i64);
            abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
            ctx.emitter.instruction("mov rsi, rax");                            // move the normalized key low word into the hash ABI key register
        }
    }
}

/// Converts one supported literal expression into an indexed-array element payload.
fn literal_array_element(
    context: &str,
    elem_type: &PhpType,
    expr: &ExprKind,
    op_name: &str,
) -> Result<LiteralArrayElement> {
    if let Some(folded) = fold_named_class_constant(expr) {
        return literal_array_element(context, elem_type, &folded, op_name);
    }
    // A nested literal recurses rather than being refused. Under a Mixed-capable element type
    // the nested elements are typed `Mixed` for the same reason the outer ones are -- the
    // container has to accept a later write of any type. A container element type reached
    // through a KEYED outer literal (`["k" => [1]]` infers the hash's value type from the
    // literal) keeps that inferred type, so the inner container is stamped the way a read of it
    // will expect. A scalar element type cannot hold a container and is refused as before.
    // Delegating to `literal_default_value` shares the top-level arms, including PHP's implicit
    // `0,1,2,…` keys for a positional literal stored into hash storage (issue #1052).
    if let Some(folded) = crate::types::fold_spreadless_mixed_array_literal(expr, crate::codegen_support::compile_php_version()) {
        return literal_array_element(context, elem_type, &folded, op_name);
    }
    if matches!(expr, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)) {
        let nested_type = match elem_type.codegen_repr() {
            PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => match expr {
                ExprKind::ArrayLiteral(_) => PhpType::Array(Box::new(PhpType::Mixed)),
                _ => PhpType::AssocArray {
                    key: Box::new(PhpType::Mixed), value: Box::new(PhpType::Mixed),
                },
            },
            ty @ (PhpType::Array(_) | PhpType::AssocArray { .. }) => ty,
            _ => return Err(unsupported_literal_default(context, elem_type, op_name)),
        };
        return match literal_default_value(context, &nested_type, expr, op_name)? {
            LiteralDefaultValue::Array { elem_type, elements } => {
                Ok(LiteralArrayElement::Array { elem_type, elements })
            }
            LiteralDefaultValue::AssocArray { value_type, entries } => {
                Ok(LiteralArrayElement::AssocArray { value_type, entries })
            }
            LiteralDefaultValue::EmptyAssocArray { value_type } => {
                Ok(LiteralArrayElement::AssocArray { value_type, entries: Vec::new() })
            }
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        };
    }
    match elem_type.codegen_repr() {
        PhpType::Array(nested_elem_type) => match expr {
            ExprKind::ArrayLiteral(items) => {
                let nested_elem_type = nested_elem_type.codegen_repr();
                let elements = items
                    .iter()
                    .map(|item| {
                        literal_array_element(context, &nested_elem_type, &item.kind, op_name)
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(LiteralArrayElement::Array {
                    elem_type: nested_elem_type,
                    elements,
                })
            }
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::AssocArray { value, .. } => match expr {
            ExprKind::ArrayLiteral(items) => {
                let value_type = value.as_ref().codegen_repr();
                let entries = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        Ok(LiteralAssocEntry {
                            key: LiteralArrayKey::Int(index as i64),
                            value: literal_array_element(
                                context,
                                &value_type,
                                &item.kind,
                                op_name,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(LiteralArrayElement::AssocArray {
                    value_type,
                    entries,
                })
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                let value_type = value.as_ref().codegen_repr();
                let entries = items
                    .iter()
                    .map(|(key, value)| {
                        Ok(LiteralAssocEntry {
                            key: literal_array_key(context, &key.kind, op_name)?,
                            value: literal_array_element(
                                context,
                                &value_type,
                                &value.kind,
                                op_name,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(LiteralArrayElement::AssocArray {
                    value_type,
                    entries,
                })
            }
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => match expr {
            ExprKind::ArrayLiteral(items) => {
                let nested_elem_type = PhpType::Mixed;
                let elements = items
                    .iter()
                    .map(|item| {
                        literal_array_element(context, &nested_elem_type, &item.kind, op_name)
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(LiteralArrayElement::Array {
                    elem_type: nested_elem_type,
                    elements,
                })
            }
            ExprKind::ArrayLiteralAssoc(items) => {
                let value_type = PhpType::Mixed;
                let entries = items
                    .iter()
                    .map(|(key, value)| {
                        Ok(LiteralAssocEntry {
                            key: literal_array_key(context, &key.kind, op_name)?,
                            value: literal_array_element(
                                context,
                                &value_type,
                                &value.kind,
                                op_name,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(LiteralArrayElement::AssocArray {
                    value_type,
                    entries,
                })
            }
            ExprKind::IntLiteral(value) => Ok(LiteralArrayElement::Int(*value)),
            ExprKind::BoolLiteral(value) => Ok(LiteralArrayElement::Bool(*value)),
            ExprKind::FloatLiteral(value) => Ok(LiteralArrayElement::Float(*value)),
            ExprKind::StringLiteral(value) => Ok(LiteralArrayElement::Str(value.clone())),
            ExprKind::Null => Ok(LiteralArrayElement::Null),
            ExprKind::Negate(inner) => match &inner.kind {
                ExprKind::IntLiteral(value) => value
                    .checked_neg()
                    .map(LiteralArrayElement::Int)
                    .ok_or_else(|| unsupported_literal_default(context, elem_type, op_name)),
                ExprKind::FloatLiteral(value) => Ok(LiteralArrayElement::Float(-value)),
                _ => Err(unsupported_literal_default(context, elem_type, op_name)),
            },
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Int => match expr {
            ExprKind::IntLiteral(value) => Ok(LiteralArrayElement::Int(*value)),
            ExprKind::Negate(inner) => match &inner.kind {
                ExprKind::IntLiteral(value) => value
                    .checked_neg()
                    .map(LiteralArrayElement::Int)
                    .ok_or_else(|| unsupported_literal_default(context, elem_type, op_name)),
                _ => Err(unsupported_literal_default(context, elem_type, op_name)),
            },
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Bool => match expr {
            ExprKind::BoolLiteral(value) => Ok(LiteralArrayElement::Bool(*value)),
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Float => match expr {
            ExprKind::FloatLiteral(value) => Ok(LiteralArrayElement::Float(*value)),
            ExprKind::IntLiteral(value) => Ok(LiteralArrayElement::Float(*value as f64)),
            ExprKind::Negate(inner) => match &inner.kind {
                ExprKind::FloatLiteral(value) => Ok(LiteralArrayElement::Float(-value)),
                ExprKind::IntLiteral(value) => Ok(LiteralArrayElement::Float(-(*value as f64))),
                _ => Err(unsupported_literal_default(context, elem_type, op_name)),
            },
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        PhpType::Str => match expr {
            ExprKind::StringLiteral(value) => Ok(LiteralArrayElement::Str(value.clone())),
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        // A pointer element's only literal is `null`, the null pointer word 0.
        PhpType::Pointer(_) => match expr {
            ExprKind::Null => Ok(LiteralArrayElement::Int(0)),
            _ => Err(unsupported_literal_default(context, elem_type, op_name)),
        },
        _ => Err(unsupported_literal_default(context, elem_type, op_name)),
    }
}

/// Converts constant hash entries recursively while sharing key coercion with top-level defaults.
fn literal_assoc_entries(
    context: &str,
    value_type: &PhpType,
    items: &[(Expr, Expr)],
    op_name: &str,
) -> Result<Vec<LiteralAssocEntry>> {
    items.iter().map(|(key, value)| {
        Ok(LiteralAssocEntry {
            key: literal_array_key(context, &key.kind, op_name)?,
            value: literal_array_element(context, value_type, &value.kind, op_name)?,
        })
    }).collect()
}

/// Converts a constant associative-array key expression into a materializable hash key, applying
/// PHP's literal key coercions: booleans and floats become integer keys, `null` becomes the empty
/// string key, and numeric strings are normalized to integer keys later at emit time. Unsupported
/// key forms fall through to the shared unsupported-default error rather than miscompiling.
fn literal_array_key(context: &str, expr: &ExprKind, op_name: &str) -> Result<LiteralArrayKey> {
    if let Some(folded) = fold_named_class_constant(expr) {
        return literal_array_key(context, &folded, op_name);
    }
    match expr {
        ExprKind::StringLiteral(value) => Ok(LiteralArrayKey::Str(value.clone())),
        ExprKind::IntLiteral(value) => Ok(LiteralArrayKey::Int(*value)),
        ExprKind::BoolLiteral(value) => Ok(LiteralArrayKey::Int(i64::from(*value))),
        ExprKind::Null => Ok(LiteralArrayKey::Str(String::new())),
        ExprKind::FloatLiteral(value) => Ok(LiteralArrayKey::Int(crate::types::php_float_array_key(*value))),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => value
                .checked_neg()
                .map(LiteralArrayKey::Int)
                .ok_or_else(|| unsupported_literal_default(context, &PhpType::Int, op_name)),
            ExprKind::FloatLiteral(value) => Ok(LiteralArrayKey::Int(crate::types::php_float_array_key(-*value))),
            _ => Err(unsupported_literal_default(context, &PhpType::Int, op_name)),
        },
        _ => Err(unsupported_literal_default(context, &PhpType::Int, op_name)),
    }
}

/// Allocates an indexed array sized for a literal property/static-property default.
fn emit_array_literal_allocation(
    ctx: &mut FunctionContext<'_>,
    elem_type: &PhpType,
    element_count: usize,
) -> Result<()> {
    let capacity = element_count.max(4);
    let elem_size = array_element_size(elem_type)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_int_immediate(ctx.emitter, "x0", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x1", elem_size);
        }
        Arch::X86_64 => {
            abi::emit_load_int_immediate(ctx.emitter, "rdi", capacity as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", elem_size);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(
        ctx.emitter, abi::int_result_reg(ctx.emitter), elem_type,
    );
    Ok(())
}

/// Emits one literal array element into the canonical result register(s).
///
/// A container element recurses into the same emitter the enclosing container used, leaving a
/// freshly allocated OWNED container in the result register. Both callers stage the enclosing
/// container's pointer on the stack around this call, and the nested emitters' own stack traffic
/// is balanced, so the nesting is safe to any depth.
fn emit_array_element_value(
    ctx: &mut FunctionContext<'_>,
    element: &LiteralArrayElement,
) -> Result<PhpType> {
    match element {
        LiteralArrayElement::Int(value) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), *value);
            Ok(PhpType::Int)
        }
        LiteralArrayElement::Bool(value) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), i64::from(*value));
            Ok(PhpType::Bool)
        }
        LiteralArrayElement::Float(value) => {
            let label = ctx.data.add_float(*value);
            abi::emit_load_symbol_to_reg(ctx.emitter, abi::float_result_reg(ctx.emitter), &label, 0);
            Ok(PhpType::Float)
        }
        LiteralArrayElement::Str(value) => {
            emit_string_literal_default_to_result(ctx, value);
            Ok(PhpType::Str)
        }
        LiteralArrayElement::Null => {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
            Ok(PhpType::Void)
        }
        LiteralArrayElement::Array {
            elem_type,
            elements,
        } => {
            emit_array_literal_default_to_result(ctx, elem_type, elements)?;
            Ok(PhpType::Array(Box::new(elem_type.clone())))
        }
        LiteralArrayElement::AssocArray {
            value_type,
            entries,
        } => {
            emit_assoc_array_literal_default_to_result(ctx, value_type, entries)?;
            Ok(PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(value_type.clone()),
            })
        }
    }
}

/// Appends the current literal element value to the array pointer saved on the stack.
///
/// `value_is_owned` distinguishes the two ways a value can arrive. A scalar has no reference,
/// so boxing it retains whatever the box needs and nothing is left over. A nested container
/// arrives with the +1 `__rt_array_new`/`__rt_hash_new` gave it and no other holder, so boxing
/// must TRANSFER that reference: the retaining box plus a release of the original leaves the
/// cell as the single owner. Retaining without releasing would leak the whole nested tree once
/// per element.
fn append_array_literal_element(
    ctx: &mut FunctionContext<'_>,
    elem_type: &PhpType,
    value_type: &PhpType,
    value_is_owned: bool,
) -> Result<()> {
    match elem_type.codegen_repr() {
        PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => {
            if value_is_owned {
                crate::codegen::emit_box_current_owned_value_as_mixed(
                    ctx.emitter,
                    &value_type.codegen_repr(),
                );
            } else {
                emit_box_current_value_as_mixed(ctx.emitter, &value_type.codegen_repr());
            }
            append_refcounted_array_literal_element(ctx, &PhpType::Mixed);
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            append_refcounted_array_literal_element(ctx, value_type);
        }
        PhpType::Int | PhpType::Bool | PhpType::Pointer(_) | PhpType::Buffer(_) => {
            append_scalar_array_literal_element(ctx)
        }
        PhpType::Float => append_float_array_literal_element(ctx),
        PhpType::Str => append_string_array_literal_element(ctx),
        PhpType::Object(_) => {
            append_refcounted_array_literal_element(ctx, &value_type.codegen_repr())
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "array default element PHP type {:?}",
                other
            )))
        }
    }
    Ok(())
}

/// Appends the current integer-like result to the array pointer saved on the stack.
fn append_scalar_array_literal_element(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // pass the scalar literal payload to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the scalar literal payload to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Appends the current floating-point result to the array pointer saved on the stack.
fn append_float_array_literal_element(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fmov x1, d0");                             // pass the float literal bits to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movq rsi, xmm0");                          // pass the float literal bits to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Appends the current string result to the array pointer saved on the stack.
fn append_string_array_literal_element(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rsi, rax");                            // pass the string literal pointer to the indexed-array append helper
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_array_push_str");
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Appends the current refcounted result and releases the temporary owner after insertion.
fn append_refcounted_array_literal_element(ctx: &mut FunctionContext<'_>, value_type: &PhpType) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_pop_reg(ctx.emitter, "x9");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x1, x0");                              // pass the boxed literal payload to the refcounted append helper
            ctx.emitter.instruction("mov x0, x9");                              // pass the saved indexed-array pointer to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, value_type);
            abi::emit_push_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            abi::emit_pop_reg(ctx.emitter, "r11");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the boxed literal payload to the refcounted append helper
            ctx.emitter.instruction("mov rdi, r11");                            // pass the saved indexed-array pointer to the refcounted append helper
            abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
            emit_release_pushed_refcounted_temp_after_array_push(ctx.emitter, value_type);
            abi::emit_push_reg(ctx.emitter, "rax");
        }
    }
}

/// Materializes the current literal value as the payload consumed by `__rt_hash_set`.
fn materialize_assoc_literal_value(
    ctx: &mut FunctionContext<'_>,
    storage_value_type: &PhpType,
    actual_value_type: &PhpType,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => materialize_assoc_literal_value_aarch64(
            ctx,
            storage_value_type,
            actual_value_type,
        ),
        Arch::X86_64 => materialize_assoc_literal_value_x86_64(
            ctx,
            storage_value_type,
            actual_value_type,
        ),
    }
}

/// Materializes the current literal value for AArch64 hash insertion.
fn materialize_assoc_literal_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    storage_value_type: &PhpType,
    actual_value_type: &PhpType,
) -> Result<()> {
    if matches!(storage_value_type.codegen_repr(), PhpType::Mixed | PhpType::Iterable) {
        return materialize_assoc_literal_concrete_value_aarch64(ctx, actual_value_type);
    }
    match actual_value_type.codegen_repr() {
        PhpType::Int | PhpType::Bool | PhpType::Float => {
            if matches!(actual_value_type.codegen_repr(), PhpType::Float) {
                ctx.emitter.instruction("fmov x3, d0");                         // pass the literal float bits as the hash value low word
            } else {
                ctx.emitter.instruction("mov x3, x0");                          // pass the literal scalar payload as the hash value low word
            }
            ctx.emitter.instruction("mov x4, xzr");                             // scalar hash values do not use the high payload word
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov x3, x1");                              // pass the persistent literal string pointer as the hash value low word
            ctx.emitter.instruction("mov x4, x2");                              // pass the literal string length as the hash value high word
            Ok(())
        }
        // A nested container arrives as an owned heap pointer. `__rt_hash_set` does not retain
        // what it stores -- it only releases what it OVERWRITES -- so handing it the pointer is
        // itself the ownership transfer, and releasing afterwards here would free a live child.
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            ctx.emitter.instruction("mov x3, x0");                              // transfer the owned nested refcounted payload into the hash bucket
            ctx.emitter.instruction("mov x4, xzr");                             // refcounted hash values use only the low payload word
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x3, xzr");                             // null hash values use a zero low payload word
            ctx.emitter.instruction("mov x4, xzr");                             // null hash values use a zero high payload word
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "assoc array default element PHP type {:?}",
            other
        ))),
    }
}

/// Materializes the current literal value for x86_64 hash insertion.
fn materialize_assoc_literal_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    storage_value_type: &PhpType,
    actual_value_type: &PhpType,
) -> Result<()> {
    if matches!(storage_value_type.codegen_repr(), PhpType::Mixed | PhpType::Iterable) {
        return materialize_assoc_literal_concrete_value_x86_64(ctx, actual_value_type);
    }
    match actual_value_type.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the literal scalar payload as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // scalar hash values do not use the high payload word
            Ok(())
        }
        PhpType::Float => {
            ctx.emitter.instruction("movq rcx, xmm0");                          // pass the literal float bits as the hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // scalar hash values do not use the high payload word
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the persistent literal string pointer as the hash value low word
            ctx.emitter.instruction("mov r8, rdx");                             // pass the literal string length as the hash value high word
            Ok(())
        }
        // A nested container arrives as an owned heap pointer; see the AArch64 twin above.
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            ctx.emitter.instruction("mov rcx, rax");                            // transfer the owned nested refcounted payload into the hash bucket
            ctx.emitter.instruction("xor r8, r8");                              // refcounted hash values use only the low payload word
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor rcx, rcx");                            // null hash values use a zero low payload word
            ctx.emitter.instruction("xor r8, r8");                              // null hash values use a zero high payload word
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "assoc array default element PHP type {:?}",
            other
        ))),
    }
}

/// Materializes the current concrete literal value for Mixed-capable AArch64 hash storage.
fn materialize_assoc_literal_concrete_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    actual_value_type: &PhpType,
) -> Result<()> {
    match actual_value_type.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("mov x3, xzr");                             // null Mixed hash values use a zero low payload word
            ctx.emitter.instruction("mov x4, xzr");                             // null Mixed hash values use a zero high payload word
            Ok(())
        }
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction("mov x3, x0");                              // pass the concrete scalar payload as the Mixed hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // concrete scalar Mixed values do not use the high payload word
            Ok(())
        }
        PhpType::Float => {
            ctx.emitter.instruction("fmov x3, d0");                             // pass the concrete float bits as the Mixed hash value low word
            ctx.emitter.instruction("mov x4, xzr");                             // concrete float Mixed values do not use the high payload word
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov x3, x1");                              // pass the persistent string pointer as the Mixed hash value low word
            ctx.emitter.instruction("mov x4, x2");                              // pass the string length as the Mixed hash value high word
            Ok(())
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            ctx.emitter.instruction("mov x3, x0");                              // transfer the owned nested refcounted payload into Mixed-capable hash storage
            ctx.emitter.instruction("mov x4, xzr");                             // refcounted Mixed hash values use only the low payload word
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "assoc array default Mixed element PHP type {:?}",
            other
        ))),
    }
}

/// Materializes the current concrete literal value for Mixed-capable x86_64 hash storage.
fn materialize_assoc_literal_concrete_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    actual_value_type: &PhpType,
) -> Result<()> {
    match actual_value_type.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            ctx.emitter.instruction("xor rcx, rcx");                            // null Mixed hash values use a zero low payload word
            ctx.emitter.instruction("xor r8, r8");                              // null Mixed hash values use a zero high payload word
            Ok(())
        }
        PhpType::Int | PhpType::Bool => {
            ctx.emitter.instruction("mov rcx, rax");                            // pass the concrete scalar payload as the Mixed hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // concrete scalar Mixed values do not use the high payload word
            Ok(())
        }
        PhpType::Float => {
            ctx.emitter.instruction("movq rcx, xmm0");                          // pass the concrete float bits as the Mixed hash value low word
            ctx.emitter.instruction("xor r8, r8");                              // concrete float Mixed values do not use the high payload word
            Ok(())
        }
        PhpType::Str => {
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            ctx.emitter.instruction("mov rcx, rax");                            // pass the persistent string pointer as the Mixed hash value low word
            ctx.emitter.instruction("mov r8, rdx");                             // pass the string length as the Mixed hash value high word
            Ok(())
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_) => {
            ctx.emitter.instruction("mov rcx, rax");                            // transfer the owned nested refcounted payload into Mixed-capable hash storage
            ctx.emitter.instruction("xor r8, r8");                              // refcounted Mixed hash values use only the low payload word
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "assoc array default Mixed element PHP type {:?}",
            other
        ))),
    }
}

/// Returns the runtime value tag to pass for one literal hash insertion. A pointer hash entry
/// keeps its raw word under `POINTER_ARRAY_VALUE_TYPE`, as `hash_set` stores it.
fn assoc_literal_value_tag(storage_value_type: &PhpType, actual_value_type: &PhpType) -> i64 {
    if matches!(storage_value_type.codegen_repr(), PhpType::Mixed | PhpType::Iterable) {
        runtime_value_tag(&actual_value_type.codegen_repr()) as i64
    } else {
        crate::codegen::container_value_tag(&storage_value_type.codegen_repr())
    }
}

/// Returns the runtime slot width for an indexed-array default.
fn array_element_size(elem_type: &PhpType) -> Result<i64> {
    match elem_type.codegen_repr() {
        PhpType::Str | PhpType::Never => Ok(16),
        PhpType::Int
        | PhpType::Bool
        | PhpType::Float
        | PhpType::Mixed
        | PhpType::Object(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Union(_)
        | PhpType::Iterable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Void => Ok(8),
        other => Err(CodegenIrError::unsupported(format!(
            "array default element PHP type {:?}",
            other
        ))),
    }
}

/// Evaluates an integer constant expression built from integer literals with php's integer
/// operators, or returns `None` when any operand is not an integer literal or the operation
/// would not stay an integer (overflow, a negative or oversized shift, division).
fn fold_int_constant_expr(expr: &ExprKind) -> Option<i64> {
    use crate::parser::ast::BinOp;
    match expr {
        ExprKind::IntLiteral(value) => Some(*value),
        ExprKind::Negate(inner) => fold_int_constant_expr(&inner.kind)?.checked_neg(),
        ExprKind::BitNot(inner) => Some(!fold_int_constant_expr(&inner.kind)?),
        ExprKind::BinaryOp { left, op, right } => {
            let left = fold_int_constant_expr(&left.kind)?;
            let right = fold_int_constant_expr(&right.kind)?;
            match op {
                BinOp::BitOr => Some(left | right),
                BinOp::BitAnd => Some(left & right),
                BinOp::BitXor => Some(left ^ right),
                BinOp::Add => left.checked_add(right),
                BinOp::Sub => left.checked_sub(right),
                BinOp::Mul => left.checked_mul(right),
                BinOp::ShiftLeft if (0..64).contains(&right) => Some(left << right),
                BinOp::ShiftRight if (0..64).contains(&right) => Some(left >> right),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Folds `[...] + [...]` over array literals into the one keyed literal php builds when the
/// class is linked: the left operand's entries in order, then each right entry whose key the
/// left does not already hold. Symfony's `CurlHttpClient::$emptyDefaults` is
/// `self::OPTIONS_DEFAULTS + ['auth_ntlm' => null]`; refusing it sent the whole class to the
/// interpreter. Only string and integer keys are folded, normalized the way php normalizes them
/// (`"8"` is the integer key 8); any other key leaves the expression alone.
fn fold_array_union_literal(expr: &ExprKind) -> Option<ExprKind> {
    use crate::parser::ast::BinOp;
    let ExprKind::BinaryOp {
        left,
        op: BinOp::Add,
        right,
    } = expr
    else {
        return None;
    };
    let left = literal_array_entries(&left.kind)?;
    let right = literal_array_entries(&right.kind)?;
    let mut seen: crate::fast_hash::FastSet<LiteralArrayKey> =
        left.iter().map(|(key, _)| key.clone()).collect();
    let mut entries = Vec::with_capacity(left.len() + right.len());
    entries.extend(left);
    for (key, value) in right {
        if seen.insert(key.clone()) {
            entries.push((key, value));
        }
    }
    Some(ExprKind::ArrayLiteralAssoc(
        entries
            .into_iter()
            .map(|(key, value)| {
                let span = value.span;
                let key = match key {
                    LiteralArrayKey::Int(index) => ExprKind::IntLiteral(index),
                    LiteralArrayKey::Str(name) => ExprKind::StringLiteral(name),
                };
                (crate::parser::ast::Expr::new(key, span), value)
            })
            .collect(),
    ))
}

/// The `(normalized key, value)` entries of an array literal operand of `+`, nested unions
/// included, or `None` when a key is not a plain string or integer literal.
fn literal_array_entries(
    expr: &ExprKind,
) -> Option<Vec<(LiteralArrayKey, crate::parser::ast::Expr)>> {
    match expr {
        ExprKind::ArrayLiteral(items) => {
            if items
                .iter()
                .any(|item| matches!(item.kind, ExprKind::Spread(_)))
            {
                return None;
            }
            Some(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| (LiteralArrayKey::Int(index as i64), item.clone()))
                    .collect(),
            )
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            let mut entries: Vec<(LiteralArrayKey, crate::parser::ast::Expr)> = Vec::new();
            for (key, value) in items {
                let key = match &key.kind {
                    ExprKind::IntLiteral(index) => LiteralArrayKey::Int(*index),
                    ExprKind::StringLiteral(name) => match canonical_int_key(name) {
                        Some(index) => LiteralArrayKey::Int(index),
                        None => LiteralArrayKey::Str(name.clone()),
                    },
                    _ => return None,
                };
                // A repeated key inside one literal keeps its LAST value at its FIRST position.
                if let Some(slot) = entries.iter_mut().find(|(seen, _)| *seen == key) {
                    slot.1 = value.clone();
                } else {
                    entries.push((key, value.clone()));
                }
            }
            Some(entries)
        }
        ExprKind::BinaryOp { .. } => match fold_array_union_literal(expr)? {
            folded @ ExprKind::ArrayLiteralAssoc(_) => literal_array_entries(&folded),
            _ => None,
        },
        _ => None,
    }
}

/// php's integer-string key rule: a decimal integer without leading zeros or `+` is an int key.
fn canonical_int_key(name: &str) -> Option<i64> {
    let value: i64 = name.parse().ok()?;
    (value.to_string() == name).then_some(value)
}

/// Folds `Foo::class` with a named receiver into the string literal it always is: the name
/// as written, already resolved against the file's namespace and imports, with no leading
/// `\`. `self::class`, `parent::class` and `static::class` depend on the class they appear in
/// and stay as they are. Shared with the Reflection default metadata, so a folded default
/// reflects as the same string it stores.
pub(crate) fn fold_named_class_constant(expr: &ExprKind) -> Option<ExprKind> {
    match expr {
        ExprKind::ClassConstant {
            receiver: crate::parser::ast::StaticReceiver::Named(name),
        } => Some(ExprKind::StringLiteral(
            crate::names::class_name_literal(name.as_str()),
        )),
        _ => None,
    }
}

/// Builds the unsupported-feature error for default forms outside this slice.
fn unsupported_literal_default(
    context: &str,
    php_type: &PhpType,
    op_name: &str,
) -> CodegenIrError {
    CodegenIrError::unsupported(format!(
        "{} for default value of {} with PHP type {:?}",
        op_name, context, php_type
    ))
}
