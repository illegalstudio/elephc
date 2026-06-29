//! Purpose:
//! Lowers the first scalar PHP builtin calls emitted as EIR `BuiltinCall` instructions.
//! Covers concrete scalar casts, type predicates, selected Mixed tag predicates, and string length.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::lower_instruction()`.
//!
//! Key details:
//! - Runtime conversions reuse existing target-aware helpers instead of duplicating parsing logic.
//! - Selected Mixed predicates inspect the boxed runtime tag through shared predicate lowering.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{Immediate, Instruction, Op, ValueDef, ValueId};
use crate::names::{define_seen_symbol, ir_global_symbol, php_symbol_key};
use crate::parser::ast::Visibility;
use crate::types::checker::builtins::is_php_visible_builtin_function;
use crate::types::PhpType;

use super::super::context::FunctionContext;
use super::{expect_data, expect_operand, load_value_to_first_int_arg, predicates, store_if_result};
use crate::codegen_ir::{CodegenIrError, Result};

pub(in crate::codegen_ir::lower_inst) mod attributes;
mod arrays;
mod buffers;
mod class_relations;
mod ctype;
mod debug;
mod io;
mod isset;
mod is_numeric;
mod json;
mod math;
mod pointers;
mod regex;
mod serialize;
mod spl;
mod system;
mod strings;
mod types;

const DEFINE_ALREADY_DEFINED_WARNING: &str =
    "Warning: define(): Constant already defined\n";

