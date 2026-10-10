/* host.c — how compiled Elephc code starts the engine and calls a hosted
 * extension function.
 *
 * The contract with Elephc is deliberately narrow: every entry point takes and
 * returns only integers, C strings and opaque pointers, so the generated PHP
 * wrappers reach it through the ordinary `extern` FFI on every target without
 * any new assembly.
 *
 * Values cross the boundary as zvals, and they are COPIED at the boundary in
 * both directions:
 *   - an argument arrives in the zval bridge's layout (`zval_pack`) and is
 *     rebuilt in engine memory, so the extension may keep a reference to it (a
 *     container storing a value) without Elephc freeing it underneath;
 *   - a result stays in engine memory, and the wrapper walks it through the
 *     value accessors below. Only object-free subtrees are exported, into a
 *     malloc'd tree in exactly the shape `zval_unpack` reads (32-byte buckets;
 *     packed tables marked with nTableMask == -2), so Elephc never interprets
 *     engine internals such as PHP 8.2 packed arrays or interned strings.
 *     Objects are rebuilt by the wrapper itself, one property at a time.
 *
 * A call runs inside a protected frame: a fatal error raised by the extension
 * longjmps back here instead of into an unset buffer, and the call reports it.
 */

#include "php.h"
#include "zend_API.h"
#include "zend_closures.h"
#include "zend_constants.h"
#include "zend_exceptions.h"
#include "zend_ini.h"
#include "zend_interfaces.h"
#include "zend_list.h"
#include "zend_smart_str.h"
#include "ext/spl/spl_exceptions.h"
#include "spprintf.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The SPL exception classes are registered from ext/spl's own generated
 * arginfo, so their names, parents and flags match PHP exactly. */
#include "ext/spl/spl_exceptions_arginfo.h"

/* Status codes returned to the generated wrapper. */
#define ELEPHC_CALL_OK 0
#define ELEPHC_CALL_EXCEPTION 1
#define ELEPHC_CALL_FATAL 2
#define ELEPHC_CALL_UNSUPPORTED 3

/* Kinds reported by elephc_php_ext_value_kind. */
#define ELEPHC_VALUE_PLAIN 0       /* no object anywhere below: export it whole */
#define ELEPHC_VALUE_STDCLASS 1    /* rebuild as stdClass, property by property */
#define ELEPHC_VALUE_ARRAY 2       /* an array holding objects: rebuild entry by entry */

/* Where elephc_walk_entry last stopped in each table being walked: see there.
 * One per table, because rebuilding a nested value walks a child between two
 * entries of its parent. The least recently used one is replaced, so the
 * parent, touched between every child, keeps its place. */
#define ELEPHC_WALK_CURSORS 32
static struct elephc_walk_cursor {
    HashTable *table;
    uint32_t index;
    uint32_t slot;
    uint64_t used;
} elephc_walk_cursors[ELEPHC_WALK_CURSORS];
static uint64_t elephc_walk_clock;

extern void elephc_stdlib_startup(void);
extern size_t elephc_zend_write_stdout(const char *str, size_t len);
extern size_t elephc_zend_printf_stdout(const char *format, ...);
extern void elephc_zend_error_cb(int type, zend_string *filename, const uint32_t lineno,
                                 zend_string *message);

/* ------------------------------------------------------------ configuration */

/* php.ini, as far as a hosted extension can tell: directives set before the
 * engine starts become the initial values REGISTER_INI_ENTRIES reads. */
static HashTable elephc_configuration;
static bool elephc_configuration_ready = false;

static void elephc_configuration_dtor(zval *value) {
    zend_string_release_ex(Z_STR_P(value), 1);
}

static void elephc_configuration_init(void) {
    if (!elephc_configuration_ready) {
        zend_hash_init(&elephc_configuration, 8, NULL, elephc_configuration_dtor, 1);
        elephc_configuration_ready = true;
    }
}

ZEND_API zval *zend_get_configuration_directive(zend_string *name) {
    return elephc_configuration_ready ? zend_hash_find(&elephc_configuration, name) : NULL;
}

/* ------------------------------------------------------------ startup */

static bool elephc_engine_started = false;

static void elephc_random_bytes_insecure(zend_random_bytes_insecure_state *state, void *bytes,
                                         size_t size) {
    (void)state;
    /* Only the allocator's shadow-pointer key uses this; it needs to differ
     * between runs, not to be cryptographic. */
    uint64_t seed = (uint64_t)(uintptr_t)&state ^ (uint64_t)(uintptr_t)bytes ^ 0x9e3779b97f4a7c15ULL;
    unsigned char *out = bytes;
    for (size_t i = 0; i < size; i++) {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        out[i] = (unsigned char)seed;
    }
}

/* Classes every extension may name as a parent or throw. */
static void elephc_register_core_classes(void) {
    zend_register_interfaces();
    zend_register_default_exception();

    zend_class_entry ce;
    INIT_CLASS_ENTRY(ce, "stdClass", NULL);
    zend_standard_class_def = zend_register_internal_class_with_flags(
        &ce, NULL, ZEND_ACC_ALLOW_DYNAMIC_PROPERTIES);

    INIT_CLASS_ENTRY(ce, "Closure", NULL);
    zend_ce_closure = zend_register_internal_class_with_flags(
        &ce, NULL, ZEND_ACC_FINAL | ZEND_ACC_NO_DYNAMIC_PROPERTIES | ZEND_ACC_NOT_SERIALIZABLE);

    spl_ce_LogicException = register_class_LogicException(zend_ce_exception);
    spl_ce_BadFunctionCallException = register_class_BadFunctionCallException(spl_ce_LogicException);
    spl_ce_BadMethodCallException = register_class_BadMethodCallException(spl_ce_BadFunctionCallException);
    spl_ce_DomainException = register_class_DomainException(spl_ce_LogicException);
    spl_ce_InvalidArgumentException = register_class_InvalidArgumentException(spl_ce_LogicException);
    spl_ce_LengthException = register_class_LengthException(spl_ce_LogicException);
    spl_ce_OutOfRangeException = register_class_OutOfRangeException(spl_ce_LogicException);
    spl_ce_RuntimeException = register_class_RuntimeException(zend_ce_exception);
    spl_ce_OutOfBoundsException = register_class_OutOfBoundsException(spl_ce_RuntimeException);
    spl_ce_OverflowException = register_class_OverflowException(spl_ce_RuntimeException);
    spl_ce_RangeException = register_class_RangeException(spl_ce_RuntimeException);
    spl_ce_UnderflowException = register_class_UnderflowException(spl_ce_RuntimeException);
    spl_ce_UnexpectedValueException = register_class_UnexpectedValueException(spl_ce_RuntimeException);
}

