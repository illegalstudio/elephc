/* Zend allocations and destructors occur only while the native image is parked. */
static ep_object_value *ep_object_host_value(ep_call *call)
{
    ep_object_request *request = EP_GLOBAL(objects);
    ep_object_value *value = calloc(1, sizeof(*value));
    if (!value) { zend_error_noreturn(E_ERROR, "PHP extension host object storage allocation failed"); }
    value->owner = call ? call->object_identity : 0;
    ZVAL_UNDEF(&value->value);
    value->next = request->values;
    request->values = value;
    return value;
}

/* A failed host allocation cannot leave a catchable, partially published native graph. */
static void ep_object_attach(ep_call *call, ep_object_lease *lease, zend_object *object)
{
    if (!ep_object_index_put(call, &lease->request->wrappers, (uintptr_t)object, lease)) {
        zend_error_noreturn(E_ERROR, "%s", call->error);
    }
    lease->wrapper = object;
    object->handlers = &ep_object_handlers;
    ep_objects_note_publication(call);
}

static zend_object *ep_object_wrapper(ep_call *call, ep_object_lease *lease)
{
    if (lease->wrapper) { return lease->wrapper; }
    if (call->object_raw && call->object_target) {
        ep_object_attach(call, lease, call->object_target);
        return lease->wrapper;
    }
    ep_object_value *value = ep_object_host_value(call);
    if (object_init_ex(&value->value, lease->type->ce) != SUCCESS) {
        zend_error_noreturn(E_ERROR, "PHP extension host object allocation failed");
    }
    ep_object_attach(call, lease, Z_OBJ(value->value));
    return lease->wrapper;
}

static void ep_object_to_zend(ep_call *call, const zval *source, zval *destination)
{
    ep_object_lease *lease = ep_object_find_native(call, Z_OBJ_P(source));
    if (!lease) { zend_error_noreturn(E_ERROR, "PHP extension lost a transported object identity"); }
    ZVAL_OBJ_COPY(destination, ep_object_wrapper(call, lease));
}

/* Materialize every field before changing any slot; object cycles use the identity index. */
static void ep_objects_materialize(ep_call *call)
{
    for (ep_object_change *change = call->object_changes; change; change = change->next) {
        ep_object_lease *lease = change->lease;
        if (!change->captured || ep_object_released(call, lease) || lease->pending_release) { continue; }
        (void)ep_object_wrapper(call, lease);
        size_t count = lease->type->property_count;
        change->host_properties = ep_object_alloc(call, count * sizeof(*change->host_properties));
        if (!change->host_properties) { zend_error_noreturn(E_ERROR, "%s", call->error); }
        for (size_t i = 0; i < count; i++) {
            ep_object_value *value = ep_object_host_value(call);
            change->host_properties[i] = value;
            if (change->initialized[i]) { ep_to_zend(call, change->properties[i], &value->value); }
        }
    }
}

/* An unset detaches this typed property's source while preserving any external alias. */
static void ep_objects_replace_properties(ep_call *call)
{
    for (ep_object_change *change = call->object_changes; change; change = change->next) {
        ep_object_lease *lease = change->lease;
        if (!change->host_properties) { continue; }
        for (size_t i = 0; i < lease->type->property_count; i++) {
            uint32_t offset = lease->type->properties[i].offset;
            zval *slot = OBJ_PROP(lease->wrapper, offset);
            ep_object_value *value = change->host_properties[i];
            zval *target = slot;
            if (Z_ISREF_P(slot)) {
                if (change->initialized[i]) { target = Z_REFVAL_P(slot); }
                else {
                    zend_property_info *property = lease->type->ce->properties_info_table[OBJ_PROP_TO_NUM(offset)];
                    ZEND_REF_DEL_TYPE_SOURCE(Z_REF_P(slot), property);
                }
            }
            zval old;
            ZVAL_COPY_VALUE(&old, target);
            ZVAL_COPY_VALUE(target, &value->value);
            ZVAL_COPY_VALUE(&value->value, &old);
            Z_PROP_FLAG_P(slot) = change->initialized[i] ? 0 : IS_PROP_UNINIT;
        }
        ep_objects_note_publication(call);
    }
}

/* Add every new root before scheduling the old roots for destructor-safe retirement. */
static void ep_objects_replace_roots(ep_call *call)
{
    for (ep_object_change *change = call->object_changes; change; change = change->next) {
        ep_object_lease *lease = change->lease;
        if (change->root_known && change->rooted && !lease->native_root && lease->wrapper) {
            GC_ADDREF(lease->wrapper);
            lease->native_root = true;
        }
    }
    for (ep_object_change *change = call->object_changes; change; change = change->next) {
        ep_object_lease *lease = change->lease;
        if (change->root_known && !change->rooted && lease->native_root) {
            ep_object_value *value = ep_object_host_value(call);
            ZVAL_OBJ(&value->value, lease->wrapper);
            lease->native_root = false;
        }
    }
}