/// Lowers a scalar builtin call by matching the canonical PHP function name.
pub(super) fn lower_builtin_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let name = ctx.function_name_data(expect_data(inst)?)?;
    let key = php_symbol_key(name.trim_start_matches('\\'));
    match key.as_str() {
        "abs" => math::lower_abs(ctx, inst),
        "floor" => math::lower_floor(ctx, inst),
        "ceil" => math::lower_ceil(ctx, inst),
        "clamp" => math::lower_clamp(ctx, inst),
        "round" => math::lower_round(ctx, inst),
        "sqrt" => math::lower_sqrt(ctx, inst),
        "intdiv" => math::lower_intdiv(ctx, inst),
        "fdiv" => math::lower_fdiv(ctx, inst),
        "fmod" => math::lower_fmod(ctx, inst),
        "pow" => math::lower_pow(ctx, inst),
        "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh"
        | "tanh" | "log2" | "log10" | "exp" => {
            math::lower_unary_libm(ctx, inst, key.as_str())
        }
        "log" => math::lower_log(ctx, inst),
        "atan2" => math::lower_atan2(ctx, inst),
        "hypot" => math::lower_hypot(ctx, inst),
        "deg2rad" => math::lower_deg2rad(ctx, inst),
        "rad2deg" => math::lower_rad2deg(ctx, inst),
        "rand" | "mt_rand" => math::lower_rand(ctx, inst, key.as_str()),
        "random_int" => math::lower_random_int(ctx, inst),
        "min" => math::lower_min_max(ctx, inst, false),
        "max" => math::lower_min_max(ctx, inst, true),
        "pi" => lower_pi(ctx, inst),
        "phpversion" => lower_phpversion(ctx, inst),
        "strlen" => lower_strlen(ctx, inst),
        "count" => lower_count(ctx, inst),
        "closure_bind" => lower_closure_bind(ctx, inst),
        "buffer_len" => buffers::lower_buffer_len(ctx, inst),
        "buffer_free" => buffers::lower_buffer_free(ctx, inst),
        "ptr" => pointers::lower_ptr(ctx, inst),
        "ptr_null" => pointers::lower_ptr_null(ctx, inst),
        "ptr_is_null" => pointers::lower_ptr_is_null(ctx, inst),
        "ptr_sizeof" => pointers::lower_ptr_sizeof(ctx, inst),
        "ptr_offset" => pointers::lower_ptr_offset(ctx, inst),
        "ptr_get" => pointers::lower_ptr_get(ctx, inst),
        "ptr_set" => pointers::lower_ptr_set(ctx, inst),
        "ptr_read8" => pointers::lower_ptr_read8(ctx, inst),
        "ptr_read16" => pointers::lower_ptr_read16(ctx, inst),
        "ptr_read32" => pointers::lower_ptr_read32(ctx, inst),
        "ptr_read_string" => pointers::lower_ptr_read_string(ctx, inst),
        "ptr_write8" => pointers::lower_ptr_write8(ctx, inst),
        "ptr_write16" => pointers::lower_ptr_write16(ctx, inst),
        "ptr_write32" => pointers::lower_ptr_write32(ctx, inst),
        "ptr_write_string" => pointers::lower_ptr_write_string(ctx, inst),
        "array_sum" => arrays::lower_array_sum(ctx, inst),
        "array_product" => arrays::lower_array_product(ctx, inst),
        "array_push" => arrays::lower_array_push(ctx, inst),
        "array_chunk" => arrays::lower_array_chunk(ctx, inst),
        "array_pad" => arrays::lower_array_pad(ctx, inst),
        "array_combine" => arrays::lower_array_combine(ctx, inst),
        "array_column" => arrays::lower_array_column(ctx, inst),
        "array_flip" => arrays::lower_array_flip(ctx, inst),
        "array_fill" => arrays::lower_array_fill(ctx, inst),
        "array_fill_keys" => arrays::lower_array_fill_keys(ctx, inst),
        "array_reverse" => arrays::lower_array_reverse(ctx, inst),
        "array_unique" => arrays::lower_array_unique(ctx, inst),
        "array_map" => arrays::lower_array_map(ctx, inst),
        "array_filter" => arrays::lower_array_filter(ctx, inst),
        "array_reduce" => arrays::lower_array_reduce(ctx, inst),
        "array_walk" => arrays::lower_array_walk(ctx, inst),
        "array_merge" => arrays::lower_array_merge(ctx, inst),
        "array_diff" => arrays::lower_array_diff(ctx, inst),
        "array_intersect" => arrays::lower_array_intersect(ctx, inst),
        "array_diff_key" => arrays::lower_array_diff_key(ctx, inst),
        "array_intersect_key" => arrays::lower_array_intersect_key(ctx, inst),
        "array_slice" => arrays::lower_array_slice(ctx, inst),
        "array_splice" => arrays::lower_array_splice(ctx, inst),
        "array_keys" => arrays::lower_array_keys(ctx, inst),
        "array_values" => arrays::lower_array_values(ctx, inst),
        "array_rand" => arrays::lower_array_rand(ctx, inst),
        "array_pop" => arrays::lower_array_pop(ctx, inst),
        "array_shift" => arrays::lower_array_shift(ctx, inst),
        "array_unshift" => arrays::lower_array_unshift(ctx, inst),
        "sort" => arrays::lower_sort(ctx, inst),
        "rsort" => arrays::lower_rsort(ctx, inst),
        "asort" => arrays::lower_asort(ctx, inst),
        "arsort" => arrays::lower_arsort(ctx, inst),
        "ksort" => arrays::lower_ksort(ctx, inst),
        "krsort" => arrays::lower_krsort(ctx, inst),
        "natsort" => arrays::lower_natsort(ctx, inst),
        "natcasesort" => arrays::lower_natcasesort(ctx, inst),
        "shuffle" => arrays::lower_shuffle(ctx, inst),
        "usort" => arrays::lower_usort(ctx, inst),
        "uksort" => arrays::lower_uksort(ctx, inst),
        "uasort" => arrays::lower_uasort(ctx, inst),
        "array_key_exists" => arrays::lower_array_key_exists(ctx, inst),
        "array_search" => arrays::lower_array_search(ctx, inst),
        "in_array" => arrays::lower_in_array(ctx, inst),
        "call_user_func" | "call_user_func_array" => {
            arrays::lower_call_user_func_builtin_escape(ctx, inst, key.as_str())
        }
        "range" => arrays::lower_range(ctx, inst),
        "intval" => lower_intval(ctx, inst),
        "floatval" => lower_floatval(ctx, inst),
        "boolval" => lower_boolval(ctx, inst),
        "empty" => lower_empty(ctx, inst),
        "settype" => types::lower_settype(ctx, inst),
        "unset" => types::lower_unset_builtin(ctx, inst),
        "isset" => isset::lower_isset(ctx, inst),
        "gettype" => lower_gettype(ctx, inst),
        "define" => lower_define(ctx, inst),
        "defined" => lower_defined(ctx, inst),
        "file_get_contents" => io::lower_file_get_contents(ctx, inst),
        "readfile" => io::lower_readfile(ctx, inst),
        "readline" => io::lower_readline(ctx, inst),
        "fopen" => io::lower_fopen(ctx, inst),
        "fclose" => io::lower_fclose(ctx, inst),
        "fread" => io::lower_fread(ctx, inst),
        "fwrite" => io::lower_fwrite(ctx, inst),
        "fprintf" => io::lower_fprintf(ctx, inst),
        "vfprintf" => io::lower_vfprintf(ctx, inst),
        "fscanf" => io::lower_fscanf(ctx, inst),
        "fgets" => io::lower_fgets(ctx, inst),
        "fgetc" => io::lower_fgetc(ctx, inst),
        "fgetcsv" => io::lower_fgetcsv(ctx, inst),
        "fputcsv" => io::lower_fputcsv(ctx, inst),
        "fpassthru" => io::lower_fpassthru(ctx, inst),
        "feof" => io::lower_feof(ctx, inst),
        "fseek" => io::lower_fseek(ctx, inst),
        "ftell" => io::lower_ftell(ctx, inst),
        "rewind" => io::lower_rewind(ctx, inst),
        "ftruncate" => io::lower_ftruncate(ctx, inst),
        "fsync" => io::lower_fsync(ctx, inst),
        "fflush" => io::lower_fflush(ctx, inst),
        "fdatasync" => io::lower_fdatasync(ctx, inst),
        "flock" => io::lower_flock(ctx, inst),
        "disk_free_space" => io::lower_disk_free_space(ctx, inst),
        "disk_total_space" => io::lower_disk_total_space(ctx, inst),
        "gethostname" => io::lower_gethostname(ctx, inst),
        "gethostbyname" => io::lower_gethostbyname(ctx, inst),
        "gethostbyaddr" => io::lower_gethostbyaddr(ctx, inst),
        "getprotobyname" => io::lower_getprotobyname(ctx, inst),
        "getprotobynumber" => io::lower_getprotobynumber(ctx, inst),
        "getservbyname" => io::lower_getservbyname(ctx, inst),
        "getservbyport" => io::lower_getservbyport(ctx, inst),
        "opendir" => io::lower_opendir(ctx, inst),
        "readdir" => io::lower_readdir(ctx, inst),
        "closedir" => io::lower_closedir(ctx, inst),
        "rewinddir" => io::lower_rewinddir(ctx, inst),
        "popen" => io::lower_popen(ctx, inst),
        "pclose" => io::lower_pclose(ctx, inst),
        "fsockopen" | "pfsockopen" => io::lower_fsockopen(ctx, inst),
        "stream_wrapper_register" => io::lower_stream_wrapper_register(ctx, inst),
        "stream_wrapper_unregister" => io::lower_stream_wrapper_unregister(ctx, inst),
        "stream_wrapper_restore" => io::lower_stream_wrapper_restore(ctx, inst),
        "stream_context_create" => io::lower_stream_context_create(ctx, inst),
        "stream_context_get_default" => io::lower_stream_context_get_default(ctx, inst),
        "stream_context_set_default" => io::lower_stream_context_set_default(ctx, inst),
        "stream_context_set_option" => io::lower_stream_context_set_option(ctx, inst),
        "stream_context_set_params" => io::lower_stream_context_set_params(ctx, inst),
        "stream_context_get_options" => io::lower_stream_context_get_options(ctx, inst),
        "stream_context_get_params" => io::lower_stream_context_get_params(ctx, inst),
        "stream_get_contents" => io::lower_stream_get_contents(ctx, inst),
        "stream_get_line" => io::lower_stream_get_line(ctx, inst),
        "stream_get_meta_data" => io::lower_stream_get_meta_data(ctx, inst),
        "stream_get_wrappers" => io::lower_stream_get_wrappers(ctx, inst),
        "stream_get_transports" => io::lower_stream_get_transports(ctx, inst),
        "stream_get_filters" => io::lower_stream_get_filters(ctx, inst),
        "stream_filter_register" => io::lower_stream_filter_register(ctx, inst),
        "stream_filter_append" | "stream_filter_prepend" => {
            io::lower_stream_filter_attach(ctx, inst, key.as_str())
        }
        "stream_filter_remove" => io::lower_stream_filter_remove(ctx, inst),
        "stream_bucket_make_writeable" => io::lower_stream_bucket_make_writeable(ctx, inst),
        "stream_bucket_new" => io::lower_stream_bucket_new(ctx, inst),
        "stream_bucket_append" | "stream_bucket_prepend" => {
            io::lower_stream_bucket_append_or_prepend(ctx, inst)
        }
        "stream_is_local" => io::lower_stream_is_local(ctx, inst),
        "stream_supports_lock" => io::lower_stream_supports_lock(ctx, inst),
        "stream_isatty" => io::lower_stream_isatty(ctx, inst),
        "stream_set_chunk_size" => io::lower_stream_set_chunk_size(ctx, inst),
        "stream_set_read_buffer" => io::lower_stream_set_buffer(ctx, inst),
        "stream_set_write_buffer" => io::lower_stream_set_buffer(ctx, inst),
        "stream_set_blocking" => io::lower_stream_set_blocking(ctx, inst),
        "stream_set_timeout" => io::lower_stream_set_timeout(ctx, inst),
        "stream_select" => io::lower_stream_select(ctx, inst),
        "stream_resolve_include_path" => io::lower_stream_resolve_include_path(ctx, inst),
        "stream_copy_to_stream" => io::lower_stream_copy_to_stream(ctx, inst),
        "stream_socket_server" => io::lower_stream_socket_server(ctx, inst),
        "stream_socket_client" => io::lower_stream_socket_client(ctx, inst),
        "stream_socket_accept" => io::lower_stream_socket_accept(ctx, inst),
        "stream_socket_pair" => io::lower_stream_socket_pair(ctx, inst),
        "stream_socket_get_name" => io::lower_stream_socket_get_name(ctx, inst),
        "stream_socket_shutdown" => io::lower_stream_socket_shutdown(ctx, inst),
        "stream_socket_enable_crypto" => io::lower_stream_socket_enable_crypto(ctx, inst),
        "stream_socket_recvfrom" => io::lower_stream_socket_recvfrom(ctx, inst),
        "stream_socket_sendto" => io::lower_stream_socket_sendto(ctx, inst),
        "file" => io::lower_file(ctx, inst),
        "realpath" => io::lower_realpath(ctx, inst),
        "realpath_cache_get" => io::lower_realpath_cache_get(ctx, inst),
        "realpath_cache_size" => io::lower_realpath_cache_size(ctx, inst),
        "file_put_contents" => io::lower_file_put_contents(ctx, inst),
        "__elephc_phar_set_compression" => io::lower_elephc_phar_set_compression(ctx, inst),
        "__elephc_phar_list_entries" => io::lower_elephc_phar_list_entries(ctx, inst),
        "__elephc_phar_get_metadata" => io::lower_elephc_phar_get_metadata(ctx, inst),
        "__elephc_phar_get_stub" => io::lower_elephc_phar_get_stub(ctx, inst),
        "__elephc_phar_set_metadata" => io::lower_elephc_phar_set_metadata(ctx, inst),
        "__elephc_phar_set_stub" => io::lower_elephc_phar_set_stub(ctx, inst),
        "__elephc_phar_get_file_metadata" => {
            io::lower_elephc_phar_get_file_metadata(ctx, inst)
        }
        "__elephc_phar_set_file_metadata" => {
            io::lower_elephc_phar_set_file_metadata(ctx, inst)
        }
        "__elephc_phar_gzip_archive" => io::lower_elephc_phar_gzip_archive(ctx, inst),
        "__elephc_phar_bzip2_archive" => io::lower_elephc_phar_bzip2_archive(ctx, inst),
        "__elephc_phar_decompress_archive" => {
            io::lower_elephc_phar_decompress_archive(ctx, inst)
        }
        "__elephc_phar_sign_openssl" => io::lower_elephc_phar_sign_openssl(ctx, inst),
        "__elephc_phar_sign_hash" => io::lower_elephc_phar_sign_hash(ctx, inst),
        "__elephc_phar_set_zip_password" => io::lower_elephc_phar_set_zip_password(ctx, inst),
        "__elephc_phar_get_signature_hash" => {
            io::lower_elephc_phar_get_signature_hash(ctx, inst)
        }
        "__elephc_phar_get_signature_type" => {
            io::lower_elephc_phar_get_signature_type(ctx, inst)
        }
        "file_exists" => io::lower_file_exists(ctx, inst),
        "copy" => io::lower_copy(ctx, inst),
        "rename" => io::lower_rename(ctx, inst),
        "unlink" => io::lower_unlink(ctx, inst),
        "mkdir" => io::lower_mkdir(ctx, inst),
        "rmdir" => io::lower_rmdir(ctx, inst),
        "chdir" => io::lower_chdir(ctx, inst),
        "chmod" => io::lower_chmod(ctx, inst),
        "chown" => io::lower_chown(ctx, inst),
        "chgrp" => io::lower_chgrp(ctx, inst),
        "lchown" => io::lower_lchown(ctx, inst),
        "lchgrp" => io::lower_lchgrp(ctx, inst),
        "umask" => io::lower_umask(ctx, inst),
        "touch" => io::lower_touch(ctx, inst),
        "getcwd" => io::lower_getcwd(ctx, inst),
        "sys_get_temp_dir" => io::lower_sys_get_temp_dir(ctx, inst),
        "tmpfile" => io::lower_tmpfile(ctx, inst),
        "tempnam" => io::lower_tempnam(ctx, inst),
        "scandir" => io::lower_scandir(ctx, inst),
        "glob" => io::lower_glob(ctx, inst),
        "basename" => io::lower_basename(ctx, inst),
        "dirname" => io::lower_dirname(ctx, inst),
        "fnmatch" => io::lower_fnmatch(ctx, inst),
        "pathinfo" => io::lower_pathinfo(ctx, inst),
        "filesize" => io::lower_filesize(ctx, inst),
        "filemtime" => io::lower_filemtime(ctx, inst),
        "linkinfo" => io::lower_linkinfo(ctx, inst),
        "symlink" => io::lower_symlink(ctx, inst),
        "link" => io::lower_link(ctx, inst),
        "readlink" => io::lower_readlink(ctx, inst),
        "fileatime" => io::lower_fileatime(ctx, inst),
        "filectime" => io::lower_filectime(ctx, inst),
        "fileperms" => io::lower_fileperms(ctx, inst),
        "fileowner" => io::lower_fileowner(ctx, inst),
        "filegroup" => io::lower_filegroup(ctx, inst),
        "fileinode" => io::lower_fileinode(ctx, inst),
        "filetype" => io::lower_filetype(ctx, inst),
        "stat" => io::lower_stat(ctx, inst),
        "lstat" => io::lower_lstat(ctx, inst),
        "fstat" => io::lower_fstat(ctx, inst),
        "clearstatcache" => io::lower_clearstatcache(ctx, inst),
        "is_file" => io::lower_is_file(ctx, inst),
        "is_dir" => io::lower_is_dir(ctx, inst),
        "is_readable" => io::lower_is_readable(ctx, inst),
        "is_writable" => io::lower_is_writable(ctx, inst),
        "is_writeable" => io::lower_is_writeable(ctx, inst),
        "is_executable" => io::lower_is_executable(ctx, inst),
        "is_link" => io::lower_is_link(ctx, inst),
        "date" => system::lower_date(ctx, inst),
        "gmdate" => system::lower_gmdate(ctx, inst),
        "date_default_timezone_get" => system::lower_date_default_timezone_get(ctx, inst),
        "date_default_timezone_set" => system::lower_date_default_timezone_set(ctx, inst),
        "microtime" => system::lower_microtime(ctx, inst),
        "mktime" => system::lower_mktime(ctx, inst),
        "gmmktime" => system::lower_gmmktime(ctx, inst),
        "__elephc_mktime_raw" => system::lower_mktime(ctx, inst),
        "__elephc_gmmktime_raw" => system::lower_gmmktime(ctx, inst),
        "checkdate" => system::lower_checkdate(ctx, inst),
        "getdate" => system::lower_getdate(ctx, inst),
        "localtime" => system::lower_localtime(ctx, inst),
        "hrtime" => system::lower_hrtime(ctx, inst),
        "http_response_code" => system::lower_http_response_code(ctx, inst),
        "header" => system::lower_header(ctx, inst),
        "sleep" => system::lower_sleep(ctx, inst),
        "strtotime" => system::lower_strtotime(ctx, inst),
        "__elephc_strtotime_raw" => system::lower_elephc_strtotime_raw(ctx, inst),
        "time" => system::lower_time(ctx, inst),
        "usleep" => system::lower_usleep(ctx, inst),
        "exit" | "die" => system::lower_exit(ctx, inst),
        "getenv" => system::lower_getenv(ctx, inst),
        "putenv" => system::lower_putenv(ctx, inst),
        "php_uname" => system::lower_php_uname(ctx, inst),
        "exec" => system::lower_exec(ctx, inst),
        "shell_exec" => system::lower_shell_exec(ctx, inst),
        "system" => system::lower_system(ctx, inst),
        "passthru" => system::lower_passthru(ctx, inst),
        "preg_match" => regex::lower_preg_match(ctx, inst),
        "preg_match_all" => regex::lower_preg_match_all(ctx, inst),
        "preg_replace" => regex::lower_preg_replace(ctx, inst),
        "preg_replace_callback" => regex::lower_preg_replace_callback(ctx, inst),
        "preg_split" => regex::lower_preg_split(ctx, inst),
        "json_decode" => json::lower_json_decode(ctx, inst),
        "json_encode" => json::lower_json_encode(ctx, inst),
        "json_last_error" => json::lower_json_last_error(ctx, inst),
        "json_last_error_msg" => json::lower_json_last_error_msg(ctx, inst),
        "json_validate" => json::lower_json_validate(ctx, inst),
        "serialize" => serialize::lower_serialize(ctx, inst),
        "unserialize" => serialize::lower_unserialize(ctx, inst),
        "function_exists" => lower_function_exists(ctx, inst),
        "class_exists" | "interface_exists" | "trait_exists" | "enum_exists" => {
            lower_class_like_exists(ctx, inst, key.as_str())
        }
        "class_alias" => types::lower_class_alias(ctx, inst),
        "get_class" | "get_parent_class" => types::lower_class_name_lookup(ctx, inst, key.as_str()),
        "is_a" | "is_subclass_of" => types::lower_is_a_relation(ctx, inst, key.as_str()),
        "class_implements" | "class_parents" | "class_uses" => {
            class_relations::lower_class_relation(ctx, inst, key.as_str())
        }
        "class_attribute_names" => attributes::lower_class_attribute_names(ctx, inst),
        "class_attribute_args" => attributes::lower_class_attribute_args(ctx, inst),
        "class_get_attributes" => attributes::lower_class_get_attributes(ctx, inst),
        "get_declared_classes" | "get_declared_interfaces" | "get_declared_traits" => {
            types::lower_get_declared_names(ctx, inst, key.as_str())
        }
        "is_callable" => lower_is_callable(ctx, inst),
        "print_r" => debug::lower_print_r(ctx, inst),
        "var_dump" => debug::lower_var_dump(ctx, inst),
        "is_int" => lower_static_type_predicate(ctx, inst, "is_int", PhpType::Int),
        "is_float" => lower_static_type_predicate(ctx, inst, "is_float", PhpType::Float),
        "is_bool" => lower_static_type_predicate(ctx, inst, "is_bool", PhpType::Bool),
        "is_null" => lower_is_null_builtin(ctx, inst),
        "is_string" => lower_static_type_predicate(ctx, inst, "is_string", PhpType::Str),
        "is_resource" => types::lower_is_resource(ctx, inst),
        "is_iterable" => lower_is_iterable(ctx, inst),
        "is_array" => lower_is_array(ctx, inst),
        "is_object" => lower_is_object(ctx, inst),
        "is_scalar" => lower_is_scalar(ctx, inst),
        "get_resource_type" => types::lower_get_resource_type(ctx, inst),
        "get_resource_id" => types::lower_get_resource_id(ctx, inst),
        "is_numeric" => is_numeric::lower_is_numeric(ctx, inst),
        "is_nan" => math::lower_is_nan(ctx, inst),
        "is_infinite" => math::lower_is_infinite(ctx, inst),
        "is_finite" => math::lower_is_finite(ctx, inst),
        "number_format" => strings::lower_number_format(ctx, inst),
        "strtolower" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "strtolower",
            "__rt_strtolower",
        ),
        "strtoupper" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "strtoupper",
            "__rt_strtoupper",
        ),
        "strrev" => strings::lower_unary_string_runtime(ctx, inst, "strrev", "__rt_strrev"),
        "grapheme_strrev" => strings::lower_grapheme_strrev(ctx, inst),
        "str_repeat" => strings::lower_str_repeat(ctx, inst),
        "substr" => strings::lower_substr(ctx, inst),
        "substr_replace" => strings::lower_substr_replace(ctx, inst),
        "strstr" => strings::lower_strstr(ctx, inst),
        "str_replace" => strings::lower_string_replace(ctx, inst, "str_replace", "__rt_str_replace"),
        "str_ireplace" => {
            strings::lower_string_replace(ctx, inst, "str_ireplace", "__rt_str_ireplace")
        }
        "explode" => strings::lower_explode(ctx, inst),
        "implode" => strings::lower_implode(ctx, inst),
        "str_split" => strings::lower_str_split(ctx, inst),
        "sscanf" => strings::lower_sscanf(ctx, inst),
        "ucfirst" => strings::lower_ucfirst(ctx, inst),
        "lcfirst" => strings::lower_lcfirst(ctx, inst),
        "ucwords" => strings::lower_unary_string_runtime(ctx, inst, "ucwords", "__rt_ucwords"),
        "trim" => strings::lower_trim_like(ctx, inst, "trim", "__rt_trim", "__rt_trim_mask"),
        "ltrim" => strings::lower_trim_like(ctx, inst, "ltrim", "__rt_ltrim", "__rt_ltrim_mask"),
        "rtrim" | "chop" => {
            strings::lower_trim_like(ctx, inst, key.as_str(), "__rt_rtrim", "__rt_rtrim_mask")
        }
        "strcmp" => strings::lower_binary_string_runtime(ctx, inst, "strcmp", "__rt_strcmp"),
        "strcasecmp" => {
            strings::lower_binary_string_runtime(ctx, inst, "strcasecmp", "__rt_strcasecmp")
        }
        "str_contains" => strings::lower_str_contains(ctx, inst),
        "strpos" => strings::lower_string_position(ctx, inst, "strpos", "__rt_strpos"),
        "strrpos" => strings::lower_string_position(ctx, inst, "strrpos", "__rt_strrpos"),
        "str_starts_with" => strings::lower_binary_string_runtime(
            ctx,
            inst,
            "str_starts_with",
            "__rt_str_starts_with",
        ),
        "str_ends_with" => strings::lower_binary_string_runtime(
            ctx,
            inst,
            "str_ends_with",
            "__rt_str_ends_with",
        ),
        "ord" => strings::lower_ord(ctx, inst),
        "chr" => strings::lower_chr(ctx, inst),
        "addslashes" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "addslashes",
            "__rt_addslashes",
        ),
        "stripslashes" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "stripslashes",
            "__rt_stripslashes",
        ),
        "nl2br" => strings::lower_unary_string_runtime(ctx, inst, "nl2br", "__rt_nl2br"),
        "wordwrap" => strings::lower_wordwrap(ctx, inst),
        "bin2hex" => strings::lower_unary_string_runtime(ctx, inst, "bin2hex", "__rt_bin2hex"),
        "hex2bin" => strings::lower_unary_string_runtime(ctx, inst, "hex2bin", "__rt_hex2bin"),
        "htmlspecialchars" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "htmlspecialchars",
            "__rt_htmlspecialchars",
        ),
        "htmlentities" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "htmlentities",
            "__rt_htmlspecialchars",
        ),
        "html_entity_decode" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "html_entity_decode",
            "__rt_html_entity_decode",
        ),
        "urlencode" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "urlencode",
            "__rt_urlencode",
        ),
        "urldecode" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "urldecode",
            "__rt_urldecode",
        ),
        "rawurlencode" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "rawurlencode",
            "__rt_rawurlencode",
        ),
        "rawurldecode" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "rawurldecode",
            "__rt_urldecode",
        ),
        "base64_encode" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "base64_encode",
            "__rt_base64_encode",
        ),
        "base64_decode" => strings::lower_unary_string_runtime(
            ctx,
            inst,
            "base64_decode",
            "__rt_base64_decode",
        ),
        "md5" => strings::lower_md5(ctx, inst),
        "sha1" => strings::lower_sha1(ctx, inst),
        "hash" => strings::lower_hash(ctx, inst),
        "hash_hmac" => strings::lower_hash_hmac(ctx, inst),
        "hash_equals" => strings::lower_hash_equals(ctx, inst),
        "hash_algos" => strings::lower_hash_algos(ctx, inst),
        "hash_init" => strings::lower_hash_init(ctx, inst),
        "hash_update" => strings::lower_hash_update(ctx, inst),
        "hash_final" => strings::lower_hash_final(ctx, inst),
        "hash_copy" => strings::lower_hash_copy(ctx, inst),
        "hash_file" => io::lower_hash_file(ctx, inst),
        "crc32" => strings::lower_crc32(ctx, inst),
        "gzcompress" => strings::lower_gzcompress(ctx, inst),
        "gzdeflate" => strings::lower_gzdeflate(ctx, inst),
        "gzinflate" => strings::lower_gzinflate(ctx, inst),
        "gzuncompress" => strings::lower_gzuncompress(ctx, inst),
        "long2ip" => strings::lower_long2ip(ctx, inst),
        "ip2long" => strings::lower_ip2long(ctx, inst),
        "inet_ntop" => strings::lower_inet(ctx, inst, "inet_ntop", "__rt_inet_ntop"),
        "inet_pton" => strings::lower_inet(ctx, inst, "inet_pton", "__rt_inet_pton"),
        "str_pad" => strings::lower_str_pad(ctx, inst),
        "sprintf" => strings::lower_sprintf(ctx, inst),
        "printf" => strings::lower_printf(ctx, inst),
        "vsprintf" => strings::lower_vsprintf(ctx, inst),
        "vprintf" => strings::lower_vprintf(ctx, inst),
        "ctype_alpha" => ctype::lower_ctype_alpha(ctx, inst),
        "ctype_digit" => ctype::lower_ctype_digit(ctx, inst),
        "ctype_alnum" => ctype::lower_ctype_alnum(ctx, inst),
        "ctype_space" => ctype::lower_ctype_space(ctx, inst),
        "spl_autoload_register" => spl::lower_spl_autoload_bool(ctx, inst, "spl_autoload_register"),
        "spl_autoload_unregister" => spl::lower_spl_autoload_bool(ctx, inst, "spl_autoload_unregister"),
        "spl_autoload_functions" => spl::lower_spl_autoload_functions(ctx, inst),
        "spl_autoload_extensions" => spl::lower_spl_autoload_extensions(ctx, inst),
        "spl_autoload_call" => spl::lower_spl_autoload_void(ctx, inst, "spl_autoload_call"),
        "spl_autoload" => spl::lower_spl_autoload_void(ctx, inst, "spl_autoload"),
        "spl_object_id" => spl::lower_spl_object_id(ctx, inst),
        "spl_object_hash" => spl::lower_spl_object_hash(ctx, inst),
        "spl_classes" => spl::lower_spl_classes(ctx, inst),
        "iterator_apply" => spl::lower_iterator_apply(ctx, inst),
        "iterator_count" => spl::lower_iterator_count(ctx, inst),
        "iterator_to_array" => spl::lower_iterator_to_array(ctx, inst),
        _ => Err(CodegenIrError::unsupported(format!("builtin call {}", name))),
    }
}

