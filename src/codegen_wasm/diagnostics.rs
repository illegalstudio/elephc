//! Purpose:
//! Rewrites the compiler-internal spellings that leak into wasm32-wasi refusal messages into
//! the PHP terms a user can act on: EIR storage/type dumps (`Heap(Mixed)/Mixed`,
//! `I64/Int`, `Object("A")`) become `mixed`, `int`, `A`, and EIR opcode names that a PHP
//! construct lowers to get a construct description beside them.
//!
//! Called from:
//! - `WasmError`'s `Display` impl in `crate::codegen_wasm` for every `Unsupported` message.
//! - `crate::codegen_wasm::capability` when it reports an unsupported opcode.
//!
//! Key details:
//! - The rewrite is purely textual and conservative: only the exact `<IrType>/<PhpType>` pair
//!   shape and the `Heap(<kind>)` / `TaggedScalar` tokens are touched, on word boundaries, so
//!   ordinary prose and PHP identifiers pass through unchanged.
//! - A `PhpType` debug token is an identifier plus an optional balanced `(...)` or `{...}`
//!   payload; anything it does not recognise is left as written rather than guessed at.

use crate::ir::Op;

/// Returns the PHP spelling of one `PhpType` debug token (`Int`, `Object("A")`, `Array(Str)`),
/// or `None` for a token this module does not recognise.
fn php_type_spelling(token: &str) -> Option<String> {
    let name_end = token
        .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .unwrap_or(token.len());
    let (name, payload) = token.split_at(name_end);
    let spelled = match name {
        "Int" => "int".to_string(),
        "Float" => "float".to_string(),
        "Str" => "string".to_string(),
        "Bool" => "bool".to_string(),
        "False" => "false".to_string(),
        "Void" => "null".to_string(),
        "Never" => "never".to_string(),
        "Iterable" => "iterable".to_string(),
        "Mixed" => "mixed".to_string(),
        "Callable" => "callable".to_string(),
        "TaggedScalar" => "int|null".to_string(),
        "Array" | "AssocArray" => "array".to_string(),
        "Union" => "union".to_string(),
        "Buffer" => "buffer".to_string(),
        "Pointer" => "ptr".to_string(),
        "Resource" => "resource".to_string(),
        "Packed" => {
            let class = payload.trim_matches(|ch| matches!(ch, '(' | ')' | '"'));
            format!("packed {class}")
        }
        "Object" => {
            let class = payload.trim_matches(|ch| matches!(ch, '(' | ')' | '"'));
            if class.is_empty() {
                "object".to_string()
            } else {
                class.to_string()
            }
        }
        _ => return None,
    };
    Some(spelled)
}

/// Returns the PHP spelling of an `IrHeapKind` debug name, or `None` when it is not one.
fn heap_kind_spelling(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "Array" | "Hash" => "array",
        "Object" => "object",
        "Mixed" => "mixed",
        "Iterable" => "iterable",
        "Union" => "union",
        "Buffer" => "buffer",
        _ => return None,
    })
}

/// Length in bytes of the token starting at `text`: an identifier plus an optional balanced
/// `(...)` or `{...}` payload. Returns 0 when `text` does not start with an identifier.
fn token_len(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut end = 0;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    if end == 0 {
        return 0;
    }
    // Rust's struct-variant Debug prints `Name { field: .. }` with a space before the brace.
    if end + 1 < bytes.len() && bytes[end] == b' ' && bytes[end + 1] == b'{' {
        end += 1;
    }
    if end < bytes.len() && matches!(bytes[end], b'(' | b'{') {
        let mut depth = 0usize;
        let mut in_string = false;
        for (offset, byte) in bytes[end..].iter().enumerate() {
            match *byte {
                b'"' => in_string = !in_string,
                b'(' | b'{' if !in_string => depth += 1,
                b')' | b'}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return end + offset + 1;
                    }
                }
                _ => {}
            }
        }
        // Unbalanced payload: leave the whole thing alone rather than guess where it ends.
        return 0;
    }
    end
}

/// Rewrites the EIR type dumps in `text` into PHP type names.
///
/// `Heap(Mixed)/Mixed` becomes `mixed`, `I64/Int` becomes `int`, `Heap(Object)/Object("A")`
/// becomes `A`, a lone `Heap(Array)` becomes `array` and a lone `TaggedScalar` becomes
/// `int|null`. Text that is not one of those shapes is copied through unchanged.
pub(crate) fn humanize_type_spellings(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        let rest = &text[index..];
        let at_boundary = text[..index]
            .chars()
            .next_back()
            .map_or(true, |ch| !(ch.is_ascii_alphanumeric() || ch == '_'));
        if at_boundary {
            if let Some((spelled, consumed)) = rewrite_at(rest) {
                out.push_str(&spelled);
                index += consumed;
                continue;
            }
        }
        let ch = rest.chars().next().expect("index is inside the text");
        out.push(ch);
        index += ch.len_utf8();
    }
    out
}