/* Modules whose surface the engine already provides, registered as started so
 * an extension that declares a hard dependency on them can boot. */
static zend_module_entry elephc_core_module = {
    STANDARD_MODULE_HEADER, "Core", NULL, NULL, NULL, NULL, NULL, NULL,
    PHP_VERSION, STANDARD_MODULE_PROPERTIES
};
static zend_module_entry elephc_spl_module = {
    STANDARD_MODULE_HEADER, "SPL", NULL, NULL, NULL, NULL, NULL, NULL,
    PHP_VERSION, STANDARD_MODULE_PROPERTIES
};
/* ext/standard's C API as far as extensions link it (base64, digests, the
 * incomplete class, the basic globals); anything beyond fails at link time
 * with the missing symbol's name, so the claim cannot hide a gap. */
static zend_module_entry elephc_standard_module = {
    STANDARD_MODULE_HEADER, "standard", NULL, NULL, NULL, NULL, NULL, NULL,
    PHP_VERSION, STANDARD_MODULE_PROPERTIES
};

static void elephc_register_provided_module(zend_module_entry *module) {
    zend_module_entry *registered = zend_register_internal_module(module);
    if (registered) {
        registered->module_started = 1;
    }
}

/* Starts the engine once per process: allocator, interned strings, the class
 * and constant tables, and the core class hierarchy. */
void elephc_zend_startup(void) {
    if (elephc_engine_started) {
        return;
    }
    elephc_engine_started = true;

    zend_random_bytes_insecure = elephc_random_bytes_insecure;
    zend_printf_to_smart_string = php_printf_to_smart_string;
    zend_printf_to_smart_str = php_printf_to_smart_str;
    zend_write = elephc_zend_write_stdout;
    zend_printf = elephc_zend_printf_stdout;
    zend_error_cb = elephc_zend_error_cb;

    start_memory_manager();
    zend_startup_hrtime();
    elephc_configuration_init();

    CG(function_table) = malloc(sizeof(HashTable));
    CG(class_table) = malloc(sizeof(HashTable));
    CG(auto_globals) = malloc(sizeof(HashTable));
    zend_hash_init(CG(function_table), 1024, NULL, ZEND_FUNCTION_DTOR, 1);
    zend_hash_init(CG(class_table), 64, NULL, ZEND_CLASS_DTOR, 1);
    zend_hash_init(CG(auto_globals), 8, NULL, NULL, 1);
    zend_hash_init(&module_registry, 32, NULL, NULL, 1);
    zend_init_rsrc_list_dtors();

    /* init_compiler()'s arena: registration allocates internal functions'
     * run-time caches, property infos and class constants from it. */
    CG(arena) = zend_arena_create(64 * 1024);
    CG(map_ptr_real_base) = NULL;
    CG(map_ptr_base) = ZEND_MAP_PTR_BIASED_BASE(NULL);
    CG(map_ptr_size) = 0;
    CG(map_ptr_last) = 0;

    EG(error_reporting) = E_ALL;
    EG(precision) = 14;
    EG(lineno_override) = -1;
    EG(filename_override) = NULL;
    EG(current_execute_data) = NULL;
    EG(current_module) = NULL;
    EG(exception) = NULL;
    EG(prev_exception) = NULL;
    EG(bailout) = NULL;
    EG(error_handling) = EH_NORMAL;
    EG(flags) = EG_FLAGS_INITIAL;
    ZVAL_UNDEF(&EG(user_error_handler));
    ZVAL_UNDEF(&EG(user_exception_handler));
    ZVAL_UNDEF(&EG(last_fatal_error_backtrace));

    zend_interned_strings_init();
    zend_startup_constants();
    zend_register_standard_constants();
    zend_ini_startup();
    zend_init_rsrc_plist();

    EG(function_table) = CG(function_table);
    EG(class_table) = CG(class_table);
    ZVAL_NULL(&EG(uninitialized_zval));
    ZVAL_ERROR(&EG(error_zval));
    zend_hash_init(&EG(symbol_table), 64, NULL, ZVAL_PTR_DTOR, 0);
    zend_init_rsrc_list();
    zend_objects_store_init(&EG(objects_store), 1024);
    EG(ht_iterators_count) = sizeof(EG(ht_iterators_slots)) / sizeof(HashTableIterator);
    EG(ht_iterators_used) = 0;
    EG(ht_iterators) = EG(ht_iterators_slots);
    memset(EG(ht_iterators), 0, sizeof(EG(ht_iterators_slots)));

    /* Core classes are owned by the Core module, as in PHP: class registration
     * records EG(current_module) on every entry it creates. */
    elephc_register_provided_module(&elephc_core_module);
    elephc_register_provided_module(&elephc_spl_module);
    elephc_register_provided_module(&elephc_standard_module);
    EG(current_module) = &elephc_core_module;
    elephc_register_core_classes();
    elephc_stdlib_startup();
    EG(current_module) = NULL;

    /* From here on the program is inside one long request, as php_module_startup
     * leaves a PHP process: interned strings made now are request strings. The
     * permanent handlers must not see request-time calls — the one for
     * zend_string_init_existing_interned asserts `permanent`, and in a release
     * build that assertion is an optimisation hint: asked for a request string
     * it silently returns a persistent (malloc'd) one, which the caller then
     * efree()s into the Zend heap. igbinary does exactly that. */
    zend_interned_strings_activate();
    zend_interned_strings_switch_storage(1);
    EG(active) = 1;
}