/// Lowers an EIR native indexed-array `isset($array[$offset])` probe.
pub(super) fn lower_array_isset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    isset::lower_array_isset(ctx, inst)
}

/// Lowers an EIR native associative-array `isset($hash[$key])` probe.
pub(super) fn lower_hash_isset(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    isset::lower_hash_isset(ctx, inst)
}

/// Lowers `define("NAME", value)` with the legacy duplicate-name runtime guard.
fn lower_define(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "define", 2)?;
    let name_value = expect_operand(inst, 0)?;
    let value = expect_operand(inst, 1)?;
    let constant_name = const_string_operand(ctx, name_value)?;
    let flag_symbol = ctx.data.add_comm(define_seen_symbol(&constant_name), 8);
    let global_symbol = ir_global_symbol(&constant_name);
    let value_ty = ctx.value_php_type(value)?;
    ctx.data
        .add_comm(global_symbol.clone(), value_ty.codegen_repr().stack_size().max(8));

    let first_label = ctx.next_label("define_first");
    let done_label = ctx.next_label("define_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &flag_symbol, 0);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &first_label);
    emit_duplicate_define_warning(ctx);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&first_label);
    ctx.load_value_to_result(value)?;
    abi::emit_store_result_to_symbol(ctx.emitter, &global_symbol, &value_ty, false);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, &flag_symbol, 0);

    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Emits the PHP warning for a repeated `define()` call.
