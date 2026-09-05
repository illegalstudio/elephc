//! Purpose:
//! Declarative eval registry entry and implementation for PHP's `mb_strtolower()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string` and the declarative direct/values hooks.
//!
//! Key details:
//! - The eval signature matches PHP's nullable optional `$encoding` parameter.
//! - Omitted/null/`UTF-8`/`UTF8` use PHP 8.5 full Unicode lowercase, including the
//!   language-agnostic Final_Sigma rule and 1:N mappings such as `İ` → `i` + combining dot.
//! - `8bit`/`binary`/`7bit` lowercase ASCII `A-Z` per byte; unknown encodings raise
//!   a catchable `ValueError`.

use super::super::super::*;

eval_builtin! {
    contract: "mb_strtolower",
    area: String,
    direct: MbStrtolower,
    values: MbStrtolower,
}

const GREEK_CAPITAL_SIGMA: u32 = 0x03A3;
const GREEK_SMALL_SIGMA: u32 = 0x03C3;
const GREEK_SMALL_FINAL_SIGMA: u32 = 0x03C2;

/// Evaluates direct `mb_strtolower()` calls while preserving PHP source-order argument evaluation.
pub(in crate::interpreter) fn eval_builtin_mb_strtolower(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    match args {
        [value] => {
            let value = eval_expr(value, context, scope, values)?;
            eval_mb_strtolower_result(value, None, context, values)
        }
        [value, encoding] => {
            let value = eval_expr(value, context, scope, values)?;
            let encoding = eval_expr(encoding, context, scope, values)?;
            eval_mb_strtolower_result(value, Some(encoding), context, values)
        }
        _ => Err(EvalStatus::RuntimeFatal),
    }
}

/// Lowercases one materialized eval string with PHP-compatible encoding selection.
pub(in crate::interpreter) fn eval_mb_strtolower_result(
    value: RuntimeCellHandle,
    encoding: Option<RuntimeCellHandle>,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let bytes = values.string_bytes(value)?;
    let encoding = match encoding {
        Some(encoding) if !values.is_null(encoding)? => Some(values.string_bytes(encoding)?),
        _ => None,
    };
    let lowered = match encoding.as_deref() {
        None => utf8_lowercase(&bytes),
        Some(encoding) if is_utf8_encoding(encoding) => utf8_lowercase(&bytes),
        Some(encoding) if is_byte_encoding(encoding) => ascii_byte_lowercase(&bytes),
        Some(encoding) => return eval_mb_strtolower_encoding_error(encoding, context, values),
    };
    values.string_bytes_value(&lowered)
}

/// Returns whether `$encoding` is PHP's default UTF-8 name or its `UTF8` alias.
fn is_utf8_encoding(encoding: &[u8]) -> bool {
    encoding.eq_ignore_ascii_case(b"UTF-8") || encoding.eq_ignore_ascii_case(b"UTF8")
}

/// Returns whether `$encoding` is a single-byte alias that lowercases ASCII `A-Z` only.
fn is_byte_encoding(encoding: &[u8]) -> bool {
    encoding.eq_ignore_ascii_case(b"8bit")
        || encoding.eq_ignore_ascii_case(b"binary")
        || encoding.eq_ignore_ascii_case(b"7bit")
}

/// Lowercases ASCII `A-Z` in each byte, leaving every other byte unchanged.
fn ascii_byte_lowercase(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .map(|byte| {
            if byte.is_ascii_uppercase() {
                byte + (b'a' - b'A')
            } else {
                *byte
            }
        })
        .collect()
}

/// Applies PHP 8.5 UTF-8 full lowercase mapping, preserving malformed byte groups.
fn utf8_lowercase(bytes: &[u8]) -> Vec<u8> {
    let tokens = tokenize_utf8(bytes);
    let mut out = Vec::with_capacity(bytes.len());
    for (index, token) in tokens.iter().enumerate() {
        match token {
            Utf8Token::Invalid(invalid) => out.extend_from_slice(invalid),
            Utf8Token::Scalar(ch) => {
                let mapped = if *ch as u32 == GREEK_CAPITAL_SIGMA {
                    char::from_u32(lower_capital_sigma(&tokens, index)).expect("sigma scalar")
                        .encode_utf8(&mut [0; 4])
                        .as_bytes()
                        .to_vec()
                } else {
                    ch.to_lowercase()
                        .collect::<String>()
                        .into_bytes()
                };
                out.extend_from_slice(&mapped);
            }
        }
    }
    out
}

/// One UTF-8 scalar or one malformed/truncated byte group.
enum Utf8Token {
    Scalar(char),
    Invalid(Vec<u8>),
}

