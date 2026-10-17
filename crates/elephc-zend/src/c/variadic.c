/* variadic.c — the part of Elephc's PHP extension ABI written in C.
 *
 * Two things stable Rust cannot do live here: define a C variadic function
 * (zend_error(), zend_parse_parameters(), spprintf()...), and set up a
 * sigsetjmp frame (the bailout a fatal error jumps back to). Every function
 * only unpacks its arguments and calls into Rust, which does the work; nothing
 * here knows the layout of a Zend structure, so values travel as void *.
 */

#include <setjmp.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ------------------------------------------------------------ Rust entry points */

void elephc_zend_report(int type, const char *message, size_t len);
void elephc_zend_report_docref(int type, const char *message, size_t len);
void *elephc_zend_throw_message(void *ce, const char *message, size_t len, int64_t code);
void elephc_zend_throw_argument_message(void *ce, uint32_t arg_num, const char *message, size_t len);
int elephc_zend_parse_args(int flags, uint32_t num_args, const char *spec, void **slots, size_t count, void *this_ptr);
int elephc_zend_parse_one_arg(int flags, uint32_t arg_num, void *arg, const char *spec, void **slots, size_t count);
void elephc_zend_class_implements_list(void *ce, int count, void **interfaces);
__attribute__((noreturn)) void elephc_zend_unsupported(const char *what);
void *zend_string_init(const char *str, size_t len, _Bool persistent);
size_t zend_write(const char *str, size_t len);
void smart_str_appendl_ex(void *dest, const char *str, size_t len, _Bool persistent);
void smart_string_appendl_ex(void *dest, const char *str, size_t len, _Bool persistent);

/* E_ERROR | E_CORE_ERROR | E_COMPILE_ERROR | E_USER_ERROR | E_RECOVERABLE_ERROR | E_PARSE */
#define ELEPHC_FATAL_ERRORS (1 | 16 | 64 | 256 | 4096 | 4)
#define E_ERROR_CODE 1

/* ------------------------------------------------------------ formatting */

/* Formats into a malloc'd buffer (emalloc is malloc), NUL-terminated, at most
 * `max_len` bytes when it is non-zero, as spprintf() does. */
static char *elephc_vformat(size_t max_len, size_t *out_len, const char *format, va_list args) {
    va_list copy;
    va_copy(copy, args);
    int needed = vsnprintf(NULL, 0, format, copy);
    va_end(copy);
    if (needed < 0) {
        needed = 0;
    }
    char *buffer = malloc((size_t)needed + 1);
    if (!buffer) {
        abort();
    }
    vsnprintf(buffer, (size_t)needed + 1, format, args);
    size_t len = (size_t)needed;
    if (max_len && len > max_len) {
        len = max_len;
        buffer[len] = '\0';
    }
    *out_len = len;
    return buffer;
}

size_t zend_vspprintf(char **pbuf, size_t max_len, const char *format, va_list ap) {
    size_t len;
    *pbuf = elephc_vformat(max_len, &len, format, ap);
    return len;
}

size_t zend_spprintf(char **message, size_t max_len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    size_t len = zend_vspprintf(message, max_len, format, args);
    va_end(args);
    return len;
}

size_t zend_spprintf_unchecked(char **message, size_t max_len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    size_t len = zend_vspprintf(message, max_len, format, args);
    va_end(args);
    return len;
}

void *zend_vstrpprintf(size_t max_len, const char *format, va_list ap) {
    size_t len;
    char *buffer = elephc_vformat(max_len, &len, format, ap);
    void *str = zend_string_init(buffer, len, 0);
    free(buffer);
    return str;
}

void *zend_strpprintf(size_t max_len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    void *str = zend_vstrpprintf(max_len, format, args);
    va_end(args);
    return str;
}

void *zend_strpprintf_unchecked(size_t max_len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    void *str = zend_vstrpprintf(max_len, format, args);
    va_end(args);
    return str;
}

int ap_php_vsnprintf(char *buf, size_t len, const char *format, va_list ap) {
    return vsnprintf(buf, len, format, ap);
}

int ap_php_snprintf(char *buf, size_t len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    int written = vsnprintf(buf, len, format, args);
    va_end(args);
    return written;
}

/* slprintf(): the length actually written, never more than fits. */
int ap_php_vslprintf(char *buf, size_t len, const char *format, va_list ap) {
    int written = vsnprintf(buf, len, format, ap);
    if (written < 0) {
        return 0;
    }
    if (len && (size_t)written >= len) {
        return (int)(len - 1);
    }
    return written;
}

