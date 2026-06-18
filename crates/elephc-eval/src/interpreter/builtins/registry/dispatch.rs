//! Purpose:
//! By-value dynamic builtin dispatch for evaluated argument lists.
//!
//! Called from:
//! - `crate::interpreter::builtins::registry` re-exports.
//!
//! Key details:
//! - Helpers are scoped to the eval interpreter and operate on already parsed
//!   EvalIR call metadata or evaluated runtime-cell handles.

use super::super::super::*;
use super::super::*;
use super::*;

/// Evaluates PHP-visible builtins when they are invoked through a dynamic callable name.
pub(in crate::interpreter) fn eval_builtin_with_values(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let result = match name {
        "abs" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.abs(*value)?
        }
        "addslashes" | "stripslashes" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_slashes_result(name, *value, values)?
        }
        "array_combine" => {
            let [keys, values_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_combine_result(*keys, *values_array, values)?
        }
        "array_column" => {
            let [array, column_key] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_column_result(*array, *column_key, values)?
        }
        "array_chunk" => {
            let [array, length] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_chunk_result(*array, *length, values)?
        }
        "array_fill" => {
            let [start, count, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_result(*start, *count, *value, values)?
        }
        "array_fill_keys" => {
            let [keys, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_fill_keys_result(*keys, *value, values)?
        }
        "array_filter" => match evaluated_args {
            [array] => eval_array_filter_result(*array, None, None, context, values)?,
            [array, callback] => {
                eval_array_filter_result(*array, Some(*callback), None, context, values)?
            }
            [array, callback, mode] => {
                eval_array_filter_result(*array, Some(*callback), Some(*mode), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_map" => {
            let Some((callback, arrays)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_map_result(*callback, arrays, context, values)?
        }
        "array_reduce" => match evaluated_args {
            [array, callback] => {
                let initial = values.null()?;
                eval_array_reduce_result(*array, *callback, initial, context, values)?
            }
            [array, callback, initial] => {
                eval_array_reduce_result(*array, *callback, *initial, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_walk" => {
            let [array, callback] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_walk_result(*array, *callback, context, values)?
        }
        "array_pop" | "array_shift" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_pop_shift_value_result(name, *array, values)?
        }
        "array_push" | "array_unshift" => {
            let Some((array, inserted)) = evaluated_args.split_first() else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_push_unshift_count_result(*array, inserted.len(), values)?
        }
        "array_splice" => {
            let result = match evaluated_args {
                [array, offset] => eval_array_splice_value_result(*array, *offset, None, values)?,
                [array, offset, length] => {
                    eval_array_splice_value_result(*array, *offset, Some(*length), values)?
                }
                [array, offset, length, _replacement] => {
                    eval_array_splice_value_result(*array, *offset, Some(*length), values)?
                }
                _ => return Err(EvalStatus::RuntimeFatal),
            };
            values.warning(
                "array_splice(): Argument #1 ($array) must be passed by reference, value given",
            )?;
            result
        }
        "arsort" | "asort" | "krsort" | "ksort" | "natcasesort" | "natsort" | "rsort"
        | "shuffle" | "sort" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_array_sort_value_result(*array, values)?
        }
        "uasort" | "uksort" | "usort" => {
            let [array, callback] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.warning(&format!(
                "{name}(): Argument #1 ($array) must be passed by reference, value given"
            ))?;
            eval_user_sort_value_result(name, *array, *callback, context, values)?
        }
        "array_flip" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_flip_result(*array, values)?
        }
        "array_pad" => {
            let [array, length, value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_pad_result(*array, *length, *value, values)?
        }
        "array_product" | "array_sum" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_aggregate_result(name, *array, values)?
        }
        "array_keys" | "array_values" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_projection_result(name, *array, values)?
        }
        "array_key_exists" => {
            let [key, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.array_key_exists(*key, *array)?
        }
        "array_diff" | "array_intersect" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_value_set_result(name, *left, *right, values)?
        }
        "array_diff_key" | "array_intersect_key" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_key_set_result(name, *left, *right, values)?
        }
        "array_merge" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_merge_result(*left, *right, values)?
        }
        "array_rand" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_rand_result(*array, values)?
        }
        "array_reverse" => match evaluated_args {
            [array] => eval_array_reverse_result(*array, false, values)?,
            [array, preserve_keys] => {
                let preserve_keys = values.truthy(*preserve_keys)?;
                eval_array_reverse_result(*array, preserve_keys, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_search" | "in_array" => {
            let [needle, array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_search_result(name, *needle, *array, values)?
        }
        "array_slice" => match evaluated_args {
            [array, offset] => eval_array_slice_result(*array, *offset, None, values)?,
            [array, offset, length] => {
                eval_array_slice_result(*array, *offset, Some(*length), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "array_unique" => {
            let [array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_array_unique_result(*array, values)?
        }
        "range" => {
            let [start, end] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_range_result(*start, *end, values)?
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
        "acos" | "asin" | "atan" | "cos" | "cosh" | "deg2rad" | "exp" | "log2" | "log10"
        | "rad2deg" | "sin" | "sinh" | "tan" | "tanh" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_unary_result(name, *value, values)?
        }
        "atan2" | "hypot" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_pair_result(name, *left, *right, values)?
        }
        "bin2hex" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_bin2hex_result(*value, values)?
        }
        "ceil" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.ceil(*value)?
        }
        "chr" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chr_result(*value, values)?
        }
        "chdir" | "mkdir" | "rmdir" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unary_path_bool_result(name, *path, values)?
        }
        "chmod" => {
            let [filename, permissions] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_chmod_result(*filename, *permissions, values)?
        }
        "clearstatcache" => {
            if evaluated_args.len() > 2 {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.null()?
        }
        "clamp" => {
            let [value, min, max] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_clamp_result(*value, *min, *max, values)?
        }
        "copy" | "link" | "rename" | "symlink" => {
            let [from, to] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_binary_path_bool_result(name, *from, *to, values)?
        }
        "floor" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.floor(*value)?
        }
        "fdiv" | "fmod" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_float_binary_result(name, *left, *right, values)?
        }
        "file" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_result(*filename, values)?
        }
        "file_exists" | "is_dir" | "is_executable" | "is_file" | "is_link" | "is_readable"
        | "is_writable" | "is_writeable" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_probe_result(name, *filename, values)?
        }
        "fileatime" | "filectime" | "filegroup" | "fileinode" | "filemtime" | "fileowner"
        | "fileperms" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_stat_scalar_result(name, *filename, values)?
        }
        "file_get_contents" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_get_contents_result(*filename, values)?
        }
        "file_put_contents" => {
            let [filename, data] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_file_put_contents_result(*filename, *data, values)?
        }
        "filesize" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filesize_result(*filename, values)?
        }
        "filetype" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_filetype_result(*filename, values)?
        }
        "fnmatch" => match evaluated_args {
            [pattern, filename] => eval_fnmatch_result(*pattern, *filename, None, values)?,
            [pattern, filename, flags] => {
                eval_fnmatch_result(*pattern, *filename, Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stat" | "lstat" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_stat_array_result(name, *filename, values)?
        }
        "linkinfo" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_linkinfo_result(*path, values)?
        }
        "log" => match evaluated_args {
            [num] => eval_log_result(*num, None, values)?,
            [num, base] => eval_log_result(*num, Some(*base), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "readfile" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_readfile_result(*filename, values)?
        }
        "pi" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.float(std::f64::consts::PI)?
        }
        "php_uname" => match evaluated_args {
            [] => eval_php_uname_result(None, values)?,
            [mode] => eval_php_uname_result(Some(*mode), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "pow" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.pow(*left, *right)?
        }
        "preg_match" => match evaluated_args {
            [pattern, subject] => eval_preg_match_result(*pattern, *subject, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_match_all" => match evaluated_args {
            [pattern, subject] => eval_preg_match_all_result(*pattern, *subject, values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_replace" => match evaluated_args {
            [pattern, replacement, subject] => {
                eval_preg_replace_result(*pattern, *replacement, *subject, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_replace_callback" => match evaluated_args {
            [pattern, callback, subject] => {
                eval_preg_replace_callback_result(*pattern, *callback, *subject, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "preg_split" => match evaluated_args {
            [pattern, subject] => eval_preg_split_result(*pattern, *subject, None, None, values)?,
            [pattern, subject, limit] => {
                eval_preg_split_result(*pattern, *subject, Some(*limit), None, values)?
            }
            [pattern, subject, limit, flags] => {
                eval_preg_split_result(*pattern, *subject, Some(*limit), Some(*flags), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "print_r" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_print_r_result(*value, values)?
        }
        "var_dump" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_var_dump_result(*value, values)?
        }
        "rand" | "mt_rand" => match evaluated_args {
            [] => eval_rand_result(None, None, values)?,
            [min, max] => eval_rand_result(Some(*min), Some(*max), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "random_int" => {
            let [min, max] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_random_int_result(*min, *max, values)?
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
        "round" => match evaluated_args {
            [value] => values.round(*value, None)?,
            [value, precision] => values.round(*value, Some(*precision))?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "scandir" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_scandir_result(*directory, values)?
        }
        "sqrt" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            values.sqrt(*value)?
        }
        "spl_classes" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_spl_classes_result(values)?
        }
        "spl_object_id" | "spl_object_hash" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_spl_object_identity_result(name, *object, values)?
        }
        "sscanf" => {
            let [input, format, ..] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_sscanf_result(*input, *format, values)?
        }
        "sprintf" | "printf" => eval_sprintf_like_result(name, evaluated_args, values)?,
        "settype" => {
            let [value, type_name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_settype_value_result(*value, *type_name, values)?
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
        "call_user_func" => {
            return eval_call_user_func_with_values(evaluated_args.to_vec(), context, values)
                .map(Some);
        }
        "call_user_func_array" => {
            let [callback, arg_array] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            return eval_call_user_func_array_with_values(*callback, *arg_array, context, values)
                .map(Some);
        }
        "boolval" | "floatval" | "intval" | "strval" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_cast_result(name, *value, values)?
        }
        "count" => match evaluated_args {
            [value] => eval_count_result(*value, None, values)?,
            [value, mode] => eval_count_result(*value, Some(*mode), values)?,
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
        "date" => match evaluated_args {
            [format] => eval_date_result(*format, None, values)?,
            [format, timestamp] => eval_date_result(*format, Some(*timestamp), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "define" => eval_define_result(evaluated_args, context, values)?,
        "defined" => eval_defined_result(evaluated_args, context, values)?,
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
        "max" | "min" => eval_min_max_result(name, evaluated_args, values)?,
        "microtime" => match evaluated_args {
            [] | [_] => eval_microtime_result(values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "mktime" => {
            let [hour, minute, second, month, day, year] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_mktime_result(*hour, *minute, *second, *month, *day, *year, values)?
        }
        "nl2br" => match evaluated_args {
            [value] => eval_nl2br_result(*value, true, values)?,
            [value, use_xhtml] => {
                let use_xhtml = values.truthy(*use_xhtml)?;
                eval_nl2br_result(*value, use_xhtml, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "number_format" => match evaluated_args {
            [value] => eval_number_format_result(*value, None, None, None, values)?,
            [value, decimals] => {
                eval_number_format_result(*value, Some(*decimals), None, None, values)?
            }
            [value, decimals, decimal_separator] => eval_number_format_result(
                *value,
                Some(*decimals),
                Some(*decimal_separator),
                None,
                values,
            )?,
            [value, decimals, decimal_separator, thousands_separator] => eval_number_format_result(
                *value,
                Some(*decimals),
                Some(*decimal_separator),
                Some(*thousands_separator),
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "basename" => match evaluated_args {
            [path] => eval_basename_result(*path, None, values)?,
            [path, suffix] => eval_basename_result(*path, Some(*suffix), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "dirname" => match evaluated_args {
            [path] => eval_dirname_result(*path, None, values)?,
            [path, levels] => eval_dirname_result(*path, Some(*levels), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "disk_free_space" | "disk_total_space" => {
            let [directory] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_disk_space_result(name, *directory, values)?
        }
        "trim" | "ltrim" | "rtrim" | "chop" => match evaluated_args {
            [value] => eval_trim_like_result(name, *value, None, values)?,
            [value, mask] => eval_trim_like_result(name, *value, Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "function_exists" | "is_callable" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            let name = values.string_bytes(*name)?;
            let name = String::from_utf8(name).map_err(|_| EvalStatus::RuntimeFatal)?;
            let name = name.trim_start_matches('\\').to_ascii_lowercase();
            values.bool_value(eval_function_probe_exists(context, &name))?
        }
        "class_exists" => eval_class_exists_result(evaluated_args, context, values)?,
        "enum_exists" | "trait_exists" => {
            eval_class_like_exists_result(name, evaluated_args, values)?
        }
        "interface_exists" => eval_interface_exists_result(evaluated_args, values)?,
        "is_a" | "is_subclass_of" => {
            eval_is_a_relation_result(name, evaluated_args, context, values)?
        }
        "json_decode" => match evaluated_args {
            [json] => eval_json_decode_result(*json, None, None, None, context, values)?,
            [json, associative] => {
                eval_json_decode_result(*json, Some(*associative), None, None, context, values)?
            }
            [json, associative, depth] => eval_json_decode_result(
                *json,
                Some(*associative),
                Some(*depth),
                None,
                context,
                values,
            )?,
            [json, associative, depth, flags] => eval_json_decode_result(
                *json,
                Some(*associative),
                Some(*depth),
                Some(*flags),
                context,
                values,
            )?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "json_encode" => match evaluated_args {
            [value] => eval_json_encode_result(*value, None, None, context, values)?,
            [value, flags] => eval_json_encode_result(*value, Some(*flags), None, context, values)?,
            [value, flags, depth] => {
                eval_json_encode_result(*value, Some(*flags), Some(*depth), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "json_last_error" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.int(context.json_last_error())?
        }
        "json_last_error_msg" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            values.string(context.json_last_error_msg())?
        }
        "json_validate" => match evaluated_args {
            [json] => eval_json_validate_result(*json, None, None, context, values)?,
            [json, depth] => eval_json_validate_result(*json, Some(*depth), None, context, values)?,
            [json, depth, flags] => {
                eval_json_validate_result(*json, Some(*depth), Some(*flags), context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "gethostbyaddr" => {
            let [ip] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyaddr_result(*ip, values)?
        }
        "gethostbyname" => {
            let [hostname] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gethostbyname_result(*hostname, values)?
        }
        "gethostname" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_gethostname_result(values)?
        }
        "getprotobyname" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobyname_result(*protocol, values)?
        }
        "getprotobynumber" => {
            let [protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getprotobynumber_result(*protocol, values)?
        }
        "getservbyname" => {
            let [service, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyname_result(*service, *protocol, values)?
        }
        "getservbyport" => {
            let [port, protocol] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getservbyport_result(*port, *protocol, values)?
        }
        "getcwd" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_getcwd_result(values)?
        }
        "getenv" => {
            let [name] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_getenv_result(*name, values)?
        }
        "get_class" => {
            let [object] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_get_class_result(*object, context, values)?
        }
        "get_parent_class" => {
            let [object_or_class] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_get_parent_class_result(*object_or_class, values)?
        }
        "get_resource_id" | "get_resource_type" => {
            let [resource] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_resource_introspection_result(name, *resource, values)?
        }
        "gettype" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_gettype_result(*value, values)?
        }
        "glob" => {
            let [pattern] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_glob_result(*pattern, values)?
        }
        "hash" | "hash_file" | "hash_hmac" | "md5" | "sha1" => {
            eval_hash_one_shot_result(name, evaluated_args, values)?
        }
        "hash_algos" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_hash_algos_result(values)?
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
        "inet_ntop" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_ntop_result(*value, values)?
        }
        "inet_pton" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_inet_pton_result(*value, values)?
        }
        "intdiv" => {
            let [left, right] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_intdiv_result(*left, *right, values)?
        }
        "iterator_apply" => match evaluated_args {
            [iterator, callback] => {
                let callback = eval_callable(*callback, values)?;
                eval_iterator_apply_result(*iterator, &callback, Vec::new(), context, values)?
            }
            [iterator, callback, args] => {
                let callback = eval_callable(*callback, values)?;
                let callback_args = eval_iterator_apply_arg_values(*args, values)?;
                eval_iterator_apply_result(*iterator, &callback, callback_args, context, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "iterator_count" => {
            let [iterator] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_iterator_count_result(*iterator, values)?
        }
        "iterator_to_array" => match evaluated_args {
            [iterator] => eval_iterator_to_array_result(*iterator, true, values)?,
            [iterator, preserve_keys] => {
                let preserve_keys = values.truthy(*preserve_keys)?;
                eval_iterator_to_array_result(*iterator, preserve_keys, values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "ip2long" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_ip2long_result(*value, values)?
        }
        "phpversion" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_phpversion_result(values)?
        }
        "pathinfo" => match evaluated_args {
            [path] => eval_pathinfo_result(*path, None, values)?,
            [path, flags] => eval_pathinfo_result(*path, Some(*flags), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "putenv" => {
            let [assignment] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_putenv_result(*assignment, values)?
        }
        "realpath" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_realpath_result(*path, values)?
        }
        "realpath_cache_get" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_realpath_cache_get_result(values)?
        }
        "realpath_cache_size" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_realpath_cache_size_result(values)?
        }
        "is_array" | "is_bool" | "is_double" | "is_finite" | "is_float" | "is_infinite"
        | "is_int" | "is_integer" | "is_iterable" | "is_long" | "is_nan" | "is_null"
        | "is_numeric" | "is_object" | "is_real" | "is_resource" | "is_string" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_type_predicate_result(name, *value, values)?
        }
        "sys_get_temp_dir" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_sys_get_temp_dir_result(values)?
        }
        "tempnam" => {
            let [directory, prefix] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_tempnam_result(*directory, *prefix, values)?
        }
        "sleep" => {
            let [seconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_sleep_result(*seconds, values)?
        }
        "time" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_time_result(values)?
        }
        "touch" => match evaluated_args {
            [filename] => eval_touch_result(*filename, None, None, values)?,
            [filename, mtime] => eval_touch_result(*filename, Some(*mtime), None, values)?,
            [filename, mtime, atime] => {
                eval_touch_result(*filename, Some(*mtime), Some(*atime), values)?
            }
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "stream_get_filters" | "stream_get_transports" | "stream_get_wrappers" => {
            if !evaluated_args.is_empty() {
                return Err(EvalStatus::RuntimeFatal);
            }
            eval_stream_introspection_result(name, values)?
        }
        "strtotime" => {
            let [datetime] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_strtotime_result(*datetime, values)?
        }
        "umask" => match evaluated_args {
            [] => eval_umask_result(None, values)?,
            [mask] => eval_umask_result(Some(*mask), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "usleep" => {
            let [microseconds] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_usleep_result(*microseconds, values)?
        }
        "readlink" => {
            let [path] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_readlink_result(*path, values)?
        }
        "unlink" => {
            let [filename] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_unlink_result(*filename, values)?
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
        "long2ip" => {
            let [value] = evaluated_args else {
                return Err(EvalStatus::RuntimeFatal);
            };
            eval_long2ip_result(*value, values)?
        }
        "ucwords" => match evaluated_args {
            [value] => eval_ucwords_result(*value, None, values)?,
            [value, separators] => eval_ucwords_result(*value, Some(*separators), values)?,
            _ => return Err(EvalStatus::RuntimeFatal),
        },
        "vsprintf" | "vprintf" => eval_vsprintf_like_result(name, evaluated_args, values)?,
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
