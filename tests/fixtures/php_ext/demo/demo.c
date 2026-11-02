/* demo.c — the elephc_demo fixture extension: module lifecycle, INI,
 * constants, exception classes, and scalar/by-reference functions. The
 * DemoCounter class lives in demo_counter.c.
 *
 * Ordinary PHP 8 extension code, compiled unmodified by `elephc extension`.
 * Every function exercises a different engine path a hosted extension relies
 * on; tests/php_ext_tests.rs asserts each against the output real PHP gives.
 */

#ifdef HAVE_CONFIG_H
#include "config.h"
#endif

#include "php.h"
#include "php_ini.h"
#include "ext/standard/info.h"
#include "ext/spl/spl_exceptions.h"
#include "zend_exceptions.h"
#include "zend_interfaces.h"
#include "php_elephc_demo.h"

ZEND_DECLARE_MODULE_GLOBALS(elephc_demo)

zend_class_entry *demo_exception_ce;
zend_class_entry *demo_parse_exception_ce;

PHP_INI_BEGIN()
    STD_PHP_INI_ENTRY("elephc_demo.greeting", "Hello", PHP_INI_ALL, OnUpdateString,
                      greeting, zend_elephc_demo_globals, elephc_demo_globals)
PHP_INI_END()

/* demo_add(int $a, int $b = 1): int — fast ZPP with a stated default. */
PHP_FUNCTION(demo_add)
{
    zend_long a, b = 1;
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_LONG(a)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(b)
    ZEND_PARSE_PARAMETERS_END();
    DEMO_G(calls)++;
    RETURN_LONG(a + b);
}

/* demo_greet(string $name, ?string $greeting = null): string — the INI value
 * is the default greeting. */
PHP_FUNCTION(demo_greet)
{
    zend_string *name;
    zend_string *greeting = NULL;
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_STR(name)
        Z_PARAM_OPTIONAL
        Z_PARAM_STR_OR_NULL(greeting)
    ZEND_PARSE_PARAMETERS_END();
    const char *prefix = greeting ? ZSTR_VAL(greeting) : DEMO_G(greeting);
#ifdef DEMO_LOUD
    RETURN_STR(zend_strpprintf(0, "%s, %s!!!", prefix, ZSTR_VAL(name)));
#else
    RETURN_STR(zend_strpprintf(0, "%s, %s.", prefix, ZSTR_VAL(name)));
#endif
}

/* demo_split(string $subject, string $separator = ","): array — a list. */
PHP_FUNCTION(demo_split)
{
    char *subject, *separator = ",";
    size_t subject_len, separator_len = 1;
    if (zend_parse_parameters(ZEND_NUM_ARGS(), "s|s", &subject, &subject_len, &separator, &separator_len) == FAILURE) {
        RETURN_THROWS();
    }
    if (separator_len == 0) {
        zend_argument_value_error(2, "cannot be empty");
        RETURN_THROWS();
    }
    array_init(return_value);
    const char *start = subject, *end = subject + subject_len, *hit;
    while ((hit = php_memnstr(start, separator, separator_len, end)) != NULL) {
        add_next_index_stringl(return_value, start, hit - start);
        start = hit + separator_len;
    }
    add_next_index_stringl(return_value, start, end - start);
}

/* demo_stats(array $numbers): array — reads a packed array, returns a hash. */
PHP_FUNCTION(demo_stats)
{
    HashTable *numbers;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ARRAY_HT(numbers)
    ZEND_PARSE_PARAMETERS_END();
    double sum = 0, min = 0, max = 0;
    zend_long count = 0;
    zval *entry;
    ZEND_HASH_FOREACH_VAL(numbers, entry) {
        double value = zval_get_double(entry);
        if (count == 0 || value < min) min = value;
        if (count == 0 || value > max) max = value;
        sum += value;
        count++;
    } ZEND_HASH_FOREACH_END();
    array_init(return_value);
    add_assoc_long(return_value, "count", count);
    add_assoc_double(return_value, "sum", sum);
    add_assoc_double(return_value, "min", min);
    add_assoc_double(return_value, "max", max);
    if (count) {
        add_assoc_double(return_value, "mean", sum / count);
    } else {
        add_assoc_null(return_value, "mean");
    }
}

