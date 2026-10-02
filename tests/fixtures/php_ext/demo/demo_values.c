/* demo_values.c — the elephc_demo functions that move composite values across
 * the boundary: round trips, nested arrays, stdClass objects, and values the
 * extension keeps after the call returns. */

#ifdef HAVE_CONFIG_H
#include "config.h"
#endif

#include "php.h"
#include "php_elephc_demo.h"

/* demo_echo($value): returns its argument unchanged. */
PHP_FUNCTION(demo_echo)
{
    zval *value;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ZVAL(value)
    ZEND_PARSE_PARAMETERS_END();
    RETURN_COPY(value);
}

/* demo_nested(): array — a hash holding a list, a stdClass and scalars. */
PHP_FUNCTION(demo_nested)
{
    ZEND_PARSE_PARAMETERS_NONE();
    zval list, object, inner;
    array_init(return_value);

    array_init(&list);
    add_next_index_long(&list, 1);
    add_next_index_double(&list, 2.5);
    add_next_index_bool(&list, 1);
    add_next_index_null(&list);
    add_assoc_zval(return_value, "list", &list);

    object_init(&object);
    add_property_string(&object, "label", "inside");
    array_init(&inner);
    add_next_index_string(&inner, "deep");
    add_property_zval(&object, "items", &inner);
    zval_ptr_dtor(&inner);
    add_assoc_zval(return_value, "object", &object);

    add_index_string(return_value, 7, "seven");
    add_assoc_string(return_value, "utf8", "caf\xc3\xa9");
}

/* demo_object(string $name): stdClass */
PHP_FUNCTION(demo_object)
{
    zend_string *name;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STR(name)
    ZEND_PARSE_PARAMETERS_END();
    object_init(return_value);
    add_property_str(return_value, "name", zend_string_copy(name));
    add_property_long(return_value, "length", (zend_long)ZSTR_LEN(name));
}

/* demo_keep(mixed $value): int — stores the argument itself (a reference, not
 * a copy) and returns how many are kept. The caller's copy must not be freed
 * underneath it. */
PHP_FUNCTION(demo_keep)
{
    zval *value;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_ZVAL(value)
    ZEND_PARSE_PARAMETERS_END();
    Z_TRY_ADDREF_P(value);
    zend_hash_next_index_insert(&DEMO_G(kept), value);
    RETURN_LONG(zend_hash_num_elements(&DEMO_G(kept)));
}

/* demo_kept(): array — everything kept so far. */
PHP_FUNCTION(demo_kept)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_ARR(zend_array_dup(&DEMO_G(kept)));
}
