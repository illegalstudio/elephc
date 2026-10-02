/* demo.c — the elephc_demo fixture extension: module lifecycle, INI,
 * constants, exception classes, and scalar/by-reference functions.
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

/* demo_warn(string $what): bool — a warning, then a normal return. */
PHP_FUNCTION(demo_warn)
{
    zend_string *what;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STR(what)
    ZEND_PARSE_PARAMETERS_END();
    php_error_docref(NULL, E_WARNING, "careful with %s", ZSTR_VAL(what));
    RETURN_TRUE;
}

/* demo_fatal(): void — an E_ERROR, which ends the program as in PHP. */
PHP_FUNCTION(demo_fatal)
{
    ZEND_PARSE_PARAMETERS_NONE();
    php_error_docref(NULL, E_ERROR, "the fixture gave up");
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
PHP_FUNCTION(demo_calls)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_LONG(DEMO_G(calls));
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_add, 0, 1, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO(0, a, IS_LONG, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, b, IS_LONG, 0, "1")
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

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_apply, 0, 2, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, callback, IS_CALLABLE, 0)
    ZEND_ARG_TYPE_INFO(0, items, IS_ARRAY, 0)
ZEND_END_ARG_INFO()

/* demo_apply(callable $callback, array $items): array — takes a callable,
 * which Elephc cannot pass yet: declared, but only to explain itself. */
PHP_FUNCTION(demo_apply)
{
    zend_fcall_info fci;
    zend_fcall_info_cache fcc;
    HashTable *items;
    ZEND_PARSE_PARAMETERS_START(2, 2)
        Z_PARAM_FUNC(fci, fcc)
        Z_PARAM_ARRAY_HT(items)
    ZEND_PARSE_PARAMETERS_END();
    RETURN_EMPTY_ARRAY();
}

static const zend_function_entry demo_functions[] = {
    PHP_FE(demo_add, arginfo_demo_add)
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
    PHP_FE(demo_error_object, arginfo_demo_error_object)
    PHP_FE(demo_cycle, arginfo_demo_cycle)
    PHP_FE(demo_echo, arginfo_demo_echo)
    PHP_FE(demo_nested, arginfo_demo_nested)
    PHP_FE(demo_object, arginfo_demo_object)
    PHP_FE(demo_keep, arginfo_demo_keep)
    PHP_FE(demo_kept, arginfo_demo_kept)
    PHP_FE(demo_apply, arginfo_demo_apply)
    PHP_FE_END
};

static PHP_GINIT_FUNCTION(elephc_demo)
{
    elephc_demo_globals->greeting = NULL;
    elephc_demo_globals->calls = 0;
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
    return SUCCESS;
}

PHP_RINIT_FUNCTION(elephc_demo)
{
    zend_hash_init(&DEMO_G(kept), 8, NULL, ZVAL_PTR_DTOR, 0);
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
    NULL,
    PHP_RINIT(elephc_demo),
    NULL,
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
