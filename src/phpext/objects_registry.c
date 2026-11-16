/* A sidecar lease preserves identity without changing standard Zend object allocation. */
typedef struct ep_object_value {
    struct ep_object_value *next;
    uint64_t owner;
    zval value;
} ep_object_value;

struct ep_object_lease {
    ep_object_lease *next;
    ep_object_lease *previous;
    ep_object_lease *native_next;
    ep_object_lease *stage_next;
    ep_call *staged_by;
    ep_call *release_staged_by;
    ep_object_request *request;
    ep_class_descriptor *type;
    const void *native;
    zend_object *wrapper;
    bool native_owner;
    bool native_root;
    bool pending_release;
};

struct ep_object_change {
    ep_object_change *next;
    ep_object_lease *lease;
    zval **properties;
    bool *initialized;
    bool prepared;
    bool captured;
    bool root_known;
    bool rooted;
    ep_object_value **host_properties;
};

struct ep_object_request {
    ep_object_lease *leases;
    ep_object_index native;
    ep_object_index wrappers;
    ep_object_value *values;
    uint64_t generation;
    size_t retiring;
};

extern void elephc_php_native_object_retain(const void *object);

static ep_object_request *ep_objects_request(ep_call *call)
{
    if (!EP_GLOBAL(objects)) {
        ep_object_request *request = calloc(1, sizeof(*request));
        if (!request) { call->error = "PHP object identity registry allocation failed"; return NULL; }
        request->generation = EP_GLOBAL(generation);
        EP_GLOBAL(objects) = request;
    }
    return EP_GLOBAL(objects);
}

static ep_class_descriptor *ep_object_native_class(const void *native)
{
    if (!native) { return NULL; }
    uintptr_t id = *(const uintptr_t *)native;
    for (size_t i = 0; i < sizeof(ep_classes) / sizeof(ep_classes[0]); i++) {
        if (ep_classes[i].native_id == id) { return &ep_classes[i]; }
    }
    return NULL;
}

static ep_object_change *ep_object_change_for(ep_call *call, ep_object_lease *lease, bool create)
{
    if (!call) { return NULL; }
    for (ep_object_change *change = call->object_changes; change; change = change->next) {
        if (change->lease == lease) { return change; }
    }
    if (!create) { return NULL; }
    bool old = call->object_transport;
    call->object_transport = true;
    ep_object_change *change = ep_alloc(call, sizeof(*change));
    call->object_transport = old;
    if (!change) { return NULL; }
    change->lease = lease;
    change->next = call->object_changes;
    call->object_changes = change;
    return change;
}

/* Sync copies are retired at every host boundary rather than growing with output volume. */
static void ep_objects_reset_transport(ep_call *call)
{
    ep_allocation *allocation = call->object_allocations;
    while (allocation) {
        ep_allocation *next = allocation->next;
        free(allocation);
        allocation = next;
    }
    call->object_allocations = NULL;
    call->transport_bytes -= call->object_transport_bytes;
    call->object_transport_bytes = 0;
    call->object_changes = NULL;
}

static void *ep_object_alloc(ep_call *call, size_t size)
{
    bool old = call->object_transport;
    call->object_transport = true;
    void *result = ep_alloc(call, size);
    call->object_transport = old;
    return result;
}

static zval *ep_object_copy_value(ep_call *call, const zval *source, bool host)
{
    bool old = call->object_transport;
    call->object_transport = true;
    zval *result = ep_copy(call, source, host, 0);
    call->object_transport = old;
    return result;
}

static void ep_objects_note_publication(ep_call *call)
{
    for (; call; call = call->parent) { call->objects_published = true; }
}

static bool ep_object_released(ep_call *call, ep_object_lease *lease)
{
    (void)call;
    return lease->release_staged_by != NULL;
}

/* A staged lease can precede its Zend wrapper; dead wrappers have pending_release set. */
static ep_object_lease *ep_object_find_native(ep_call *call, const void *native)
{
    ep_object_request *request = EP_GLOBAL(objects);
    if (!request) { return NULL; }
    ep_object_lease *lease = ep_object_index_find(&request->native, (uintptr_t)native);
    for (; lease; lease = lease->native_next) {
        if (lease->native_owner && !lease->pending_release && !ep_object_released(call, lease)) { return lease; }
    }
    return NULL;
}

static ep_object_lease *ep_object_new_lease(ep_call *call, const void *native, ep_class_descriptor *type)
{
    ep_object_request *request = ep_objects_request(call);
    if (!request) { return NULL; }
    ep_object_lease *lease = calloc(1, sizeof(*lease));
    if (!lease) { call->error = "PHP object lease allocation failed"; return NULL; }
    lease->native_next = ep_object_index_find(&request->native, (uintptr_t)native);
    if (!ep_object_index_put(call, &request->native, (uintptr_t)native, lease)) { free(lease); return NULL; }
    lease->request = request;
    lease->type = type;
    lease->native = native;
    lease->native_owner = true;
    lease->staged_by = call;
    lease->stage_next = call->object_new_leases;
    call->object_new_leases = lease;
    lease->next = request->leases;
    if (request->leases) { request->leases->previous = lease; }
    request->leases = lease;
    elephc_php_native_object_retain(native);
    return lease;
}

