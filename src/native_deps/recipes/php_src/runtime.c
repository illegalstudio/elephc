/* runtime.c — the part of the Zend engine an ahead-of-time host replaces.
 *
 * Hosted extensions link against the engine's real data-structure code,
 * compiled from the pinned php-src tree: zend_hash, zend_string, zend_API
 * (argument parsing, class registration), zend_operators, zend_objects,
 * zend_exceptions and friends. What they cannot get from there is the part of
 * Zend that exists to run opcodes: the executor, the compiler, the garbage
 * collector, the observer API. An Elephc binary has none of those.
 *
 * This file supplies exactly that remainder, and nothing the real sources
 * already provide:
 *   - the engine globals (executor_globals, compiler_globals and the utility
 *     function pointers php's main.c would normally install);
 *   - error reporting, formatted the way Elephc reports its own warnings;
 *   - the "active function" queries argument parsing uses to name the callee;
 *   - class lookup and initialisation without a compiler;
 *   - the bailout contract (zend_bailout longjmps to the host's frame).
 *
 * Paths that only a VM can reach — calling user code, compiling a string,
 * lazy objects, property hooks — abort with the symbol's name. Static linking
 * proves a symbol exists, never that it behaves; a plausible return value on a
 * path nobody implemented would be a silent miscompile, so these fail loudly.
 */

#include "php.h"
#include "zend_API.h"
#include "zend_attributes.h"
#include "zend_closures.h"
#include "zend_constants.h"
#include "zend_enum.h"
#include "zend_exceptions.h"
#include "zend_extensions.h"
#include "zend_frameless_function.h"
#include "zend_inheritance.h"
#include "zend_interfaces.h"
#include "zend_lazy_objects.h"
#include "zend_observer.h"
#include "zend_property_hooks.h"
#include "zend_smart_str.h"
#include "zend_smart_string.h"
#include "zend_weakrefs.h"
#include "ext/spl/spl_exceptions.h"
#include "ext/standard/info.h"
#include "php_ini.h"
#include "spprintf.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Reports a path this host deliberately does not implement, then aborts.
 * Shared with stdlib.c. */
ZEND_COLD ZEND_NORETURN void elephc_zend_unsupported(const char *what) {
    fflush(stdout);
    fprintf(stderr, "Fatal error: hosted PHP extension reached '%s', which an "
                    "ahead-of-time Elephc binary does not provide\n", what);
    abort();
}
#define ELEPHC_UNSUPPORTED(what) elephc_zend_unsupported(what)

/* ------------------------------------------------------------ engine globals */

ZEND_API zend_executor_globals executor_globals;
ZEND_API struct _zend_compiler_globals compiler_globals;
ZEND_API zend_utility_values zend_uv;
ZEND_API zend_class_entry *zend_standard_class_def = NULL;
ZEND_API zend_class_entry *zend_ce_closure = NULL;

ZEND_API size_t (*zend_printf)(const char *format, ...);
ZEND_API zend_write_func_t zend_write;
ZEND_API void (*zend_error_cb)(int type, zend_string *error_filename,
                               const uint32_t error_lineno, zend_string *message);
void (*zend_printf_to_smart_string)(smart_string *buf, const char *format, va_list ap);
void (*zend_printf_to_smart_str)(smart_str *buf, const char *format, va_list ap);
ZEND_API void (*zend_random_bytes_insecure)(zend_random_bytes_insecure_state *state,
                                            void *bytes, size_t size);
ZEND_API void (*zend_execute_ex)(zend_execute_data *execute_data);

/* The optimizer and observer API are never active: no opcodes exist. */
ZEND_API bool zend_observer_errors_observed = false;
ZEND_API bool zend_observer_class_linked_observed = false;
ZEND_API int zend_observer_fcall_op_array_extension = -1;

/* Frameless calls are an opcode optimisation; there are none to register. */
size_t zend_flf_count = 0;
size_t zend_flf_capacity = 0;
ZEND_API void **zend_flf_handlers = NULL;
ZEND_API zend_function **zend_flf_functions = NULL;

/* The SPL exception hierarchy extensions throw into. Registered at startup by
 * elephc_zend_register_classes() from ext/spl's own generated arginfo. */
PHPAPI zend_class_entry *spl_ce_LogicException;
PHPAPI zend_class_entry *spl_ce_BadFunctionCallException;
PHPAPI zend_class_entry *spl_ce_BadMethodCallException;
PHPAPI zend_class_entry *spl_ce_DomainException;
PHPAPI zend_class_entry *spl_ce_InvalidArgumentException;
PHPAPI zend_class_entry *spl_ce_LengthException;
PHPAPI zend_class_entry *spl_ce_OutOfRangeException;
PHPAPI zend_class_entry *spl_ce_RuntimeException;
PHPAPI zend_class_entry *spl_ce_OutOfBoundsException;
PHPAPI zend_class_entry *spl_ce_OverflowException;
PHPAPI zend_class_entry *spl_ce_RangeException;
PHPAPI zend_class_entry *spl_ce_UnderflowException;
PHPAPI zend_class_entry *spl_ce_UnexpectedValueException;

/* ------------------------------------------------------------ output/errors */

size_t elephc_zend_write_stdout(const char *str, size_t len) {
    return fwrite(str, 1, len, stdout);
}

size_t elephc_zend_printf_stdout(const char *format, ...) {
    va_list args;
    char *buffer = NULL;
    va_start(args, format);
    size_t len = zend_vspprintf(&buffer, 0, format, args);
    va_end(args);
    size_t written = elephc_zend_write_stdout(buffer, len);
    efree(buffer);
    return written;
}