static void ep_objects_publish(ep_call *call)
{
    if (call->object_raw) { return; }
    ep_objects_materialize(call);
    ep_objects_replace_properties(call);
    ep_objects_replace_roots(call);
}

/* A parked ancestor can retain a journal even after a nested call has committed its release. */
static bool ep_object_has_active_reference(ep_object_lease *lease)
{
    if (lease->staged_by || lease->release_staged_by) { return true; }
    for (ep_call *call = EP_GLOBAL(active); call; call = call->parent) {
        for (ep_object_change *change = call->object_changes; change; change = change->next) {
            if (change->lease == lease) { return true; }
        }
    }
    return false;
}

/* Completed callback journals no longer keep dead leases alive during long native calls. */
static void ep_objects_sweep(void)
{
    if (!EP_GLOBAL(objects)) { return; }
    ep_object_lease *lease = EP_GLOBAL(objects)->leases;
    while (lease) {
        ep_object_lease *next = lease->next;
        if (!lease->native_owner && !lease->wrapper && !ep_object_has_active_reference(lease)) { ep_object_unlink(lease); }
        lease = next;
    }
    ep_object_request *request = EP_GLOBAL(objects);
    if (!request->native.count) {
        free(request->native.entries);
        memset(&request->native, 0, sizeof(request->native));
    }
    if (!request->wrappers.count) {
        free(request->wrappers.entries);
        memset(&request->wrappers, 0, sizeof(request->wrappers));
    }
}

/* Pop and free each node before user WeakMap/destructor callbacks can reenter or bailout. */
static void ep_objects_retire(uint64_t identity)
{
    ep_object_request *request = EP_GLOBAL(objects);
    if (!request) { return; }
    request->retiring++;
    for (;;) {
        ep_object_value **cursor = &request->values;
        while (*cursor && (*cursor)->owner != identity) { cursor = &(*cursor)->next; }
        if (!*cursor) { break; }
        ep_object_value *value = *cursor;
        *cursor = value->next;
        zval retired;
        ZVAL_COPY_VALUE(&retired, &value->value);
        free(value);
        if (!Z_ISUNDEF(retired)) { zval_ptr_dtor(&retired); }
    }
    request->retiring--;
    ep_objects_sweep();
}

/* Published identities cannot be rolled back to a pre-entry image after host callbacks. */
static void ep_objects_break(ep_call *call)
{
    if (!call->error || !call->objects_published) { return; }
    EP_GLOBAL(broken) = true;
    for (; call; call = call->parent) {
        if (!call->error) { call->error = "PHP extension object context failed during an earlier call"; }
    }
}

/* Fatal teardown never calls Zend destructors, whose request storage may already be gone. */
static void ep_objects_abandon(ep_object_request *request)
{
    if (!request) { return; }
    for (ep_object_lease *lease = request->leases; lease;) {
        ep_object_lease *next = lease->next;
        if (lease->wrapper) { lease->wrapper->handlers = zend_get_std_object_handlers(); }
        free(lease);
        lease = next;
    }
    for (ep_object_value *value = request->values; value;) {
        ep_object_value *next = value->next;
        free(value);
        value = next;
    }
    free(request->native.entries);
    free(request->wrappers.entries);
    free(request);
}

/* Surviving PHP objects keep their standard properties after a native infrastructure failure. */
static void ep_objects_finish_broken(void)
{
    if (!EP_GLOBAL(broken) || EP_GLOBAL(active)) { return; }
    ep_object_request *request = EP_GLOBAL(objects);
    if (!request || request->retiring) { return; }
    for (ep_object_lease *lease = request->leases; lease; lease = lease->next) {
        if (!lease->wrapper) { continue; }
        if (lease->native_root) {
            ep_object_value *value = ep_object_host_value(NULL);
            ZVAL_OBJ(&value->value, lease->wrapper);
            lease->native_root = false;
        }
        zend_object *wrapper = lease->wrapper;
        wrapper->handlers = zend_get_std_object_handlers();
        ep_object_index_remove(&request->wrappers, (uintptr_t)wrapper);
        lease->wrapper = NULL;
    }
    while (request->leases) { ep_object_unlink(request->leases); }
    /* Keep request reachable until every possible destructor has finished. */
    while (request->values) { ep_objects_retire(request->values->owner); }
    ep_objects_abandon(request);
    EP_GLOBAL(objects) = NULL;
    free(EP_GLOBAL(snapshot));
    EP_GLOBAL(snapshot) = NULL;
}

/* Keep native roots through parked Fiber unwinds, then release them before Zend's final GC. */
static void ep_objects_shutdown(void)
{
    ep_object_request *request = EP_GLOBAL(objects);
    if (EP_GLOBAL(initialized) || EP_GLOBAL(active) || !request || request->retiring) { return; }
    for (ep_object_lease *lease = request->leases; lease; lease = lease->next) {
        if (lease->native_root && lease->wrapper) {
            ep_object_value *value = ep_object_host_value(NULL);
            ZVAL_OBJ(&value->value, lease->wrapper);
            lease->native_root = false;
        }
    }
    ep_objects_retire(0);
}