/// Splits `bytes` into valid Unicode scalars and mbstring-style invalid groups.
fn tokenize_utf8(bytes: &[u8]) -> Vec<Utf8Token> {
    let mut tokens = Vec::new();
    let mut offset = 0usize;
    while offset < bytes.len() {
        match std::str::from_utf8(&bytes[offset..]) {
            Ok(valid) => {
                tokens.extend(valid.chars().map(Utf8Token::Scalar));
                break;
            }
            Err(error) => {
                let valid_len = error.valid_up_to();
                let valid = std::str::from_utf8(&bytes[offset..offset + valid_len])
                    .expect("from_utf8 valid prefix");
                tokens.extend(valid.chars().map(Utf8Token::Scalar));
                match error.error_len() {
                    Some(invalid_len) => {
                        tokens.push(Utf8Token::Invalid(
                            bytes[offset + valid_len..offset + valid_len + invalid_len].to_vec(),
                        ));
                        offset += valid_len + invalid_len;
                    }
                    None => {
                        tokens.push(Utf8Token::Invalid(bytes[offset + valid_len..].to_vec()));
                        break;
                    }
                }
            }
        }
    }
    tokens
}

/// Applies Unicode Final_Sigma: end-of-word capital sigma becomes `ς`, otherwise `σ`.
fn lower_capital_sigma(tokens: &[Utf8Token], index: usize) -> u32 {
    if preceding_cased_letter(tokens, index) && !following_cased_letter(tokens, index) {
        GREEK_SMALL_FINAL_SIGMA
    } else {
        GREEK_SMALL_SIGMA
    }
}

/// Returns whether a cased letter occurs before `index` after skipping Case_Ignorable tokens.
fn preceding_cased_letter(tokens: &[Utf8Token], index: usize) -> bool {
    for token in tokens[..index].iter().rev() {
        match token {
            Utf8Token::Invalid(_) => return false,
            Utf8Token::Scalar(ch) if is_case_ignorable(*ch) => continue,
            Utf8Token::Scalar(ch) => return is_cased(*ch),
        }
    }
    false
}

/// Returns whether a cased letter occurs after `index` after skipping Case_Ignorable tokens.
fn following_cased_letter(tokens: &[Utf8Token], index: usize) -> bool {
    for token in tokens[index + 1..].iter() {
        match token {
            Utf8Token::Invalid(_) => return false,
            Utf8Token::Scalar(ch) if is_case_ignorable(*ch) => continue,
            Utf8Token::Scalar(ch) => return is_cased(*ch),
        }
    }
    false
}

/// Returns whether `ch` has a Unicode case mapping in either direction.
fn is_cased(ch: char) -> bool {
    let as_string = ch.to_string();
    ch.to_lowercase().to_string() != as_string || ch.to_uppercase().to_string() != as_string
}

/// Returns whether `ch` is in a Case_Ignorable class used by Final_Sigma scanning.
fn is_case_ignorable(ch: char) -> bool {
    matches!(
        ch,
        '\u{00AD}'
            | '\u{0300}'..='\u{036F}'
            | '\u{0483}'..='\u{0489}'
            | '\u{0591}'..='\u{05BD}'
            | '\u{05BF}'
            | '\u{05C1}'..='\u{05C2}'
            | '\u{05C4}'..='\u{05C5}'
            | '\u{05C7}'
            | '\u{0610}'..='\u{061A}'
            | '\u{064B}'..='\u{065F}'
            | '\u{0670}'
            | '\u{06D6}'..='\u{06DC}'
            | '\u{06DF}'..='\u{06E4}'
            | '\u{06E7}'..='\u{06E8}'
            | '\u{06EA}'..='\u{06ED}'
            | '\u{0711}'
            | '\u{0730}'..='\u{074A}'
            | '\u{07A6}'..='\u{07B0}'
            | '\u{07EB}'..='\u{07F3}'
            | '\u{07FD}'
            | '\u{0816}'..='\u{0819}'
            | '\u{081B}'..='\u{0823}'
            | '\u{0825}'..='\u{0827}'
            | '\u{0829}'..='\u{082D}'
            | '\u{0859}'..='\u{085B}'
            | '\u{0898}'..='\u{089F}'
            | '\u{08CA}'..='\u{08E1}'
            | '\u{08E3}'..='\u{0902}'
            | '\u{093A}'
            | '\u{093C}'
            | '\u{0941}'..='\u{0948}'
            | '\u{094D}'
            | '\u{0951}'..='\u{0957}'
            | '\u{0962}'..='\u{0963}'
            | '\u{1AB0}'..='\u{1ADE}'
            | '\u{1AE0}'..='\u{1AEB}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}'
            | '\u{20D0}'..='\u{20F0}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{FEFF}'
            | '\u{FF9E}'..='\u{FF9F}'
    )
}

/// Raises PHP's catchable `ValueError` for an encoding name rejected by the runtime.
fn eval_mb_strtolower_encoding_error<T>(
    encoding: &[u8],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let encoding = String::from_utf8_lossy(encoding);
    let message = format!(
        "mb_strtolower(): Argument #2 ($encoding) must be a valid encoding, \"{}\" given",
        encoding
    );
    let exception = values.new_object("ValueError")?;
    let message = values.string(&message)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}