/* Label Elephc prints before a diagnostic, mirroring PHP's display names. */
static const char *elephc_error_label(int type) {
    switch (type) {
        case E_ERROR: case E_CORE_ERROR: case E_COMPILE_ERROR:
        case E_USER_ERROR: case E_RECOVERABLE_ERROR:
            return "Fatal error";
        case E_WARNING: case E_CORE_WARNING: case E_COMPILE_WARNING:
        case E_USER_WARNING:
            return "Warning";
        case E_NOTICE: case E_USER_NOTICE:
            return "Notice";
        case E_DEPRECATED: case E_USER_DEPRECATED:
            return "Deprecated";
        default:
            return "Unknown error";
    }
}

/* Errors that end the request in PHP end it here too, through bailout. */
static bool elephc_error_is_fatal(int type) {
    return (type & (E_ERROR | E_CORE_ERROR | E_COMPILE_ERROR | E_USER_ERROR
                    | E_RECOVERABLE_ERROR | E_PARSE)) != 0;
}

void elephc_zend_error_cb(int type, zend_string *filename, const uint32_t lineno,
                            zend_string *message) {
    (void)filename; (void)lineno;
    if (!elephc_error_is_fatal(type) && !(EG(error_reporting) & type)) {
        return;
    }
    fflush(stdout);
    fprintf(stderr, "%s: %s\n", elephc_error_label(type), ZSTR_VAL(message));
    if (elephc_error_is_fatal(type)) {
        EG(exit_status) = 255;
        zend_bailout();
    }
}

static void elephc_error_va(int type, const char *format, va_list args) {
    zend_string *message = zend_vstrpprintf(0, format, args);
    zend_error_cb(type, NULL, 0, message);
    zend_string_release(message);
}

ZEND_API ZEND_COLD void zend_error(int type, const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_error_va(type, format, args);
    va_end(args);
}

ZEND_API ZEND_COLD void zend_error_unchecked(int type, const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_error_va(type, format, args);
    va_end(args);
}

ZEND_API ZEND_COLD void zend_error_at(int type, zend_string *filename, uint32_t lineno,
                                      const char *format, ...) {
    (void)filename; (void)lineno;
    va_list args;
    va_start(args, format);
    elephc_error_va(type, format, args);
    va_end(args);
}

ZEND_API ZEND_COLD ZEND_NORETURN void zend_error_noreturn(int type, const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_error_va(type, format, args);
    va_end(args);
    abort(); /* a noreturn error that returned is a host bug */
}

ZEND_API ZEND_COLD ZEND_NORETURN void zend_error_at_noreturn(int type, zend_string *filename,
                                                             uint32_t lineno,
                                                             const char *format, ...) {
    (void)filename; (void)lineno;
    va_list args;
    va_start(args, format);
    elephc_error_va(type, format, args);
    va_end(args);
    abort();
}

/* Warnings raised while an extension is being linked are not recorded. */
ZEND_API void zend_begin_record_errors(void) { EG(record_errors) = false; }
ZEND_API void zend_emit_recorded_errors(void) { EG(record_errors) = false; }
ZEND_API void zend_free_recorded_errors(void) { EG(num_errors) = 0; EG(errors) = NULL; }

/* php_error_docref prefixes the active function, as PHP's main.c does. */
PHPAPI ZEND_COLD void php_error_docref(const char *docref, int type, const char *format, ...) {
    (void)docref;
    va_list args;
    va_start(args, format);
    zend_string *message = zend_vstrpprintf(0, format, args);
    va_end(args);
    const char *function = get_active_function_name();
    if (function) {
        zend_error(type, "%s(): %s", function, ZSTR_VAL(message));
    } else {
        zend_error(type, "%s", ZSTR_VAL(message));
    }
    zend_string_release(message);
}

/* The host always runs inside a call frame, so errors become exceptions the
 * way they do at runtime in PHP rather than compile-time fatals. */
static void elephc_throw_formatted(zend_class_entry *ce, const char *format, va_list args) {
    char *message = NULL;
    zend_vspprintf(&message, 0, format, args);
    zend_throw_exception(ce, message, 0);
    efree(message);
}

ZEND_API ZEND_COLD void zend_throw_error(zend_class_entry *exception_ce, const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_throw_formatted(exception_ce ? exception_ce : zend_ce_error, format, args);
    va_end(args);
}

ZEND_API ZEND_COLD void zend_type_error(const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_throw_formatted(zend_ce_type_error, format, args);
    va_end(args);
}

ZEND_API ZEND_COLD void zend_value_error(const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_throw_formatted(zend_ce_value_error, format, args);
    va_end(args);
}

/* zend_execute_API.c's check: letters, digits, `_`, `\` and bytes >= 0x80. */
ZEND_API bool zend_is_valid_class_name(zend_string *name) {
    for (size_t i = 0; i < ZSTR_LEN(name); i++) {
        unsigned char c = ZSTR_VAL(name)[i];
        if (!(isalnum(c) || c == '_' || c == '\\' || c >= 0x80)) {
            return false;
        }
    }
    return true;
}

ZEND_API ZEND_COLD void zend_argument_count_error(const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_throw_formatted(zend_ce_argument_count_error, format, args);
    va_end(args);
}

ZEND_API ZEND_COLD void zend_illegal_container_offset(const zend_string *container,
                                                     const zval *offset, int type) {
    (void)type;
    zend_type_error("Cannot access offset of type %s on %s",
                    zend_zval_type_name(offset), ZSTR_VAL(container));
}

ZEND_API ZEND_COLD void ZEND_FASTCALL zend_use_resource_as_offset(const zval *dim) {
    zend_error(E_WARNING, "Resource ID#" ZEND_LONG_FMT " used as offset, casting to integer (" ZEND_LONG_FMT ")",
               (zend_long)Z_RES_HANDLE_P(dim), (zend_long)Z_RES_HANDLE_P(dim));
}