/* Runs `startup` (a module's MINIT) with permanent interned-string storage,
 * where PHP runs every MINIT, then returns to request storage. */
static zend_result elephc_run_module_startup(zend_module_entry *module) {
    zend_interned_strings_switch_storage(0);
    zend_result result = zend_startup_module_ex(module);
    zend_interned_strings_switch_storage(1);
    return result;
}

/* Sets an INI directive, as a php.ini line would. Called by the generated
 * prelude before the first hosted call, so the value is in place when an
 * extension's MINIT registers the directive; a directive set after that is
 * applied to the running engine instead. */
void elephc_php_ext_ini(const char *name, const char *value) {
    elephc_configuration_init();
    zval setting;
    ZVAL_STR(&setting, zend_string_init(value, strlen(value), 1));
    zend_hash_str_update(&elephc_configuration, name, strlen(name), &setting);
    if (elephc_engine_started) {
        zend_alter_ini_entry_chars(Z_STR(setting), value, strlen(value), PHP_INI_SYSTEM,
                                   PHP_INI_STAGE_RUNTIME);
    }
}

/* Registers and starts one module (MINIT, then RINIT) the first time it is
 * used. Returns false and prints the reason when the module cannot start. */
static bool elephc_boot_module(zend_module_entry *module) {
    elephc_zend_startup();
    if (module->module_started) {
        return true;
    }
    JMP_BUF *outer = EG(bailout);
    JMP_BUF frame;
    bool booted = false;
    EG(bailout) = &frame;
    if (SETJMP(frame) == 0) {
        zend_interned_strings_switch_storage(0);
        zend_module_entry *registered = zend_register_internal_module(module);
        zend_interned_strings_switch_storage(1);
        if (registered && elephc_run_module_startup(registered) == SUCCESS) {
            if (registered->request_startup_func) {
                EG(current_module) = registered;
                booted = registered->request_startup_func(registered->type,
                                                          registered->module_number) == SUCCESS;
                EG(current_module) = NULL;
            } else {
                booted = true;
            }
        }
    }
    EG(bailout) = outer;
    /* A MINIT that bailed out skipped the switch back. */
    zend_interned_strings_switch_storage(1);
    if (!booted) {
        fflush(stdout);
        fprintf(stderr, "Fatal error: hosted PHP extension '%s' failed to start\n", module->name);
    }
    return booted;
}

/* ------------------------------------------------------------ Elephc -> engine */

/* zval_pack builds PHP's own structures, so the engine's accessors read them;
 * only the packed-table convention differs (32-byte buckets, mask -2). */
#define ELEPHC_PACKED_MASK ((uint32_t)-2)

static bool elephc_import(zval *dst, const zval *src);

static bool elephc_import_array(zval *dst, const HashTable *src) {
    uint32_t used = src->nNumUsed;
    array_init_size(dst, used);
    HashTable *target = Z_ARRVAL_P(dst);
    const Bucket *buckets = src->arData;
    bool packed = src->nTableMask == ELEPHC_PACKED_MASK;
    for (uint32_t i = 0; i < used; i++) {
        const Bucket *bucket = &buckets[i];
        if (Z_TYPE(bucket->val) == IS_UNDEF) {
            continue;
        }
        zval value;
        if (!elephc_import(&value, &bucket->val)) {
            return false;
        }
        if (packed) {
            zend_hash_next_index_insert_new(target, &value);
        } else if (bucket->key) {
            zend_string *key = zend_string_init(ZSTR_VAL(bucket->key), ZSTR_LEN(bucket->key), 0);
            zend_symtable_update(target, key, &value);
            zend_string_release(key);
        } else {
            zend_hash_index_update(target, bucket->h, &value);
        }
    }
    return true;
}

/* Rebuilds an Elephc-owned zval in engine memory. Returns false for a value
 * with no engine counterpart yet (an Elephc object). */
static bool elephc_import(zval *dst, const zval *src) {
    switch (Z_TYPE_P(src)) {
        case IS_UNDEF:
        case IS_NULL:
            ZVAL_NULL(dst);
            return true;
        case IS_FALSE:
            ZVAL_FALSE(dst);
            return true;
        case IS_TRUE:
            ZVAL_TRUE(dst);
            return true;
        case IS_LONG:
            ZVAL_LONG(dst, Z_LVAL_P(src));
            return true;
        case IS_DOUBLE:
            ZVAL_DOUBLE(dst, Z_DVAL_P(src));
            return true;
        case IS_STRING:
            ZVAL_STRINGL(dst, Z_STRVAL_P(src), Z_STRLEN_P(src));
            return true;
        case IS_ARRAY:
            if (!elephc_import_array(dst, Z_ARRVAL_P(src))) {
                zval_ptr_dtor(dst);
                ZVAL_NULL(dst);
                return false;
            }
            return true;
        default:
            ZVAL_NULL(dst);
            return false;
    }
}

/* ------------------------------------------------------------ engine -> Elephc */

/* Follows references and indirections to the value itself. */
static zval *elephc_value(zval *zv) {
    ZVAL_DEREF(zv);
    if (Z_TYPE_P(zv) == IS_INDIRECT) {
        zv = Z_INDIRECT_P(zv);
        ZVAL_DEREF(zv);
    }
    return zv;
}

/* True for a property key the outside world can see: mangled names (private
 * and protected properties) start with a NUL byte. */
static bool elephc_visible_key(const zend_string *key) {
    return !key || !ZSTR_LEN(key) || ZSTR_VAL(key)[0] != '\0';
}

static HashTable *elephc_object_properties(zval *zv) {
    return Z_OBJ_HT_P(zv)->get_properties(Z_OBJ_P(zv));
}

/* True when `zv` holds an object or resource anywhere below it. Only called
 * on values elephc_representable accepted, so it never meets a cycle. */