/* demo_inc(int $value, &$overflowed = null): int — writes through a reference. */
PHP_FUNCTION(demo_inc)
{
    zend_long value;
    zval *overflowed = NULL;
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_LONG(value)
        Z_PARAM_OPTIONAL
        Z_PARAM_ZVAL(overflowed)
    ZEND_PARSE_PARAMETERS_END();
    bool wrapped = value == ZEND_LONG_MAX;
    if (overflowed) {
        ZEND_TRY_ASSIGN_REF_BOOL(overflowed, wrapped);
    }
    RETURN_LONG(wrapped ? ZEND_LONG_MIN : value + 1);
}

/* demo_parse(string $input): int — throws the extension's own exception. */
PHP_FUNCTION(demo_parse)
{
    zend_string *input;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STR(input)
    ZEND_PARSE_PARAMETERS_END();
    char *end;
    zend_long value = ZEND_STRTOL(ZSTR_VAL(input), &end, 10);
    if (ZSTR_LEN(input) == 0 || *end != '\0') {
        zend_throw_exception_ex(demo_parse_exception_ce, 42, "'%s' is not an integer", ZSTR_VAL(input));
        RETURN_THROWS();
    }
    RETURN_LONG(value);
}

/* demo_fail(): never — an SPL exception, not one of the extension's. */
PHP_FUNCTION(demo_fail)
{
    ZEND_PARSE_PARAMETERS_NONE();
    zend_throw_exception(spl_ce_OutOfRangeException, "out of range", 7);
}

/* demo_warn(string $what): bool — a warning, then a normal return. "pair"
 * warns through php_error_docref2, whose parameters PHP prints inside the
 * function's parentheses. */
PHP_FUNCTION(demo_warn)
{
    zend_string *what;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STR(what)
    ZEND_PARSE_PARAMETERS_END();
    if (zend_string_equals_literal(what, "pair")) {
        php_error_docref2(NULL, "left", "right", E_WARNING, "paired");
        RETURN_TRUE;
    }
    php_error_docref(NULL, E_WARNING, "careful with %s", ZSTR_VAL(what));
    RETURN_TRUE;
}

/* demo_fatal(): void — an E_ERROR, which ends the program as in PHP. */
PHP_FUNCTION(demo_fatal)
{
    ZEND_PARSE_PARAMETERS_NONE();
    php_error_docref(NULL, E_ERROR, "the fixture gave up");
}

/* demo_count(string $haystack, string $needle, &$positions = null, int $offset = 0): int
 * — an optional by-reference parameter followed by a by-value one. */
PHP_FUNCTION(demo_count)
{
    zend_string *haystack, *needle;
    zval *positions = NULL;
    zend_long offset = 0, found = 0;
    ZEND_PARSE_PARAMETERS_START(2, 4)
        Z_PARAM_STR(haystack)
        Z_PARAM_STR(needle)
        Z_PARAM_OPTIONAL
        Z_PARAM_ZVAL(positions)
        Z_PARAM_LONG(offset)
    ZEND_PARSE_PARAMETERS_END();
    if (positions) {
        positions = zend_try_array_init(positions);
        if (!positions) {
            RETURN_THROWS();
        }
    }
    if (ZSTR_LEN(needle) == 0) {
        zend_argument_value_error(2, "cannot be empty");
        RETURN_THROWS();
    }
    for (size_t at = offset < 0 ? 0 : (size_t) offset; at + ZSTR_LEN(needle) <= ZSTR_LEN(haystack); at++) {
        if (memcmp(ZSTR_VAL(haystack) + at, ZSTR_VAL(needle), ZSTR_LEN(needle)) == 0) {
            found++;
            if (positions) {
                add_next_index_long(positions, (zend_long) at);
            }
        }
    }
    RETURN_LONG(found);
}