ZEND_API ZEND_COLD void ZEND_FASTCALL zend_readonly_property_modification_error(
        const zend_property_info *info) {
    zend_throw_error(NULL, "Cannot modify readonly property %s::$%s",
                     ZSTR_VAL(info->ce->name), zend_get_unmangled_property_name(info->name));
}

ZEND_API ZEND_COLD void ZEND_FASTCALL zend_readonly_property_indirect_modification_error(
        const zend_property_info *info) {
    zend_throw_error(NULL, "Cannot indirectly modify readonly property %s::$%s",
                     ZSTR_VAL(info->ce->name), zend_get_unmangled_property_name(info->name));
}

ZEND_API ZEND_COLD void ZEND_FASTCALL zend_asymmetric_visibility_property_modification_error(
        const zend_property_info *info, const char *operation) {
    zend_throw_error(NULL, "Cannot %s property %s::$%s", operation,
                     ZSTR_VAL(info->ce->name), zend_get_unmangled_property_name(info->name));
}

ZEND_API ZEND_COLD void ZEND_FASTCALL zend_object_released_while_assigning_to_property_error(
        const zend_property_info *info) {
    zend_throw_error(NULL, "Object was released while assigning to property %s::$%s",
                     ZSTR_VAL(info->ce->name), zend_get_unmangled_property_name(info->name));
}

ZEND_API ZEND_COLD void ZEND_FASTCALL zend_deprecated_constant(const zend_constant *c,
                                                               const zend_string *name) {
    (void)c;
    zend_error(E_DEPRECATED, "Constant %s is deprecated", ZSTR_VAL(name));
}

ZEND_API ZEND_COLD void ZEND_FASTCALL zend_deprecated_class_constant(const zend_class_constant *c,
                                                                     const zend_string *name) {
    zend_error(E_DEPRECATED, "Constant %s::%s is deprecated", ZSTR_VAL(c->ce->name), ZSTR_VAL(name));
}

ZEND_API ZEND_COLD void zend_use_of_deprecated_trait(zend_class_entry *trait,
                                                     const zend_string *used_by) {
    zend_error(E_DEPRECATED, "Trait %s used by %s is deprecated",
               ZSTR_VAL(trait->name), ZSTR_VAL(used_by));
}

ZEND_API ZEND_COLD void zend_user_exception_handler(void) {
    ELEPHC_UNSUPPORTED("zend_user_exception_handler");
}

/* ------------------------------------------------------------ bailout */

ZEND_API ZEND_COLD ZEND_NORETURN void _zend_bailout(const char *filename, uint32_t lineno) {
    if (!EG(bailout)) {
        /* No protected frame: a genuine fatal with nowhere to return to.
         * Longjmping into an unset buffer would be the silent failure. */
        fflush(stdout);
        fprintf(stderr, "Fatal error: hosted PHP extension bailed out at %s:%u "
                        "with no protected frame\n", filename, lineno);
        exit(255);
    }
    CG(unclean_shutdown) = 1;
    LONGJMP(*EG(bailout), FAILURE);
}

/* ------------------------------------------------------------ printf family */

ZEND_API size_t zend_vspprintf(char **pbuf, size_t max_len, const char *format, va_list ap) {
    smart_string buf = {0};
    if (!pbuf) {
        return 0;
    }
    zend_printf_to_smart_string(&buf, format, ap);
    if (max_len && buf.len > max_len) {
        buf.len = max_len;
    }
    smart_string_0(&buf);
    if (buf.c) {
        *pbuf = buf.c;
        return buf.len;
    }
    *pbuf = estrndup("", 0);
    return 0;
}

ZEND_API size_t zend_spprintf(char **message, size_t max_len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    size_t len = zend_vspprintf(message, max_len, format, args);
    va_end(args);
    return len;
}

ZEND_API zend_string *zend_vstrpprintf(size_t max_len, const char *format, va_list ap) {
    smart_str buf = {0};
    zend_printf_to_smart_str(&buf, format, ap);
    if (!buf.s) {
        return ZSTR_EMPTY_ALLOC();
    }
    if (max_len && ZSTR_LEN(buf.s) > max_len) {
        ZSTR_LEN(buf.s) = max_len;
    }
    return smart_str_extract(&buf);
}

ZEND_API zend_string *zend_strpprintf(size_t max_len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    zend_string *str = zend_vstrpprintf(max_len, format, args);
    va_end(args);
    return str;
}

ZEND_API zend_string *zend_strpprintf_unchecked(size_t max_len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    zend_string *str = zend_vstrpprintf(max_len, format, args);
    va_end(args);
    return str;
}

/* php's own string helpers that extensions reach through main/. */
PHPAPI size_t php_strlcpy(char *dst, const char *src, size_t size) {
    size_t len = strlen(src);
    if (size) {
        size_t copy = len >= size ? size - 1 : len;
        memcpy(dst, src, copy);
        dst[copy] = '\0';
    }
    return len;
}

PHPAPI size_t php_strlcat(char *dst, const char *src, size_t size) {
    size_t used = strnlen(dst, size);
    if (used == size) {
        return size + strlen(src);
    }
    return used + php_strlcpy(dst + used, src, size - used);
}

/* Output an extension writes itself goes where Elephc's echo goes. */
PHPAPI size_t php_write(void *buf, size_t size) {
    return zend_write((const char *)buf, size);
}

static size_t elephc_vprintf(const char *format, va_list args) {
    char *buffer = NULL;
    size_t len = zend_vspprintf(&buffer, 0, format, args);
    size_t written = zend_write(buffer, len);
    efree(buffer);
    return written;
}

PHPAPI size_t php_printf(const char *format, ...) {
    va_list args;
    va_start(args, format);
    size_t written = elephc_vprintf(format, args);
    va_end(args);
    return written;
}