/// Whether `token` is an EIR storage type (`I64`, `Heap(Mixed)`, ...) rather than a PHP one.
fn is_ir_token(token: &str) -> bool {
    matches!(token, "I64" | "F64" | "Str" | "Void" | "TaggedScalar")
        || (token.starts_with("Heap(") && token.ends_with(')'))
}

/// Tries to rewrite the type dump that starts `rest`, answering the replacement and the bytes
/// it covers. `None` leaves the character to be copied through.
fn rewrite_at(rest: &str) -> Option<(String, usize)> {
    let first_len = token_len(rest);
    if first_len == 0 {
        return None;
    }
    let first = &rest[..first_len];
    // The wasm value representation (`Ptr("")`) carries no PHP-level information; drop it
    // together with the space that separated it from the type it annotates.
    if first.starts_with("Ptr(") {
        let skip = usize::from(rest[first_len..].starts_with(' '));
        return Some((String::new(), first_len + skip));
    }
    // `<A>/<B>` is one value's two spellings (storage type and PHP type, in either order).
    if rest[first_len..].starts_with('/') {
        let second_len = token_len(&rest[first_len + 1..]);
        if second_len > 0 {
            let second = &rest[first_len + 1..first_len + 1 + second_len];
            let spelled = if (is_ir_token(first) && !is_ir_token(second)) || first == second {
                php_type_spelling(second)
            } else if is_ir_token(second) {
                php_type_spelling(first)
            } else {
                None
            }
            .or_else(|| {
                // Storage-only pairs such as `Heap(Hash)/Heap(Array)` keep the first kind.
                if is_ir_token(first) && is_ir_token(second) {
                    lone_ir_spelling(first)
                } else {
                    None
                }
            });
            if let Some(spelled) = spelled {
                return Some((spelled, first_len + 1 + second_len));
            }
        }
    }
    // A class type is distinctive enough to rewrite without its storage twin beside it.
    if first.starts_with("Object(\"") || first.starts_with("AssocArray {") {
        return php_type_spelling(first).map(|spelled| (spelled, first_len));
    }
    lone_ir_spelling(first).map(|spelled| (spelled, first_len))
}

/// The PHP spelling of an EIR storage token standing alone: a heap kind or the tagged scalar.
/// Plain `I64`/`F64`/`Str` are not rewritten on their own, since without a PHP type beside
/// them they could be ordinary words.
fn lone_ir_spelling(token: &str) -> Option<String> {
    if token == "TaggedScalar" {
        return Some("int|null".to_string());
    }
    let kind = token.strip_prefix("Heap(")?.strip_suffix(')')?;
    heap_kind_spelling(kind).map(str::to_string)
}

/// Returns the PHP construct an unsupported EIR opcode comes from, when the mapping is certain.
///
/// Only opcodes whose source construct is unambiguous are listed; the rest keep their bare
/// opcode name in the message, which still identifies the defect for a report.
pub(crate) fn unsupported_op_construct(op: Op) -> Option<&'static str> {
    Some(match op {
        Op::DynamicObjectNewMixed | Op::DynamicObjectNewWithoutConstructorMixed => {
            "`new $class(...)` with a class name computed at run time"
        }
        Op::InvokerRefArg => "passing a by-reference argument through a call the compiler cannot resolve statically",
        Op::DescriptorArgSet | Op::DescriptorArgKeyExists | Op::ThrowNamedParameterOverwrite => {
            "named arguments or a spread (`...$args`) on a call through a callable value"
        }
        Op::ClosureBind => "`Closure::bind()` / `bindTo()` / `call()`",
        Op::PipeCall => "the pipe operator `|>`",
        Op::FiberRuntimeCall => "a `Fiber`",
        Op::EvalLiteralCall
        | Op::EvalScopeGet
        | Op::EvalScopeSet
        | Op::EvalFunctionCall
        | Op::EvalFunctionCallArray
        | Op::EvalFunctionExists
        | Op::EvalClassExists
        | Op::EvalConstantExists
        | Op::EvalConstantFetch
        | Op::EvalObjectNew
        | Op::EvalStaticMethodCall => "`eval()`",
        Op::ExternCall | Op::ExternGlobalLoad | Op::ExternGlobalStore => "an `extern` declaration",
        Op::PtrCast
        | Op::PtrRead
        | Op::PtrWrite
        | Op::PtrReadString
        | Op::PtrWriteString
        | Op::PtrOffset
        | Op::PtrCheckNonnull => "the raw pointer builtins (`ptr_*`)",
        Op::BufferNew | Op::BufferLen | Op::BufferGet | Op::BufferSet | Op::BufferFree => {
            "a `buffer<T>`"
        }
        Op::PackedFieldGet | Op::PackedFieldSet | Op::PackedFieldMixedToInt => {
            "a `packed class` field"
        }
        Op::GcControl => {
            "the cycle-collector controls (`gc_collect_cycles()`, `gc_enable()`, `gc_disable()`, \
             `gc_enabled()`, `gc_status()`, `gc_mem_caches()`)"
        }
        _ => return None,
    })
}