int ap_php_slprintf(char *buf, size_t len, const char *format, ...) {
    va_list args;
    va_start(args, format);
    int written = ap_php_vslprintf(buf, len, format, args);
    va_end(args);
    return written;
}

static size_t elephc_vprintf(const char *format, va_list args) {
    size_t len;
    char *buffer = elephc_vformat(0, &len, format, args);
    size_t written = zend_write(buffer, len);
    free(buffer);
    return written;
}

size_t php_printf(const char *format, ...) {
    va_list args;
    va_start(args, format);
    size_t written = elephc_vprintf(format, args);
    va_end(args);
    return written;
}

size_t php_printf_unchecked(const char *format, ...) {
    va_list args;
    va_start(args, format);
    size_t written = elephc_vprintf(format, args);
    va_end(args);
    return written;
}

void smart_str_append_printf(void *dest, const char *format, ...) {
    va_list args;
    size_t len;
    va_start(args, format);
    char *buffer = elephc_vformat(0, &len, format, args);
    va_end(args);
    smart_str_appendl_ex(dest, buffer, len, 0);
    free(buffer);
}

void smart_string_append_printf(void *dest, const char *format, ...) {
    va_list args;
    size_t len;
    va_start(args, format);
    char *buffer = elephc_vformat(0, &len, format, args);
    va_end(args);
    smart_string_appendl_ex(dest, buffer, len, 0);
    free(buffer);
}

/* ------------------------------------------------------------ errors */

static void elephc_error_va(int type, const char *format, va_list args) {
    size_t len;
    char *message = elephc_vformat(0, &len, format, args);
    elephc_zend_report(type, message, len);
    free(message);
}

void zend_error(int type, const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_error_va(type, format, args);
    va_end(args);
}

void zend_error_unchecked(int type, const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_error_va(type, format, args);
    va_end(args);
}

void zend_error_at(int type, void *filename, uint32_t lineno, const char *format, ...) {
    (void)filename;
    (void)lineno;
    va_list args;
    va_start(args, format);
    elephc_error_va(type, format, args);
    va_end(args);
}

/* A noreturn error is fatal whatever level it names: report it as E_ERROR
 * when it is not, so it bails out. */
__attribute__((noreturn)) void zend_error_noreturn(int type, const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_error_va((type & ELEPHC_FATAL_ERRORS) ? type : E_ERROR_CODE, format, args);
    va_end(args);
    abort();
}

__attribute__((noreturn)) void zend_error_at_noreturn(int type, void *filename, uint32_t lineno, const char *format, ...) {
    (void)filename;
    (void)lineno;
    va_list args;
    va_start(args, format);
    elephc_error_va((type & ELEPHC_FATAL_ERRORS) ? type : E_ERROR_CODE, format, args);
    va_end(args);
    abort();
}

void php_verror(const char *docref, const char *params, int type, const char *format, va_list args) {
    (void)docref;
    (void)params;
    size_t len;
    char *message = elephc_vformat(0, &len, format, args);
    elephc_zend_report_docref(type, message, len);
    free(message);
}

void php_error_docref(const char *docref, int type, const char *format, ...) {
    va_list args;
    va_start(args, format);
    php_verror(docref, "", type, format, args);
    va_end(args);
}

void php_error_docref1(const char *docref, const char *param1, int type, const char *format, ...) {
    va_list args;
    va_start(args, format);
    php_verror(docref, param1, type, format, args);
    va_end(args);
}

void php_error_docref2(const char *docref, const char *param1, const char *param2, int type, const char *format, ...) {
    (void)param2;
    va_list args;
    va_start(args, format);
    php_verror(docref, param1, type, format, args);
    va_end(args);
}

/* ------------------------------------------------------------ exceptions */

/* A NULL class means the base Exception (or Error, for the *_error helpers,
 * which the Rust side picks). */
static void *elephc_throw_va(void *ce, int64_t code, const char *format, va_list args) {
    size_t len;
    char *message = elephc_vformat(0, &len, format, args);
    void *object = elephc_zend_throw_message(ce, message, len, code);
    free(message);
    return object;
}

void *elephc_zend_class_error(void);
void *elephc_zend_class_type_error(void);
void *elephc_zend_class_value_error(void);
void *elephc_zend_class_argument_count_error(void);

void zend_throw_error(void *exception_ce, const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_throw_va(exception_ce ? exception_ce : elephc_zend_class_error(), 0, format, args);
    va_end(args);
}