static bool elephc_contains_objects(zval *zv) {
    zv = elephc_value(zv);
    if (Z_TYPE_P(zv) == IS_OBJECT || Z_TYPE_P(zv) == IS_RESOURCE) {
        return true;
    }
    if (Z_TYPE_P(zv) != IS_ARRAY) {
        return false;
    }
    zval *entry;
    ZEND_HASH_FOREACH_VAL(Z_ARRVAL_P(zv), entry) {
        if (elephc_contains_objects(entry)) {
            return true;
        }
    } ZEND_HASH_FOREACH_END();
    return false;
}

static zend_string *elephc_export_string(const char *val, size_t len) {
    zend_string *str = malloc(_ZSTR_STRUCT_SIZE(len));
    GC_SET_REFCOUNT(str, 1);
    GC_TYPE_INFO(str) = GC_STRING;
    ZSTR_H(str) = 0;
    ZSTR_LEN(str) = len;
    memcpy(ZSTR_VAL(str), val, len);
    ZSTR_VAL(str)[len] = '\0';
    return str;
}

static void elephc_export(zval *dst, zval *src);

static void elephc_export_array(zval *dst, HashTable *src) {
    uint32_t count = zend_hash_num_elements(src);
    uint32_t size = count ? count : 1;
    bool list = zend_array_is_list(src);
    HashTable *ht = calloc(1, sizeof(HashTable));
    Bucket *buckets = calloc(size, sizeof(Bucket));
    GC_SET_REFCOUNT(ht, 1);
    GC_TYPE_INFO(ht) = GC_ARRAY;
    HT_FLAGS(ht) = list ? (HASH_FLAG_PACKED | HASH_FLAG_STATIC_KEYS) : 0;
    /* zval_unpack tells the two layouts apart by this mask alone; a hash mask
     * must therefore never be -2, whatever the table's size. */
    ht->nTableMask = list ? ELEPHC_PACKED_MASK : (uint32_t)-(int32_t)(size < 8 ? 8 : size);
    ht->arData = buckets;
    ht->nTableSize = size;
    uint32_t i = 0;
    zend_string *key;
    zend_ulong h;
    zval *val;
    ZEND_HASH_FOREACH_KEY_VAL(src, h, key, val) {
        Bucket *bucket = &buckets[i];
        elephc_export(&bucket->val, val);
        if (key && !list) {
            bucket->key = elephc_export_string(ZSTR_VAL(key), ZSTR_LEN(key));
            bucket->h = 0;
        } else {
            bucket->key = NULL;
            bucket->h = list ? i : h;
            if ((zend_long)bucket->h >= ht->nNextFreeElement) {
                ht->nNextFreeElement = (zend_long)bucket->h + 1;
            }
        }
        i++;
    } ZEND_HASH_FOREACH_END();
    ht->nNumUsed = i;
    ht->nNumOfElements = i;
    ZVAL_ARR(dst, ht);
}

/* Copies an object-free value into zval_unpack's layout. */
static void elephc_export(zval *dst, zval *src) {
    src = elephc_value(src);
    switch (Z_TYPE_P(src)) {
        case IS_FALSE: ZVAL_FALSE(dst); return;
        case IS_TRUE: ZVAL_TRUE(dst); return;
        case IS_LONG: ZVAL_LONG(dst, Z_LVAL_P(src)); return;
        case IS_DOUBLE: ZVAL_DOUBLE(dst, Z_DVAL_P(src)); return;
        case IS_STRING:
            ZVAL_STR(dst, elephc_export_string(Z_STRVAL_P(src), Z_STRLEN_P(src)));
            return;
        case IS_ARRAY: elephc_export_array(dst, Z_ARRVAL_P(src)); return;
        default: ZVAL_NULL(dst); return;
    }
}

/* Frees an exported tree. Every node was malloc'd by elephc_export. */
static void elephc_free_export(zval *value) {
    if (Z_TYPE_P(value) == IS_STRING) {
        free(Z_STR_P(value));
    } else if (Z_TYPE_P(value) == IS_ARRAY) {
        HashTable *ht = Z_ARRVAL_P(value);
        for (uint32_t i = 0; i < ht->nNumUsed; i++) {
            Bucket *bucket = &ht->arData[i];
            if (bucket->key) {
                free(bucket->key);
            }
            elephc_free_export(&bucket->val);
        }
        free(ht->arData);
        free(ht);
    }
    ZVAL_UNDEF(value);
}

/* ------------------------------------------------------------ calls */

typedef struct elephc_export_node {
    struct elephc_export_node *next;
    zval value;
} elephc_export_node;

typedef struct {
    zend_function *function;
    uint32_t argc;
    zend_execute_data *frame;
    zval result;                 /* engine-owned return value */
    elephc_export_node *exports; /* every exported subtree, freed with the call */
    char *error_class;
    char *error_message;
    int64_t error_code;
} elephc_call;

static char *elephc_strdup_zstr(zend_string *str) {
    char *copy = malloc(ZSTR_LEN(str) + 1);
    memcpy(copy, ZSTR_VAL(str), ZSTR_LEN(str) + 1);
    return copy;
}

static void elephc_call_fail(elephc_call *call, const char *class_name, const char *message) {
    free(call->error_class);
    free(call->error_message);
    call->error_class = strdup(class_name);
    call->error_message = strdup(message);
    call->error_code = 0;
}

/* Calls prepared and not yet freed: every wrapper path, the error ones
 * included, must bring this back to zero. */
static int64_t elephc_live_calls;

int64_t elephc_php_ext_live_calls(void) {
    return elephc_live_calls;
}

/* Starts `module` if needed and prepares a call to `name` with `argc` slots.
 * Returns NULL when the module cannot start or does not export `name`. */