/* This query runs inside native marking and performs no Zend API operation or allocation. */
uintptr_t elephc_php_native_wrapper_owner(const void *native)
{
    ep_object_request *request = EP_GLOBAL(objects);
    if (!request) { return 0; }
    uintptr_t count = 0;
    ep_object_lease *lease = ep_object_index_find(&request->native, (uintptr_t)native);
    for (; lease; lease = lease->native_next) {
        if (lease->native_owner && !ep_object_released(ep_current, lease)) { count++; }
    }
    return count;
}

bool elephc_php_native_object_is_mirrored(const void *native)
{
    return ep_object_find_native(ep_current, native) != NULL;
}

/* Publish only independently native-owned objects; their Zend properties own their children. */
void elephc_php_native_object_root(const void *native, bool rooted)
{
    ep_call *call = ep_current;
    if (call->error) { return; }
    ep_class_descriptor *type = ep_object_native_class(native);
    if (!type) { return; }
    ep_object_lease *lease = ep_object_find_native(call, native);
    if (!lease) { return; }
    ep_object_change *change = ep_object_change_for(call, lease, true);
    if (change) { change->root_known = true; change->rooted = rooted; }
}

static zval *ep_object_copy(ep_call *call, const zval *source, bool host)
{
    const void *native;
    if (host) {
        ep_object_request *request = EP_GLOBAL(objects);
        ep_object_lease *lease = request ? ep_object_index_find(&request->wrappers, (uintptr_t)Z_OBJ_P(source)) : NULL;
        if (!lease || Z_OBJ_HT_P(source) != &ep_object_handlers || !lease->native_owner
            || lease->request->generation != EP_GLOBAL(generation) || lease->pending_release) {
            call->error = "PHP extension object is not bound to this module and request";
            return NULL;
        }
        native = lease->native;
    } else {
        native = Z_OBJ_P(source);
        ep_class_descriptor *type = ep_object_native_class(native);
        if (!type) { call->error = "PHP extension returned an object whose class is not exported"; return NULL; }
        if (call->object_raw && call->object_target && call->object_target->ce != type->ce) {
            call->error = "PHP extension native object allocation returned the wrong class";
            return NULL;
        }
        if (!ep_object_find_native(call, native) && !ep_object_new_lease(call, native, type)) { return NULL; }
    }
    zval *copy = ep_alloc(call, sizeof(*copy));
    if (copy) { ZVAL_OBJ(copy, (zend_object *)native); }
    return copy;
}

/* Native ownership is already committed or rolled back before removing an identity record. */
static void ep_object_unlink(ep_object_lease *lease)
{
    ep_object_request *request = lease->request;
    if (lease->previous) { lease->previous->next = lease->next; }
    else { request->leases = lease->next; }
    if (lease->next) { lease->next->previous = lease->previous; }
    ep_object_lease *head = ep_object_index_find(&request->native, (uintptr_t)lease->native);
    if (head == lease) {
        if (lease->native_next) { ep_object_index_replace(&request->native, (uintptr_t)lease->native, lease->native_next); }
        else { ep_object_index_remove(&request->native, (uintptr_t)lease->native); }
    } else {
        while (head && head->native_next != lease) { head = head->native_next; }
        if (head) { head->native_next = lease->native_next; }
    }
    free(lease);
}

static void ep_objects_rollback(ep_call *call)
{
    ep_object_lease *lease = call->object_new_leases;
    call->object_new_leases = NULL;
    while (lease) {
        ep_object_lease *next = lease->stage_next;
        ep_object_unlink(lease);
        lease = next;
    }
    ep_object_request *request = EP_GLOBAL(objects);
    if (request) {
        for (lease = request->leases; lease; lease = lease->next) {
            if (lease->release_staged_by == call) { lease->release_staged_by = NULL; }
        }
    }
    call->object_changes = NULL;
}

static void ep_objects_commit(ep_call *call)
{
    ep_object_lease *lease = call->object_new_leases;
    call->object_new_leases = NULL;
    while (lease) {
        ep_object_lease *next = lease->stage_next;
        lease->staged_by = NULL;
        lease->stage_next = NULL;
        lease = next;
    }
    ep_object_request *request = EP_GLOBAL(objects);
    if (request) {
        for (lease = request->leases; lease; lease = lease->next) {
            if (lease->release_staged_by == call) {
                lease->native_owner = false;
                lease->release_staged_by = NULL;
            }
        }
    }
}