void zend_type_error(const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_throw_va(elephc_zend_class_type_error(), 0, format, args);
    va_end(args);
}

void zend_value_error(const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_throw_va(elephc_zend_class_value_error(), 0, format, args);
    va_end(args);
}

void zend_argument_count_error(const char *format, ...) {
    va_list args;
    va_start(args, format);
    elephc_throw_va(elephc_zend_class_argument_count_error(), 0, format, args);
    va_end(args);
}

void *zend_throw_exception_ex(void *exception_ce, int64_t code, const char *format, ...) {
    va_list args;
    va_start(args, format);
    void *object = elephc_throw_va(exception_ce, code, format, args);
    va_end(args);
    return object;
}

void zend_argument_error_variadic(void *error_ce, uint32_t arg_num, const char *format, va_list va) {
    size_t len;
    char *message = elephc_vformat(0, &len, format, va);
    elephc_zend_throw_argument_message(error_ce, arg_num, message, len);
    free(message);
}

void zend_argument_error(void *error_ce, uint32_t arg_num, const char *format, ...) {
    va_list args;
    va_start(args, format);
    zend_argument_error_variadic(error_ce, arg_num, format, args);
    va_end(args);
}

void zend_argument_type_error(uint32_t arg_num, const char *format, ...) {
    va_list args;
    va_start(args, format);
    zend_argument_error_variadic(elephc_zend_class_type_error(), arg_num, format, args);
    va_end(args);
}

void zend_argument_value_error(uint32_t arg_num, const char *format, ...) {
    va_list args;
    va_start(args, format);
    zend_argument_error_variadic(elephc_zend_class_value_error(), arg_num, format, args);
    va_end(args);
}

/* ------------------------------------------------------------ argument parsing */

#define ELEPHC_ZPP_MAX_SLOTS 128

/* How many pointer arguments each type spec letter consumes. '!' after l, d
 * or b adds the is_null flag pointer; the modifiers consume none. Returns -1
 * for a letter PHP does not define. */
static int elephc_spec_slots(const char *spec, size_t at) {
    char c = spec[at];
    char next = spec[at + 1];
    switch (c) {
        case 'l': case 'd': case 'b':
            return next == '!' ? 2 : 1;
        case 's': case 'p':
            return 2;
        case 'S': case 'P': case 'a': case 'A': case 'h': case 'H': case 'o': case 'C':
        case 'z': case 'r': case 'n':
            return 1;
        case 'O': case 'f':
            return 2;
        case '*': case '+':
            return 2;
        case '|': case '/': case '!':
            return 0;
        default:
            return -1;
    }
}

static int elephc_collect_slots(const char *spec, va_list args, void **slots, size_t *count) {
    size_t n = 0;
    for (size_t i = 0; spec[i]; i++) {
        int slots_for = elephc_spec_slots(spec, i);
        if (slots_for < 0) {
            return -1;
        }
        for (int k = 0; k < slots_for; k++) {
            if (n >= ELEPHC_ZPP_MAX_SLOTS) {
                return -1;
            }
            slots[n++] = va_arg(args, void *);
        }
    }
    *count = n;
    return 0;
}

static int elephc_parse_va(int flags, uint32_t num_args, void *this_ptr, const char *spec, va_list args) {
    void *slots[ELEPHC_ZPP_MAX_SLOTS];
    size_t count = 0;
    if (elephc_collect_slots(spec, args, slots, &count) != 0) {
        elephc_zend_unsupported("zend_parse_parameters() with a type spec this engine does not know");
    }
    return elephc_zend_parse_args(flags, num_args, spec, slots, count, this_ptr);
}

int zend_parse_parameters(uint32_t num_args, const char *type_spec, ...) {
    va_list args;
    va_start(args, type_spec);
    int result = elephc_parse_va(0, num_args, NULL, type_spec, args);
    va_end(args);
    return result;
}

int zend_parse_parameters_ex(int flags, uint32_t num_args, const char *type_spec, ...) {
    va_list args;
    va_start(args, type_spec);
    int result = elephc_parse_va(flags, num_args, NULL, type_spec, args);
    va_end(args);
    return result;
}

/* With a $this, the spec's leading 'O' (object, class) describes it: the
 * first two pointers receive it, the rest parse the arguments. */
int zend_parse_method_parameters(uint32_t num_args, void *this_ptr, const char *type_spec, ...) {
    va_list args;
    va_start(args, type_spec);
    int result = elephc_parse_va(0, num_args, this_ptr, type_spec, args);
    va_end(args);
    return result;
}