void *elephc_php_ext_call_new(void *module, const char *name, int64_t argc) {
    if (!elephc_boot_module((zend_module_entry *)module)) {
        return NULL;
    }
    zend_function *function = zend_hash_str_find_ptr_lc(CG(function_table), name, strlen(name));
    if (!function || function->type != ZEND_INTERNAL_FUNCTION) {
        fflush(stdout);
        fprintf(stderr, "Fatal error: hosted PHP extension '%s' does not export %s()\n",
                ((zend_module_entry *)module)->name, name);
        return NULL;
    }
    elephc_call *call = calloc(1, sizeof(elephc_call));
    call->function = function;
    call->argc = (uint32_t)argc;
    size_t slots = ZEND_CALL_FRAME_SLOT + (argc > 0 ? (size_t)argc : 1);
    call->frame = ecalloc(slots, sizeof(zval));
    ZVAL_UNDEF(&call->result);
    elephc_live_calls++;
    return call;
}

/* Copies argument `index` (zero-based) into the frame. A by-reference
 * argument is wrapped in a zend_reference, which is what the callee writes
 * through. Returns 0, or ELEPHC_CALL_UNSUPPORTED for a value that cannot yet
 * be passed (an object). */
int64_t elephc_php_ext_call_arg(void *handle, int64_t index, void *value, int64_t by_ref) {
    elephc_call *call = handle;
    zval *slot = ZEND_CALL_ARG(call->frame, (uint32_t)index + 1);
    zval imported;
    if (!elephc_import(&imported, (const zval *)value)) {
        ZVAL_NULL(slot);
        elephc_call_fail(call, "TypeError", "objects cannot be passed to a hosted PHP extension yet");
        return ELEPHC_CALL_UNSUPPORTED;
    }
    if (by_ref) {
        ZVAL_NEW_REF(slot, &imported);
    } else {
        ZVAL_COPY_VALUE(slot, &imported);
    }
    return ELEPHC_CALL_OK;
}

/* Reads the pending exception's class, message and code, then clears it. */
static void elephc_capture_exception(elephc_call *call) {
    zend_object *exception = EG(exception);
    zval rv;
    zval *message = zend_read_property_ex(exception->ce, exception, ZSTR_KNOWN(ZEND_STR_MESSAGE), 1, &rv);
    free(call->error_message);
    call->error_message = (message && Z_TYPE_P(message) == IS_STRING)
        ? elephc_strdup_zstr(Z_STR_P(message)) : strdup("");
    zval *code = zend_read_property_ex(exception->ce, exception, ZSTR_KNOWN(ZEND_STR_CODE), 1, &rv);
    call->error_code = (code && Z_TYPE_P(code) == IS_LONG) ? Z_LVAL_P(code) : 0;
    /* The class and every ancestor, most derived first and comma-separated:
     * the wrapper rethrows the first one Elephc declares, so a subclass it
     * cannot declare still arrives as its nearest declared parent. */
    smart_str chain = {0};
    for (zend_class_entry *ce = exception->ce; ce; ce = ce->parent) {
        if (chain.s) {
            smart_str_appendc(&chain, ',');
        }
        smart_str_append(&chain, ce->name);
    }
    smart_str_0(&chain);
    free(call->error_class);
    call->error_class = elephc_strdup_zstr(chain.s);
    smart_str_free(&chain);
    zend_clear_exception();
}

/* Runs the call inside a protected frame. Returns ELEPHC_CALL_OK, _EXCEPTION
 * (read it with the error accessors) or _FATAL (already reported). */
int64_t elephc_php_ext_call_invoke(void *handle) {
    elephc_call *call = handle;
    zend_execute_data *frame = call->frame;
    frame->func = call->function;
    frame->prev_execute_data = EG(current_execute_data);
    frame->return_value = &call->result;
    frame->opline = NULL;
    frame->run_time_cache = NULL;
    frame->extra_named_params = NULL;
    frame->symbol_table = NULL;
    ZVAL_UNDEF(&frame->This);
    ZEND_CALL_NUM_ARGS(frame) = call->argc;
    ZVAL_NULL(&call->result);

    JMP_BUF *outer = EG(bailout);
    JMP_BUF protected_frame;
    int64_t status = ELEPHC_CALL_OK;
    EG(current_execute_data) = frame;
    EG(bailout) = &protected_frame;
    if (SETJMP(protected_frame) == 0) {
        call->function->internal_function.handler(frame, &call->result);
    } else {
        status = ELEPHC_CALL_FATAL;
    }
    EG(bailout) = outer;
    EG(current_execute_data) = frame->prev_execute_data;
    if (status == ELEPHC_CALL_OK && EG(exception)) {
        elephc_capture_exception(call);
        status = ELEPHC_CALL_EXCEPTION;
    }
    return status;
}

/* The engine-owned return value, valid until elephc_php_ext_call_free. */
void *elephc_php_ext_call_result(void *handle) {
    return elephc_value(&((elephc_call *)handle)->result);
}

/* The final value of by-reference argument `index`, or NULL when that slot was
 * not passed by reference. */
void *elephc_php_ext_call_ref(void *handle, int64_t index) {
    elephc_call *call = handle;
    zval *arg = ZEND_CALL_ARG(call->frame, (uint32_t)index + 1);
    return Z_ISREF_P(arg) ? elephc_value(arg) : NULL;
}

const char *elephc_php_ext_call_error_class(void *handle) {
    elephc_call *call = handle;
    return call->error_class ? call->error_class : "";
}

const char *elephc_php_ext_call_error_message(void *handle) {
    elephc_call *call = handle;
    return call->error_message ? call->error_message : "";
}

int64_t elephc_php_ext_call_error_code(void *handle) {
    return ((elephc_call *)handle)->error_code;
}

/* Releases the frame's arguments and result (refcount-aware: a value the
 * extension kept survives) and every exported copy. A cyclic result the
 * wrapper refused is not reclaimed: the collector is not hosted. */