PHPAPI size_t php_printf_unchecked(const char *format, ...) {
    va_list args;
    va_start(args, format);
    size_t written = elephc_vprintf(format, args);
    va_end(args);
    return written;
}

/* phpinfo() output has no meaning in a compiled binary: a hosted extension's
 * MINFO is never called, but it is still linked. */
PHPAPI void php_info_print_table_start(void) {}
PHPAPI void php_info_print_table_end(void) {}
PHPAPI void php_info_print_table_header(int num_cols, ...) { (void)num_cols; }
PHPAPI void php_info_print_table_row(int num_cols, ...) { (void)num_cols; }
PHPAPI void php_info_print_table_row_ex(int num_cols, const char *css, ...) { (void)num_cols; (void)css; }
PHPAPI void php_info_print_table_colspan_header(int num_cols, const char *header) { (void)num_cols; (void)header; }
PHPAPI void php_info_print_box_start(int bg) { (void)bg; }
PHPAPI void php_info_print_box_end(void) {}
PHPAPI void php_info_print_hr(void) {}
PHPAPI void display_ini_entries(zend_module_entry *module) { (void)module; }

/* ------------------------------------------------------------ active frame */

ZEND_API zend_function *zend_active_function_ex(zend_execute_data *execute_data) {
    return EX(func);
}

ZEND_API const char *get_active_function_name(void) {
    zend_execute_data *ex = EG(current_execute_data);
    if (!ex || !ex->func || !ex->func->common.function_name) {
        return NULL;
    }
    return ZSTR_VAL(ex->func->common.function_name);
}

ZEND_API const char *get_active_class_name(const char **space) {
    zend_execute_data *ex = EG(current_execute_data);
    zend_class_entry *ce = (ex && ex->func) ? ex->func->common.scope : NULL;
    if (space) {
        *space = ce ? "::" : "";
    }
    return ce ? ZSTR_VAL(ce->name) : "";
}

ZEND_API zend_string *get_active_function_or_method_name(void) {
    zend_execute_data *ex = EG(current_execute_data);
    zend_function *func = ex ? ex->func : NULL;
    if (!func || !func->common.function_name) {
        return ZSTR_INIT_LITERAL("main", 0);
    }
    if (func->common.scope) {
        return zend_create_member_string(func->common.scope->name, func->common.function_name);
    }
    return zend_string_copy(func->common.function_name);
}

ZEND_API const char *get_active_function_arg_name(uint32_t arg_num) {
    zend_execute_data *ex = EG(current_execute_data);
    zend_function *func = ex ? ex->func : NULL;
    if (!func || arg_num == 0 || func->common.num_args < arg_num) {
        return NULL;
    }
    return ((zend_internal_arg_info *)func->common.arg_info)[arg_num - 1].name;
}

/* Compiled code has no PHP file or line of its own to report. */
ZEND_API zend_string *zend_get_executed_filename_ex(void) { return EG(filename_override); }
ZEND_API const char *zend_get_executed_filename(void) {
    zend_string *filename = zend_get_executed_filename_ex();
    return filename ? ZSTR_VAL(filename) : "[no active file]";
}
ZEND_API uint32_t zend_get_executed_lineno(void) {
    return EG(lineno_override) != -1 ? (uint32_t)EG(lineno_override) : 0;
}
ZEND_API zend_class_entry *zend_get_executed_scope(void) {
    zend_execute_data *ex = EG(current_execute_data);
    return (ex && ex->func) ? ex->func->common.scope : NULL;
}
ZEND_API zend_class_entry *zend_get_called_scope(zend_execute_data *ex) {
    if (!ex || !ex->func) {
        return NULL;
    }
    if (Z_TYPE(ex->This) == IS_OBJECT) {
        return Z_OBJCE(ex->This);
    }
    return Z_CE(ex->This) ? Z_CE(ex->This) : ex->func->common.scope;
}
ZEND_API zend_object *zend_get_this_object(zend_execute_data *ex) {
    return (ex && Z_TYPE(ex->This) == IS_OBJECT) ? Z_OBJ(ex->This) : NULL;
}
ZEND_API zend_string *zend_get_compiled_filename(void) { return NULL; }
ZEND_API int zend_get_compiled_lineno(void) { return 0; }
void zend_file_context_begin(zend_file_context *prev_context) { (void)prev_context; }
void zend_file_context_end(zend_file_context *prev_context) { (void)prev_context; }

/* A backtrace needs user frames; a compiled binary's are native. Exceptions
 * built by extensions therefore carry an empty trace. */
ZEND_API void zend_fetch_debug_backtrace(zval *return_value, int skip_last, int options, int limit) {
    (void)skip_last; (void)options; (void)limit;
    RETVAL_EMPTY_ARRAY();
}

/* ------------------------------------------------------------ classes */

ZEND_API zend_class_entry *zend_lookup_class_ex(zend_string *name, zend_string *key, uint32_t flags) {
    (void)flags;
    zend_string *lc = key ? zend_string_copy(key) : zend_string_tolower(name);
    const char *start = ZSTR_VAL(lc);
    size_t len = ZSTR_LEN(lc);
    if (len && start[0] == '\\') {
        start++;
        len--;
    }
    zend_class_entry *ce = zend_hash_str_find_ptr(EG(class_table), start, len);
    zend_string_release(lc);
    return ce;
}

ZEND_API zend_class_entry *zend_lookup_class(zend_string *name) {
    return zend_lookup_class_ex(name, NULL, 0);
}

ZEND_API zend_class_entry *zend_fetch_class_by_name(zend_string *class_name, zend_string *lcname,
                                                    uint32_t fetch_type) {
    zend_class_entry *ce = zend_lookup_class_ex(class_name, lcname, fetch_type);
    if (!ce && !(fetch_type & ZEND_FETCH_CLASS_SILENT)) {
        zend_throw_error(NULL, "Class \"%s\" not found", ZSTR_VAL(class_name));
    }
    return ce;
}