/* demo_throw_code(int $code): never — throws DemoException with $code, a
 * full zend_long. */
PHP_FUNCTION(demo_throw_code)
{
    zend_long code;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(code)
    ZEND_PARSE_PARAMETERS_END();
    zend_throw_exception(demo_exception_ce, "coded", code);
}

/* demo_late(): string — not in the module's function table: MINIT
 * registers it with zend_register_functions. */
PHP_FUNCTION(demo_late)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_STRING("registered in MINIT");
}

/* demo_consume(int $amount, &$left): int — writes through the reference,
 * then throws when the amount overdraws it. PHP keeps the write. */
PHP_FUNCTION(demo_consume)
{
    zend_long amount;
    zval *left;
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_LONG(amount)
        Z_PARAM_ZVAL(left)
    ZEND_PARSE_PARAMETERS_END();
    zval *current = Z_REFVAL_P(left);
    zend_long remaining = (Z_TYPE_P(current) == IS_LONG ? Z_LVAL_P(current) : 0) - amount;
    ZEND_TRY_ASSIGN_REF_LONG(left, remaining < 0 ? 0 : remaining);
    if (remaining < 0) {
        zend_throw_exception(spl_ce_UnderflowException, "overdrawn", 3);
        RETURN_THROWS();
    }
    RETURN_LONG(remaining);
}

/* demo_error_object(): object — an object of a class other than stdClass,
 * which the host refuses to rebuild. */
PHP_FUNCTION(demo_error_object)
{
    ZEND_PARSE_PARAMETERS_NONE();
    object_init_ex(return_value, demo_parse_exception_ce);
}

/* demo_cycle(): stdClass — an object whose property is itself. */
PHP_FUNCTION(demo_cycle)
{
    ZEND_PARSE_PARAMETERS_NONE();
    object_init(return_value);
    add_property_zval(return_value, "self", return_value);
}

/* demo_calls(): int — module globals survive between calls. */
/* demo_scale(float $x, bool $negate = false): float — float and bool
 * arguments and a float result. */
PHP_FUNCTION(demo_scale)
{
    double x;
    bool negate = false;
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_DOUBLE(x)
        Z_PARAM_OPTIONAL
        Z_PARAM_BOOL(negate)
    ZEND_PARSE_PARAMETERS_END();
    RETURN_DOUBLE(negate ? -x * 2.0 : x * 2.0);
}

/* demo_is_even(int $n): bool */
PHP_FUNCTION(demo_is_even)
{
    zend_long n;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(n)
    ZEND_PARSE_PARAMETERS_END();
    RETURN_BOOL(n % 2 == 0);
}

PHP_FUNCTION(demo_calls)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_LONG(DEMO_G(calls));
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_add, 0, 1, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, a, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, b, IS_LONG, 0, "1")
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_scale, 0, 1, IS_DOUBLE, 0)
    ZEND_ARG_TYPE_INFO(0, x, IS_DOUBLE, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, negate, _IS_BOOL, 0, "false")
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_is_even, 0, 1, _IS_BOOL, 0)
    ZEND_ARG_TYPE_INFO(0, n, IS_LONG, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_greet, 0, 1, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO(0, name, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, greeting, IS_STRING, 1, "null")
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_split, 0, 1, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, subject, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, separator, IS_STRING, 0, "\",\"")
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_stats, 0, 1, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, numbers, IS_ARRAY, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_inc, 0, 1, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, value, IS_LONG, 0)
    ZEND_ARG_INFO_WITH_DEFAULT_VALUE(1, overflowed, "null")
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_parse, 0, 1, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, input, IS_STRING, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_fail, 0, 0, IS_NEVER, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_warn, 0, 1, _IS_BOOL, 0)
    ZEND_ARG_TYPE_INFO(0, what, IS_STRING, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_fatal, 0, 0, IS_VOID, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_calls, 0, 0, IS_LONG, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_count, 0, 2, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, haystack, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO(0, needle, IS_STRING, 0)
    ZEND_ARG_INFO_WITH_DEFAULT_VALUE(1, positions, "null")
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, offset, IS_LONG, 0, "0")
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_throw_code, 0, 1, IS_NEVER, 0)
    ZEND_ARG_TYPE_INFO(0, code, IS_LONG, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_late, 0, 0, IS_STRING, 0)