void elephc_php_ext_call_free(void *handle) {
    elephc_call *call = handle;
    if (!call) {
        return;
    }
    for (uint32_t i = 0; i < call->argc; i++) {
        zval_ptr_dtor(ZEND_CALL_ARG(call->frame, i + 1));
    }
    zval_ptr_dtor(&call->result);
    /* The walk cursors name tables by address; one freed here may come back
     * at the same address in the next call's result. */
    memset(elephc_walk_cursors, 0, sizeof elephc_walk_cursors);
    for (elephc_export_node *node = call->exports; node;) {
        elephc_export_node *next = node->next;
        elephc_free_export(&node->value);
        free(node);
        node = next;
    }
    efree(call->frame);
    free(call->error_class);
    free(call->error_message);
    free(call);
    elephc_live_calls--;
}

/* ------------------------------------------------------------ value walking */

/* Whether `zv` can cross into Elephc: objects are stdClass, there is no
 * resource, and no array or object contains itself. On refusal the reason is
 * left on `call` as its error. A value shared along two paths is accepted
 * (and arrives as two copies); only one on its own path is a cycle. */
static bool elephc_representable(elephc_call *call, zval *zv) {
    char reason[256];
    zv = elephc_value(zv);
    if (Z_TYPE_P(zv) == IS_RESOURCE
            || (Z_TYPE_P(zv) == IS_OBJECT && Z_OBJCE_P(zv) != zend_standard_class_def)) {
        const char *type = Z_TYPE_P(zv) == IS_OBJECT ? ZSTR_VAL(Z_OBJCE_P(zv)->name) : "resource";
        snprintf(reason, sizeof reason,
            "a hosted PHP extension returned a value of type %s, which Elephc cannot represent yet", type);
        elephc_call_fail(call, "Error", reason);
        return false;
    }
    HashTable *table;
    zend_refcounted *guard;
    if (Z_TYPE_P(zv) == IS_OBJECT) {
        table = elephc_object_properties(zv);
        guard = (zend_refcounted *)Z_OBJ_P(zv);
    } else if (Z_TYPE_P(zv) == IS_ARRAY && !(GC_FLAGS(Z_ARRVAL_P(zv)) & GC_IMMUTABLE)) {
        /* An immutable array holds only immutable scalars and arrays. */
        table = Z_ARRVAL_P(zv);
        guard = (zend_refcounted *)table;
    } else {
        return true;
    }
    if (GC_IS_RECURSIVE(guard)) {
        snprintf(reason, sizeof reason,
            "a hosted PHP extension returned a recursive %s, which Elephc cannot represent yet",
            Z_TYPE_P(zv) == IS_OBJECT ? "object" : "array");
        elephc_call_fail(call, "Error", reason);
        return false;
    }
    GC_PROTECT_RECURSION(guard);
    bool accepted = true;
    zval *entry;
    ZEND_HASH_FOREACH_VAL_IND(table, entry) {
        if (!elephc_representable(call, entry)) {
            accepted = false;
            break;
        }
    } ZEND_HASH_FOREACH_END();
    GC_UNPROTECT_RECURSION(guard);
    return accepted;
}

/* Called once on each value the wrapper rebuilds (the result, each by-ref
 * argument) before any walk: 0 when it can cross, 1 with the reason left as
 * the call's error. */
int64_t elephc_php_ext_value_check(void *handle, void *value) {
    return value && !elephc_representable(handle, value) ? 1 : 0;
}

/* How the wrapper must turn `value` (an engine zval) into an Elephc value. */
int64_t elephc_php_ext_value_kind(void *value) {
    zval *zv = elephc_value(value);
    if (Z_TYPE_P(zv) == IS_OBJECT) {
        return ELEPHC_VALUE_STDCLASS;
    }
    return elephc_contains_objects(zv) ? ELEPHC_VALUE_ARRAY : ELEPHC_VALUE_PLAIN;
}

/* Exports an object-free value in zval_unpack's layout. The copy belongs to
 * `call` and is freed with it. */
void *elephc_php_ext_value_export(void *handle, void *value) {
    elephc_call *call = handle;
    elephc_export_node *node = malloc(sizeof(elephc_export_node));
    elephc_export(&node->value, value);
    node->next = call->exports;
    call->exports = node;
    return &node->value;
}

/* The table a stdClass or array value is walked through. */
static HashTable *elephc_walked_table(zval *zv) {
    zv = elephc_value(zv);
    if (Z_TYPE_P(zv) == IS_OBJECT) {
        return elephc_object_properties(zv);
    }
    return Z_TYPE_P(zv) == IS_ARRAY ? Z_ARRVAL_P(zv) : NULL;
}

/* The cursor for `table`, or the least recently used one, reset to it. */
static struct elephc_walk_cursor *elephc_walk_cursor_for(HashTable *table) {
    struct elephc_walk_cursor *oldest = &elephc_walk_cursors[0];
    for (int i = 0; i < ELEPHC_WALK_CURSORS; i++) {
        struct elephc_walk_cursor *cursor = &elephc_walk_cursors[i];
        if (cursor->table == table) {
            cursor->used = ++elephc_walk_clock;
            return cursor;
        }
        if (cursor->used < oldest->used) {
            oldest = cursor;
        }
    }
    oldest->table = table;
    oldest->index = 0;
    oldest->slot = 0;
    oldest->used = ++elephc_walk_clock;
    return oldest;
}

/* The `index`-th visible entry, and its slot in `*slot_out`. Wrappers walk
 * each table in order (the key, then the value, of one entry after another),
 * so each table's previous answer is remembered and the next lookup is O(1). */
