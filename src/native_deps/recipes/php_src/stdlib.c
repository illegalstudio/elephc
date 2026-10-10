/* stdlib.c — the slice of PHP's standard library (main/, SAPI, output
 * buffering, streams, ext/standard, SPL, ext/hash, ext/json) that extensions
 * reference but a compiled program does not route through PHP.
 *
 * Three kinds of entry live here, and the difference matters:
 *   - STATE extensions read: the core, file and SAPI globals, with PHP's CLI
 *     defaults. Defined, not stubbed: an extension reads them freely.
 *   - REGISTRATIONS made at MINIT: a stream wrapper, an output-handler alias, a
 *     header. They succeed and are dropped, because an extension that fails
 *     its MINIT is unusable even for the functions that never touch them —
 *     zstd registers `compress.zstd://` and an output handler, and its
 *     zstd_compress() needs neither.
 *   - OPERATIONS only PHP's runtime can perform: reading a stream, running an
 *     output handler, calling back into PHP. (serialize()/unserialize() are
 *     not among them: ext/standard/var.c and var_unserializer.c are compiled in.)
 *     These abort with their name when reached. Returning something plausible
 *     would be a silent miscompile; failing where the gap is keeps it visible.
 */

#include "php.h"
#include "php_globals.h"
#include "php_output.h"
#include "SAPI.h"
#include "zend_enum.h"
#include "zend_interfaces.h"
#include "ext/hash/php_hash.h"
#include "ext/json/php_json.h"
#include "ext/spl/spl_iterators.h"
#include "ext/standard/basic_functions.h"
#include "ext/standard/file.h"
#include "ext/standard/php_incomplete_class.h"
#include "ext/standard/php_string.h"
#include "ext/standard/php_var.h"
#include "ext/pcre/php_pcre.h"

#include <string.h>
#include <sys/time.h>

extern ZEND_COLD ZEND_NORETURN void elephc_zend_unsupported(const char *what);

/* ------------------------------------------------------------ state */

ZEND_API struct _php_core_globals core_globals;
PHPAPI php_basic_globals basic_globals;
PHPAPI php_file_globals file_globals;
SAPI_API sapi_globals_struct sapi_globals;
PHPAPI php_stream_ops php_stream_stdio_ops = { .label = "STDIO" };
PHP_JSON_API zend_class_entry *php_json_serializable_ce;
/* The SAPI a hosted extension sees is the CLI one: a compiled program is a
 * command-line process even when it serves HTTP itself. */
SAPI_API sapi_module_struct sapi_module = { .name = "cli", .pretty_name = "Command Line Interface" };

SAPI_API double sapi_get_request_time(void) {
    struct timeval now;
    gettimeofday(&now, NULL);
    return (double)now.tv_sec + (double)now.tv_usec / 1000000.0;
}

/* php_error_docref's va_list form, from main/main.c: the active function
 * prefixes the message. */
PHPAPI ZEND_COLD void php_verror(const char *docref, const char *params, int type, const char *format,
                                 va_list args) {
    (void)docref; (void)params;
    zend_string *message = zend_vstrpprintf(0, format, args);
    const char *function = get_active_function_name();
    if (function) {
        zend_error(type, "%s(): %s", function, ZSTR_VAL(message));
    } else {
        zend_error(type, "%s", ZSTR_VAL(message));
    }
    zend_string_release(message);
}

/* Called by elephc_zend_startup while the Core module is current. */
void elephc_stdlib_startup(void) {
    /* php-cli's defaults for what extensions consult. */
    PG(serialize_precision) = -1;
    PG(display_errors) = 1;
    PG(log_errors) = 0;
    PG(html_errors) = 0;
    PG(during_request_startup) = 0;
    PG(modules_activated) = 1;
    SG(request_info).no_headers = 1;
    SG(headers_sent) = 0;

    /* ext/json's interface, which class-based extensions implement. Declared
     * without its method: nothing calls jsonSerialize() through the engine. */
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "JsonSerializable", NULL);
    php_json_serializable_ce = zend_register_internal_interface(&ce);

    /* ext/standard's stand-in for a serialized class nobody declared: var.c,
     * igbinary and msgpack build one instead of failing, and dereference the
     * entry unconditionally. Registered as basic_functions.c does, minus the
     * #[AllowDynamicProperties] attribute nothing here reads. */
    INIT_CLASS_ENTRY(ce, "__PHP_Incomplete_Class", NULL);
    php_ce_incomplete_class = zend_register_internal_class_with_flags(
        &ce, NULL, ZEND_ACC_FINAL | ZEND_ACC_ALLOW_DYNAMIC_PROPERTIES);
    php_register_incomplete_class_handlers();
}

/* ------------------------------------------------------------ registrations */

PHPAPI zend_result php_register_url_stream_wrapper(const char *protocol, const php_stream_wrapper *wrapper) {
    (void)protocol; (void)wrapper;
    return SUCCESS; /* a compiled program opens streams through Elephc, not PHP */
}