ZEND_END_ARG_INFO()

static const zend_function_entry demo_late_functions[] = {
    PHP_FE(demo_late, arginfo_demo_late)
    PHP_FE_END
};

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_consume, 0, 2, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, amount, IS_LONG, 0)
    ZEND_ARG_INFO(1, left)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_error_object, 0, 0, IS_OBJECT, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_OBJ_INFO_EX(arginfo_demo_cycle, 0, 0, stdClass, 0)
ZEND_END_ARG_INFO()

/* Old-style arginfo, as simdjson writes it: no types, no defaults. */
ZEND_BEGIN_ARG_INFO_EX(arginfo_demo_echo, 0, 0, 1)
    ZEND_ARG_INFO(0, value)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_nested, 0, 0, IS_ARRAY, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_OBJ_INFO_EX(arginfo_demo_object, 0, 1, stdClass, 0)
    ZEND_ARG_TYPE_INFO(0, name, IS_STRING, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_keep, 0, 1, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, value, IS_MIXED, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_kept, 0, 0, IS_ARRAY, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_counters_freed, 0, 0, IS_LONG, 0)
ZEND_END_ARG_INFO()

/* Same signature: php-src stubs (gen_stub.php) alias identical arginfo the same way. */
#define arginfo_demo_iterators_freed arginfo_demo_counters_freed

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_inspect, 0, 1, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO(0, value, IS_OBJECT, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_slice, 0, 1, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, items, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, offset, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, length, IS_LONG, 1)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_apply, 0, 2, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, callback, IS_CALLABLE, 0)
    ZEND_ARG_TYPE_INFO(0, items, IS_ARRAY, 0)
ZEND_END_ARG_INFO()

/* demo_apply(callable $callback, array $items): array — calls the program's
 * callable on each item (value, key) and collects the results; stops at the
 * first exception the callable throws. */