static Bucket *elephc_walk_entry(void *value, int64_t index, uint32_t *slot_out) {
    HashTable *table = elephc_walked_table(value);
    if (!table || index < 0) {
        return NULL;
    }
    bool is_object = Z_TYPE_P(elephc_value(value)) == IS_OBJECT;
    struct elephc_walk_cursor *cursor = elephc_walk_cursor_for(table);
    uint32_t visible = 0;
    uint32_t slot = 0;
    if (cursor->index <= (uint32_t)index) {
        visible = cursor->index;
        slot = cursor->slot;
    }
    for (; slot < table->nNumUsed; slot++) {
        Bucket *bucket;
        if (HT_IS_PACKED(table)) {
            /* Packed tables store bare zvals; only their position is a key. */
            zval *entry = &table->arPacked[slot];
            if (Z_TYPE_P(entry) == IS_UNDEF) {
                continue;
            }
            static Bucket packed_view;
            packed_view.val = *entry;
            packed_view.h = slot;
            packed_view.key = NULL;
            bucket = &packed_view;
        } else {
            bucket = &table->arData[slot];
            if (Z_TYPE(bucket->val) == IS_UNDEF
                    || (Z_TYPE(bucket->val) == IS_INDIRECT && Z_TYPE_P(Z_INDIRECT(bucket->val)) == IS_UNDEF)) {
                continue;
            }
            if (is_object && !elephc_visible_key(bucket->key)) {
                continue;
            }
        }
        if (visible == (uint32_t)index) {
            cursor->index = visible;
            cursor->slot = slot;
            if (slot_out) {
                *slot_out = slot;
            }
            return bucket;
        }
        visible++;
    }
    return NULL;
}

/* Number of entries (array) or visible properties (stdClass). */
int64_t elephc_php_ext_value_count(void *value) {
    HashTable *table = elephc_walked_table(value);
    if (!table) {
        return 0;
    }
    if (Z_TYPE_P(elephc_value(value)) != IS_OBJECT) {
        return zend_hash_num_elements(table);
    }
    int64_t count = 0;
    zend_string *key;
    zval *entry;
    ZEND_HASH_FOREACH_STR_KEY_VAL_IND(table, key, entry) {
        (void)entry;
        if (elephc_visible_key(key)) {
            count++;
        }
    } ZEND_HASH_FOREACH_END();
    return count;
}

/* True when an array value's keys are exactly 0..n-1 in order. */
int64_t elephc_php_ext_value_is_list(void *value) {
    zval *zv = elephc_value(value);
    return Z_TYPE_P(zv) == IS_ARRAY && zend_array_is_list(Z_ARRVAL_P(zv)) ? 1 : 0;
}

/* The key of entry `index`, exported like a value: a property name or string
 * key with its full length (a key may hold NUL bytes), or an integer key. */
void *elephc_php_ext_value_key(void *handle, void *value, int64_t index) {
    Bucket *bucket = elephc_walk_entry(value, index, NULL);
    zval key;
    if (!bucket) {
        ZVAL_NULL(&key);
    } else if (bucket->key) {
        ZVAL_STR(&key, bucket->key); /* borrowed: the export copies it */
    } else {
        ZVAL_LONG(&key, (zend_long)bucket->h);
    }
    return elephc_php_ext_value_export(handle, &key);
}

/* The name of visible property `index` of a stdClass, as a C string. Elephc
 * sets a property from a string it copies, and a property table's keys are
 * always strings; array keys, which may hold NUL bytes, go through
 * elephc_php_ext_value_key instead. */
const char *elephc_php_ext_value_name(void *value, int64_t index) {
    Bucket *bucket = elephc_walk_entry(value, index, NULL);
    return bucket && bucket->key ? ZSTR_VAL(bucket->key) : "";
}

/* The value of entry `index`, as an engine zval the wrapper walks further. */
void *elephc_php_ext_value_at(void *value, int64_t index) {
    uint32_t slot;
    Bucket *bucket = elephc_walk_entry(value, index, &slot);
    if (!bucket) {
        return NULL;
    }
    if (HT_IS_PACKED(elephc_walked_table(value))) {
        return elephc_value(&elephc_walked_table(value)->arPacked[slot]);
    }
    return elephc_value(&bucket->val);
}

/* ------------------------------------------------------------ introspection */

static void elephc_json_string(FILE *out, const char *str) {
    fputc('"', out);
    for (const unsigned char *p = (const unsigned char *)str; *p; p++) {
        switch (*p) {
            case '"': fputs("\\\"", out); break;
            case '\\': fputs("\\\\", out); break;
            case '\n': fputs("\\n", out); break;
            case '\r': fputs("\\r", out); break;
            case '\t': fputs("\\t", out); break;
            default:
                if (*p < 0x20) {
                    fprintf(out, "\\u%04x", *p);
                } else {
                    fputc(*p, out);
                }
        }
    }
    fputc('"', out);
}

static void elephc_json_type(FILE *out, zend_type type) {
    if (!ZEND_TYPE_IS_SET(type)) {
        fputs("null", out);
        return;
    }
    zend_string *rendered = zend_type_to_string(type);
    elephc_json_string(out, ZSTR_VAL(rendered));
    zend_string_release(rendered);
}

static void elephc_describe_function(FILE *out, zend_function *function) {
    zend_internal_function *fn = &function->internal_function;
    fputs("{\"name\":", out);
    elephc_json_string(out, ZSTR_VAL(fn->function_name));
    fprintf(out, ",\"required\":%u", fn->required_num_args);
    bool variadic = (fn->fn_flags & ZEND_ACC_VARIADIC) != 0;
    uint32_t count = fn->num_args + (variadic ? 1 : 0);
    fputs(",\"returns\":", out);
    if (fn->arg_info) {
        zend_internal_arg_info *ret = fn->arg_info - 1;
        elephc_json_type(out, ret->type);
    } else {
        fputs("null", out);
    }
    fprintf(out, ",\"returns_by_ref\":%s", (fn->fn_flags & ZEND_ACC_RETURN_REFERENCE) ? "true" : "false");
    fprintf(out, ",\"deprecated\":%s", (fn->fn_flags & ZEND_ACC_DEPRECATED) ? "true" : "false");
    fputs(",\"params\":[", out);
    for (uint32_t i = 0; fn->arg_info && i < count; i++) {
        zend_internal_arg_info *arg = &fn->arg_info[i];
        if (i) {
            fputc(',', out);
        }
        fputs("{\"name\":", out);
        elephc_json_string(out, arg->name);
        fputs(",\"type\":", out);
        elephc_json_type(out, arg->type);
        fprintf(out, ",\"by_ref\":%s,\"variadic\":%s,\"default\":",
                ZEND_ARG_SEND_MODE(arg) ? "true" : "false",
                ZEND_ARG_IS_VARIADIC(arg) ? "true" : "false");
        if (arg->default_value) {
            elephc_json_string(out, arg->default_value);
        } else {
            fputs("null", out);
        }
        fputc('}', out);
    }
    fputs("]}", out);
}