fn emit_duplicate_define_warning(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.adrp("x1", "_diag_define_already_defined_msg");
            ctx.emitter.add_lo12("x1", "x1", "_diag_define_already_defined_msg");
            ctx.emitter.instruction(&format!("mov x2, #{}", DEFINE_ALREADY_DEFINED_WARNING.len())); // pass the duplicate-define warning byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("lea rdi, [rip + _diag_define_already_defined_msg]"); // pass the duplicate-define warning pointer
            ctx.emitter.instruction(&format!("mov esi, {}", DEFINE_ALREADY_DEFINED_WARNING.len())); // pass the duplicate-define warning byte length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Lowers `pi()` as the same data-section float constant used by the legacy backend.
fn lower_pi(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "pi", 0)?;
    let label = ctx.data.add_float(std::f64::consts::PI);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.adrp("x9", &label);                                     // load the page address that contains the M_PI floating constant
            ctx.emitter.ldr_lo12("d0", "x9", &label);                          // load the M_PI floating constant into the floating result register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("movsd xmm0, QWORD PTR [rip + {}]", label)); // load the M_PI floating constant into the floating result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `gettype(value)` for statically concrete PHP types.
fn lower_gettype(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "gettype", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.raw_value_php_type(value)?;
    if matches!(ty, PhpType::TaggedScalar) {
        emit_tagged_scalar_gettype(ctx, value)?;
        return store_if_result(ctx, inst);
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        emit_mixed_gettype(ctx, value)?;
        return store_if_result(ctx, inst);
    }
    let Some(type_name) = static_gettype_name(&ty) else {
        return Err(CodegenIrError::unsupported(format!(
            "gettype for PHP type {:?}",
            ty
        )));
    };
    emit_type_name_result(ctx, type_name);
    store_if_result(ctx, inst)
}

/// Emits `gettype()` for an inline tagged scalar by dispatching on its tag word.
fn emit_tagged_scalar_gettype(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let null_case = ctx.next_label("gettype_tagged_null");
    let done = ctx.next_label("gettype_tagged_done");
    ctx.load_value_to_result(value)?;
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &null_case);
    emit_type_name_result(ctx, b"integer");
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&null_case);
    emit_type_name_result(ctx, b"NULL");
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits `gettype()` for a boxed Mixed or Union payload by dispatching on runtime tags.
fn emit_mixed_gettype(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let integer_case = ctx.next_label("gettype_mixed_integer");
    let double_case = ctx.next_label("gettype_mixed_double");
    let string_case = ctx.next_label("gettype_mixed_string");
    let boolean_case = ctx.next_label("gettype_mixed_boolean");
    let null_case = ctx.next_label("gettype_mixed_null");
    let array_case = ctx.next_label("gettype_mixed_array");
    let object_case = ctx.next_label("gettype_mixed_object");
    let resource_case = ctx.next_label("gettype_mixed_resource");
    let done = ctx.next_label("gettype_mixed_done");
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    emit_branch_on_gettype_mixed_tag(ctx, 0, &integer_case);
    emit_branch_on_gettype_mixed_tag(ctx, 1, &string_case);
    emit_branch_on_gettype_mixed_tag(ctx, 2, &double_case);
    emit_branch_on_gettype_mixed_tag(ctx, 3, &boolean_case);
    emit_branch_on_gettype_mixed_tag(ctx, 4, &array_case);
    emit_branch_on_gettype_mixed_tag(ctx, 5, &array_case);
    emit_branch_on_gettype_mixed_tag(ctx, 6, &object_case);
    emit_branch_on_gettype_mixed_tag(ctx, 9, &resource_case);
    abi::emit_jump(ctx.emitter, &null_case);

    emit_mixed_gettype_case(ctx, &integer_case, b"integer", &done);
    emit_mixed_gettype_case(ctx, &double_case, b"double", &done);
    emit_mixed_gettype_case(ctx, &string_case, b"string", &done);
    emit_mixed_gettype_case(ctx, &boolean_case, b"boolean", &done);
    emit_mixed_gettype_case(ctx, &null_case, b"NULL", &done);
    emit_mixed_gettype_case(ctx, &array_case, b"array", &done);
    emit_mixed_gettype_case(ctx, &object_case, b"object", &done);
    emit_mixed_gettype_case(ctx, &resource_case, b"resource", &done);
    ctx.emitter.label(&done);
    Ok(())
}