PHP_FUNCTION(demo_apply)
{
    zend_fcall_info fci;
    zend_fcall_info_cache fcc;
    HashTable *items;
    zend_string *key;
    zend_ulong index;
    zval *item, args[2], result;
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_FUNC(fci, fcc)
        Z_PARAM_ARRAY_HT(items)
    ZEND_PARSE_PARAMETERS_END();
    array_init(return_value);
    ZEND_HASH_FOREACH_KEY_VAL(items, index, key, item) {
        ZVAL_COPY(&args[0], item);
        if (key) {
            ZVAL_STR_COPY(&args[1], key);
        } else {
            ZVAL_LONG(&args[1], (zend_long) index);
        }
        fci.retval = &result;
        fci.params = args;
        fci.param_count = 2;
        if (zend_call_function(&fci, &fcc) == SUCCESS && !EG(exception)) {
            add_next_index_zval(return_value, &result);
        }
        zval_ptr_dtor(&args[0]);
        zval_ptr_dtor(&args[1]);
        if (EG(exception)) {
            RETURN_THROWS();
        }
    } ZEND_HASH_FOREACH_END();
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_try, 0, 2, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO(0, callback, IS_CALLABLE, 0)
    ZEND_ARG_TYPE_INFO(0, wrap, _IS_BOOL, 0)
ZEND_END_ARG_INFO()

/* demo_try(callable $callback, bool $wrap): string — calls the callable once;
 * when it throws, either swallows the exception and returns "cleared", or
 * throws its own DemoException("wrapped") over it, as extensions do. */
PHP_FUNCTION(demo_try)
{
    zend_fcall_info fci;
    zend_fcall_info_cache fcc;
    bool wrap;
    zval result;
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_FUNC(fci, fcc)
        Z_PARAM_BOOL(wrap)
    ZEND_PARSE_PARAMETERS_END();
    fci.retval = &result;
    fci.params = NULL;
    fci.param_count = 0;
    if (zend_call_function(&fci, &fcc) == SUCCESS && !EG(exception)) {
        zval_ptr_dtor(&result);
        RETURN_STRING("returned");
    }
    if (wrap) {
        zend_throw_exception(demo_exception_ce, "wrapped", 7);
        RETURN_THROWS();
    }
    zend_clear_exception();
    RETURN_STRING("cleared");
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_call_all, 0, 0, IS_ARRAY, 0)
    ZEND_ARG_VARIADIC_TYPE_INFO(0, callbacks, IS_CALLABLE, 0)
ZEND_END_ARG_INFO()

/* demo_call_all(callable ...$callbacks): array — calls every callable of a
 * variadic list once and collects the results; stops at the first exception. */
PHP_FUNCTION(demo_call_all)
{
    zval *callbacks = NULL, result;
    zend_fcall_info fci;
    zend_fcall_info_cache fcc;
    uint32_t count = 0, i;
    ZEND_PARSE_PARAMETERS_START(0, -1)
        Z_PARAM_VARIADIC('*', callbacks, count)
    ZEND_PARSE_PARAMETERS_END();
    array_init(return_value);
    for (i = 0; i < count; i++) {
        if (zend_fcall_info_init(&callbacks[i], 0, &fci, &fcc, NULL, NULL) != SUCCESS) {
            zend_throw_error(NULL, "demo_call_all(): Argument #%u must be a valid callback", i + 1);
            RETURN_THROWS();
        }
        fci.retval = &result;
        fci.params = NULL;
        fci.param_count = 0;
        if (zend_call_function(&fci, &fcc) == SUCCESS && !EG(exception)) {
            add_next_index_zval(return_value, &result);
        }
        if (EG(exception)) {
            RETURN_THROWS();
        }
    }
}

/* demo_inspect(object $value): string — takes an object of the program's own
 * classes, which Elephc cannot pass yet: declared, but only to explain itself. */
PHP_FUNCTION(demo_inspect)
{
    zend_object *value;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_OBJ(value)
    ZEND_PARSE_PARAMETERS_END();
    RETURN_STR_COPY(value->ce->name);
}

/* demo_slice(array $items, int $offset, ?int $length): array — optional
 * parameters whose defaults only the C code knows: an int and a nullable int. */
PHP_FUNCTION(demo_slice)
{
    HashTable *items;
    zend_long offset = 0, length = 0, position = 0;
    bool length_null = 1;
    zval *item;
    ZEND_PARSE_PARAMETERS_START(1, 3)
        Z_PARAM_ARRAY_HT(items)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(offset)
        Z_PARAM_LONG_OR_NULL(length, length_null)
    ZEND_PARSE_PARAMETERS_END();
    array_init(return_value);
    ZEND_HASH_FOREACH_VAL(items, item) {
        if (position >= offset && (length_null || position < offset + length)) {
            Z_TRY_ADDREF_P(item);
            add_next_index_zval(return_value, item);
        }
        position++;
    } ZEND_HASH_FOREACH_END();
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_twin, 0, 2, IS_LONG, 0)
    ZEND_ARG_INFO(1, first)
    ZEND_ARG_INFO(1, second)
ZEND_END_ARG_INFO()

/* demo_twin(&$first, &$second): int — writes 10 through the first reference, then reads the
 * second and writes one more than it read back through it. Given two different variables the
 * second keeps its own value plus one; given the same variable twice (PHP shares one
 * reference) the read sees 10 and the variable ends at 11. */
PHP_FUNCTION(demo_twin)
{
    zval *first, *second;
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_ZVAL(first)
        Z_PARAM_ZVAL(second)
    ZEND_PARSE_PARAMETERS_END();
    ZEND_TRY_ASSIGN_REF_LONG(first, 10);
    zend_long seen = zval_get_long(Z_ISREF_P(second) ? Z_REFVAL_P(second) : second);
    ZEND_TRY_ASSIGN_REF_LONG(second, seen + 1);
    RETURN_LONG(seen);
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_argc, 0, 0, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, first, IS_MIXED, 0, "null")
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, second, IS_MIXED, 0, "null")
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, third, IS_MIXED, 0, "null")
ZEND_END_ARG_INFO()

