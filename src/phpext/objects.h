/* Class metadata is shared after MINIT; every instance lease belongs to one request. */
#include <stddef.h>
#include <stdint.h>
#include <Zend/zend_execute.h>

typedef void (*ep_native_function)(const zval *const *);
typedef struct ep_call ep_call;
typedef struct ep_object_lease ep_object_lease;
typedef struct ep_object_change ep_object_change;
typedef struct ep_object_request ep_object_request;

typedef struct ep_property_descriptor {
    uint32_t offset;
    ep_native_function read;
    ep_native_function write;
    ep_native_function clear;
    ep_native_function initialized;
} ep_property_descriptor;

typedef struct ep_class_descriptor {
    zend_class_entry *ce;
    uintptr_t native_id;
    size_t property_count;
    ep_property_descriptor *properties;
    ep_native_function allocate;
    ep_native_function release;
    ep_native_function roots;
    ep_native_function collect;
} ep_class_descriptor;

static zend_object_handlers ep_object_handlers;
static zend_object *ep_object_clone(zend_object *source);
static void ep_object_free(zend_object *object);
static zval *ep_object_copy(ep_call *call, const zval *source, bool host);
static void ep_object_to_zend(ep_call *call, const zval *source, zval *destination);
static void ep_objects_prepare(ep_call *call);
static void ep_objects_release_pending(ep_call *call);
static void ep_objects_import(ep_call *call);
static void ep_objects_capture(ep_call *call);
static void ep_objects_commit(ep_call *call);
static void ep_objects_publish(ep_call *call);
static void ep_objects_rollback(ep_call *call);
static void ep_objects_retire(uint64_t call_identity);
static void ep_objects_abandon(ep_object_request *request);
static void ep_objects_reset_transport(ep_call *call);
static void ep_objects_note_publication(ep_call *call);
static void ep_objects_break(ep_call *call);
static void ep_objects_finish_broken(void);
static void ep_objects_shutdown(void);
static const char *ep_objects_bind_graph(zval *values, size_t count);
static void ep_invoke_ex(INTERNAL_FUNCTION_PARAMETERS, ep_native_function function, zval *values,
    size_t count, bool integer_to_float, zend_object *target, const char **raw_error);