/// Branches to a `gettype()` case when the unboxed Mixed runtime tag matches.
fn emit_branch_on_gettype_mixed_tag(ctx: &mut FunctionContext<'_>, tag: u8, label: &str) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp x0, #{}", tag));              // compare the unboxed Mixed tag against this gettype() case
            ctx.emitter.instruction(&format!("b.eq {}", label));                // branch to the matching gettype() type-name case
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp rax, {}", tag));              // compare the unboxed Mixed tag against this gettype() case
            ctx.emitter.instruction(&format!("je {}", label));                  // branch to the matching gettype() type-name case
        }
    }
}

/// Selects one static PHP type-name string and rejoins the `gettype()` dispatch.
fn emit_mixed_gettype_case(ctx: &mut FunctionContext<'_>, label: &str, type_name: &[u8], done: &str) {
    ctx.emitter.label(label);
    emit_type_name_result(ctx, type_name);
    abi::emit_jump(ctx.emitter, done);
}

/// Returns PHP's `gettype()` spelling for concrete statically known types.
fn static_gettype_name(ty: &PhpType) -> Option<&'static [u8]> {
    match ty {
        PhpType::Int => Some(b"integer".as_slice()),
        PhpType::Float => Some(b"double".as_slice()),
        PhpType::Str => Some(b"string".as_slice()),
        PhpType::Bool => Some(b"boolean".as_slice()),
        PhpType::Void | PhpType::Never => Some(b"NULL".as_slice()),
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            Some(b"array".as_slice())
        }
        PhpType::Callable => Some(b"callable".as_slice()),
        PhpType::Object(_) => Some(b"object".as_slice()),
        PhpType::Pointer(_) => Some(b"pointer".as_slice()),
        PhpType::Buffer(_) => Some(b"buffer".as_slice()),
        PhpType::Packed(_) => Some(b"packed".as_slice()),
        PhpType::Resource(_) => Some(b"resource".as_slice()),
        PhpType::Mixed | PhpType::Union(_) | PhpType::TaggedScalar => None,
    }
}

/// Emits a static PHP type-name string into the target string result registers.
fn emit_type_name_result(ctx: &mut FunctionContext<'_>, type_name: &[u8]) {
    let (label, len) = ctx.data.add_string(type_name);
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
}

/// Lowers `phpversion()` as the compiler package version string.
fn lower_phpversion(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "phpversion", 0)?;
    let (label, len) = ctx.data.add_string(env!("CARGO_PKG_VERSION").as_bytes());
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    store_if_result(ctx, inst)
}

/// Lowers `defined("NAME")` for compile-time string constant names.
fn lower_defined(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "defined", 1)?;
    let value = expect_operand(inst, 0)?;
    let constant_name = const_string_operand(ctx, value)?;
    emit_static_bool(ctx, ctx.has_global_name(&constant_name));
    store_if_result(ctx, inst)
}

