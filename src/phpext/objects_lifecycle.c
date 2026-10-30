/* Bound objects use standard Zend storage, including its normal GC property traversal. */
static void ep_object_free(zend_object *object)
{
    ep_object_request *request = EP_GLOBAL(objects);
    ep_object_lease *lease = request ? ep_object_index_find(&request->wrappers, (uintptr_t)object) : NULL;
    if (lease) {
        ep_object_index_remove(&request->wrappers, (uintptr_t)object);
        lease->wrapper = NULL;
        lease->pending_release = true;
    }
    zend_object_std_dtor(object);
}

/* A standard shallow clone is bound lazily, preserving properties and external references. */
static zend_object *ep_object_clone(zend_object *source)
{
    zend_object *clone = zend_objects_clone_obj(source);
    clone->handlers = zend_get_std_object_handlers();
    return clone;
}

static ep_class_descriptor *ep_object_host_class(zend_object *object)
{
    for (size_t i = 0; i < sizeof(ep_classes) / sizeof(ep_classes[0]); i++) {
        if (ep_classes[i].ce == object->ce) { return &ep_classes[i]; }
    }
    return NULL;
}

/* The queue is Zend-owned and is built before any malloc-owned ordinary call exists. */
static void ep_objects_enqueue(HashTable *queue, const zval *source)
{
    zval copy;
    if (Z_ISREF_P(source)) { source = Z_REFVAL_P(source); }
    if (Z_TYPE_P(source) != IS_ARRAY && Z_TYPE_P(source) != IS_OBJECT) { return; }
    ZVAL_COPY(&copy, source);
    zend_hash_next_index_insert(queue, &copy);
}

static const char *ep_objects_bind_graph(zval *values, size_t count)
{
    HashTable queue, seen;
    zend_hash_init(&queue, 8, NULL, ZVAL_PTR_DTOR, false);
    zend_hash_init(&seen, 8, NULL, NULL, false);
    for (size_t i = 0; i < count; i++) { ep_objects_enqueue(&queue, &values[i]); }
    ep_object_request *request = EP_GLOBAL(objects);
    if (request) {
        for (ep_object_lease *lease = request->leases; lease; lease = lease->next) {
            if (lease->wrapper) {
                zval object;
                ZVAL_OBJ(&object, lease->wrapper);
                ep_objects_enqueue(&queue, &object);
            }
        }
    }
    const char *error = NULL;
    for (uint32_t i = 0; i < queue.nNumUsed && !error; i++) {
        /* Queue growth may move its packed storage; never retain a pointer into it. */
        zval value;
        ZVAL_COPY_VALUE(&value, zend_hash_index_find(&queue, i));
        uintptr_t key;
        if (Z_TYPE(value) == IS_ARRAY) { key = (uintptr_t)Z_ARR(value) | 1; }
        else if (Z_TYPE(value) == IS_OBJECT) { key = (uintptr_t)Z_OBJ(value) | 2; }
        else { continue; }
        if (zend_hash_index_exists(&seen, key)) { continue; }
        zval visited;
        ZVAL_TRUE(&visited);
        zend_hash_index_add_new(&seen, key, &visited);
        if (Z_TYPE(value) == IS_ARRAY) {
            zval *child;
            ZEND_HASH_FOREACH_VAL(Z_ARR(value), child) { ep_objects_enqueue(&queue, child); } ZEND_HASH_FOREACH_END();
            continue;
        }
        ep_class_descriptor *type = ep_object_host_class(Z_OBJ(value));
        if (!type) { continue; }
        request = EP_GLOBAL(objects);
        if (!request || !ep_object_index_find(&request->wrappers, (uintptr_t)Z_OBJ(value))) {
            zval result;
            ZVAL_UNDEF(&result);
            ep_invoke_ex(NULL, &result, type->allocate, NULL, 0, false, Z_OBJ(value), &error);
            if (!Z_ISUNDEF(result)) { zval_ptr_dtor(&result); }
            if (error) { break; }
        }
        for (size_t property = 0; property < type->property_count; property++) {
            zval *slot = OBJ_PROP(Z_OBJ(value), type->properties[property].offset);
            if (Z_TYPE_P(slot) != IS_UNDEF) { ep_objects_enqueue(&queue, slot); }
        }
    }
    zend_hash_destroy(&seen);
    zend_hash_destroy(&queue);
    return error;
}