PHPAPI zend_result php_unregister_url_stream_wrapper(const char *protocol) {
    (void)protocol;
    return SUCCESS;
}

PHPAPI zend_result php_output_handler_alias_register(const char *name, size_t name_len,
                                                     php_output_handler_alias_ctor_t func) {
    (void)name; (void)name_len; (void)func;
    return SUCCESS;
}

PHPAPI zend_result php_output_handler_conflict_register(const char *name, size_t name_len,
                                                        php_output_handler_conflict_check_t check) {
    (void)name; (void)name_len; (void)check;
    return SUCCESS;
}

/* No PHP output buffer is ever active in a compiled program. */
PHPAPI int php_output_get_level(void) { return 0; }
PHPAPI int php_output_get_status(void) { return 0; }
PHPAPI bool php_output_handler_started(const char *name, size_t name_len) {
    (void)name; (void)name_len;
    return false;
}
PHPAPI bool php_output_handler_conflict(const char *handler_new, size_t handler_new_len,
                                        const char *handler_set, size_t handler_set_len) {
    (void)handler_new; (void)handler_new_len; (void)handler_set; (void)handler_set_len;
    return false;
}

/* Output an extension writes goes where Elephc's echo goes. */
PHPAPI size_t php_output_write(const char *str, size_t len) {
    return zend_write(str, len);
}

/* Headers are dropped, as the CLI SAPI drops them. */
SAPI_API int sapi_add_header_ex(const char *header_line, size_t header_line_len, bool duplicate, bool replace) {
    (void)header_line; (void)header_line_len; (void)duplicate; (void)replace;
    return SUCCESS;
}

/* No open_basedir is configured, so every path is allowed. */
PHPAPI int php_check_open_basedir(const char *path) {
    (void)path;
    return 0;
}

/* Superglobals belong to Elephc's own runtime, not to the engine's. */
ZEND_API bool zend_is_auto_global(zend_string *name) { (void)name; return false; }
ZEND_API bool zend_is_auto_global_str(const char *name, size_t len) { (void)name; (void)len; return false; }

/* ------------------------------------------------------------ ext/hash */

PHP_HASH_API zend_result php_hash_copy(const void *ops, const void *orig_context, void *dest_context) {
    memcpy(dest_context, orig_context, ((const php_hash_ops *)ops)->context_size);
    return SUCCESS;
}

/* Serializing a HashContext object is a feature of the HashContext class,
 * which is not hosted; refusing is the documented failure result. */
PHP_HASH_API hash_spec_result php_hash_serialize(const php_hashcontext_object *context, zend_long *magic, zval *zv) {
    (void)context; (void)magic; (void)zv;
    return HASH_SPEC_FAILURE;
}
PHP_HASH_API hash_spec_result php_hash_unserialize(php_hashcontext_object *context, zend_long magic, const zval *zv) {
    (void)context; (void)magic; (void)zv;
    return HASH_SPEC_FAILURE;
}

/* ------------------------------------------------------------ operations */

PHPAPI php_output_handler *php_output_handler_create_internal(const char *name, size_t name_len,
                                                              php_output_handler_context_func_t handler,
                                                              size_t chunk_size, int flags) {
    (void)name; (void)name_len; (void)handler; (void)chunk_size; (void)flags;
    elephc_zend_unsupported("php_output_handler_create_internal (PHP output buffering)");
}
PHPAPI void php_output_handler_set_context(php_output_handler *handler, void *opaq, void (*dtor)(void *)) {
    (void)handler; (void)opaq; (void)dtor;
    elephc_zend_unsupported("php_output_handler_set_context (PHP output buffering)");
}
PHPAPI zend_result php_output_handler_start(php_output_handler *handler) {
    (void)handler;
    elephc_zend_unsupported("php_output_handler_start (PHP output buffering)");
}
PHPAPI zend_result php_output_handler_hook(php_output_handler_hook_t type, void *arg) {
    (void)type; (void)arg;
    elephc_zend_unsupported("php_output_handler_hook (PHP output buffering)");
}