/* demo_argc(mixed $first = null, mixed $second = null, mixed $third = null): int — how many
 * arguments the caller wrote, which the literal defaults must not inflate. */
PHP_FUNCTION(demo_argc)
{
    zval *first = NULL, *second = NULL, *third = NULL;
    ZEND_PARSE_PARAMETERS_START(0, 3)
        Z_PARAM_OPTIONAL
        Z_PARAM_ZVAL(first)
        Z_PARAM_ZVAL(second)
        Z_PARAM_ZVAL(third)
    ZEND_PARSE_PARAMETERS_END();
    RETURN_LONG(ZEND_NUM_ARGS());
}

/* DemoDetailException: an exception class with a method of its own, which reads the
 * message and code the exception carries. */
zend_class_entry *demo_detail_exception_ce;

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_detail_describe, 0, 0, IS_STRING, 0)
ZEND_END_ARG_INFO()

PHP_METHOD(DemoDetailException, describe)
{
    zval rv, *message, *code;
    ZEND_PARSE_PARAMETERS_NONE();
    message = zend_read_property(zend_ce_exception, Z_OBJ_P(ZEND_THIS), "message", sizeof("message") - 1, 1, &rv);
    zval code_rv;
    code = zend_read_property(zend_ce_exception, Z_OBJ_P(ZEND_THIS), "code", sizeof("code") - 1, 1, &code_rv);
    RETURN_STR(zend_strpprintf(0, "%s: %s (%ld)", ZSTR_VAL(Z_OBJCE_P(ZEND_THIS)->name),
                               Z_TYPE_P(message) == IS_STRING ? Z_STRVAL_P(message) : "",
                               (long) zval_get_long(code)));
}

static const zend_function_entry demo_detail_exception_methods[] = {
    PHP_ME(DemoDetailException, describe, arginfo_demo_detail_describe, ZEND_ACC_PUBLIC)
    PHP_FE_END
};

/* DemoDetailChildException extends DemoDetailException with a method of its own, so the engine
 * object behind a subclass instance must be of the subclass. */
zend_class_entry *demo_detail_child_exception_ce;

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_detail_tag, 0, 0, IS_STRING, 0)
ZEND_END_ARG_INFO()

PHP_METHOD(DemoDetailChildException, tag)
{
    zval rv, *message;
    ZEND_PARSE_PARAMETERS_NONE();
    message = zend_read_property(zend_ce_exception, Z_OBJ_P(ZEND_THIS), "message", sizeof("message") - 1, 1, &rv);
    RETURN_STR(zend_strpprintf(0, "child:%s", Z_TYPE_P(message) == IS_STRING ? Z_STRVAL_P(message) : ""));
}

static const zend_function_entry demo_detail_child_exception_methods[] = {
    PHP_ME(DemoDetailChildException, tag, arginfo_demo_detail_tag, ZEND_ACC_PUBLIC)
    PHP_FE_END
};

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_throw_detail, 0, 1, IS_NEVER, 0)
    ZEND_ARG_TYPE_INFO(0, message, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, code, IS_LONG, 0, "0")
ZEND_END_ARG_INFO()

