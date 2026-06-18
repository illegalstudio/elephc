//! Purpose:
//! Dispatches already evaluated string, hash, encoding, and ctype builtins by dynamic callable name.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry::dispatch`.
//!
//! Key details:
//! - Returns `Ok(None)` for names outside this domain so the parent dispatcher can
//!   continue probing other builtin families.

use super::super::super::super::*;
use super::super::super::*;

/// Attempts to dispatch evaluated string, hash, encoding, and ctype builtins.
pub(in crate::interpreter) fn eval_strings_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "addslashes" | "stripslashes" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_slashes_result(name, *value, values)?
        }
        "base64_encode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_base64_encode_result(*value, values)?
        }
        "base64_decode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_base64_decode_result(*value, values)?
        }
        "bin2hex" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_bin2hex_result(*value, values)?
        }
        "chr" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chr_result(*value, values)?
        }
        "grapheme_strrev" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_grapheme_strrev_result(*value, values)?
        }
        "gzcompress" | "gzdeflate" | "gzinflate" | "gzuncompress" => {
            eval_gzip_result(name, evaluated_args, values)?
        }
        "rawurldecode" | "urldecode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_url_decode_result(name, *value, values)?
        }
        "rawurlencode" | "urlencode" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_url_encode_result(name, *value, values)?
        }
        "strrev" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.strrev(*value)?
        }
        "str_repeat" => {
            let [value, times] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_str_repeat_result(*value, *times, values)?
        }
        "str_replace" | "str_ireplace" => {
            let [search, replace, subject] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_str_replace_result(name, *search, *replace, *subject, values)?
        }
        "str_pad" => match evaluated_args {
            [value, length] => eval_str_pad_result(*value, *length, None, None, values)?,
            [value, length, pad_string] => {
                eval_str_pad_result(*value, *length, Some(*pad_string), None, values)?
            }
            [value, length, pad_string, pad_type] => {
                eval_str_pad_result(*value, *length, Some(*pad_string), Some(*pad_type), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "str_split" => match evaluated_args {
            [value] => eval_str_split_result(*value, None, values)?,
            [value, length] => eval_str_split_result(*value, Some(*length), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "substr" => match evaluated_args {
            [value, offset] => eval_substr_result(*value, *offset, None, values)?,
            [value, offset, length] => eval_substr_result(*value, *offset, Some(*length), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "substr_replace" => match evaluated_args {
            [value, replace, offset] => {
                eval_substr_replace_result(*value, *replace, *offset, None, values)?
            }
            [value, replace, offset, length] => {
                eval_substr_replace_result(*value, *replace, *offset, Some(*length), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "crc32" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_crc32_result(*value, values)?
        }
        "ctype_alnum" | "ctype_alpha" | "ctype_digit" | "ctype_space" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ctype_result(name, *value, values)?
        }
        "explode" => {
            let [separator, string] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_explode_result(*separator, *string, values)?
        }
        "ord" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ord_result(*value, values)?
        }
        "implode" => {
            let [separator, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_implode_result(*separator, *array, values)?
        }
        "nl2br" => match evaluated_args {
            [value] => eval_nl2br_result(*value, true, values)?,
            [value, use_xhtml] => {
                let use_xhtml = values.truthy(*use_xhtml)?;
                eval_nl2br_result(*value, use_xhtml, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "trim" | "ltrim" | "rtrim" | "chop" => match evaluated_args {
            [value] => eval_trim_like_result(name, *value, None, values)?,
            [value, mask] => eval_trim_like_result(name, *value, Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "hash" | "hash_file" | "hash_hmac" | "md5" | "sha1" => {
            eval_hash_one_shot_result(name, evaluated_args, values)?
        }
        "hash_algos" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_hash_algos_result(values)?
        }
        "hash_copy" => {
            let [hash_context] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_copy_result(*hash_context, context, values)?
        }
        "hash_final" => match evaluated_args {
            [hash_context] => eval_hash_final_result(*hash_context, false, context, values)?,
            [hash_context, binary] => {
                let binary = values.truthy(*binary)?;
                eval_hash_final_result(*hash_context, binary, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "hash_init" => {
            let [algo] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_init_result(*algo, context, values)?
        }
        "hash_update" => {
            let [hash_context, data] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_update_result(*hash_context, *data, context, values)?
        }
        "stream_is_local" | "stream_supports_lock" => {
            let [stream] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stream_bool_predicate_result(name, *stream, values)?
        }
        "hash_equals" => {
            let [known, user] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hash_equals_result(*known, *user, values)?
        }
        "hex2bin" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_hex2bin_result(*value, values)?
        }
        "html_entity_decode" | "htmlentities" | "htmlspecialchars" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_html_entity_result(name, *value, values)?
        }
        "strlen" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let bytes = values.string_bytes(*value)?;
            let len = i64::try_from(bytes.len()).map_err(|_| EvalStatus::RuntimeFatal)?;
            values.int(len)?
        }
        "strpos" | "strrpos" => {
            let [haystack, needle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_position_result(name, *haystack, *needle, values)?
        }
        "str_contains" | "str_starts_with" | "str_ends_with" => {
            let [haystack, needle] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_search_result(name, *haystack, *needle, values)?
        }
        "strstr" => match evaluated_args {
            [haystack, needle] => eval_strstr_result(*haystack, *needle, false, values)?,
            [haystack, needle, before_needle] => {
                let before_needle = values.truthy(*before_needle)?;
                eval_strstr_result(*haystack, *needle, before_needle, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "strcmp" | "strcasecmp" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_compare_result(name, *left, *right, values)?
        }
        "lcfirst" | "strtolower" | "strtoupper" | "ucfirst" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_string_case_result(name, *value, values)?
        }
        "ucwords" => match evaluated_args {
            [value] => eval_ucwords_result(*value, None, values)?,
            [value, separators] => eval_ucwords_result(*value, Some(*separators), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "wordwrap" => match evaluated_args {
            [value] => eval_wordwrap_result(*value, None, None, None, values)?,
            [value, width] => eval_wordwrap_result(*value, Some(*width), None, None, values)?,
            [value, width, break_string] => {
                eval_wordwrap_result(*value, Some(*width), Some(*break_string), None, values)?
            }
            [value, width, break_string, cut] => eval_wordwrap_result(
                *value,
                Some(*width),
                Some(*break_string),
                Some(*cut),
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        _ => return Ok(None),
    };
    Ok(Some(result))
}
