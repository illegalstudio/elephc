/* wordstat.c — a small, ordinary PHP extension: word statistics over a text.
 *
 * Nothing here knows about Elephc. The same file builds with phpize for PHP
 * (`phpize && ./configure && make`), and `elephc extension add wordstat --path
 * ext/wordstat` hosts it in a compiled program.
 */

#ifdef HAVE_CONFIG_H
#include "config.h"
#endif

#include "php.h"
#include "ext/spl/spl_exceptions.h"
#include "zend_exceptions.h"

#define WORDSTAT_VERSION "1.0.0"

static zend_class_entry *wordstat_exception_ce;

/* Splits `text` into lowercase words of letters and digits. */
static void wordstat_words(zend_string *text, zval *words)
{
    array_init(words);
    const char *p = ZSTR_VAL(text), *end = p + ZSTR_LEN(text);
    while (p < end) {
        while (p < end && !isalnum((unsigned char)*p)) p++;
        const char *start = p;
        while (p < end && isalnum((unsigned char)*p)) p++;
        if (p > start) {
            zend_string *word = zend_string_tolower(zend_string_init(start, p - start, 0));
            add_next_index_str(words, word);
        }
    }
}

/* Counts each word of `text` into `counts`, in order of first appearance. */
static void wordstat_tally(zend_string *text, zval *counts)
{
    zval words, *word;
    wordstat_words(text, &words);
    array_init(counts);
    ZEND_HASH_FOREACH_VAL(Z_ARRVAL(words), word) {
        zval *count = zend_hash_find(Z_ARRVAL_P(counts), Z_STR_P(word));
        if (count) {
            Z_LVAL_P(count)++;
        } else {
            add_assoc_long_ex(counts, Z_STRVAL_P(word), Z_STRLEN_P(word), 1);
        }
    } ZEND_HASH_FOREACH_END();
    zval_ptr_dtor(&words);
}

/* wordstat_count(string $text): array<string, int> */
PHP_FUNCTION(wordstat_count)
{
    zend_string *text;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STR(text)
    ZEND_PARSE_PARAMETERS_END();

    wordstat_tally(text, return_value);
    if (zend_hash_num_elements(Z_ARRVAL_P(return_value)) == 0) {
        zend_throw_exception(wordstat_exception_ce, "the text has no words", 1);
        RETURN_THROWS();
    }
}

/* wordstat_top(string $text, int $limit = 3): list<string> — the most frequent
 * words, most frequent first, ties in order of first appearance. */
PHP_FUNCTION(wordstat_top)
{
    zend_string *text;
    zend_long limit = 3;
    ZEND_PARSE_PARAMETERS_START(1, 2)
        Z_PARAM_STR(text)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(limit)
    ZEND_PARSE_PARAMETERS_END();
    if (limit < 1) {
        zend_argument_value_error(2, "must be greater than 0");
        RETURN_THROWS();
    }

    zval counts;
    wordstat_tally(text, &counts);
    array_init(return_value);
    for (zend_long picked = 0; picked < limit; picked++) {
        zend_string *best = NULL, *key;
        zend_long best_count = 0;
        zval *count;
        ZEND_HASH_FOREACH_STR_KEY_VAL(Z_ARRVAL(counts), key, count) {
            if (Z_LVAL_P(count) > best_count) {
                best = key;
                best_count = Z_LVAL_P(count);
            }
        } ZEND_HASH_FOREACH_END();
        if (!best) {
            break;
        }
        add_next_index_str(return_value, zend_string_copy(best));
        zend_hash_del(Z_ARRVAL(counts), best);
    }
    zval_ptr_dtor(&counts);
}

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_wordstat_count, 0, 1, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, text, IS_STRING, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_wordstat_top, 0, 1, IS_ARRAY, 0)
    ZEND_ARG_TYPE_INFO(0, text, IS_STRING, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, limit, IS_LONG, 0, "3")
ZEND_END_ARG_INFO()

static const zend_function_entry wordstat_functions[] = {
    PHP_FE(wordstat_count, arginfo_wordstat_count)
    PHP_FE(wordstat_top, arginfo_wordstat_top)
    PHP_FE_END
};

PHP_MINIT_FUNCTION(wordstat)
{
    REGISTER_STRING_CONSTANT("WORDSTAT_VERSION", WORDSTAT_VERSION, CONST_CS | CONST_PERSISTENT);
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "WordstatException", NULL);
    wordstat_exception_ce = zend_register_internal_class_ex(&ce, spl_ce_InvalidArgumentException);
    return SUCCESS;
}

zend_module_entry wordstat_module_entry = {
    STANDARD_MODULE_HEADER,
    "wordstat",
    wordstat_functions,
    PHP_MINIT(wordstat),
    NULL,
    NULL,
    NULL,
    NULL,
    WORDSTAT_VERSION,
    STANDARD_MODULE_PROPERTIES
};

#ifdef COMPILE_DL_WORDSTAT
ZEND_GET_MODULE(wordstat)
#endif