/* demo_throw_detail(string $message, int $code = 0): never — throws DemoDetailException. */
PHP_FUNCTION(demo_throw_detail)
{
    zend_string *message;
    zend_long code = 0;
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_STR(message)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(code)
    ZEND_PARSE_PARAMETERS_END();
    zend_throw_exception(demo_detail_exception_ce, ZSTR_VAL(message), code);
}

/* DemoBag: a Countable written the way extensions were before PHP 8.1 — count() has no
 * return type in its arginfo. */
zend_class_entry *demo_bag_ce;

ZEND_BEGIN_ARG_INFO_EX(arginfo_demo_bag_count, 0, 0, 0)
ZEND_END_ARG_INFO()

PHP_METHOD(DemoBag, count)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_LONG(3);
}

static const zend_function_entry demo_bag_methods[] = {
    PHP_ME(DemoBag, count, arginfo_demo_bag_count, ZEND_ACC_PUBLIC)
    PHP_FE_END
};

static const zend_function_entry demo_functions[] = {
    PHP_FE(demo_add, arginfo_demo_add)
    PHP_FE(demo_scale, arginfo_demo_scale)
    PHP_FE(demo_is_even, arginfo_demo_is_even)
    PHP_FE(demo_greet, arginfo_demo_greet)
    PHP_FE(demo_split, arginfo_demo_split)
    PHP_FE(demo_stats, arginfo_demo_stats)
    PHP_FE(demo_inc, arginfo_demo_inc)
    PHP_FE(demo_parse, arginfo_demo_parse)
    PHP_FE(demo_fail, arginfo_demo_fail)
    PHP_FE(demo_warn, arginfo_demo_warn)
    PHP_FE(demo_fatal, arginfo_demo_fatal)
    PHP_FE(demo_calls, arginfo_demo_calls)
    PHP_FE(demo_consume, arginfo_demo_consume)
    PHP_FE(demo_count, arginfo_demo_count)
    PHP_FE(demo_throw_code, arginfo_demo_throw_code)
    PHP_FE(demo_error_object, arginfo_demo_error_object)
    PHP_FE(demo_cycle, arginfo_demo_cycle)
    PHP_FE(demo_echo, arginfo_demo_echo)
    PHP_FE(demo_nested, arginfo_demo_nested)
    PHP_FE(demo_object, arginfo_demo_object)
    PHP_FE(demo_keep, arginfo_demo_keep)
    PHP_FE(demo_kept, arginfo_demo_kept)
    PHP_FE(demo_apply, arginfo_demo_apply)
    PHP_FE(demo_try, arginfo_demo_try)
    PHP_FE(demo_call_all, arginfo_demo_call_all)
    PHP_FE(demo_inspect, arginfo_demo_inspect)
    PHP_FE(demo_slice, arginfo_demo_slice)
    PHP_FE(demo_counters_freed, arginfo_demo_counters_freed)
    PHP_FE(demo_iterators_freed, arginfo_demo_iterators_freed)
    PHP_FE(demo_twin, arginfo_demo_twin)
    PHP_FE(demo_argc, arginfo_demo_argc)
    PHP_FE(demo_throw_detail, arginfo_demo_throw_detail)
    PHP_FE_END
};

static PHP_GINIT_FUNCTION(elephc_demo)
{
    elephc_demo_globals->greeting = NULL;
    elephc_demo_globals->calls = 0;
    elephc_demo_globals->counters_freed = 0;
    elephc_demo_globals->iterators_freed = 0;
}