/// Lowers `function_exists("name")` for compile-time string names.
///
/// Recognizes user functions, externs, catalog builtins, and the date/time procedural aliases
/// that `name_resolver` desugars (including the injected timezone-introspection prelude
/// functions). The aliases are matched through `is_date_procedural_alias` rather than the catalog
/// because their call sites are rewritten before codegen, so they never reach the builtin catalog
/// yet must still report as existing to match PHP.
fn lower_function_exists(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "function_exists", 1)?;
    let value = expect_operand(inst, 0)?;
    let function_name = const_string_operand(ctx, value)?;
    if let Some(group_name) = ctx.function_variant_group_name(&function_name) {
        emit_variant_function_exists(ctx, &group_name);
    } else {
        let exists = ctx.function_by_name(&function_name).is_some()
            || ctx.has_extern_function(&function_name)
            || is_php_visible_builtin_function(function_name.trim_start_matches('\\'))
            || crate::name_resolver::is_date_procedural_alias(&function_name);
        emit_static_bool(ctx, exists);
    }
    store_if_result(ctx, inst)
}

/// Lowers AOT class/interface/enum existence checks for literal names.
fn lower_class_like_exists(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 1, 2)?;
    let value = expect_operand(inst, 0)?;
    let symbol_name = const_string_operand(ctx, value)?;
    let exists = match name {
        "class_exists" => contains_folded(
            ctx.module
                .class_infos
                .keys()
                .filter(|class_name| !is_internal_synthetic_class_name(class_name)),
            &symbol_name,
        ),
        "interface_exists" => contains_folded(ctx.module.interface_infos.keys(), &symbol_name),
        "trait_exists" => contains_folded(ctx.module.trait_table.names.iter(), &symbol_name),
        "enum_exists" => contains_folded(ctx.module.enum_infos.keys(), &symbol_name),
        _ => false,
    };
    emit_static_bool(ctx, exists);
    store_if_result(ctx, inst)
}

/// Lowers `is_callable(value)` through static lookup or runtime callable-shape helpers.
fn lower_is_callable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_callable", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Callable => emit_static_bool(ctx, true),
        PhpType::Str => {
            if let Ok(function_name) = const_string_operand(ctx, value) {
                if let Some((class_name, method_name)) = function_name.rsplit_once("::") {
                    emit_static_bool(ctx, static_method_string_is_callable(ctx, class_name, method_name));
                } else {
                    emit_static_bool(ctx, callable_name_exists(ctx, &function_name));
                }
            } else {
                ctx.load_value_to_result(value)?;
                emit_is_callable_dynamic_string_lookup(ctx);
            }
        }
        PhpType::Array(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_array");
        }
        PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_assoc");
        }
        PhpType::Object(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_object");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_mixed");
        }
        PhpType::Iterable => {
            ctx.load_value_to_result(value)?;
            emit_is_callable_pointer_lookup(ctx, "__rt_is_callable_heap");
        }
        PhpType::Int
        | PhpType::Bool
        | PhpType::Float
        | PhpType::Void
        | PhpType::Never
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => {
            emit_static_bool(ctx, false);
        }
    }
    store_if_result(ctx, inst)
}

/// Calls the runtime `is_callable` helper for pointer-shaped values already in result regs.
fn emit_is_callable_pointer_lookup(ctx: &mut FunctionContext<'_>, label: &str) {
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // move pointer-shaped value into helper argument 0
    }
    abi::emit_call_label(ctx.emitter, label);
}

/// Calls the runtime `is_callable` string-name helper for a loaded dynamic string value.
fn emit_is_callable_dynamic_string_lookup(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // move string pointer into helper argument 0
            ctx.emitter.instruction("mov x1, x2");                              // move string length into helper argument 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // move string pointer into helper argument 0
            ctx.emitter.instruction("mov rsi, rdx");                            // move string length into helper argument 1
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_is_callable_string");
}

/// Returns true when a static `Class::method` string names a public static method.
fn static_method_string_is_callable(
    ctx: &FunctionContext<'_>,
    class_name: &str,
    method_name: &str,
) -> bool {
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    let Some((_, class_info)) = ctx.module.class_infos.iter().find(|(candidate, _)| {
        php_symbol_key(candidate.trim_start_matches('\\')) == class_key
    }) else {
        return false;
    };
    let method_key = php_symbol_key(method_name);
    if !class_info.static_methods.contains_key(&method_key) {
        return false;
    }
    class_info.static_method_visibilities.get(&method_key) == Some(&Visibility::Public)
}

/// Emits a runtime check for whether an include-loaded function variant is active.
fn emit_variant_function_exists(ctx: &mut FunctionContext<'_>, function_name: &str) {
    let active_symbol = crate::names::function_variant_active_symbol(function_name);
    ctx.data.add_comm(active_symbol.clone(), 8);
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &active_symbol, 0);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));        // test whether an include has activated this function variant
            ctx.emitter.instruction(&format!("cset {}, ne", result_reg));       // return true only when a function variant is active
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("test {}, {}", result_reg, result_reg)); // test whether an include has activated this function variant
            ctx.emitter.instruction("setne al");                                // return true only when a function variant is active
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Lowers `count(array)` for concrete array values by reading the runtime length header.
fn lower_count(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "count", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?.codegen_repr();
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => {
            ctx.load_value_to_result(value)?;
            let result_reg = abi::int_result_reg(ctx.emitter);
            abi::emit_load_from_address(ctx.emitter, result_reg, result_reg, 0);
            store_if_result(ctx, inst)
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_count");
            store_if_result(ctx, inst)
        }
        PhpType::Object(class_name)
            if super::class_implements_interface(ctx, &class_name, "Countable") =>
        {
            if let Some(intrinsic) = super::runtime_backed_instance_intrinsic(&class_name, "count") {
                super::lower_instance_runtime_intrinsic(ctx, inst, &class_name, "count", intrinsic)
            } else {
                super::lower_runtime_object_method_call(ctx, inst, &class_name, "count")
            }
        }
        other => Err(CodegenIrError::unsupported(format!(
            "count for PHP type {:?}",
            other
        ))),
    }
}

/// Lowers the synthetic `closure_bind` call: rebinds a closure's captured
/// `$this` to a new receiver via `__rt_closure_bind(descriptor, new_this)`,
/// returning the rebound closure descriptor.
fn lower_closure_bind(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "closure_bind", 2)?;
    let descriptor = expect_operand(inst, 0)?;
    let new_this = expect_operand(inst, 1)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.load_value_to_reg(descriptor, "x0")?;
            ctx.load_value_to_reg(new_this, "x1")?;
        }
        Arch::X86_64 => {
            ctx.load_value_to_reg(descriptor, "rdi")?;
            ctx.load_value_to_reg(new_this, "rsi")?;
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_closure_bind");
    store_if_result(ctx, inst)
}

