//! Purpose:
//! php-src's CSV record reader and writer, shared by eval `fgetcsv()`, `str_getcsv()` and
//! `fputcsv()`.
//!
//! Called from:
//! - `super::fgetcsv`, `super::fputcsv` and `crate::interpreter::builtins::string::str_getcsv`.
//!
//! Key details:
//! - `eval_php_fgetcsv` is a line-for-line port of `php_fgetcsv()` (ext/standard/file.c,
//!   php 8.5) for the single-byte locale `php -n` runs in, where `php_mblen()` answers 1 for
//!   every byte. Each earlier hand-written parser disagreed with php somewhere: an escape
//!   byte is KEPT (`"a\"b"` is `a\"b`), whitespace before an enclosure is skipped, text after
//!   a closing enclosure is appended, and unquoted fields lose trailing line terminators only.
//! - A quoted field that runs past its line pulls the next line from `next_line`; `str_getcsv()`
//!   passes none, so its record ends at the end of the subject, as php's NULL stream does.
//! - An escape of `None` is php's `PHP_CSV_NO_ESCAPE` (an empty `$escape`).

use super::super::super::*;
use super::*;

/// Parses one CSV record from `first`, reading continuation lines while a quoted field is open.
///
/// Returns `None` for php's BLANK LINE (nothing but a line terminator), which the callers turn
/// into the single-element `[null]` array `php_bc_fgetcsv_empty_line()` builds.
pub(in crate::interpreter) fn eval_php_fgetcsv(
    first: Vec<u8>,
    delimiter: u8,
    enclosure: u8,
    escape: Option<u8>,
    mut next_line: impl FnMut() -> Option<Vec<u8>>,
) -> Option<Vec<Vec<u8>>> {
    let mut buf = first;
    // php reads the NUL terminator past the buffer; `at` stands for that read.
    let at = |buf: &[u8], index: usize| buf.get(index).copied().unwrap_or(0);
    let mut bptr = 0_usize;
    let mut limit = lookup_trailing_spaces(&buf);
    let mut line_end = limit;
    let mut line_end_len = buf.len() - limit;
    let mut values = Vec::new();
    let mut first_field = true;
    loop {
        let mut temp = Vec::new();
        let mut inc = usize::from(bptr < limit);
        if inc == 1 {
            let mut tmp = bptr;
            while at(&buf, tmp) != delimiter && is_c_space(at(&buf, tmp)) {
                tmp += 1;
            }
            if at(&buf, tmp) == enclosure && tmp < limit {
                bptr = tmp;
            }
        }
        if first_field && bptr == line_end {
            return None;
        }
        first_field = false;
        if inc != 0 && at(&buf, bptr) == enclosure {
            // 2A. An enclosure-delimited field.
            let mut state = 0;
            bptr += 1;
            let mut hunk = bptr;
            loop {
                if inc == 0 {
                    if state == 2 {
                        temp.extend_from_slice(&buf[hunk..bptr - 1]);
                        hunk = bptr;
                        break;
                    }
                    if state == 1 {
                        temp.extend_from_slice(&buf[hunk..bptr]);
                        hunk = bptr;
                    }
                    if hunk != line_end {
                        temp.extend_from_slice(&buf[hunk..bptr]);
                        hunk = bptr;
                    }
                    // The embedded line end belongs to the field.
                    temp.extend_from_slice(&buf[line_end..line_end + line_end_len]);
                    let Some(new_buf) = next_line() else {
                        // An unterminated enclosure: everything read so far is the last field.
                        break;
                    };
                    buf = new_buf;
                    bptr = 0;
                    hunk = 0;
                    limit = lookup_trailing_spaces(&buf);
                    line_end = limit;
                    line_end_len = buf.len() - limit;
                    state = 0;
                } else {
                    match state {
                        1 => {
                            // The escaped byte stays in the field, escape and all.
                            bptr += 1;
                            state = 0;
                        }
                        2 => {
                            if at(&buf, bptr) != enclosure {
                                // A real closing enclosure.
                                temp.extend_from_slice(&buf[hunk..bptr - 1]);
                                hunk = bptr;
                                break;
                            }
                            // A doubled enclosure is one literal enclosure.
                            temp.extend_from_slice(&buf[hunk..bptr]);
                            bptr += 1;
                            hunk = bptr;
                            state = 0;
                        }
                        _ => {
                            let byte = at(&buf, bptr);
                            if byte == enclosure {
                                state = 2;
                            } else if escape == Some(byte) {
                                state = 1;
                            }
                            bptr += 1;
                        }
                    }
                }
                inc = usize::from(bptr < limit);
            }
            // Whatever follows the closing enclosure, up to the delimiter, is kept too.
            while inc != 0 && at(&buf, bptr) != delimiter {
                bptr += 1;
                inc = usize::from(bptr < limit);
            }
            temp.extend_from_slice(&buf[hunk..bptr]);
            bptr += inc;
        } else {
            // 2B. A bare field.
            let hunk = bptr;
            while inc != 0 && at(&buf, bptr) != delimiter {
                bptr += 1;
                inc = usize::from(bptr < limit);
            }
            temp.extend_from_slice(&buf[hunk..bptr]);
            temp.truncate(lookup_trailing_spaces(&temp));
            if at(&buf, bptr) == delimiter {
                bptr += 1;
            }
        }
        values.push(temp);
        if inc == 0 {
            break;
        }
    }
    Some(values)
}