PHP_MINIT_FUNCTION(elephc_demo)
{
    REGISTER_INI_ENTRIES();
    REGISTER_LONG_CONSTANT("DEMO_ANSWER", 42, CONST_CS | CONST_PERSISTENT);
    REGISTER_STRING_CONSTANT("DEMO_NAME", "demo", CONST_CS | CONST_PERSISTENT);
    REGISTER_DOUBLE_CONSTANT("DEMO_RATIO", 0.25, CONST_CS | CONST_PERSISTENT);

    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "DemoException", NULL);
    demo_exception_ce = zend_register_internal_class_ex(&ce, spl_ce_RuntimeException);
    INIT_CLASS_ENTRY(ce, "DemoParseException", NULL);
    demo_parse_exception_ce = zend_register_internal_class_ex(&ce, demo_exception_ce);
    INIT_CLASS_ENTRY(ce, "DemoDetailException", demo_detail_exception_methods);
    demo_detail_exception_ce = zend_register_internal_class_ex(&ce, demo_exception_ce);
    INIT_CLASS_ENTRY(ce, "DemoDetailChildException", demo_detail_child_exception_methods);
    demo_detail_child_exception_ce = zend_register_internal_class_ex(&ce, demo_detail_exception_ce);
    INIT_CLASS_ENTRY(ce, "DemoBag", demo_bag_methods);
    demo_bag_ce = zend_register_internal_class(&ce);
    zend_class_implements(demo_bag_ce, 1, zend_ce_countable);
    demo_register_counter();
    zend_register_functions(NULL, demo_late_functions, NULL, MODULE_PERSISTENT);
    return SUCCESS;
}

static void demo_trace(const char *phase);

PHP_RINIT_FUNCTION(elephc_demo)
{
    /* ELEPHC_DEMO_FAIL_RINIT_AFTER=N makes every request startup after the first N fail, so a
     * test can check that the host treats a failed RINIT as php-src does. */
    static int started = 0;
    const char *fail_after = getenv("ELEPHC_DEMO_FAIL_RINIT_AFTER");
    if (fail_after != NULL && started >= atoi(fail_after)) {
        demo_trace("RINIT failed");
        return FAILURE;
    }
    started++;
    demo_trace("RINIT");
    zend_hash_init(&DEMO_G(kept), 8, NULL, ZVAL_PTR_DTOR, 0);
    return SUCCESS;
}

/* Reports a lifecycle phase on stderr when ELEPHC_DEMO_TRACE is set, so a test can
 * observe that the host runs RINIT/RSHUTDOWN/MSHUTDOWN in PHP's order. */
static void demo_trace(const char *phase)
{
    if (getenv("ELEPHC_DEMO_TRACE") != NULL) {
        fprintf(stderr, "elephc_demo: %s\n", phase);
    }
}

/* Destroys what demo_keep() kept: RINIT built the table for this request. */
PHP_RSHUTDOWN_FUNCTION(elephc_demo)
{
    demo_trace("RSHUTDOWN");
    zend_hash_destroy(&DEMO_G(kept));
    return SUCCESS;
}

PHP_MSHUTDOWN_FUNCTION(elephc_demo)
{
    demo_trace("MSHUTDOWN");
    UNREGISTER_INI_ENTRIES();
    return SUCCESS;
}

PHP_MINFO_FUNCTION(elephc_demo)
{
    php_info_print_table_start();
    php_info_print_table_row(2, "elephc_demo support", "enabled");
    php_info_print_table_end();
    DISPLAY_INI_ENTRIES();
}

zend_module_entry elephc_demo_module_entry = {
    STANDARD_MODULE_HEADER,
    "elephc_demo",
    demo_functions,
    PHP_MINIT(elephc_demo),
    PHP_MSHUTDOWN(elephc_demo),
    PHP_RINIT(elephc_demo),
    PHP_RSHUTDOWN(elephc_demo),
    PHP_MINFO(elephc_demo),
    PHP_ELEPHC_DEMO_VERSION,
    PHP_MODULE_GLOBALS(elephc_demo),
    PHP_GINIT(elephc_demo),
    NULL,
    NULL,
    STANDARD_MODULE_PROPERTIES_EX
};

#ifdef COMPILE_DL_ELEPHC_DEMO
ZEND_GET_MODULE(elephc_demo)
#endif