ZEND_API zend_class_entry *zend_fetch_class(zend_string *class_name, uint32_t fetch_type) {
    return zend_fetch_class_by_name(class_name, NULL, fetch_type);
}

ZEND_API zend_function *ZEND_FASTCALL zend_fetch_function(zend_string *name) {
    return zend_hash_find_ptr(EG(function_table), name);
}

ZEND_API zend_function *ZEND_FASTCALL zend_fetch_function_str(const char *name, size_t len) {
    return zend_hash_str_find_ptr(EG(function_table), name, len);
}

/* zend_compile.c's version, minus the compiler options no host sets. */
ZEND_API void zend_initialize_class_data(zend_class_entry *ce, bool nullify_handlers) {
    bool persistent = ce->type == ZEND_INTERNAL_CLASS;
    ce->refcount = 1;
    ce->ce_flags = ZEND_ACC_CONSTANTS_UPDATED;
    ce->default_properties_table = NULL;
    ce->default_static_members_table = NULL;
    zend_hash_init(&ce->properties_info, 8, NULL, NULL, persistent);
    zend_hash_init(&ce->constants_table, 8, NULL, NULL, persistent);
    zend_hash_init(&ce->function_table, 8, NULL, ZEND_FUNCTION_DTOR, persistent);
    ce->doc_comment = NULL;
    ZEND_MAP_PTR_INIT(ce->static_members_table, NULL);
    ZEND_MAP_PTR_INIT(ce->mutable_data, NULL);
    ce->default_object_handlers = &std_object_handlers;
    ce->default_properties_count = 0;
    ce->default_static_members_count = 0;
    ce->properties_info_table = NULL;
    ce->attributes = NULL;
    ce->enum_backing_type = IS_UNDEF;
    ce->backed_enum_table = NULL;
    if (nullify_handlers) {
        ce->constructor = NULL;
        ce->destructor = NULL;
        ce->clone = NULL;
        ce->__get = NULL;
        ce->__set = NULL;
        ce->__unset = NULL;
        ce->__isset = NULL;
        ce->__call = NULL;
        ce->__callstatic = NULL;
        ce->__tostring = NULL;
        ce->__serialize = NULL;
        ce->__unserialize = NULL;
        ce->__debugInfo = NULL;
        ce->create_object = NULL;
        ce->get_iterator = NULL;
        ce->iterator_funcs_ptr = NULL;
        ce->arrayaccess_funcs_ptr = NULL;
        ce->get_static_method = NULL;
        ce->parent = NULL;
        ce->parent_name = NULL;
        ce->num_interfaces = 0;
        ce->interfaces = NULL;
        ce->num_traits = 0;
        ce->num_hooked_props = 0;
        ce->num_hooked_prop_variance_checks = 0;
        ce->trait_names = NULL;
        ce->trait_aliases = NULL;
        ce->trait_precedences = NULL;
        ce->serialize = NULL;
        ce->unserialize = NULL;
        if (persistent) {
            ce->info.internal.module = NULL;
            ce->info.internal.builtin_functions = NULL;
        }
    }
}

void zend_assert_valid_class_name(const zend_string *name, const char *type) {
    (void)name; (void)type; /* internal class names are fixed by their C source */
}

ZEND_API zend_string *zend_create_member_string(zend_string *class_name, zend_string *member_name) {
    return zend_string_concat3(ZSTR_VAL(class_name), ZSTR_LEN(class_name), "::", 2,
                               ZSTR_VAL(member_name), ZSTR_LEN(member_name));
}

ZEND_API zend_string *zend_mangle_property_name(const char *src1, size_t src1_length,
                                                const char *src2, size_t src2_length,
                                                bool internal) {
    size_t length = 1 + src1_length + 1 + src2_length;
    zend_string *name = zend_string_alloc(length, internal);
    ZSTR_VAL(name)[0] = '\0';
    memcpy(ZSTR_VAL(name) + 1, src1, src1_length + 1);
    memcpy(ZSTR_VAL(name) + 1 + src1_length + 1, src2, src2_length + 1);
    return name;
}

ZEND_API zend_result zend_unmangle_property_name_ex(const zend_string *name, const char **class_name,
                                                    const char **prop_name, size_t *prop_len) {
    *class_name = NULL;
    if (!ZSTR_LEN(name) || ZSTR_VAL(name)[0] != '\0') {
        *prop_name = ZSTR_VAL(name);
        if (prop_len) {
            *prop_len = ZSTR_LEN(name);
        }
        return SUCCESS;
    }
    size_t class_len = zend_strnlen(ZSTR_VAL(name) + 1, ZSTR_LEN(name) - 2);
    if (ZSTR_LEN(name) < 3 || class_len >= ZSTR_LEN(name) - 2) {
        *prop_name = ZSTR_VAL(name);
        if (prop_len) {
            *prop_len = ZSTR_LEN(name);
        }
        return FAILURE;
    }
    *class_name = ZSTR_VAL(name) + 1;
    *prop_name = ZSTR_VAL(name) + class_len + 2;
    if (prop_len) {
        *prop_len = ZSTR_LEN(name) - class_len - 2;
    }
    return SUCCESS;
}