/// Formats one CSV record the way `php_fputcsv()` does.
///
/// A field is enclosed when it holds the delimiter, the enclosure, the escape byte, a newline,
/// a carriage return, a tab or a space. Inside it an enclosure is doubled unless an escape byte
/// directly precedes it. The record ends in `eol`, or `"\n"` when none is given.
pub(in crate::interpreter) fn eval_php_fputcsv(
    fields: &[Vec<u8>],
    delimiter: u8,
    enclosure: u8,
    escape: Option<u8>,
    eol: Option<&[u8]>,
) -> Vec<u8> {
    let mut output = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let needs_enclosure = field.iter().any(|&byte| {
            byte == delimiter
                || byte == enclosure
                || escape == Some(byte)
                || matches!(byte, b'\n' | b'\r' | b'\t' | b' ')
        });
        if needs_enclosure {
            output.push(enclosure);
            let mut escaped = false;
            for &byte in field {
                if escape == Some(byte) {
                    escaped = true;
                } else if !escaped && byte == enclosure {
                    output.push(enclosure);
                } else {
                    escaped = false;
                }
                output.push(byte);
            }
            output.push(enclosure);
        } else {
            output.extend_from_slice(field);
        }
        if index + 1 != fields.len() {
            output.push(delimiter);
        }
    }
    output.extend_from_slice(eol.unwrap_or(b"\n"));
    output
}