/// Lowers `strlen()` by coercing string-like values and returning the byte length.
fn lower_strlen(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "strlen", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    match ty.codegen_repr() {
        PhpType::Str => {}
        PhpType::Mixed | PhpType::Union(_) => {
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "strlen for PHP type {:?}",
                other
            )));
        }
    }
    let result_reg = abi::int_result_reg(ctx.emitter);
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, len_reg)); // return the byte length of the loaded PHP string
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", result_reg, len_reg)); // return the byte length of the loaded PHP string
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `intval()` for concrete scalar operands.
fn lower_intval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "intval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            abi::emit_float_result_to_int_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_int");
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "intval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `floatval()` for concrete scalar operands.
fn lower_floatval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "floatval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
            abi::emit_int_result_to_float_result(ctx.emitter);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_str_to_number");
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "floatval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `boolval()` using the same concrete scalar PHP truthiness rules as `IsTruthy`.
fn lower_boolval(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "boolval", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Bool | PhpType::Int => {
            ctx.load_value_to_result(value)?;
            predicates::emit_int_result_nonzero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            predicates::emit_float_result_nonzero_bool(ctx);
        }
        PhpType::Str => {
            predicates::emit_string_truthiness(ctx, value)?;
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "boolval for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `empty()` for concrete scalar and array-like operands.
fn lower_empty(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "empty", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.raw_value_php_type(value)? {
        PhpType::Int | PhpType::Bool | PhpType::Pointer(_) => {
            ctx.load_value_to_result(value)?;
            emit_int_result_zero_bool(ctx);
        }
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
        }
        PhpType::Float => {
            ctx.load_value_to_result(value)?;
            emit_float_result_zero_bool(ctx);
        }
        PhpType::Str => {
            ctx.load_value_to_result(value)?;
            emit_string_length_zero_bool(ctx);
        }
        PhpType::TaggedScalar => {
            emit_tagged_scalar_empty_bool(ctx, value)?;
        }
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => {
            predicates::emit_array_truthiness(ctx, value)?;
            invert_bool_result(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_is_empty");
        }
        PhpType::Callable | PhpType::Object(_) | PhpType::Resource(_) => {
            abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        }
        other => {
            return Err(CodegenIrError::unsupported(format!(
                "empty for PHP type {:?}",
                other
            )))
        }
    }
    store_if_result(ctx, inst)
}

/// Emits true for a tagged scalar that is null or an integer zero.
fn emit_tagged_scalar_empty_bool(ctx: &mut FunctionContext<'_>, value: crate::ir::ValueId) -> Result<()> {
    let empty_label = ctx.next_label("empty_tagged_true");
    let done_label = ctx.next_label("empty_tagged_done");
    ctx.load_value_to_result(value)?;
    crate::codegen::sentinels::emit_branch_if_tagged_scalar_null(ctx.emitter, &empty_label);
    emit_int_result_zero_bool(ctx);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&empty_label);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits true when the canonical integer result register is zero.
fn emit_int_result_zero_bool(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", result_reg));        // compare the empty() integer operand against zero
            ctx.emitter.instruction(&format!("cset {}, eq", result_reg));       // return true when the integer operand is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", result_reg));         // compare the empty() integer operand against zero
            ctx.emitter.instruction("sete al");                                 // materialize true when the integer operand is zero
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Emits true when the canonical float result register is zero.
fn emit_float_result_zero_bool(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("fcmp d0, #0.0");                           // compare the empty() float operand against zero
            ctx.emitter.instruction("cset x0, eq");                             // return true when the float operand is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xorpd xmm1, xmm1");                        // materialize a zero float register for empty() comparison
            ctx.emitter.instruction("ucomisd xmm0, xmm1");                      // compare the empty() float operand against zero
            ctx.emitter.instruction("sete al");                                 // materialize true when the float operand is zero
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Emits true when the loaded string length register is zero.
fn emit_string_length_zero_bool(ctx: &mut FunctionContext<'_>) {
    let len_reg = abi::string_result_regs(ctx.emitter).1;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, #0", len_reg));           // compare the empty() string length against zero
            ctx.emitter.instruction("cset x0, eq");                             // return true when the string length is zero
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, 0", len_reg));            // compare the empty() string length against zero
            ctx.emitter.instruction("sete al");                                 // materialize true when the string length is zero
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
}

/// Inverts a canonical 0/1 boolean result in the integer result register.
fn invert_bool_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("eor x0, x0, #1");                          // invert the canonical boolean result for empty()
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor rax, 1");                              // invert the canonical boolean result for empty()
        }
    }
}

/// Lowers a static `is_*` predicate for concrete non-Mixed values.
fn lower_static_type_predicate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    expected: PhpType,
) -> Result<()> {
    ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?;
    if ty == PhpType::TaggedScalar {
        if expected == PhpType::Int {
            emit_tagged_scalar_int_predicate(ctx, value)?;
        } else {
            emit_static_bool(ctx, false);
        }
        return store_if_result(ctx, inst);
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        if let Some(tag) = mixed_type_predicate_tag(&expected) {
            predicates::emit_mixed_tag_eq(ctx, value, tag)?;
        } else {
            emit_static_bool(ctx, false);
        }
        return store_if_result(ctx, inst);
    }
    emit_static_bool(ctx, ty == expected);
    store_if_result(ctx, inst)
}

/// Emits `is_int()` for a tagged scalar by checking that its tag is not null.
fn emit_tagged_scalar_int_predicate(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
) -> Result<()> {
    ctx.load_value_to_result(value)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            let cmp_inst = format!(
                "cmp x1, #{}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // does the tagged scalar carry the runtime null tag?
            ctx.emitter.instruction("cset x0, ne");                             // materialize true when the tagged scalar holds an integer
        }
        Arch::X86_64 => {
            let cmp_inst = format!(
                "cmp rdx, {}",
                crate::codegen::sentinels::TAGGED_SCALAR_TAG_NULL
            );
            ctx.emitter.instruction(&cmp_inst);                                 // does the tagged scalar carry the runtime null tag?
            ctx.emitter.instruction("setne al");                                // materialize true when the tagged scalar holds an integer
            ctx.emitter.instruction("movzx rax, al");                           // widen the boolean byte into the integer result register
        }
    }
    Ok(())
}

/// Lowers `is_iterable()` for concrete values and boxed Mixed payloads.
fn lower_is_iterable(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_iterable", 1)?;
    let value = expect_operand(inst, 0)?;
    let ty = ctx.value_php_type(value)?;
    let result = match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => true,
        PhpType::Object(name) => object_type_implements_iterable(ctx, &name),
        PhpType::Int
        | PhpType::Float
        | PhpType::Str
        | PhpType::Bool
        | PhpType::Void
        | PhpType::Never
        | PhpType::Callable
        | PhpType::Pointer(_)
        | PhpType::Buffer(_)
        | PhpType::Packed(_)
        | PhpType::Resource(_)
        | PhpType::TaggedScalar => false,
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_is_iterable(ctx, value)?;
            return store_if_result(ctx, inst);
        }
    };
    emit_static_bool(ctx, result);
    store_if_result(ctx, inst)
}

/// Emits runtime `is_iterable()` checks for a boxed Mixed or Union value.
fn emit_mixed_is_iterable(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let true_case = ctx.next_label("is_iterable_mixed_true");
    let object_case = ctx.next_label("is_iterable_mixed_object");
    let done = ctx.next_label("is_iterable_mixed_done");
    let ty = ctx.load_value_to_result(value)?;
    if !matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "is_iterable Mixed check for PHP type {:?}",
            ty
        )));
    }
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #4");                              // check for a boxed indexed-array payload
            ctx.emitter.instruction(&format!("b.eq {}", true_case));            // indexed arrays satisfy is_iterable
            ctx.emitter.instruction("cmp x0, #5");                              // check for a boxed associative-array payload
            ctx.emitter.instruction(&format!("b.eq {}", true_case));            // associative arrays satisfy is_iterable
            ctx.emitter.instruction("cmp x0, #6");                              // check for a boxed object payload
            ctx.emitter.instruction(&format!("b.eq {}", object_case));          // objects need a Traversable interface check
            ctx.emitter.instruction("mov x0, #0");                              // all other Mixed payloads are not iterable
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the truthy result path
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 4");                              // check for a boxed indexed-array payload
            ctx.emitter.instruction(&format!("je {}", true_case));              // indexed arrays satisfy is_iterable
            ctx.emitter.instruction("cmp rax, 5");                              // check for a boxed associative-array payload
            ctx.emitter.instruction(&format!("je {}", true_case));              // associative arrays satisfy is_iterable
            ctx.emitter.instruction("cmp rax, 6");                              // check for a boxed object payload
            ctx.emitter.instruction(&format!("je {}", object_case));            // objects need a Traversable interface check
            ctx.emitter.instruction("mov rax, 0");                              // all other Mixed payloads are not iterable
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the truthy result path
        }
    }
    ctx.emitter.label(&object_case);
    emit_runtime_object_iterable_check(ctx, &true_case, &done);
    ctx.emitter.label(&true_case);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    ctx.emitter.label(&done);
    Ok(())
}

/// Emits the object half of runtime `is_iterable()` by checking Traversable interfaces.
fn emit_runtime_object_iterable_check(
    ctx: &mut FunctionContext<'_>,
    true_case: &str,
    done: &str,
) {
    let object_true = ctx.next_label("is_iterable_object_true");
    let interface_ids = traversable_interface_ids(ctx);
    if interface_ids.is_empty() {
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_jump(ctx.emitter, done);
        return;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("str x1, [sp, #-16]!");                     // preserve the unboxed object pointer across Traversable checks
            for interface_id in interface_ids {
                emit_saved_object_interface_check(ctx, interface_id, &object_true);
            }
            ctx.emitter.instruction("add sp, sp, #16");                         // discard the saved object pointer after failed checks
            ctx.emitter.instruction("mov x0, #0");                              // non-Traversable objects are not iterable
            ctx.emitter.instruction(&format!("b {}", done));                    // skip the truthy result path
            ctx.emitter.label(&object_true);
            ctx.emitter.instruction("add sp, sp, #16");                         // discard the saved object pointer before returning true
            ctx.emitter.instruction(&format!("b {}", true_case));               // continue through the shared truthy result path
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rdi");
            for interface_id in interface_ids {
                emit_saved_object_interface_check(ctx, interface_id, &object_true);
            }
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("xor eax, eax");                            // non-Traversable objects are not iterable
            ctx.emitter.instruction(&format!("jmp {}", done));                  // skip the truthy result path
            ctx.emitter.label(&object_true);
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction(&format!("jmp {}", true_case));             // continue through the shared truthy result path
        }
    }
}