ZEND_API void zend_set_function_arg_flags(zend_function *func) {
    func->common.arg_flags[0] = 0;
    func->common.arg_flags[1] = 0;
    func->common.arg_flags[2] = 0;
    if (!func->common.arg_info) {
        return;
    }
    uint32_t n = MIN(func->common.num_args, MAX_ARG_FLAG_NUM);
    uint32_t i = 0;
    while (i < n) {
        ZEND_SET_ARG_FLAG(func, i + 1, ZEND_ARG_SEND_MODE(&func->common.arg_info[i]));
        i++;
    }
    if ((func->common.fn_flags & ZEND_ACC_VARIADIC) && ZEND_ARG_SEND_MODE(&func->common.arg_info[i])) {
        uint32_t by_ref = ZEND_ARG_SEND_MODE(&func->common.arg_info[i]);
        while (i < MAX_ARG_FLAG_NUM) {
            ZEND_SET_ARG_FLAG(func, i + 1, by_ref);
            i++;
        }
    }
}

static zend_string *elephc_add_type(zend_string *type, zend_string *next) {
    if (!type) {
        return zend_string_copy(next);
    }
    zend_string *joined = zend_string_concat3(ZSTR_VAL(type), ZSTR_LEN(type), "|", 1,
                                              ZSTR_VAL(next), ZSTR_LEN(next));
    zend_string_release(type);
    return joined;
}

/* zend_compile.c's rendering, used by argument-parsing error messages. */
zend_string *zend_type_to_string_resolved(zend_type type, zend_class_entry *scope) {
    (void)scope;
    zend_string *str = NULL;
    if (ZEND_TYPE_HAS_LIST(type)) {
        const zend_type *single;
        ZEND_TYPE_LIST_FOREACH(ZEND_TYPE_LIST(type), single) {
            if (ZEND_TYPE_HAS_NAME(*single)) {
                str = elephc_add_type(str, ZEND_TYPE_NAME(*single));
            }
        } ZEND_TYPE_LIST_FOREACH_END();
    } else if (ZEND_TYPE_HAS_NAME(type)) {
        str = zend_string_copy(ZEND_TYPE_NAME(type));
    }
    uint32_t mask = ZEND_TYPE_PURE_MASK(type);
    if (mask == MAY_BE_ANY) {
        return elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_MIXED));
    }
    if (mask & MAY_BE_STATIC) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_STATIC));
    if (mask & MAY_BE_CALLABLE) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_CALLABLE));
    if (mask & MAY_BE_OBJECT) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_OBJECT));
    if (mask & MAY_BE_ARRAY) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_ARRAY));
    if (mask & MAY_BE_STRING) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_STRING));
    if (mask & MAY_BE_LONG) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_INT));
    if (mask & MAY_BE_DOUBLE) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_FLOAT));
    if ((mask & MAY_BE_BOOL) == MAY_BE_BOOL) {
        str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_BOOL));
    } else if (mask & MAY_BE_FALSE) {
        str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_FALSE));
    } else if (mask & MAY_BE_TRUE) {
        str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_TRUE));
    }
    if (mask & MAY_BE_VOID) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_VOID));
    if (mask & MAY_BE_NEVER) str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_NEVER));
    if (mask & MAY_BE_NULL) {
        bool is_union = !str || memchr(ZSTR_VAL(str), '|', ZSTR_LEN(str)) != NULL;
        if (!is_union) {
            zend_string *nullable = zend_string_concat2("?", 1, ZSTR_VAL(str), ZSTR_LEN(str));
            zend_string_release(str);
            return nullable;
        }
        str = elephc_add_type(str, ZSTR_KNOWN(ZEND_STR_NULL_LOWERCASE));
    }
    return str;
}

ZEND_API zend_string *zend_type_to_string(zend_type type) {
    return zend_type_to_string_resolved(type, NULL);
}

zend_string *zval_make_interned_string(zval *zv) {
    Z_STR_P(zv) = zend_new_interned_string(Z_STR_P(zv));
    if (ZSTR_IS_INTERNED(Z_STR_P(zv))) {
        Z_TYPE_FLAGS_P(zv) = 0;
    }
    return Z_STR_P(zv);
}

/* ------------------------------------------------------------ map_ptr */

ZEND_API size_t zend_map_ptr_static_size;
ZEND_API size_t zend_map_ptr_static_last;

ZEND_API void *zend_map_ptr_new(void) {
    if (CG(map_ptr_last) >= CG(map_ptr_size)) {
        CG(map_ptr_size) = ZEND_MM_ALIGNED_SIZE_EX(CG(map_ptr_last) + 1, 4096);
        CG(map_ptr_real_base) = perealloc(CG(map_ptr_real_base),
                                          (zend_map_ptr_static_size + CG(map_ptr_size)) * sizeof(void *), 1);
        CG(map_ptr_base) = ZEND_MAP_PTR_BIASED_BASE(CG(map_ptr_real_base));
    }
    void **ptr = (void **)CG(map_ptr_real_base) + zend_map_ptr_static_size + CG(map_ptr_last);
    *ptr = NULL;
    CG(map_ptr_last)++;
    return ZEND_MAP_PTR_PTR2OFFSET(ptr);
}

ZEND_API void zend_alloc_ce_cache(zend_string *type_name) {
    /* The class-entry cache is a runtime-cache optimisation keyed by opcodes;
     * internal registration works without it. */
    (void)type_name;
}

ZEND_API size_t zend_internal_run_time_cache_reserved_size(void) { return 0; }

/* ------------------------------------------------------------ configuration */

ZEND_API zend_extension *zend_get_extension(const char *extension_name) {
    (void)extension_name; /* Zend extensions hook the VM and are never hosted */
    return NULL;
}

/* ------------------------------------------------------------ garbage collection */

/* Without the cycle collector, cyclic garbage created inside an extension is
 * reclaimed at request end with the rest of request memory, not earlier. */
ZEND_API void ZEND_FASTCALL gc_possible_root(zend_refcounted *ref) { (void)ref; }
ZEND_API void ZEND_FASTCALL gc_remove_from_buffer(zend_refcounted *ref) { (void)ref; }

ZEND_API zend_get_gc_buffer *zend_get_gc_buffer_create(void) {
    zend_get_gc_buffer *buffer = &EG(get_gc_buffer);
    buffer->cur = buffer->start;
    return buffer;
}