/// Resolves a CSV `$escape` argument as `php_csv_handle_escape_argument()` does.
///
/// An omitted argument is deprecated since php 8.4 and still means `\`; an empty one disables
/// escaping (`None`); anything longer than one byte is a `ValueError`.
pub(in crate::interpreter) fn eval_csv_escape_argument(
    value: Option<RuntimeCellHandle>,
    function: &'static str,
    position: usize,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<u8>, EvalStatus> {
    if value.is_none() {
        values.warning(&format!(
            "Deprecated: {}(): the $escape parameter must be provided as its default value \
             will change\n",
            function
        ))?;
    }
    let escape = eval_csv_control_byte(
        value,
        b'\\',
        CsvControlArgument {
            function,
            position,
            parameter: "escape",
            empty_allowed: true,
        },
        context,
        values,
    )?;
    Ok((escape != 0).then_some(escape))
}

/// Raises php's catchable `ValueError` with `text` as its message.
pub(in crate::interpreter) fn eval_csv_raise_value_error<T>(
    text: &str,
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<T, EvalStatus> {
    let exception = values.new_object("ValueError")?;
    let message = values.string(text)?;
    let code = values.int(0)?;
    values.construct_object(exception, vec![message, code])?;
    context.set_pending_throw(exception);
    Err(EvalStatus::UncaughtThrowable)
}

/// Builds the array a parsed record answers, or php's `[null]` for a blank line.
pub(in crate::interpreter) fn eval_csv_record_array(
    record: Option<Vec<Vec<u8>>>,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let Some(fields) = record else {
        let result = values.array_new(0)?;
        let key = values.int(0)?;
        let null = values.null()?;
        return values.array_set(result, key, null);
    };
    let mut result = values.array_new(fields.len())?;
    for (index, field) in fields.iter().enumerate() {
        result = super::scandir::eval_array_set_indexed_bytes(result, index, field, values)?;
    }
    Ok(result)
}

/// Answers where a line's trailing `\r\n`, `\n` or `\r` begins (`php_fgetcsv_lookup_trailing_spaces`).
fn lookup_trailing_spaces(bytes: &[u8]) -> usize {
    match bytes {
        [.., b'\r', b'\n'] => bytes.len() - 2,
        [.., b'\n'] | [.., b'\r'] => bytes.len() - 1,
        _ => bytes.len(),
    }
}

/// C's `isspace()` in the "C" locale.
fn is_c_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str, escape: Option<u8>) -> Option<Vec<String>> {
        eval_php_fgetcsv(line.as_bytes().to_vec(), b',', b'"', escape, || None).map(|fields| {
            fields
                .into_iter()
                .map(|field| String::from_utf8(field).unwrap())
                .collect()
        })
    }

    /// Verifies the cases php 8.5.6 answers, measured with `str_getcsv()` on `php -n`.
    #[test]
    fn eval_php_fgetcsv_matches_php_on_escapes_and_enclosures() {
        let esc = Some(b'\\');
        assert_eq!(parse("\"a\\\"b\",c", esc).unwrap(), ["a\\\"b", "c"]);
        assert_eq!(parse("\"a\\\"b\",c", None).unwrap(), ["a\\b\"", "c"]);
        assert_eq!(parse("\"a\\\\\"b\",c", esc).unwrap(), ["a\\\\b\"", "c"]);
        assert_eq!(parse("\"a\"\"b\",c", esc).unwrap(), ["a\"b", "c"]);
        assert_eq!(parse("x\"y,z", esc).unwrap(), ["x\"y", "z"]);
        assert_eq!(parse("\"ab\"cd,e", esc).unwrap(), ["abcd", "e"]);
        assert_eq!(parse("\"a", esc).unwrap(), ["a"]);
        assert_eq!(parse(" \"a\",b", esc).unwrap(), ["a", "b"]);
        assert_eq!(parse("a,", esc).unwrap(), ["a", ""]);
        assert_eq!(parse("\n", esc), None);
        assert_eq!(parse("", esc), None);
        assert_eq!(parse("   \n", esc).unwrap(), ["   "]);
    }

    /// Verifies an open enclosure pulls the next line and keeps the embedded line end.
    #[test]
    fn eval_php_fgetcsv_continues_an_open_enclosure_across_lines() {
        let mut rest = vec![b"b\",c\n".to_vec()].into_iter();
        let fields = eval_php_fgetcsv(b"\"a\n".to_vec(), b',', b'"', Some(b'\\'), || rest.next())
            .unwrap();
        assert_eq!(fields, [b"a\nb".to_vec(), b"c".to_vec()]);
    }

    /// Verifies php's enclosure rule and the escape byte that suppresses doubling.
    #[test]
    fn eval_php_fputcsv_matches_php_quoting() {
        let fields = [b"a b".to_vec(), b"x".to_vec(), b"q\"r".to_vec(), b"e\\\"f".to_vec()];
        assert_eq!(
            eval_php_fputcsv(&fields, b',', b'"', Some(b'\\'), None),
            b"\"a b\",x,\"q\"\"r\",\"e\\\"f\"\n".to_vec()
        );
        assert_eq!(
            eval_php_fputcsv(&fields, b',', b'"', None, Some(b"\r\n")),
            b"\"a b\",x,\"q\"\"r\",\"e\\\"\"f\"\r\n".to_vec()
        );
    }
}
