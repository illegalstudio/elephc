/* demo_counter.c — DemoCounter, the fixture's class: an internal object with
 * its own C state (custom create_object and free_obj), instance and static
 * methods, a fluent method, an object argument, a declared property, objects
 * inside a returned array, a destructor count, a clone handler copying the C
 * state, a debug-info handler, and an uncloneable class.
 *
 * Ordinary PHP 8 extension code; tests/php_ext_tests.rs compares a program
 * using it with the output real PHP gives.
 */

#ifdef HAVE_CONFIG_H
#include "config.h"
#endif

#include "php.h"
#include "zend_exceptions.h"
#include "zend_interfaces.h"
#include "ext/spl/spl_exceptions.h"
#include "php_elephc_demo.h"

zend_class_entry *demo_counter_ce;
static zend_object_handlers demo_counter_handlers;

typedef struct {
    zend_long value;
    zend_object std;
} demo_counter;

static inline demo_counter *counter_of(zend_object *object)
{
    return (demo_counter *) ((char *) object - XtOffsetOf(demo_counter, std));
}

static zend_object *demo_counter_create(zend_class_entry *ce)
{
    demo_counter *counter = zend_object_alloc(sizeof(demo_counter), ce);
    counter->value = 0;
    zend_object_std_init(&counter->std, ce);
    object_properties_init(&counter->std, ce);
    counter->std.handlers = &demo_counter_handlers;
    return &counter->std;
}

/* clone: a new counter carrying the C state, then the properties, as an
 * extension with its own struct must do. */
static zend_object *demo_counter_clone(zend_object *old)
{
    zend_object *copy = demo_counter_create(old->ce);
    counter_of(copy)->value = counter_of(old)->value;
    zend_objects_clone_members(copy, old);
    return copy;
}

/* var_dump(): the declared properties plus the C state, in a temporary table. */
static HashTable *demo_counter_debug_info(zend_object *object, int *is_temp)
{
    HashTable *table = zend_array_dup(zend_std_get_properties(object));
    zval value;
    ZVAL_LONG(&value, counter_of(object)->value);
    zend_hash_str_update(table, "value", sizeof("value") - 1, &value);
    *is_temp = 1;
    return table;
}

static void demo_counter_free(zend_object *object)
{
    DEMO_G(counters_freed)++;
    zend_object_std_dtor(object);
}

/* DemoCounter::__construct(int $start = 0) */
PHP_METHOD(DemoCounter, __construct)
{
    zend_long start = 0;
    ZEND_PARSE_PARAMETERS_START(0, 1)
        Z_PARAM_OPTIONAL
        Z_PARAM_LONG(start)
    ZEND_PARSE_PARAMETERS_END();
    counter_of(Z_OBJ_P(ZEND_THIS))->value = start;
}

/* DemoCounter::add(int $by): static — fluent: returns the same object. */
PHP_METHOD(DemoCounter, add)
{
    zend_long by;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(by)
    ZEND_PARSE_PARAMETERS_END();
    if (by < 0) {
        zend_argument_value_error(1, "must be greater than or equal to 0");
        RETURN_THROWS();
    }
    counter_of(Z_OBJ_P(ZEND_THIS))->value += by;
    RETURN_OBJ_COPY(Z_OBJ_P(ZEND_THIS));
}

/* DemoCounter::value(): int */
PHP_METHOD(DemoCounter, value)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_LONG(counter_of(Z_OBJ_P(ZEND_THIS))->value);
}

/* DemoCounter::merge(DemoCounter $other): static — an object argument. */
PHP_METHOD(DemoCounter, merge)
{
    zval *other;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_OBJECT_OF_CLASS(other, demo_counter_ce)
    ZEND_PARSE_PARAMETERS_END();
    counter_of(Z_OBJ_P(ZEND_THIS))->value += counter_of(Z_OBJ_P(other))->value;
    RETURN_OBJ_COPY(Z_OBJ_P(ZEND_THIS));
}