int zend_parse_method_parameters_ex(int flags, uint32_t num_args, void *this_ptr, const char *type_spec, ...) {
    va_list args;
    va_start(args, type_spec);
    int result = elephc_parse_va(flags, num_args, this_ptr, type_spec, args);
    va_end(args);
    return result;
}

int zend_parse_parameter(int flags, uint32_t arg_num, void *arg, const char *spec, ...) {
    void *slots[ELEPHC_ZPP_MAX_SLOTS];
    size_t count = 0;
    va_list args;
    va_start(args, spec);
    int status = elephc_collect_slots(spec, args, slots, &count);
    va_end(args);
    if (status != 0) {
        elephc_zend_unsupported("zend_parse_parameter() with a type spec this engine does not know");
    }
    return elephc_zend_parse_one_arg(flags, arg_num, arg, spec, slots, count);
}

/* ------------------------------------------------------------ variadic registrations */

void zend_class_implements(void *class_entry, int num_interfaces, ...) {
    void *interfaces[64];
    if (num_interfaces < 0 || num_interfaces > 64) {
        elephc_zend_unsupported("zend_class_implements() with more than 64 interfaces");
    }
    va_list args;
    va_start(args, num_interfaces);
    for (int i = 0; i < num_interfaces; i++) {
        interfaces[i] = va_arg(args, void *);
    }
    va_end(args);
    elephc_zend_class_implements_list(class_entry, num_interfaces, interfaces);
}

void zend_fcall_info_argn(void *fci, uint32_t argc, ...) {
    (void)fci;
    (void)argc;
    elephc_zend_unsupported("zend_fcall_info_argn (calling back into PHP code)");
}

void zend_hash_apply_with_arguments(void *ht, void *apply_func, int num_args, ...) {
    (void)ht;
    (void)apply_func;
    (void)num_args;
    elephc_zend_unsupported("zend_hash_apply_with_arguments");
}

int zend_set_hash_symbol(void *symbol, const char *name, size_t name_length, _Bool is_ref, int num_symbol_tables, ...) {
    (void)symbol;
    (void)name;
    (void)name_length;
    (void)is_ref;
    (void)num_symbol_tables;
    elephc_zend_unsupported("zend_set_hash_symbol (PHP symbol tables)");
}

void php_stream_wrapper_log_error(const void *wrapper, int options, const char *fmt, ...) {
    (void)wrapper;
    (void)options;
    (void)fmt;
    elephc_zend_unsupported("php_stream_wrapper_log_error (PHP streams)");
}

void php_info_print_table_header(int num_cols, ...) { (void)num_cols; }
void php_info_print_table_row(int num_cols, ...) { (void)num_cols; }
void php_info_print_table_row_ex(int num_cols, const char *css, ...) { (void)num_cols; (void)css; }

/* ------------------------------------------------------------ the program's runtime */

/* Elephc's own stdout writer (output buffering, --web capture, print_r
 * capture all hang off it) and its `@` depth. Their assembler names are the
 * same on every target, hence __asm__. A program compiled by Elephc defines
 * both; the weak fallbacks serve the extension introspector and unit tests. */
void elephc_rt_stdout_write(const char *str, size_t len) __asm__("__rt_stdout_write");
extern long long elephc_rt_diag_suppression __asm__("_rt_diag_suppression");

__attribute__((weak)) void elephc_rt_stdout_write(const char *str, size_t len) {
    while (len > 0) {
        long written = (long)fwrite(str, 1, len, stdout);
        if (written <= 0) {
            break;
        }
        str += written;
        len -= (size_t)written;
    }
    fflush(stdout);
}

__attribute__((weak)) long long elephc_rt_diag_suppression = 0;

void elephc_zend_stdout_write(const char *str, size_t len) {
    elephc_rt_stdout_write(str, len);
}

int elephc_zend_diag_suppressed(void) {
    return elephc_rt_diag_suppression > 0;
}

/* ------------------------------------------------------------ bailout */

int elephc_zend_protected_call(void (*handler)(void *, void *), void *execute_data, void *return_value,
                               sigjmp_buf **slot) {
    sigjmp_buf *outer = *slot;
    sigjmp_buf frame;
    int bailed = 0;
    *slot = &frame;
    if (sigsetjmp(frame, 0) == 0) {
        handler(execute_data, return_value);
    } else {
        bailed = 1;
    }
    *slot = outer;
    return bailed;
}

__attribute__((noreturn)) void elephc_zend_longjmp(sigjmp_buf *target) {
    siglongjmp(*target, 1);
}