ZEND_API void zend_get_gc_buffer_grow(zend_get_gc_buffer *buffer) {
    size_t old = buffer->end - buffer->start;
    size_t size = old ? old * 2 : 64;
    buffer->start = erealloc(buffer->start, size * sizeof(zval));
    buffer->end = buffer->start + size;
    buffer->cur = buffer->start + old;
}

ZEND_API void zend_weakrefs_notify(zend_object *object) { (void)object; }

/* ------------------------------------------------------------ functions */

ZEND_API void function_add_ref(zend_function *function) {
    if (function->type == ZEND_INTERNAL_FUNCTION && function->common.function_name) {
        zend_string_addref(function->common.function_name);
    }
}

ZEND_API void zend_function_dtor(zval *zv) {
    zend_function *function = Z_PTR_P(zv);
    if (function->type == ZEND_INTERNAL_FUNCTION && (function->common.fn_flags & ZEND_ACC_ARENA_ALLOCATED) == 0) {
        pefree(function, 1);
    }
}

/* Internal classes live as long as the process; the class table is never torn
 * down while a hosted extension could still hold one of its entries. */
ZEND_API void destroy_zend_class(zval *zv) { (void)zv; }

/* ------------------------------------------------------------ attributes */

ZEND_API zend_attribute *zend_add_attribute(HashTable **attributes, zend_string *name, uint32_t argc,
                                            uint32_t flags, uint32_t offset, uint32_t lineno) {
    bool persistent = flags & ZEND_ATTRIBUTE_PERSISTENT;
    if (*attributes == NULL) {
        *attributes = pemalloc(sizeof(HashTable), persistent);
        zend_hash_init(*attributes, 8, NULL, NULL, persistent);
    }
    zend_attribute *attr = pemalloc(ZEND_ATTRIBUTE_SIZE(argc), persistent);
    attr->name = zend_string_copy(name);
    attr->lcname = zend_string_tolower_ex(attr->name, persistent);
    attr->validation_error = NULL;
    attr->flags = flags;
    attr->lineno = lineno;
    attr->offset = offset;
    attr->argc = argc;
    for (uint32_t i = 0; i < argc; i++) {
        attr->args[i].name = NULL;
        ZVAL_UNDEF(&attr->args[i].value);
    }
    zend_hash_next_index_insert_ptr(*attributes, attr);
    return attr;
}

ZEND_API zend_attribute *zend_get_attribute_str(HashTable *attributes, const char *str, size_t len) {
    if (!attributes) {
        return NULL;
    }
    zend_attribute *attr;
    ZEND_HASH_PACKED_FOREACH_PTR(attributes, attr) {
        if (attr->offset == 0 && ZSTR_LEN(attr->lcname) == len
                && memcmp(ZSTR_VAL(attr->lcname), str, len) == 0) {
            return attr;
        }
    } ZEND_HASH_FOREACH_END();
    return NULL;
}

/* ------------------------------------------------------------ typed properties */

/* Typed-reference bookkeeping only exists for references to typed user
 * properties, which compiled Elephc code never hands to an extension. */
ZEND_API void ZEND_FASTCALL zend_ref_add_type_source(zend_property_info_source_list *list,
                                                     zend_property_info *prop) {
    (void)list; (void)prop;
}
ZEND_API void ZEND_FASTCALL zend_ref_del_type_source(zend_property_info_source_list *list,
                                                     const zend_property_info *prop) {
    (void)list; (void)prop;
}
ZEND_API bool zend_verify_property_type(const zend_property_info *info, zval *property, bool strict) {
    (void)info; (void)property; (void)strict;
    return true; /* internal classes type their own properties in C */
}
ZEND_API bool zend_verify_class_constant_type(const zend_class_constant *c, const zend_string *name,
                                              zval *constant) {
    (void)c; (void)name; (void)constant;
    return true;
}
ZEND_API bool ZEND_FASTCALL zend_verify_ref_assignable_zval(zend_reference *ref, zval *zv, bool strict) {
    (void)ref; (void)zv; (void)strict;
    return true;
}
ZEND_API bool ZEND_FASTCALL zend_verify_prop_assignable_by_ref(const zend_property_info *prop_info,
                                                              zval *orig_val, bool strict) {
    (void)prop_info; (void)orig_val; (void)strict;
    return true;
}
ZEND_API bool ZEND_FASTCALL zend_verify_prop_assignable_by_ref_ex(
        const zend_property_info *prop_info, zval *orig_val, bool strict,
        zend_verify_prop_assignable_by_ref_context context) {
    (void)prop_info; (void)orig_val; (void)strict; (void)context;
    return true;
}
ZEND_API zval *zend_assign_to_typed_ref_ex(zval *variable_ptr, zval *value, uint8_t value_type,
                                          bool strict, zend_refcounted **garbage_ptr) {
    (void)value_type; (void)strict;
    zval *target = Z_REFVAL_P(variable_ptr);
    if (garbage_ptr && Z_REFCOUNTED_P(target)) {
        *garbage_ptr = Z_COUNTED_P(target);
    } else {
        zval_ptr_dtor(target);
    }
    ZVAL_COPY(target, value);
    return target;
}
ZEND_API zval *zend_assign_to_typed_ref(zval *variable_ptr, zval *value, uint8_t value_type, bool strict) {
    return zend_assign_to_typed_ref_ex(variable_ptr, value, value_type, strict, NULL);
}

/* ------------------------------------------------------------ constant expressions */

ZEND_API zend_result ZEND_FASTCALL zval_update_constant_ex(zval *pp, zend_class_entry *scope) {
    (void)scope;
    if (Z_TYPE_P(pp) == IS_CONSTANT_AST) {
        ELEPHC_UNSUPPORTED("zval_update_constant_ex on a constant expression");
    }
    return SUCCESS;
}