/* DemoCounter::fromString(string $digits): DemoCounter — a static factory. */
PHP_METHOD(DemoCounter, fromString)
{
    zend_string *digits;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STR(digits)
    ZEND_PARSE_PARAMETERS_END();
    object_init_ex(return_value, demo_counter_ce);
    counter_of(Z_OBJ_P(return_value))->value = ZEND_STRTOL(ZSTR_VAL(digits), NULL, 10);
}

/* DemoCounter::split(): array — two new counters, each half the value. */
PHP_METHOD(DemoCounter, split)
{
    zend_long value = counter_of(Z_OBJ_P(ZEND_THIS))->value;
    zval half;
    ZEND_PARSE_PARAMETERS_NONE();
    array_init(return_value);
    for (int i = 0; i < 2; i++) {
        object_init_ex(&half, demo_counter_ce);
        counter_of(Z_OBJ(half))->value = i == 0 ? value / 2 : value - value / 2;
        add_next_index_zval(return_value, &half);
    }
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_demo_counter_construct, 0, 0, 0)
    ZEND_ARG_TYPE_INFO_WITH_DEFAULT_VALUE(0, start, IS_LONG, 0, "0")
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_counter_add, 0, 1, IS_STATIC, 0)
    ZEND_ARG_TYPE_INFO(0, by, IS_LONG, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_counter_value, 0, 0, IS_LONG, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_counter_merge, 0, 1, IS_STATIC, 0)
    ZEND_ARG_OBJ_INFO(0, other, DemoCounter, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_OBJ_INFO_EX(arginfo_demo_counter_from_string, 0, 1, DemoCounter, 0)
    ZEND_ARG_TYPE_INFO(0, digits, IS_STRING, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_counter_split, 0, 0, IS_ARRAY, 0)
ZEND_END_ARG_INFO()

static const zend_function_entry demo_counter_methods[] = {
    PHP_ME(DemoCounter, __construct, arginfo_demo_counter_construct, ZEND_ACC_PUBLIC)
    PHP_ME(DemoCounter, add, arginfo_demo_counter_add, ZEND_ACC_PUBLIC)
    PHP_ME(DemoCounter, value, arginfo_demo_counter_value, ZEND_ACC_PUBLIC)
    PHP_ME(DemoCounter, merge, arginfo_demo_counter_merge, ZEND_ACC_PUBLIC)
    PHP_ME(DemoCounter, fromString, arginfo_demo_counter_from_string, ZEND_ACC_PUBLIC | ZEND_ACC_STATIC)
    PHP_ME(DemoCounter, split, arginfo_demo_counter_split, ZEND_ACC_PUBLIC)
    PHP_FE_END
};

/* DemoToken: a private constructor, so only its factory makes one; the
 * factory's object carries state the program must keep. */
zend_class_entry *demo_token_ce;

/* DemoToken::__construct() — private. */
PHP_METHOD(DemoToken, __construct)
{
    ZEND_PARSE_PARAMETERS_NONE();
}

/* DemoToken::issue(string $id): DemoToken */
PHP_METHOD(DemoToken, issue)
{
    zend_string *id;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_STR(id)
    ZEND_PARSE_PARAMETERS_END();
    object_init_ex(return_value, demo_token_ce);
    zend_update_property_str(demo_token_ce, Z_OBJ_P(return_value), "id", sizeof("id") - 1, id);
}

/* DemoToken::id(): string */
PHP_METHOD(DemoToken, id)
{
    zval rv;
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_COPY(zend_read_property(demo_token_ce, Z_OBJ_P(ZEND_THIS), "id", sizeof("id") - 1, 0, &rv));
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_demo_token_construct, 0, 0, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_OBJ_INFO_EX(arginfo_demo_token_issue, 0, 1, DemoToken, 0)
    ZEND_ARG_TYPE_INFO(0, id, IS_STRING, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_TYPE_INFO_EX(arginfo_demo_token_id, 0, 0, IS_STRING, 0)
ZEND_END_ARG_INFO()

static const zend_function_entry demo_token_methods[] = {
    PHP_ME(DemoToken, __construct, arginfo_demo_token_construct, ZEND_ACC_PRIVATE)
    PHP_ME(DemoToken, issue, arginfo_demo_token_issue, ZEND_ACC_PUBLIC | ZEND_ACC_STATIC)
    PHP_ME(DemoToken, id, arginfo_demo_token_id, ZEND_ACC_PUBLIC)
    PHP_FE_END
};

/* DemoRange: an IteratorAggregate whose iteration runs through its class's
 * get_iterator handler, yielding "k0" => 0, "k1" => 10, ... for
 * `new DemoRange(n)`, as ds's collections do. */
zend_class_entry *demo_range_ce;
static zend_object_handlers demo_range_handlers;

typedef struct {
    zend_long size;
    zend_object std;
} demo_range;

typedef struct {
    zend_object_iterator intern;
    zend_long position;
    zval current;
} demo_range_iterator;

static inline demo_range *range_of(zend_object *object)
{
    return (demo_range *) ((char *) object - XtOffsetOf(demo_range, std));
}

static zend_object *demo_range_create(zend_class_entry *ce)
{
    demo_range *range = zend_object_alloc(sizeof(demo_range), ce);
    range->size = 0;
    zend_object_std_init(&range->std, ce);
    object_properties_init(&range->std, ce);
    range->std.handlers = &demo_range_handlers;
    return &range->std;
}

static void demo_range_it_dtor(zend_object_iterator *iter)
{
    demo_range_iterator *it = (demo_range_iterator *) iter;
    zval_ptr_dtor(&it->current);
    zval_ptr_dtor(&iter->data);
    DEMO_G(iterators_freed)++;
}

static zend_result demo_range_it_valid(zend_object_iterator *iter)
{
    demo_range_iterator *it = (demo_range_iterator *) iter;
    return it->position < range_of(Z_OBJ(iter->data))->size ? SUCCESS : FAILURE;
}

static zval *demo_range_it_current(zend_object_iterator *iter)
{
    demo_range_iterator *it = (demo_range_iterator *) iter;
    zval_ptr_dtor(&it->current);
    ZVAL_LONG(&it->current, it->position * 10);
    return &it->current;
}

static void demo_range_it_key(zend_object_iterator *iter, zval *key)
{
    demo_range_iterator *it = (demo_range_iterator *) iter;
    ZVAL_STR(key, zend_strpprintf(0, "k" ZEND_LONG_FMT, it->position));
}

static void demo_range_it_next(zend_object_iterator *iter)
{
    ((demo_range_iterator *) iter)->position++;
}

static void demo_range_it_rewind(zend_object_iterator *iter)
{
    ((demo_range_iterator *) iter)->position = 0;
}

static const zend_object_iterator_funcs demo_range_it_funcs = {
    demo_range_it_dtor,
    demo_range_it_valid,
    demo_range_it_current,
    demo_range_it_key,
    demo_range_it_next,
    demo_range_it_rewind,
    NULL,
    NULL,
};

static zend_object_iterator *demo_range_get_iterator(zend_class_entry *ce, zval *object, int by_ref)
{
    demo_range_iterator *it;
    if (by_ref) {
        zend_throw_error(NULL, "An iterator cannot be used with foreach by reference");
        return NULL;
    }
    if (range_of(Z_OBJ_P(object))->size < 0) {
        zend_throw_exception(spl_ce_RuntimeException, "a negative range cannot be iterated", 7);
        return NULL;
    }
    it = ecalloc(1, sizeof(demo_range_iterator));
    zend_iterator_init(&it->intern);
    ZVAL_OBJ_COPY(&it->intern.data, Z_OBJ_P(object));
    it->intern.funcs = &demo_range_it_funcs;
    ZVAL_UNDEF(&it->current);
    return &it->intern;
}

/* DemoRange::__construct(int $size) */
PHP_METHOD(DemoRange, __construct)
{
    zend_long size;
    ZEND_PARSE_PARAMETERS_START(1, 1)
        Z_PARAM_LONG(size)
    ZEND_PARSE_PARAMETERS_END();
    range_of(Z_OBJ_P(ZEND_THIS))->size = size;
}

/* DemoRange::getIterator(): Iterator — PHP's internal iterator over the
 * get_iterator handler. */
PHP_METHOD(DemoRange, getIterator)
{
    ZEND_PARSE_PARAMETERS_NONE();
    zend_create_internal_iterator_zval(return_value, ZEND_THIS);
}

ZEND_BEGIN_ARG_INFO_EX(arginfo_demo_range_construct, 0, 0, 1)
    ZEND_ARG_TYPE_INFO(0, size, IS_LONG, 0)
ZEND_END_ARG_INFO()

ZEND_BEGIN_ARG_WITH_RETURN_OBJ_INFO_EX(arginfo_demo_range_get_iterator, 0, 0, Iterator, 0)
ZEND_END_ARG_INFO()

static const zend_function_entry demo_range_methods[] = {
    PHP_ME(DemoRange, __construct, arginfo_demo_range_construct, ZEND_ACC_PUBLIC)
    PHP_ME(DemoRange, getIterator, arginfo_demo_range_get_iterator, ZEND_ACC_PUBLIC)
    PHP_FE_END
};

/* demo_iterators_freed(): int — how many DemoRange iterators were destroyed. */
PHP_FUNCTION(demo_iterators_freed)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_LONG(DEMO_G(iterators_freed));
}

/* demo_counters_freed(): int — how many counters the engine has freed. */
PHP_FUNCTION(demo_counters_freed)
{
    ZEND_PARSE_PARAMETERS_NONE();
    RETURN_LONG(DEMO_G(counters_freed));
}

void demo_register_counter(void)
{
    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "DemoCounter", demo_counter_methods);
    demo_counter_ce = zend_register_internal_class(&ce);
    demo_counter_ce->create_object = demo_counter_create;
    zend_declare_property_string(demo_counter_ce, "label", sizeof("label") - 1, "counter", ZEND_ACC_PUBLIC);
    zend_declare_class_constant_long(demo_counter_ce, "STEP", sizeof("STEP") - 1, 5);

    INIT_CLASS_ENTRY(ce, "DemoRange", demo_range_methods);
    demo_range_ce = zend_register_internal_class(&ce);
    demo_range_ce->create_object = demo_range_create;
    demo_range_ce->get_iterator = demo_range_get_iterator;
    zend_class_implements(demo_range_ce, 1, zend_ce_aggregate);
    memcpy(&demo_range_handlers, &std_object_handlers, sizeof(zend_object_handlers));
    demo_range_handlers.offset = XtOffsetOf(demo_range, std);
    demo_range_handlers.clone_obj = NULL;

    INIT_CLASS_ENTRY(ce, "DemoToken", demo_token_methods);
    demo_token_ce = zend_register_internal_class(&ce);
    demo_token_ce->ce_flags |= ZEND_ACC_FINAL;
    zend_declare_property_string(demo_token_ce, "id", sizeof("id") - 1, "", ZEND_ACC_PUBLIC);

    memcpy(&demo_counter_handlers, &std_object_handlers, sizeof(zend_object_handlers));
    demo_counter_handlers.offset = XtOffsetOf(demo_counter, std);
    demo_counter_handlers.free_obj = demo_counter_free;
    demo_counter_handlers.clone_obj = demo_counter_clone;
    demo_counter_handlers.get_debug_info = demo_counter_debug_info;
}