static void elephc_describe_constant_value(FILE *out, zval *value) {
    switch (Z_TYPE_P(value)) {
        case IS_NULL: fputs("{\"type\":\"null\"}", out); break;
        case IS_FALSE: fputs("{\"type\":\"bool\",\"value\":false}", out); break;
        case IS_TRUE: fputs("{\"type\":\"bool\",\"value\":true}", out); break;
        case IS_LONG: fprintf(out, "{\"type\":\"int\",\"value\":" ZEND_LONG_FMT "}", Z_LVAL_P(value)); break;
        case IS_DOUBLE: {
            smart_str buf = {0};
            smart_str_append_double(&buf, Z_DVAL_P(value), 17, false);
            smart_str_0(&buf);
            fputs("{\"type\":\"float\",\"value\":", out);
            elephc_json_string(out, ZSTR_VAL(buf.s));
            fputc('}', out);
            smart_str_free(&buf);
            break;
        }
        case IS_STRING:
            fputs("{\"type\":\"string\",\"value\":", out);
            elephc_json_string(out, Z_STRVAL_P(value));
            fputc('}', out);
            break;
        default:
            fprintf(out, "{\"type\":\"%s\"}", zend_zval_type_name(value));
    }
}

/* Describes what `module` registers once started — its functions with their
 * arginfo, its classes, constants and INI directives — as one JSON document.
 * This is the surface Elephc exposes: taken from the built extension itself,
 * so an #ifdef, an alias or a C-side registration cannot be missed. Returns 0,
 * or 1 when the module could not start. */
int elephc_php_ext_describe(void *handle, FILE *out) {
    zend_module_entry *module = handle;
    if (!elephc_boot_module(module)) {
        return 1;
    }
    fputs("{\"module\":", out);
    elephc_json_string(out, module->name);
    fputs(",\"version\":", out);
    elephc_json_string(out, module->version ? module->version : "");

    fputs(",\"dependencies\":[", out);
    bool first = true;
    for (const zend_module_dep *dep = module->deps; dep && dep->name; dep++) {
        if (dep->type != MODULE_DEP_REQUIRED) {
            continue;
        }
        if (!first) fputc(',', out);
        first = false;
        elephc_json_string(out, dep->name);
    }

    fputs("],\"functions\":[", out);
    first = true;
    for (const zend_function_entry *entry = module->functions; entry && entry->fname; entry++) {
        zend_function *function = zend_hash_str_find_ptr_lc(CG(function_table), entry->fname,
                                                            strlen(entry->fname));
        if (!function) {
            continue;
        }
        if (!first) fputc(',', out);
        first = false;
        elephc_describe_function(out, function);
    }

    fputs("],\"classes\":[", out);
    first = true;
    zend_class_entry *ce;
    ZEND_HASH_MAP_FOREACH_PTR(CG(class_table), ce) {
        if (ce->type != ZEND_INTERNAL_CLASS || ce->info.internal.module != module) {
            continue;
        }
        if (!first) fputc(',', out);
        first = false;
        fputs("{\"name\":", out);
        elephc_json_string(out, ZSTR_VAL(ce->name));
        fputs(",\"parent\":", out);
        if (ce->parent) {
            elephc_json_string(out, ZSTR_VAL(ce->parent->name));
        } else {
            fputs("null", out);
        }
        fprintf(out, ",\"interface\":%s,\"abstract\":%s,\"final\":%s",
                (ce->ce_flags & ZEND_ACC_INTERFACE) ? "true" : "false",
                (ce->ce_flags & ZEND_ACC_EXPLICIT_ABSTRACT_CLASS) ? "true" : "false",
                (ce->ce_flags & ZEND_ACC_FINAL) ? "true" : "false");
        fputs(",\"methods\":[", out);
        bool first_method = true;
        zend_function *method;
        ZEND_HASH_MAP_FOREACH_PTR(&ce->function_table, method) {
            if (method->common.scope != ce) {
                continue; /* inherited */
            }
            if (!first_method) fputc(',', out);
            first_method = false;
            elephc_describe_function(out, method);
        } ZEND_HASH_FOREACH_END();
        fputs("]}", out);
    } ZEND_HASH_FOREACH_END();

    fputs("],\"constants\":[", out);
    first = true;
    zend_constant *constant;
    ZEND_HASH_MAP_FOREACH_PTR(EG(zend_constants), constant) {
        if (ZEND_CONSTANT_MODULE_NUMBER(constant) != module->module_number) {
            continue;
        }
        if (!first) fputc(',', out);
        first = false;
        fputs("{\"name\":", out);
        elephc_json_string(out, ZSTR_VAL(constant->name));
        fputs(",\"value\":", out);
        elephc_describe_constant_value(out, &constant->value);
        fputc('}', out);
    } ZEND_HASH_FOREACH_END();

    fputs("],\"ini\":[", out);
    first = true;
    zend_ini_entry *ini;
    ZEND_HASH_MAP_FOREACH_PTR(EG(ini_directives), ini) {
        if (ini->module_number != module->module_number) {
            continue;
        }
        if (!first) fputc(',', out);
        first = false;
        fputs("{\"name\":", out);
        elephc_json_string(out, ZSTR_VAL(ini->name));
        fputs(",\"default\":", out);
        if (ini->value) {
            elephc_json_string(out, ZSTR_VAL(ini->value));
        } else {
            fputs("null", out);
        }
        fputc('}', out);
    } ZEND_HASH_FOREACH_END();
    fputs("]}\n", out);
    return 0;
}