/* ------------------------------------------------------------ VM-only paths */

ZEND_API void execute_ex(zend_execute_data *ex) { (void)ex; ELEPHC_UNSUPPORTED("execute_ex"); }

ZEND_API zend_result zend_call_function(zend_fcall_info *fci, zend_fcall_info_cache *fci_cache) {
    (void)fci; (void)fci_cache;
    ELEPHC_UNSUPPORTED("zend_call_function (calling back into PHP code)");
}
ZEND_API void zend_call_known_function(zend_function *fn, zend_object *object,
                                       zend_class_entry *called_scope, zval *retval_ptr,
                                       uint32_t param_count, zval *params, HashTable *named_params) {
    (void)fn; (void)object; (void)called_scope; (void)retval_ptr;
    (void)param_count; (void)params; (void)named_params;
    ELEPHC_UNSUPPORTED("zend_call_known_function (calling back into PHP code)");
}
ZEND_API void zend_call_known_instance_method_with_2_params(zend_function *fn, zend_object *object,
                                                            zval *retval_ptr, zval *param1,
                                                            zval *param2) {
    (void)fn; (void)object; (void)retval_ptr; (void)param1; (void)param2;
    ELEPHC_UNSUPPORTED("zend_call_known_instance_method_with_2_params");
}
ZEND_API zend_ast *zend_compile_string_to_ast(zend_string *code, struct _zend_arena **ast_arena,
                                              zend_string *filename) {
    (void)code; (void)ast_arena; (void)filename;
    ELEPHC_UNSUPPORTED("zend_compile_string_to_ast");
}
void zend_const_expr_to_zval(zval *result, zend_ast **ast_ptr, bool allow_dynamic) {
    (void)result; (void)ast_ptr; (void)allow_dynamic;
    ELEPHC_UNSUPPORTED("zend_const_expr_to_zval");
}
ZEND_API void ZEND_FASTCALL zend_ast_destroy(zend_ast *ast) {
    if (ast) ELEPHC_UNSUPPORTED("zend_ast_destroy");
}
ZEND_API void ZEND_FASTCALL zend_ast_ref_destroy(zend_ast_ref *ast) {
    (void)ast; ELEPHC_UNSUPPORTED("zend_ast_ref_destroy");
}
ZEND_API const zend_function *zend_get_closure_method_def(zend_object *obj) {
    (void)obj; ELEPHC_UNSUPPORTED("zend_get_closure_method_def");
}
zend_result zend_enum_build_backed_enum_table(zend_class_entry *ce) {
    (void)ce; ELEPHC_UNSUPPORTED("zend_enum_build_backed_enum_table");
}
void zend_enum_register_funcs(zend_class_entry *ce) {
    (void)ce; ELEPHC_UNSUPPORTED("zend_enum_register_funcs");
}
void zend_verify_enum(const zend_class_entry *ce) {
    (void)ce; ELEPHC_UNSUPPORTED("zend_verify_enum");
}
ZEND_API zend_array *zend_hooked_object_build_properties(zend_object *zobj) {
    (void)zobj; ELEPHC_UNSUPPORTED("zend_hooked_object_build_properties");
}
ZEND_API zend_object_iterator *zend_hooked_object_get_iterator(zend_class_entry *ce, zval *object,
                                                               int by_ref) {
    (void)ce; (void)object; (void)by_ref;
    ELEPHC_UNSUPPORTED("zend_hooked_object_get_iterator");
}

/* Lazy objects are created only by ReflectionClass::newLazy*() from PHP code,
 * so no object an extension sees can be one. */
ZEND_API zend_object *zend_lazy_object_init(zend_object *obj) { (void)obj; ELEPHC_UNSUPPORTED("zend_lazy_object_init"); }
zend_object *zend_lazy_object_get_instance(zend_object *obj) { (void)obj; ELEPHC_UNSUPPORTED("zend_lazy_object_get_instance"); }
zend_lazy_object_flags_t zend_lazy_object_get_flags(const zend_object *obj) { (void)obj; ELEPHC_UNSUPPORTED("zend_lazy_object_get_flags"); }
void zend_lazy_object_del_info(const zend_object *obj) { (void)obj; ELEPHC_UNSUPPORTED("zend_lazy_object_del_info"); }
ZEND_API HashTable *zend_lazy_object_get_properties(zend_object *object) { (void)object; ELEPHC_UNSUPPORTED("zend_lazy_object_get_properties"); }
zend_object *zend_lazy_object_clone(zend_object *old_obj) { (void)old_obj; ELEPHC_UNSUPPORTED("zend_lazy_object_clone"); }
HashTable *zend_lazy_object_debug_info(zend_object *object, int *is_temp) { (void)object; (void)is_temp; ELEPHC_UNSUPPORTED("zend_lazy_object_debug_info"); }
HashTable *zend_lazy_object_get_gc(zend_object *zobj, zval **table, int *n) { (void)zobj; (void)table; (void)n; ELEPHC_UNSUPPORTED("zend_lazy_object_get_gc"); }
ZEND_API zend_property_info *zend_lazy_object_get_property_info_for_slot(zend_object *obj, zval *slot) { (void)obj; (void)slot; ELEPHC_UNSUPPORTED("zend_lazy_object_get_property_info_for_slot"); }

ZEND_API void ZEND_FASTCALL _zend_observer_class_linked_notify(zend_class_entry *ce, zend_string *name) {
    (void)ce; (void)name;
}
ZEND_API void _zend_observer_error_notify(int type, zend_string *error_filename, uint32_t error_lineno,
                                          zend_string *message) {
    (void)type; (void)error_filename; (void)error_lineno; (void)message;
}