PHPAPI php_stream *_php_stream_alloc(const php_stream_ops *ops, void *abstract, const char *persistent_id,
                                     const char *mode STREAMS_DC) {
    (void)ops; (void)abstract; (void)persistent_id; (void)mode;
    elephc_zend_unsupported("_php_stream_alloc (PHP streams)");
}
PHPAPI php_stream *_php_stream_open_wrapper_ex(const char *path, const char *mode, int options,
                                               zend_string **opened_path, php_stream_context *context STREAMS_DC) {
    (void)path; (void)mode; (void)options; (void)opened_path; (void)context;
    elephc_zend_unsupported("_php_stream_open_wrapper_ex (PHP streams)");
}
PHPAPI bool _php_stream_eof(php_stream *stream) {
    (void)stream;
    elephc_zend_unsupported("_php_stream_eof (PHP streams)");
}
PHPAPI int _php_stream_free(php_stream *stream, int close_options) {
    (void)stream; (void)close_options;
    elephc_zend_unsupported("_php_stream_free (PHP streams)");
}
PHPAPI ssize_t _php_stream_read(php_stream *stream, char *buf, size_t count) {
    (void)stream; (void)buf; (void)count;
    elephc_zend_unsupported("_php_stream_read (PHP streams)");
}
PHPAPI ssize_t _php_stream_write(php_stream *stream, const char *buf, size_t count) {
    (void)stream; (void)buf; (void)count;
    elephc_zend_unsupported("_php_stream_write (PHP streams)");
}
PHPAPI int _php_stream_set_option(php_stream *stream, int option, int value, void *ptrparam) {
    (void)stream; (void)option; (void)value; (void)ptrparam;
    elephc_zend_unsupported("_php_stream_set_option (PHP streams)");
}
PHPAPI zend_string *_php_stream_copy_to_mem(php_stream *src, size_t maxlen, bool persistent STREAMS_DC) {
    (void)src; (void)maxlen; (void)persistent;
    elephc_zend_unsupported("_php_stream_copy_to_mem (PHP streams)");
}
PHPAPI php_stream_context *php_stream_context_alloc(void) {
    elephc_zend_unsupported("php_stream_context_alloc (PHP streams)");
}
/* No context ever carries options: there are no PHP stream contexts. */
PHPAPI zval *php_stream_context_get_option(const php_stream_context *context, const char *wrappername,
                                           const char *optionname) {
    (void)context; (void)wrappername; (void)optionname;
    return NULL;
}

/* ext/standard/string.c is not part of the engine; var_export() of a string
 * (php_var_export_ex) is the only path serialization takes into it. */
PHPAPI zend_string *php_addcslashes(zend_string *str, const char *what, size_t what_len) {
    (void)str; (void)what; (void)what_len;
    elephc_zend_unsupported("php_addcslashes (PHP var_export())");
}
PHPAPI zend_string *php_addcslashes_str(const char *str, size_t len, const char *what, size_t what_len) {
    (void)str; (void)len; (void)what; (void)what_len;
    elephc_zend_unsupported("php_addcslashes_str (PHP var_export())");
}
PHPAPI zend_string *php_str_to_str(const char *haystack, size_t length, const char *needle,
                                   size_t needle_len, const char *str, size_t str_len) {
    (void)haystack; (void)length; (void)needle; (void)needle_len; (void)str; (void)str_len;
    elephc_zend_unsupported("php_str_to_str (PHP var_export())");
}

/* PHP's regex engine (ext/pcre) is not part of the hosted engine; APCu, for
 * one, reaches it only for APCUIterator's pattern search. */
PHPAPI pcre_cache_entry *pcre_get_compiled_regex_cache(zend_string *regex) {
    (void)regex;
    elephc_zend_unsupported("pcre_get_compiled_regex_cache (PHP regular expressions)");
}
PHPAPI pcre2_match_context *php_pcre_mctx(void) {
    elephc_zend_unsupported("php_pcre_mctx (PHP regular expressions)");
}
PHPAPI pcre2_general_context *php_pcre_gctx(void) {
    elephc_zend_unsupported("php_pcre_gctx (PHP regular expressions)");
}
PHPAPI pcre2_code *php_pcre_pce_re(pcre_cache_entry *entry) {
    (void)entry;
    elephc_zend_unsupported("php_pcre_pce_re (PHP regular expressions)");
}
/* pcre2.h renames these to php_pcre2_*, the bundled library's names. */
int pcre2_match(const pcre2_code *code, PCRE2_SPTR subject, PCRE2_SIZE length, PCRE2_SIZE start,
                uint32_t options, pcre2_match_data *match_data, pcre2_match_context *context) {
    (void)code; (void)subject; (void)length; (void)start; (void)options; (void)match_data; (void)context;
    elephc_zend_unsupported("pcre2_match (PHP regular expressions)");
}
pcre2_match_data *pcre2_match_data_create_from_pattern(const pcre2_code *code, pcre2_general_context *context) {
    (void)code; (void)context;
    elephc_zend_unsupported("pcre2_match_data_create_from_pattern (PHP regular expressions)");
}
/* Freeing is reachable from cleanup paths that never created match data. */
void pcre2_match_data_free(pcre2_match_data *match_data) {
    (void)match_data;
}

PHPAPI zend_result spl_iterator_apply(zval *obj, spl_iterator_apply_func_t apply_func, void *puser) {
    (void)obj; (void)apply_func; (void)puser;
    elephc_zend_unsupported("spl_iterator_apply (iterating a PHP object)");
}

ZEND_API zend_object *zend_enum_get_case(zend_class_entry *ce, zend_string *name) {
    (void)ce; (void)name;
    elephc_zend_unsupported("zend_enum_get_case (PHP enums)");
}

ZEND_API zend_result _call_user_function_impl(zval *object, zval *function_name, zval *retval_ptr,
                                              uint32_t param_count, zval params[], HashTable *named_params) {
    (void)object; (void)function_name; (void)retval_ptr; (void)param_count; (void)params; (void)named_params;
    elephc_zend_unsupported("call_user_function (calling back into PHP code)");
}