/// Formats the audit line for an unsupported opcode: the PHP construct when known, and always
/// the opcode name so a bug report can still quote it.
pub(crate) fn describe_unsupported_op(op: Op) -> String {
    match unsupported_op_construct(op) {
        Some(construct) => format!("unsupported construct: {construct} (EIR op {})", op.name()),
        None => format!("unsupported op {}", op.name()),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the rewrite of internal type spellings and opcode names in refusal messages.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The end-to-end shape (a real refusal reaching the user) is pinned by the
    //!   `test_cli_wasm_refusal_*` tests in `tests/codegen/cli.rs`.

    use super::*;

    /// Type pairs, lone heap kinds and the tagged scalar are rewritten to PHP spellings.
    #[test]
    fn rewrites_type_dumps_to_php_names() {
        assert_eq!(humanize_type_spellings("got Heap(Mixed)/Mixed"), "got mixed");
        assert_eq!(humanize_type_spellings("is I64/Int, not Str/Str"), "is int, not string");
        assert_eq!(
            humanize_type_spellings("foreach got Heap(Object)/Object(\"A\")"),
            "foreach got A"
        );
        assert_eq!(
            humanize_type_spellings("return Heap(Mixed)/Mixed differs from signature Object(\"Generator\")"),
            "return mixed differs from signature Generator"
        );
        assert_eq!(humanize_type_spellings("operand #1 is TaggedScalar, not an int"), "operand #1 is int|null, not an int");
        assert_eq!(humanize_type_spellings("result Heap(Array) not Heap(Hash)"), "result array not array");
        assert_eq!(
            humanize_type_spellings("Heap(Array)/Array(Union([Int, Str]))"),
            "array"
        );
        assert_eq!(
            humanize_type_spellings(
                "from Ptr(\"\") (AssocArray { key: Int, value: Mixed }/Heap(Hash)) to Ptr(\"\") (Array(Int)/Heap(Array))"
            ),
            "from (array) to (array)"
        );
        assert_eq!(
            humanize_type_spellings("a yielded TaggedScalar/TaggedScalar has no boxing"),
            "a yielded int|null has no boxing"
        );
    }

    /// `Display` keeps the internal spellings the backend's tests quote; `user_message` is the
    /// rewritten form the compiler prints.
    #[test]
    fn user_message_rewrites_while_display_keeps_internals() {
        let error = crate::codegen_wasm::WasmError::Unsupported(
            "bcadd scale is Heap(Mixed)/Mixed".to_string(),
        );
        assert!(error.to_string().contains("Heap(Mixed)/Mixed"));
        assert_eq!(
            error.user_message(),
            "bcadd scale is mixed is not yet supported on the wasm32-wasi target"
        );
    }

    /// The generic suffix is added once, and only to a message that does not already say it is a
    /// refusal on this target: an audit list, or a message naming the target or "unsupported".
    #[test]
    fn refusal_suffix_is_never_repeated() {
        let unsupported = |text: &str| {
            crate::codegen_wasm::WasmError::Unsupported(text.to_string()).user_message()
        };
        assert_eq!(
            unsupported("property $p of type Callable not yet supported on wasm32-wasi"),
            "property $p of type Callable not yet supported on wasm32-wasi"
        );
        let audit = "WASM capability audit found 2 issue(s):\n  - a: unsupported op x\n  - b: unsupported op y";
        assert_eq!(unsupported(audit), audit);
        assert_eq!(
            unsupported("unsupported runtime function f shape: g"),
            "unsupported runtime function f shape: g"
        );
        assert_eq!(
            unsupported("generator delegation"),
            "generator delegation is not yet supported on the wasm32-wasi target"
        );
    }

    /// Prose, PHP identifiers and unrecognised payloads pass through unchanged.
    #[test]
    fn leaves_everything_else_alone() {
        for text in [
            "unsupported runtime function sprintf shape: format wants argument #2",
            "class Str64 and I64x stay",
            "Objects(\"A\") is not a type dump",
            "unbalanced Heap(Mixed",
            "Heap(Unknown) alone",
        ] {
            assert_eq!(humanize_type_spellings(text), text);
        }
    }

    /// A mapped opcode names the PHP construct; an unmapped one keeps its bare name.
    #[test]
    fn describes_unsupported_ops_by_php_construct() {
        let mapped = describe_unsupported_op(Op::DynamicObjectNewMixed);
        assert!(mapped.contains("new $class(...)"), "{mapped}");
        assert!(mapped.contains("dynamic_object_new_mixed"), "{mapped}");
        assert_eq!(describe_unsupported_op(Op::StrEq), "unsupported op str_eq");
        let gc = describe_unsupported_op(Op::GcControl);
        assert!(gc.contains("`gc_collect_cycles()`"), "{gc}");
    }
}
