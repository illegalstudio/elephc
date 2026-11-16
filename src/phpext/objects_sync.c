/* Property transport never borrows a Zend allocation while the native image is active. */
static zval *ep_object_helper(ep_call *call, ep_native_function function, const zval *const *arguments)
{
    zval *saved = call->result;
    bool old = call->object_transport;
    call->object_transport = true;
    call->result = NULL;
    function(arguments);
    zval *result = call->result;
    call->result = saved;
    call->object_transport = old;
    if (!result && !call->error) { call->error = "PHP extension object synchronization failed"; }
    return result;
}

/* Relative property names denote the declaring scope, not one shared class called self. */
static zend_string *ep_object_property_type_name(const zend_property_info *property)
{
    zend_string *name = ZEND_TYPE_NAME(property->type);
    if (zend_string_equals_literal_ci(name, "self")) { return property->ce->name; }
    if (zend_string_equals_literal_ci(name, "parent") && property->ce->parent) { return property->ce->parent->name; }
    return name;
}

/* External aliases may share a property's exact type, but cannot join two native storage slots. */
static bool ep_object_property_reference_supported(ep_call *call, ep_object_lease *lease,
    size_t property, const zval *slot)
{
    if (!Z_ISREF_P(slot)) { return true; }
    uint32_t offset = lease->type->properties[property].offset;
    zend_property_info *own = lease->type->ce->properties_info_table[OBJ_PROP_TO_NUM(offset)];
    size_t native_sources = 0;
    zend_property_info *source;
    ZEND_REF_FOREACH_TYPE_SOURCES(Z_REF_P(slot), source) {
        zend_type actual = source->type;
        zend_type expected = own->type;
        bool same = ZEND_TYPE_FULL_MASK(actual) == ZEND_TYPE_FULL_MASK(expected)
            && !ZEND_TYPE_HAS_LIST(actual) && !ZEND_TYPE_HAS_LIST(expected);
        if (same && ZEND_TYPE_HAS_NAME(actual)) {
            same = zend_string_equals_ci(ep_object_property_type_name(source), ep_object_property_type_name(own));
        }
        if (!same) {
            call->error = "PHP extension property references require identical declared types";
            return false;
        }
        for (size_t i = 0; i < sizeof(ep_classes) / sizeof(ep_classes[0]); i++) {
            if (source->ce == ep_classes[i].ce) { native_sources++; break; }
        }
    } ZEND_REF_FOREACH_TYPE_SOURCES_END();
    if (native_sources != 1) {
        call->error = "PHP extension properties cannot share a reference cell with another exported property";
        return false;
    }
    return true;
}

/* Capture host property state only after the pre-binding walk has established every identity. */
static void ep_objects_prepare(ep_call *call)
{
    ep_object_request *request = EP_GLOBAL(objects);
    if (!request) { return; }
    for (ep_object_lease *lease = request->leases; lease && !call->error; lease = lease->next) {
        if (!lease->wrapper || !lease->native_owner || lease->pending_release) { continue; }
        ep_object_change *change = ep_object_change_for(call, lease, true);
        if (!change) { return; }
        size_t count = lease->type->property_count;
        change->properties = ep_object_alloc(call, count * sizeof(*change->properties));
        change->initialized = ep_object_alloc(call, count * sizeof(*change->initialized));
        if (!change->properties || !change->initialized) { return; }
        for (size_t i = 0; i < count; i++) {
            zval *value = OBJ_PROP(lease->wrapper, lease->type->properties[i].offset);
            if (!ep_object_property_reference_supported(call, lease, i, value)) { return; }
            ZVAL_DEREF(value);
            change->initialized[i] = Z_TYPE_P(value) != IS_UNDEF;
            if (change->initialized[i]) {
                change->properties[i] = ep_object_copy_value(call, value, true);
                if (!change->properties[i]) { return; }
            }
        }
        change->prepared = true;
        change->captured = false;
    }
}

/* Release intent uses the existing lease, without allocating argument, result or journal boxes. */
static void ep_objects_release_pending(ep_call *call)
{
    ep_object_request *request = EP_GLOBAL(objects);
    if (!request) { return; }
    bool released = false;
    for (ep_object_lease *lease = request->leases; lease && !call->error; lease = lease->next) {
        if (!lease->pending_release || !lease->native_owner || ep_object_released(call, lease)) { continue; }
        zval pointer;
        ZVAL_LONG(&pointer, (zend_long)(uintptr_t)lease->native);
        const zval *arguments[] = {&pointer};
        if (!ep_object_helper(call, lease->type->release, arguments)) { return; }
        lease->release_staged_by = call;
        released = true;
    }
    if (released && !call->error) { (void)ep_object_helper(call, ep_classes[0].collect, NULL); }
}

/* Writes use the ordinary EIR property ABI, including COW, reference ownership and init markers. */
static void ep_objects_import(ep_call *call)
{
    for (ep_object_change *change = call->object_changes; change && !call->error; change = change->next) {
        ep_object_lease *lease = change->lease;
        if (!change->prepared || ep_object_released(call, lease) || !lease->wrapper || lease->pending_release) { continue; }
        zval object;
        ZVAL_OBJ(&object, (zend_object *)lease->native);
        for (size_t i = 0; i < lease->type->property_count; i++) {
            const zval *arguments[] = {&object, change->properties[i]};
            ep_native_function function = change->initialized[i] ? lease->type->properties[i].write : lease->type->properties[i].clear;
            if (!ep_object_helper(call, function, arguments)) { return; }
        }
        change->prepared = false;
    }
}

static void ep_object_capture_fields(ep_call *call, ep_object_change *change)
{
    ep_object_lease *lease = change->lease;
    size_t count = lease->type->property_count;
    change->properties = ep_object_alloc(call, count * sizeof(*change->properties));
    change->initialized = ep_object_alloc(call, count * sizeof(*change->initialized));
    if (!change->properties || !change->initialized) { return; }
    zval object;
    ZVAL_OBJ(&object, (zend_object *)lease->native);
    const zval *arguments[] = {&object};
    for (size_t i = 0; i < count; i++) {
        zval *initialized = ep_object_helper(call, lease->type->properties[i].initialized, arguments);
        if (!initialized) { return; }
        change->initialized[i] = Z_TYPE_P(initialized) == IS_TRUE;
        if (change->initialized[i]) {
            change->properties[i] = ep_object_helper(call, lease->type->properties[i].read, arguments);
            if (!change->properties[i]) { return; }
        }
    }
    change->captured = true;
}

/* Field capture extends this worklist when a previously unseen child object crosses the boundary. */
static bool ep_objects_capture_unseen(ep_call *call)
{
    ep_object_request *request = EP_GLOBAL(objects);
    if (!request) { return false; }
    bool captured = false;
    for (ep_object_lease *lease = request->leases; lease && !call->error; lease = lease->next) {
        if (!lease->native_owner || lease->pending_release || ep_object_released(call, lease)) { continue; }
        ep_object_change *change = ep_object_change_for(call, lease, true);
        if (!change) { return false; }
        if (!change->captured) {
            ep_object_capture_fields(call, change);
            captured = true;
        }
    }
    return captured;
}

/* Getter temporary owners are gone before classifying independently native-owned objects. */
static void ep_objects_capture(ep_call *call)
{
    if (!EP_GLOBAL(objects)) { return; }
    while (!call->error && ep_objects_capture_unseen(call)) {}
    while (!call->error) {
        if (!ep_object_helper(call, ep_classes[0].roots, NULL)) { return; }
        if (!ep_objects_capture_unseen(call)) { return; }
        while (!call->error && ep_objects_capture_unseen(call)) {}
    }
}