/// Emits one interface matcher call for a saved object pointer.
fn emit_saved_object_interface_check(
    ctx: &mut FunctionContext<'_>,
    interface_id: u64,
    true_case: &str,
) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the object pointer as matcher argument 1
            abi::emit_load_int_immediate(ctx.emitter, "x1", interface_id as i64);
            abi::emit_load_int_immediate(ctx.emitter, "x2", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");        // check whether the object implements the Traversable interface
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the runtime matcher succeeded
            ctx.emitter.instruction(&format!("b.ne {}", true_case));            // a matching interface makes the object iterable
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the object pointer as matcher argument 1
            abi::emit_load_int_immediate(ctx.emitter, "rsi", interface_id as i64);
            abi::emit_load_int_immediate(ctx.emitter, "rdx", 1);
            abi::emit_call_label(ctx.emitter, "__rt_exception_matches");        // check whether the object implements the Traversable interface
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime matcher succeeded
            ctx.emitter.instruction(&format!("jne {}", true_case));             // a matching interface makes the object iterable
        }
    }
}

/// Returns runtime interface IDs for the interfaces that make an object iterable.
fn traversable_interface_ids(ctx: &FunctionContext<'_>) -> Vec<u64> {
    ["Iterator", "IteratorAggregate"]
        .into_iter()
        .filter_map(|name| {
            ctx.module
                .interface_infos
                .get(name)
                .map(|info| info.interface_id)
        })
        .collect()
}

/// Returns whether a statically known class or interface satisfies `is_iterable()`.
fn object_type_implements_iterable(ctx: &FunctionContext<'_>, type_name: &str) -> bool {
    let normalized = normalized_type_name(type_name);
    if let Some(class_info) = ctx.module.class_infos.get(normalized) {
        return class_info.interfaces.iter().any(|interface_name| {
            is_traversable_interface_name(interface_name)
                || interface_extends_traversable(ctx, interface_name)
        });
    }
    if ctx.module.interface_infos.contains_key(normalized) {
        return is_traversable_interface_name(normalized)
            || interface_extends_traversable(ctx, normalized);
    }
    false
}

/// Returns whether an interface name is one of PHP's Traversable contracts.
fn is_traversable_interface_name(interface_name: &str) -> bool {
    let key = php_symbol_key(normalized_type_name(interface_name));
    key == php_symbol_key("Iterator") || key == php_symbol_key("IteratorAggregate")
}

/// Returns whether an interface extends Iterator or IteratorAggregate.
fn interface_extends_traversable(ctx: &FunctionContext<'_>, interface_name: &str) -> bool {
    let mut stack = vec![normalized_type_name(interface_name).to_string()];
    while let Some(current) = stack.pop() {
        if is_traversable_interface_name(&current) {
            return true;
        }
        if let Some(interface_info) = ctx.module.interface_infos.get(&current) {
            stack.extend(
                interface_info
                    .parents
                    .iter()
                    .map(|parent| normalized_type_name(parent).to_string()),
            );
        }
    }
    false
}

/// Normalizes a PHP class or interface name for metadata lookups.
fn normalized_type_name(type_name: &str) -> &str {
    type_name.trim_start_matches('\\')
}

/// Lowers `is_null()` for concrete scalar values and boxed Mixed payloads.
fn lower_is_null_builtin(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_null", 1)?;
    let value = expect_operand(inst, 0)?;
    predicates::emit_is_null_result(ctx, value)?;
    store_if_result(ctx, inst)
}

/// Lowers `is_array()`: true for statically-known arrays/hashes, or a boxed Mixed/Union value
/// whose runtime tag is an indexed (4) or associative (5) array. An `iterable`-typed value is
/// not treated as a definite array here (it may hold a Traversable); use `is_iterable` for that.
fn lower_is_array(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_array", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Array(_) | PhpType::AssocArray { .. } => emit_static_bool(ctx, true),
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[4, 5])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_object()`: true for statically-known objects, or a boxed Mixed/Union value whose
/// runtime tag is an object (6).
fn lower_is_object(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_object", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Object(_) => emit_static_bool(ctx, true),
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[6])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Lowers `is_scalar()`: true for int/float/string/bool, a non-null tagged scalar, or a boxed
/// Mixed/Union value whose runtime tag is int (0), string (1), float (2), or bool (3). Null,
/// arrays, objects, and resources are not scalars, matching PHP.
fn lower_is_scalar(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "is_scalar", 1)?;
    let value = expect_operand(inst, 0)?;
    match ctx.value_php_type(value)? {
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool => {
            emit_static_bool(ctx, true)
        }
        PhpType::TaggedScalar => emit_tagged_scalar_int_predicate(ctx, value)?,
        PhpType::Mixed | PhpType::Union(_) => {
            predicates::emit_mixed_tag_membership(ctx, value, &[0, 1, 2, 3])?;
        }
        _ => emit_static_bool(ctx, false),
    }
    store_if_result(ctx, inst)
}

/// Returns the runtime Mixed tag used by a supported type predicate.
fn mixed_type_predicate_tag(expected: &PhpType) -> Option<u8> {
    match expected {
        PhpType::Int => Some(0),
        PhpType::Str => Some(1),
        PhpType::Float => Some(2),
        PhpType::Bool => Some(3),
        _ => None,
    }
}

/// Emits a boolean immediate into the integer result register.
fn emit_static_bool(ctx: &mut FunctionContext<'_>, value: bool) {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        i64::from(value),
    );
}

/// Returns true when a static callable name resolves to any known callable function.
fn callable_name_exists(ctx: &FunctionContext<'_>, name: &str) -> bool {
    ctx.function_variant_group_name(name).is_some()
        || ctx.function_by_name(name).is_some()
        || ctx.has_extern_function(name)
        || is_php_visible_builtin_function(name.trim_start_matches('\\'))
}

/// Checks whether a PHP symbol is present in an iterator of known names.
fn contains_folded<'a>(
    mut names: impl Iterator<Item = &'a String>,
    needle: &str,
) -> bool {
    let needle_key = php_symbol_key(needle.trim_start_matches('\\'));
    names.any(|name| php_symbol_key(name.trim_start_matches('\\')) == needle_key)
}

/// Returns true for internal helper classes that should not be visible to PHP class_exists().
fn is_internal_synthetic_class_name(name: &str) -> bool {
    php_symbol_key(name).starts_with("__elephc")
}

/// Returns a string literal value defined by a `ConstStr` instruction.
fn const_string_operand(ctx: &FunctionContext<'_>, value: ValueId) -> Result<String> {
    let value_ref = ctx
        .function
        .value(value)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))?;
    let ValueDef::Instruction { inst, .. } = value_ref.def else {
        return Err(CodegenIrError::unsupported(
            "function_exists with non-literal function name",
        ));
    };
    let inst_ref = ctx
        .function
        .instruction(inst)
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
    if inst_ref.op != Op::ConstStr {
        return Err(CodegenIrError::unsupported(
            "function_exists with non-literal function name",
        ));
    }
    let Some(Immediate::Data(data)) = inst_ref.immediate else {
        return Err(CodegenIrError::invalid_module(
            "function_exists string literal has no data id",
        ));
    };
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Verifies that the builtin call has the expected number of lowered operands.
fn ensure_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() == expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}

/// Verifies that the builtin call has at least the expected number of lowered operands.
fn ensure_min_arg_count(inst: &Instruction, name: &str, expected: usize) -> Result<()> {
    if inst.operands.len() >= expected {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected at least {} args, got {}",
        name,
        expected,
        inst.operands.len()
    )))
}

/// Verifies that the builtin call has between the expected lowered operand counts.
fn ensure_arg_count_between(
    inst: &Instruction,
    name: &str,
    min: usize,
    max: usize,
) -> Result<()> {
    if (min..=max).contains(&inst.operands.len()) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} to {} args, got {}",
        name,
        min,
        max,
        inst.operands.len()
    )))
}
