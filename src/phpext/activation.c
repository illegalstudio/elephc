/* Native execution uses no Zend API. The mutex protects complete request snapshots. */
#include <pthread.h>
#include <Zend/zend_exceptions.h>

extern unsigned char elephc_state_begin[], elephc_state_end[];
extern unsigned char elephc_state_zero_begin[], elephc_state_zero_end[];
extern const uintptr_t ep_class_name_count __asm__("_class_name_count");
extern const uintptr_t ep_class_name_entries[] __asm__("_class_name_entries");
extern uintptr_t ep_native_stack_limit __asm__("_stack_limit");
extern uintptr_t ep_native_stack_limit_main __asm__("_stack_limit_main");
extern uintptr_t ep_boundary_status __asm__("_elephc_boundary_status");
extern uintptr_t ep_replacement_owner_count
    __asm__("_elephc_phpext_replacement_owner_count");
extern const uintptr_t *ep_replacement_owners[]
    __asm__("_elephc_phpext_replacement_owners");

typedef void (*ep_native_function)(const zval *const *);
static pthread_mutex_t ep_native_lock = PTHREAD_MUTEX_INITIALIZER;
static unsigned char *ep_template;
static size_t ep_data_size, ep_zero_size;
static ep_call *ep_current;

static void ep_insert_previous(ep_call *call, ep_exception_copy *copy,
    ep_host_predecessor *container);
static zend_object *ep_host_previous(zend_object *exception);

/* Initialized and zero-filled native storage need not be adjacent in the loaded module. */
static void ep_state_save(unsigned char *image)
{
    memcpy(image, elephc_state_begin, ep_data_size);
    memcpy(image + ep_data_size, elephc_state_zero_begin, ep_zero_size);
}

static void ep_state_restore(const unsigned char *image)
{
    memcpy(elephc_state_begin, image, ep_data_size);
    memcpy(elephc_state_zero_begin, image + ep_data_size, ep_zero_size);
}

/* The initial zero region is implicit, so MINIT stores only initialized bytes. */
static void ep_state_reset(void)
{
    memcpy(elephc_state_begin, ep_template, ep_data_size);
    memset(elephc_state_zero_begin, 0, ep_zero_size);
}

static int ep_module_startup(void)
{
    ep_data_size = (uintptr_t)elephc_state_end - (uintptr_t)elephc_state_begin;
    ep_zero_size = (uintptr_t)elephc_state_zero_end - (uintptr_t)elephc_state_zero_begin;
    if (ep_zero_size > SIZE_MAX - ep_data_size) { return FAILURE; }
    ep_state_size = ep_data_size + ep_zero_size;
    ep_template = malloc(ep_data_size ? ep_data_size : 1);
    if (!ep_template) { return FAILURE; }
    memcpy(ep_template, elephc_state_begin, ep_data_size);
    return SUCCESS;
}

/* Zend knows the current thread or Fiber stack, unlike process RLIMIT_STACK. */
static uintptr_t ep_stack_floor(void)
{
    if (EG(stack_limit)) { return (uintptr_t)EG(stack_limit); }
#if defined(__APPLE__)
    uintptr_t top = (uintptr_t)pthread_get_stackaddr_np(pthread_self());
    size_t size = pthread_get_stacksize_np(pthread_self());
    return size > 65536 && top > size ? top - size + 65536 : 0;
#elif defined(__linux__)
    pthread_attr_t attributes;
    void *base = NULL;
    size_t size = 0;
    if (pthread_getattr_np(pthread_self(), &attributes)) { return 0; }
    int result = pthread_attr_getstack(&attributes, &base, &size);
    pthread_attr_destroy(&attributes);
    return !result && size > 65536 ? (uintptr_t)base + 65536 : 0;
#else
    return 0;
#endif
}

/* This callback only reads the activation record and cannot allocate or bailout. */
uintptr_t elephc_php_stack_floor(void)
{
    return ep_current->stack_floor;
}

ZEND_BEGIN_MODULE_GLOBALS(@IDENT@)
    unsigned char *snapshot;
    ep_call *active;
    bool initialized;
ZEND_END_MODULE_GLOBALS(@IDENT@)
ZEND_DECLARE_MODULE_GLOBALS(@IDENT@)
#define EP_GLOBAL(member) ZEND_MODULE_GLOBALS_ACCESSOR(@IDENT@, member)

/* Requests that never invoke an export do not allocate or touch the native heap. */
static int ep_request_startup(void)
{
    ep_call_abandon_chain(EP_GLOBAL(active));
    free(EP_GLOBAL(snapshot));
    EP_GLOBAL(snapshot) = NULL;
    EP_GLOBAL(active) = NULL;
    EP_GLOBAL(initialized) = true;
    return SUCCESS;
}

/* Native warning lines become length-aware messages for the host error handler. */
static void ep_issue_warning(ep_call *call)
{
    const char *message = call->output;
    size_t length = call->output_len;
    int level = E_WARNING;
    size_t prefix = 0;
    if (length >= 9 && memcmp(message, "Warning: ", 9) == 0) { prefix = 9; }
    else if (length >= 8 && memcmp(message, "Notice: ", 8) == 0) { prefix = 8; level = E_NOTICE; }
    else if (length >= 12 && memcmp(message, "Deprecated: ", 12) == 0) { prefix = 12; level = E_DEPRECATED; }
    message += prefix;
    length -= prefix;
    while (length && message[length - 1] == '\n') { length--; }
    call->warning = zend_string_init(message, length, false);
    zend_error_zstr(level, call->warning);
}

/* A real exit remains authoritative; Fiber destruction uses ordinary finally semantics. */
static bool ep_is_host_exit(void)
{
    return EG(exception) && zend_is_unwind_exit(EG(exception));
}

/* Move Zend's saved outer escape into adapter storage without chaining it under the callback. */
static bool ep_park_saved_host_exception(ep_call *call, zend_object *saved)
{
    ep_parked_host_exception *parked = calloc(1, sizeof(*parked));
    if (!parked) { return false; }
    if (EG(prev_exception) == saved) {
        ZVAL_OBJ(&parked->exception, saved);
        EG(prev_exception) = NULL;
    } else {
        zend_object *cursor = EG(exception);
        while (cursor) {
            zend_object *previous = ep_host_previous(cursor);
            if (previous == saved) {
                GC_ADDREF(saved);
                ZVAL_OBJ(&parked->exception, saved);
                zend_update_property_null(zend_get_exception_base(cursor), cursor,
                    "previous", sizeof("previous") - 1);
                break;
            }
            cursor = previous;
        }
        if (Z_ISUNDEF(parked->exception)) {
            free(parked);
            return false;
        }
    }
    parked->previous_exceptions = call->previous_exceptions;
    parked->previous_exceptions_tail = call->previous_exceptions_tail;
    parked->previous_exception_anchor = call->previous_exception_anchor;
    parked->active_host_predecessor = call->active_host_predecessor;
    call->previous_exceptions = NULL;
    call->previous_exceptions_tail = NULL;
    call->previous_exception_anchor = NULL;
    call->active_host_predecessor = NULL;
    parked->next = call->parked_host_exceptions;
    call->parked_host_exceptions = parked;
    return true;
}

/* Restore only the immediate outer host escape after the newest callback escape settles. */
static bool ep_restore_parked_host_exception(ep_call *call)
{
    ep_parked_host_exception *parked = call->parked_host_exceptions;
    if (!parked || EG(exception)) { return false; }
    call->parked_host_exceptions = parked->next;
    EG(exception) = Z_OBJ(parked->exception);
    ZVAL_UNDEF(&parked->exception);
    if (!parked->state_activated) {
        call->previous_exceptions = parked->previous_exceptions;
        call->previous_exceptions_tail = parked->previous_exceptions_tail;
        call->previous_exception_anchor = parked->previous_exception_anchor;
        call->active_host_predecessor = parked->active_host_predecessor;
    } else if (call->discarded_host_exception_depth) {
        call->discarded_host_exception_depth--;
    }
    free(parked);
    call->host_exception = true;
    return true;
}

/* Merge native copies from an outer host level after its exception leaves that finally. */
static void ep_append_parked_exception_copies(ep_call *call,
    ep_parked_host_exception *parked)
{
    if (!parked->previous_exceptions) { return; }
    if (call->previous_exceptions_tail) {
        call->previous_exceptions_tail->next = parked->previous_exceptions;
    } else {
        call->previous_exceptions = parked->previous_exceptions;
    }
    call->previous_exceptions_tail = parked->previous_exceptions_tail;
}

/* Consume one outer level after the newest exception crosses that level's finally. */
static void ep_commit_parked_host_exception(ep_call *call)
{
    ep_parked_host_exception *parked = call->parked_host_exceptions;
    if (!parked) { return; }
    call->parked_host_exceptions = parked->next;
    ep_append_parked_exception_copies(call, parked);
    ep_host_predecessor *predecessor = parked->active_host_predecessor;
    if (predecessor) {
        if (!Z_ISUNDEF(predecessor->exception)) {
            zval_ptr_dtor(&predecessor->exception);
        }
        predecessor->exception = parked->exception;
        ZVAL_UNDEF(&parked->exception);
        predecessor->detach_pending = false;
    } else if (EG(exception)) {
        zend_exception_set_previous(EG(exception), Z_OBJ(parked->exception));
        ZVAL_UNDEF(&parked->exception);
    } else {
        EG(exception) = Z_OBJ(parked->exception);
        ZVAL_UNDEF(&parked->exception);
    }
    free(parked);
}

/* Commit only levels proven to have been crossed by the current native escape. */
static void ep_commit_crossed_host_exceptions(ep_call *call)
{
    while (call->parked_host_exceptions
        && call->parked_host_exceptions->crossed_finally) {
        ep_commit_parked_host_exception(call);
    }
    call->host_exception = EG(exception) != NULL;
}

/* Append every still-parked outer escape only when the newest exception leaves the export. */
static void ep_chain_parked_host_exceptions(ep_call *call)
{
    if (!EG(exception)) { ep_restore_parked_host_exception(call); }
    while (EG(exception) && call->parked_host_exceptions) {
        ep_commit_parked_host_exception(call);
    }
    call->host_exception = EG(exception) != NULL;
}

/* Return the newest replacement registered after any older record began settlement. */
static ep_host_predecessor *ep_logical_host_predecessor(ep_call *call)
{
    ep_host_predecessor *selected = call->active_host_predecessor;
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        ep_host_predecessor *candidate = &call->host_predecessors[i];
        if (candidate->activation_pending
            && (!selected || candidate->sequence > selected->sequence)) {
            selected = candidate;
        }
    }
    return selected;
}

/* Make the oldest queued replacement active after the preceding record settles. */
static void ep_activate_pending_host_predecessor(ep_call *call)
{
    if (call->active_host_predecessor) { return; }
    ep_host_predecessor *selected = NULL;
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        ep_host_predecessor *candidate = &call->host_predecessors[i];
        if (candidate->activation_pending
            && (!selected || candidate->sequence < selected->sequence)) {
            selected = candidate;
        }
    }
    if (selected) {
        selected->activation_pending = false;
        call->active_host_predecessor = selected;
    }
}

/* Advance native metadata to the parked level that resumes after a deferred host discard. */
static void ep_defer_host_exception_discard(ep_call *call)
{
    if (!call->host_exception) { return; }
    ep_parked_host_exception *resumed = call->parked_host_exceptions;
    for (size_t skip = call->discarded_host_exception_depth;
        skip && resumed; skip--) {
        resumed = resumed->next;
    }
    call->discarded_host_exception_depth++;
    while (resumed && resumed->crossed_finally) {
        call->discarded_host_exception_depth++;
        resumed = resumed->next;
    }
    call->previous_exceptions = NULL;
    call->previous_exceptions_tail = NULL;
    call->previous_exception_anchor = NULL;
    call->active_host_predecessor = NULL;
    if (resumed) {
        call->previous_exceptions = resumed->previous_exceptions;
        call->previous_exceptions_tail = resumed->previous_exceptions_tail;
        call->previous_exception_anchor = resumed->previous_exception_anchor;
        call->active_host_predecessor = resumed->active_host_predecessor;
        resumed->previous_exceptions = NULL;
        resumed->previous_exceptions_tail = NULL;
        resumed->previous_exception_anchor = NULL;
        resumed->active_host_predecessor = NULL;
        resumed->state_activated = true;
    }
}

/* Keep Zend's caller-owned saved slot outside this adapter's save/restore pair. */
static void ep_stash_ambient_prev_exception(ep_call *call)
{
    call->ambient_prev_exception = EG(prev_exception);
    call->ambient_prev_exception_stashed = true;
    EG(prev_exception) = NULL;
}

/* Finish any adapter-owned save before returning the caller's saved slot unchanged. */
static void ep_restore_ambient_prev_exception(ep_call *call)
{
    if (!call->ambient_prev_exception_stashed) { return; }
    if (EG(prev_exception)) { zend_exception_restore(); }
    EG(prev_exception) = call->ambient_prev_exception;
    call->ambient_prev_exception = NULL;
    call->ambient_prev_exception_stashed = false;
}

/* A nested export returns the parent's saved callback exception only at callback completion. */
static void ep_restore_nested_prev_exception(ep_call *call)
{
    if (!call->nested_prev_exception) { return; }
    if (EG(prev_exception)) { zend_exception_restore(); }
    EG(prev_exception) = call->nested_prev_exception;
    call->nested_prev_exception = NULL;
}

/* A return or infrastructure failure overrides every parked host escape in this invocation. */
static void ep_clear_parked_host_exceptions(ep_call *call)
{
    ep_parked_host_exception *parked = call->parked_host_exceptions;
    call->parked_host_exceptions = NULL;
    while (parked) {
        ep_parked_host_exception *next = parked->next;
        zval exception = parked->exception;
        free(parked);
        zval_ptr_dtor(&exception);
        parked = next;
    }
}

/* Infrastructure statuses remain authoritative across native finally return and throw. */
static const char *ep_infrastructure_error(uintptr_t status)
{
    switch (status) {
        case 1: return "PHP extension result exceeds the maximum nesting depth (64)";
        case 3: return "elephc heap allocation failed";
        case 4: return "elephc native runtime failed";
        case 6: return "PHP extension exception chain exceeds the maximum live replacement pin count (64)";
        default: return NULL;
    }
}

/* Read the compact runtime refcount without depending on its private allocation header type. */
static uint32_t ep_native_refcount(const uintptr_t *object)
{
    uint32_t refcount;
    memcpy(&refcount, (const unsigned char *)object - 12, sizeof(refcount));
    return refcount & UINT32_C(0x7fffffff);
}

/* Count every adapter pin for one native identity across the synchronous export stack. */
static size_t ep_replacement_pin_count(const uintptr_t *object)
{
    size_t count = ep_replacement_owner_count;
    if (count > EP_HOST_PREDECESSOR_LIMIT) { count = EP_HOST_PREDECESSOR_LIMIT; }
    size_t pins = 0;
    for (size_t i = 0; i < count; i++) {
        if (ep_replacement_owners[i] == object) { pins++; }
    }
    return pins;
}

/* Repeated replacement events for one live native object share one non-reusable identity. */
static uint64_t ep_root_identity(ep_call *call, const uintptr_t *object)
{
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        ep_host_predecessor *predecessor = &call->host_predecessors[i];
        if (predecessor->root == object) { return predecessor->root_identity; }
    }
    return 0;
}

/* An identity remains relevant while at least one replacement record still pins its root. */
static bool ep_root_identity_is_live(ep_call *call, uint64_t identity)
{
    if (!identity) { return false; }
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        ep_host_predecessor *predecessor = &call->host_predecessors[i];
        if (predecessor->root && predecessor->root_identity == identity) { return true; }
    }
    return false;
}

/* Track native identities already materialized inside one displaced host exception chain. */
static void ep_record_contained_root(ep_call *call, ep_host_predecessor *predecessor,
    uint64_t identity)
{
    if (!predecessor || !identity || call->error) { return; }
    size_t write = 0;
    bool present = false;
    for (size_t read = 0; read < predecessor->contained_root_identity_count; read++) {
        uint64_t candidate = predecessor->contained_root_identities[read];
        if (!ep_root_identity_is_live(call, candidate)) { continue; }
        predecessor->contained_root_identities[write++] = candidate;
        if (candidate == identity) { present = true; }
    }
    predecessor->contained_root_identity_count = write;
    if (present) { return; }
    if (write >= EP_HOST_PREDECESSOR_LIMIT) {
        call->error = "PHP extension exception identity tracking overflow";
        return;
    }
    predecessor->contained_root_identities[write] = identity;
    predecessor->contained_root_identity_count = write + 1;
}

/* Preserve the native identities already reachable through an attached host predecessor. */
static void ep_merge_contained_roots(ep_call *call, ep_host_predecessor *destination,
    const ep_host_predecessor *source)
{
    if (!destination || destination == source) { return; }
    for (size_t i = 0; i < source->contained_root_identity_count; i++) {
        ep_record_contained_root(call, destination, source->contained_root_identities[i]);
    }
}

/* Detect the Zend cycle case using the stable identity of the original native object. */
static bool ep_predecessor_contains_own_root(const ep_host_predecessor *predecessor)
{
    if (!predecessor->root_identity) { return false; }
    for (size_t i = 0; i < predecessor->contained_root_identity_count; i++) {
        if (predecessor->contained_root_identities[i] == predecessor->root_identity) {
            return true;
        }
    }
    return false;
}

/* Clear all identity and lifecycle metadata after a replacement record is settled. */
static void ep_reset_host_predecessor(ep_host_predecessor *predecessor)
{
    predecessor->root = NULL;
    predecessor->tail = NULL;
    predecessor->sequence = 0;
    predecessor->root_identity = 0;
    predecessor->contained_root_identity_count = 0;
    predecessor->detach_pending = false;
    predecessor->discard_pending = false;
    predecessor->activation_pending = false;
}

/* Forget every replacement whose native root is now owned only by adapter pins. */
const uintptr_t *elephc_php_stale_finally_replacement(
    const uintptr_t *const *owners, size_t base, size_t count)
{
    ep_call *call = ep_current;
    if (!call || call->error || base > count) { return NULL; }
    for (size_t i = base; i < count; i++) {
        const uintptr_t *root = owners[i];
        if (!root) { continue; }
        size_t pins = 0;
        for (size_t j = base; j < count; j++) {
            if (owners[j] == root) { pins++; }
        }
        if (ep_native_refcount(root) != pins) { continue; }

        ep_retired_host_exception *retired[EP_HOST_PREDECESSOR_LIMIT];
        size_t retired_count = 0;
        for (size_t j = 0; j < call->host_predecessor_count; j++) {
            ep_host_predecessor *predecessor = &call->host_predecessors[j];
            if (predecessor->root != root || Z_ISUNDEF(predecessor->exception)) { continue; }
            ep_retired_host_exception *node = calloc(1, sizeof(*node));
            if (!node) {
                for (size_t k = 0; k < retired_count; k++) { free(retired[k]); }
                call->error = "PHP extension replacement retirement allocation failed";
                return NULL;
            }
            retired[retired_count++] = node;
        }
        size_t retired_index = 0;
        for (size_t j = 0; j < call->host_predecessor_count; j++) {
            ep_host_predecessor *predecessor = &call->host_predecessors[j];
            if (predecessor->root != root) { continue; }
            if (!Z_ISUNDEF(predecessor->exception)) {
                ep_retired_host_exception *node = retired[retired_index++];
                node->exception = predecessor->exception;
                node->next = call->retired_host_exceptions;
                call->retired_host_exceptions = node;
                ZVAL_UNDEF(&predecessor->exception);
            }
            bool active = call->active_host_predecessor == predecessor;
            if (active && (predecessor->detach_pending || predecessor->discard_pending)) {
                ep_defer_host_exception_discard(call);
            } else if (active) {
                call->active_host_predecessor = NULL;
            }
            ep_reset_host_predecessor(predecessor);
        }
        ep_activate_pending_host_predecessor(call);
        return root;
    }
    return NULL;
}

/* Retire a host exception only after the native image is parked and Zend is callable. */
static void ep_clear_discarded_host_exception(ep_call *call)
{
    if (!call->discarded_host_exception_depth || ep_is_host_exit()) { return; }
    size_t depth = call->discarded_host_exception_depth;
    if (EG(exception)) { zend_clear_exception(); }
    while (depth > 1 && call->parked_host_exceptions) {
        ep_parked_host_exception *parked = call->parked_host_exceptions;
        call->parked_host_exceptions = parked->next;
        ep_host_predecessor *predecessor = parked->active_host_predecessor;
        if (predecessor) {
            zval stored = predecessor->exception;
            ZVAL_UNDEF(&predecessor->exception);
            ep_reset_host_predecessor(predecessor);
            if (!Z_ISUNDEF(stored)) { zval_ptr_dtor(&stored); }
        }
        zval exception = parked->exception;
        free(parked);
        zval_ptr_dtor(&exception);
        depth--;
        call->discarded_host_exception_depth--;
    }
    if (!ep_restore_parked_host_exception(call)) {
        call->discarded_host_exception_depth = 0;
        call->active_host_predecessor = NULL;
        call->previous_exceptions = NULL;
        call->previous_exceptions_tail = NULL;
        call->previous_exception_anchor = NULL;
    }
    call->host_exception = EG(exception) != NULL;
}

/* Destroy discarded replacement predecessors at the first safe Zend boundary. */
static void ep_clear_discarded_host_predecessors(ep_call *call)
{
    if (ep_is_host_exit()) { return; }
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        ep_host_predecessor *predecessor = &call->host_predecessors[i];
        if (!predecessor->discard_pending) { continue; }
        bool active = call->active_host_predecessor == predecessor;
        zval stored = predecessor->exception;
        ZVAL_UNDEF(&predecessor->exception);
        ep_reset_host_predecessor(predecessor);
        if (active) {
            call->active_host_predecessor = NULL;
            if (call->host_exception && EG(exception)) { zend_clear_exception(); }
            call->discarded_host_exception_depth = 0;
            call->previous_exceptions = NULL;
            call->previous_exceptions_tail = NULL;
            call->previous_exception_anchor = NULL;
            ep_restore_parked_host_exception(call);
            call->host_exception = EG(exception) != NULL;
            ep_activate_pending_host_predecessor(call);
        }
        if (!Z_ISUNDEF(stored)) { zval_ptr_dtor(&stored); }
    }
}

/* Move a caught replacement's host predecessor out of EG(exception) at a safe Zend boundary. */
static void ep_detach_host_predecessor(ep_call *call, bool force)
{
    bool force_current = force;
    while (call->active_host_predecessor) {
        ep_host_predecessor *predecessor = call->active_host_predecessor;
        if ((!force_current && !predecessor->detach_pending)
            || !call->host_exception || !EG(exception) || ep_is_host_exit()) {
            return;
        }
        for (ep_exception_copy *copy = call->previous_exceptions; copy; copy = copy->next) {
            ep_insert_previous(call, copy, predecessor);
        }
        call->previous_exceptions = NULL;
        call->previous_exceptions_tail = NULL;
        call->previous_exception_anchor = NULL;
        if (!Z_ISUNDEF(predecessor->exception)) {
            zval_ptr_dtor(&predecessor->exception);
        }
        ZVAL_OBJ(&predecessor->exception, EG(exception));
        EG(exception) = NULL;
        predecessor->detach_pending = false;
        call->active_host_predecessor = NULL;
        call->discarded_host_exception_depth = 0;
        ep_restore_parked_host_exception(call);
        call->host_exception = EG(exception) != NULL;
        ep_activate_pending_host_predecessor(call);
        force_current = false;
    }
}

/* Park the native image around a guarded Zend callback, including nested module calls. */
static int ep_deliver(const char *bytes, size_t length, bool warning)
{
    ep_call *call = ep_current;
    if (call->error || call->host_bailout || ep_is_host_exit()) { return 1; }
    if (!length) { return 0; }
    if (!call->checkpoint) {
        /* Recovery storage is bounded independently and does not consume transport bytes. */
        call->checkpoint = malloc(ep_state_size ? ep_state_size : 1);
        if (!call->checkpoint) { call->error = "PHP extension checkpoint allocation failed"; return 1; }
        memcpy(call->checkpoint, call->snapshot, ep_state_size);
    }
    if (length > call->output_capacity) {
        char *buffer = realloc(call->output, length);
        if (!buffer) { call->error = "PHP extension output allocation failed"; return 1; }
        call->output = buffer;
        call->output_capacity = length;
    }
    memcpy(call->output, bytes, length);
    call->output_len = length;
    ep_state_save(call->snapshot);
    ep_state_reset();
    if (warning) { call->parked_warning_buffer = (char *)bytes; }
    ep_current = NULL;
    pthread_mutex_unlock(&ep_native_lock);
    zend_try {
        ep_clear_discarded_host_exception(call);
        ep_commit_crossed_host_exceptions(call);
        ep_clear_discarded_host_predecessors(call);
        ep_detach_host_predecessor(call, false);
    } zend_catch {
        call->host_bailout = true;
    } zend_end_try();
    /* Native finally blocks run while an older host callback exception is pending.
     * Keep that escape separate until the newer callback either settles or leaves. */
    zend_object *saved_exception_object = EG(exception);
    bool saved_exception = saved_exception_object != NULL;
    volatile bool saved_exception_can_anchor = saved_exception
        && !zend_is_unwind_exit(saved_exception_object)
        && !zend_is_graceful_exit(saved_exception_object);
    volatile bool saved_exception_parked = false;
    if (!call->host_bailout) {
        if (saved_exception) {
            zend_exception_save();
            saved_exception_parked = true;
        }
        zend_try {
            if (warning) {
                ep_issue_warning(call);
            } else {
                PHPWRITE(call->output, length);
            }
        } zend_catch {
            call->host_bailout = true;
        } zend_end_try();
    }
    ep_restore_nested_prev_exception(call);
    if (call->warning) { zend_string_release(call->warning); call->warning = NULL; }
    call->parked_warning_buffer = NULL;
    bool callback_exception = EG(exception) != NULL;
    if (saved_exception_parked) {
        if (callback_exception && !call->host_bailout) {
            if (!ep_park_saved_host_exception(call, saved_exception_object)) {
                zend_exception_restore();
                if (!call->error) {
                    call->error = "PHP extension parked exception allocation failed";
                }
            }
        } else {
            zend_exception_restore();
        }
    }
    if (callback_exception) {
        call->previous_exception_anchor = saved_exception_can_anchor ? saved_exception_object : NULL;
    }
    call->host_exception = EG(exception) != NULL;
    /* The module-local normal mutex remains initialized for every active request. */
    pthread_mutex_lock(&ep_native_lock);
    ep_state_restore(call->snapshot);
    ep_native_stack_limit = ep_native_stack_limit_main = call->stack_floor;
    ep_current = call;
    return call->host_bailout || callback_exception;
}

/* No Zend bailout escapes this C frame into the native stack. */
int elephc_php_write(const char *bytes, size_t length) { return ep_deliver(bytes, length, false); }

/* Warnings use the host error reporting mask and host PHP error handler. */
int elephc_php_warning(const char *bytes, size_t length) { return ep_deliver(bytes, length, true); }

/* The native result is copied before its heap owner is retired. */
void elephc_php_result(const zval *value)
{
    if (ep_current && !ep_current->error) {
        const char *error = ep_infrastructure_error(ep_boundary_status);
        if (error) { ep_current->error = error; return; }
        ep_current->result = ep_copy(ep_current, value, false, 0);
    }
}

/* Begin one outer-to-inner discard walk while every removed native owner is still live. */
void elephc_php_discard_exception_begin(void)
{
    if (ep_current) { ep_current->discard_chain_transient = true; }
}

/* Native code classifies each discarded chain node before releasing its incoming owner. */
void elephc_php_discard_exception(const uintptr_t *object)
{
    ep_call *call = ep_current;
    if (!call || call->host_bailout) { return; }
    if (!object) {
        ep_defer_host_exception_discard(call);
        return;
    }
    size_t pins = ep_replacement_pin_count(object);
    bool transient = call->discard_chain_transient
        && ep_native_refcount(object) == pins + 1;
    call->discard_chain_transient = transient;
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        ep_host_predecessor *predecessor = &call->host_predecessors[i];
        if (predecessor->root != object) { continue; }
        if (transient) { predecessor->discard_pending = true; }
        else { predecessor->detach_pending = true; }
    }
}

/* A null pending native exception proves that this chain root replaced a host escape. */
void elephc_php_finally_replacement(const uintptr_t *object, const uintptr_t *tail)
{
    ep_call *call = ep_current;
    if (!object || !call || !call->host_exception || call->host_bailout || call->error) {
        return;
    }
    uint64_t root_identity = ep_root_identity(call, object);
    if (!root_identity) {
        root_identity = ++call->root_identity_sequence;
        if (!root_identity) { root_identity = ++call->root_identity_sequence; }
    }
    ep_host_predecessor *predecessor = NULL;
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        if (!call->host_predecessors[i].root
            && Z_ISUNDEF(call->host_predecessors[i].exception)) {
            predecessor = &call->host_predecessors[i];
            break;
        }
    }
    if (!predecessor) {
        if (call->host_predecessor_count >= EP_HOST_PREDECESSOR_LIMIT) {
            call->error = "PHP extension exception chain exceeds the maximum live replacement pin count (64)";
            return;
        }
        predecessor = &call->host_predecessors[call->host_predecessor_count++];
    }
    predecessor->root = object;
    predecessor->tail = tail ? tail : object;
    predecessor->sequence = ++call->host_predecessor_sequence;
    predecessor->root_identity = root_identity;
    predecessor->contained_root_identity_count = 0;
    predecessor->detach_pending = false;
    predecessor->discard_pending = false;
    predecessor->activation_pending = false;
    ZVAL_UNDEF(&predecessor->exception);
    ep_host_predecessor *logical = ep_logical_host_predecessor(call);
    bool activation_pending = call->active_host_predecessor
        && (call->active_host_predecessor->detach_pending
            || call->active_host_predecessor->discard_pending
            || logical != call->active_host_predecessor);
    if (activation_pending) {
        predecessor->activation_pending = true;
    } else {
        call->active_host_predecessor = predecessor;
    }
}

/* Select the next older host level that has not already joined the escaping chain. */
static ep_parked_host_exception *ep_finally_chain_candidate(ep_call *call)
{
    ep_parked_host_exception *parked = call->parked_host_exceptions;
    for (size_t skip = call->discarded_host_exception_depth;
        skip && parked; skip--) {
        parked = parked->next;
    }
    while (parked && parked->crossed_finally) { parked = parked->next; }
    return parked;
}

/* Match a native pending-chain node to the replacement owned by the parked level. */
static void ep_match_finally_chain_pending(ep_call *call, const uintptr_t *pending)
{
    if (call->finally_chain_matched || !pending) { return; }
    ep_parked_host_exception *parked = ep_finally_chain_candidate(call);
    if (parked && parked->active_host_predecessor
        && parked->active_host_predecessor->root == pending) {
        parked->crossed_finally = true;
        call->finally_chain_matched = true;
    }
}

/* Match an escaping replacement-chain node to the newest logical replacement. */
static void ep_match_finally_chain_replacement(ep_call *call, const uintptr_t *replacement)
{
    if (call->finally_chain_matched || !replacement) { return; }
    ep_host_predecessor *logical = ep_logical_host_predecessor(call);
    if (logical && logical->root == replacement) {
        ep_parked_host_exception *parked = ep_finally_chain_candidate(call);
        if (parked) {
            parked->crossed_finally = true;
            call->finally_chain_matched = true;
        }
    }
}

/* Begin one finally crossing with both the escaping and pending native identities. */
void elephc_php_finally_chain(const uintptr_t *replacement, const uintptr_t *pending)
{
    ep_call *call = ep_current;
    if (!call || call->host_bailout || call->error) { return; }
    call->finally_chain_matched = false;
    if (pending) {
        ep_match_finally_chain_pending(call, pending);
        return;
    }
    if (replacement) {
        ep_match_finally_chain_replacement(call, replacement);
        return;
    }
    ep_parked_host_exception *parked = ep_finally_chain_candidate(call);
    if (parked) {
        parked->crossed_finally = true;
        call->finally_chain_matched = true;
    }
}

/* Continue matching the escaping replacement chain without crossing twice. */
void elephc_php_finally_chain_replacement(const uintptr_t *replacement)
{
    ep_call *call = ep_current;
    if (call && !call->host_bailout && !call->error) {
        ep_match_finally_chain_replacement(call, replacement);
    }
}

/* Continue matching the remaining native pending chain without crossing twice. */
void elephc_php_finally_chain_pending(const uintptr_t *pending)
{
    ep_call *call = ep_current;
    if (call && !call->host_bailout && !call->error) {
        ep_match_finally_chain_pending(call, pending);
    }
}

/* User catch binding consumes only the replacement proved by the runtime chain operation. */
void elephc_php_catch_host_replacement(const uintptr_t *object)
{
    ep_call *call = ep_current;
    if (!object || !call || !call->host_exception || call->host_bailout) { return; }
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        ep_host_predecessor *predecessor = &call->host_predecessors[i];
        if (predecessor->root == object) {
            predecessor->detach_pending = true;
            return;
        }
    }
}

/* Native conversion reports a type failure without allocating or entering Zend. */
void elephc_php_unsupported_result(void)
{
    if (ep_current && !ep_current->error) {
        ep_current->error = "PHP extension boundary supports only int, float, bool, string, array and null";
    }
}

/* Copy compact native Throwable fields while their owned message remains live. */
static int ep_copy_exception(ep_call *call, const uintptr_t *object, char **name,
    size_t *name_len, char **message, size_t *message_len, zend_long *code)
{
    uintptr_t id = object[0];
    if (id < ep_class_name_count) {
        *name_len = ep_class_name_entries[id * 2 + 1];
        *name = ep_alloc(call, *name_len + 1);
        if (*name) {
            memcpy(*name, (void *)ep_class_name_entries[id * 2], *name_len);
        }
    }
    *message_len = object[2];
    *message = ep_alloc(call, *message_len + 1);
    if (*message) {
        memcpy(*message, (void *)object[1], *message_len);
    }
    *code = (zend_long)object[3];
    return call->error != NULL;
}

/* Copy one native previous-chain node and append it to the active copied root. */
int elephc_php_exception_previous(const uintptr_t *object)
{
    ep_call *call = ep_current;
    if (!call || !object || call->error) { return 1; }
    ep_exception_copy *copy = ep_alloc(call, sizeof(ep_exception_copy));
    if (!copy || ep_copy_exception(call, object, &copy->name, &copy->name_len,
        &copy->message, &copy->message_len, &copy->code)) {
        return 1;
    }
    copy->host_predecessor_key = object;
    copy->root_identity = ep_root_identity(call, object);
    if (call->exception_chain_tail) {
        call->exception_chain_tail->previous = copy;
    } else {
        call->exception_previous = copy;
    }
    call->exception_chain_tail = copy;
    return 0;
}

/* Preserve an older native Throwable before a host callback escape consumes its owner. */
void elephc_php_previous_exception(const uintptr_t *object)
{
    ep_call *call = ep_current;
    if (!call || !object || call->error) { return; }
    ep_exception_copy *copy = ep_alloc(call, sizeof(ep_exception_copy));
    if (!copy || ep_copy_exception(call, object, &copy->name, &copy->name_len,
        &copy->message, &copy->message_len, &copy->code)) {
        return;
    }
    copy->host_predecessor_key = object;
    copy->root_identity = ep_root_identity(call, object);
    copy->anchor = call->previous_exception_anchor;
    if (call->previous_exceptions_tail) {
        call->previous_exceptions_tail->next = copy;
    } else {
        call->previous_exceptions = copy;
    }
    call->previous_exceptions_tail = copy;
    call->exception_chain_tail = copy;
}

/* Copy the primary compact native Throwable at the invocation boundary. */
int elephc_php_exception(const uintptr_t *object, uintptr_t status)
{
    ep_call *call = ep_current;
    if (!call) { return 1; }
    if (call->host_bailout) { return 0; }
    if (call->error) { return 1; }
    call->exception_host_predecessor_key = object;
    const char *error = ep_infrastructure_error(status);
    if (error) { call->error = error; return 1; }
    if (call->host_exception && !object && status == 2) { return 0; }
    if (!object) {
        call->error = "elephc native runtime failed";
        return 1;
    }
    call->exception_previous = NULL;
    call->exception_chain_tail = NULL;
    return ep_copy_exception(call, object, &call->exception_name, &call->exception_name_len,
        &call->exception_message, &call->exception_message_len, &call->exception_code);
}

/* Resolve existing host Throwable classes without invoking user autoloaders. */
static zend_class_entry *ep_exception_class(char *name, size_t name_len)
{
    zend_class_entry *ce = NULL;
    if (name) {
        for (size_t i = 0; i < name_len; i++) {
            unsigned char c = (unsigned char)name[i];
            if (c >= 'A' && c <= 'Z') { name[i] = (char)(c + ('a' - 'A')); }
        }
        ce = zend_hash_str_find_ptr(CG(class_table), name, name_len);
    }
    if (!ce || ce->type != ZEND_INTERNAL_CLASS || !instanceof_function(ce, zend_ce_throwable)) {
        ce = zend_ce_exception;
    }
    return ce;
}

/* Materialize one copied native Throwable without invoking a user autoloader. */
static void ep_materialize_exception(char *name, size_t name_len, char *message,
    size_t message_len, zend_long code, zval *value)
{
    zend_class_entry *ce = ep_exception_class(name, name_len);
    object_init_ex(value, ce);
    zend_object *exception = Z_OBJ_P(value);
    zend_update_property_stringl(ce, exception, "message", sizeof("message") - 1,
        message, message_len);
    zend_update_property_long(ce, exception, "code", sizeof("code") - 1, code);
}

/* Throwable previous properties are private on either the Exception or Error base. */
static zend_object *ep_host_previous(zend_object *exception)
{
    zval rv;
    zval *previous = zend_read_property(zend_get_exception_base(exception), exception,
        "previous", sizeof("previous") - 1, true, &rv);
    ZVAL_DEREF(previous);
    return Z_TYPE_P(previous) == IS_OBJECT ? Z_OBJ_P(previous) : NULL;
}

/* Discover every stable identity a copied native chain will import before moving owners. */
static void ep_declare_attached_roots(ep_call *call, ep_host_predecessor *container,
    const uintptr_t *key)
{
    if (!container || !key) { return; }
    for (size_t i = 0; i < call->host_predecessor_count; i++) {
        ep_host_predecessor *candidate = &call->host_predecessors[i];
        if (candidate == container || candidate->tail != key
            || Z_ISUNDEF(candidate->exception)
            || ep_predecessor_contains_own_root(candidate)) {
            continue;
        }
        zend_object *host = Z_OBJ(candidate->exception);
        if (zend_is_unwind_exit(host) || zend_is_graceful_exit(host)) { continue; }
        ep_merge_contained_roots(call, container, candidate);
    }
}

/* Splice every displaced host exception for one native chain point in creation order. */
static zend_object *ep_attach_host_predecessors(ep_call *call, zend_object *tail,
    const uintptr_t *key, ep_host_predecessor *container)
{
    uint64_t sequence = 0;
    while (key) {
        ep_host_predecessor *selected = NULL;
        for (size_t i = 0; i < call->host_predecessor_count; i++) {
            ep_host_predecessor *candidate = &call->host_predecessors[i];
            if (candidate->tail == key && candidate->sequence > sequence
                && !Z_ISUNDEF(candidate->exception)
                && (!selected || candidate->sequence < selected->sequence)) {
                selected = candidate;
            }
        }
        if (!selected) { break; }
        sequence = selected->sequence;
        zend_object *host = Z_OBJ(selected->exception);
        if (zend_is_unwind_exit(host) || zend_is_graceful_exit(host)) {
            zval_ptr_dtor(&selected->exception);
            ZVAL_UNDEF(&selected->exception);
            continue;
        }
        if (ep_predecessor_contains_own_root(selected)) {
            zval_ptr_dtor(&selected->exception);
            ZVAL_UNDEF(&selected->exception);
            continue;
        }
        ep_merge_contained_roots(call, container, selected);
        bool preserve = container && ep_predecessor_contains_own_root(container);
        if (preserve) { GC_ADDREF(host); }
        zend_exception_set_previous(tail, host);
        if (!preserve) { ZVAL_UNDEF(&selected->exception); }
        zend_object *previous;
        while ((previous = ep_host_previous(tail))) { tail = previous; }
    }
    return tail;
}

/* Materialize copied previous links and splice host predecessors at their recorded chain points. */
static zend_object *ep_materialize_previous_chain(ep_call *call, zend_object *tail,
    const uintptr_t *root_key, ep_exception_copy *copy,
    ep_host_predecessor *container)
{
    tail = ep_attach_host_predecessors(call, tail, root_key, container);
    while (copy) {
        ep_record_contained_root(call, container, copy->root_identity);
        zval value;
        ep_materialize_exception(copy->name, copy->name_len, copy->message,
            copy->message_len, copy->code, &value);
        zend_object *previous = Z_OBJ(value);
        zend_exception_set_previous(tail, previous);
        ZVAL_UNDEF(&value);
        while ((previous = ep_host_previous(tail))) { tail = previous; }
        tail = ep_attach_host_predecessors(call, tail,
            copy->host_predecessor_key, container);
        copy = copy->previous;
    }
    return tail;
}

/* Materialize one copied native Throwable and every native previous link it owns. */
static zend_object *ep_materialize_exception_copy(ep_call *call,
    ep_exception_copy *copy, zval *value, ep_host_predecessor *container)
{
    for (ep_exception_copy *node = copy; node; node = node->previous) {
        ep_record_contained_root(call, container, node->root_identity);
        ep_declare_attached_roots(call, container, node->host_predecessor_key);
    }
    ep_materialize_exception(copy->name, copy->name_len, copy->message,
        copy->message_len, copy->code, value);
    return ep_materialize_previous_chain(call, Z_OBJ_P(value),
        copy->host_predecessor_key, copy->previous, container);
}

/* Insert a transported native Throwable at the host-chain point where it arose. */
static void ep_insert_previous(ep_call *call, ep_exception_copy *copy,
    ep_host_predecessor *container)
{
    zval value;
    zend_object *native_tail = ep_materialize_exception_copy(call, copy, &value, container);
    if (copy->anchor) {
        zend_object *cursor = EG(exception);
        while (cursor) {
            zend_object *previous = ep_host_previous(cursor);
            if (previous == copy->anchor) {
                GC_ADDREF(copy->anchor);
                zend_exception_set_previous(native_tail, copy->anchor);
                zend_update_property(zend_get_exception_base(cursor), cursor,
                    "previous", sizeof("previous") - 1, &value);
                zval_ptr_dtor(&value);
                return;
            }
            cursor = previous;
        }
    }
    zend_exception_set_previous(EG(exception), Z_OBJ(value));
    ZVAL_UNDEF(&value);
}

/* No Zend frame, allocation or callback lies below the native invocation. */
static void ep_invoke(INTERNAL_FUNCTION_PARAMETERS, ep_native_function function, zval *values,
    size_t count, bool integer_to_float)
{
    (void)execute_data;
    if (!EP_GLOBAL(initialized)) {
        zend_throw_error(NULL, "elephc PHP extension request is not initialized");
        return;
    }
    /* Request images contain pointers into parked native stacks. Only synchronous
     * reentry on their owning host context can safely share those handler chains. */
    if (EP_GLOBAL(active) && EP_GLOBAL(active)->fiber_context != EG(current_fiber_context)) {
        zend_throw_error(NULL, "elephc PHP extension has an active native call on another Fiber");
        return;
    }
    ep_call *parent = EP_GLOBAL(active);
    if (parent && EG(prev_exception)) {
        parent->nested_prev_exception = EG(prev_exception);
        EG(prev_exception) = NULL;
    }
    if (!EP_GLOBAL(snapshot)) {
        EP_GLOBAL(snapshot) = calloc(1, ep_state_size);
        if (!EP_GLOBAL(snapshot)) { zend_throw_error(NULL, "PHP extension snapshot allocation failed"); return; }
        memcpy(EP_GLOBAL(snapshot), ep_template, ep_data_size);
    }
    ep_call *call = calloc(1, sizeof(ep_call));
    if (!call) { zend_throw_error(NULL, "PHP extension activation allocation failed"); return; }
    call->parent = parent;
    call->stack_floor = ep_stack_floor();
    call->fiber_context = EG(current_fiber_context);
    call->snapshot = EP_GLOBAL(snapshot);
    ep_stash_ambient_prev_exception(call);
    if (!call->stack_floor) { call->error = "Cannot determine the PHP thread stack boundary"; }
    const zval **inputs = ep_alloc(call, count * sizeof(zval *));
    if (inputs) {
        for (size_t i = 0; i < count; i++) {
            inputs[i] = ep_copy(call, &values[i], true, 0);
            if (!inputs[i]) { break; }
        }
    }
    if (!call->error) {
        int locked = pthread_mutex_lock(&ep_native_lock);
        if (locked) {
            call->error = "PHP extension runtime lock failed";
        } else {
            ep_state_restore(EP_GLOBAL(snapshot));
            EP_GLOBAL(active) = call;
            ep_current = call;
            function(inputs);
            ep_current = NULL;
            /* The thunk has retired every native frame, including contained unwinds. */
            EP_GLOBAL(active) = call->parent;
            /* Infrastructure failures can leave partial native imports or results. Roll back
             * the entire image; ordinary PHP exceptions keep their completed state changes. */
            if (!call->error) { ep_state_save(call->snapshot); }
            else if (call->checkpoint) { memcpy(call->snapshot, call->checkpoint, ep_state_size); }
            ep_state_reset();
            pthread_mutex_unlock(&ep_native_lock);
        }
    }
    /* The call allocation is on the heap: setjmp cannot invalidate its cleanup fields. */
    zend_try {
        if (call->host_bailout) { zend_bailout(); }
        ep_clear_discarded_host_exception(call);
        ep_commit_crossed_host_exceptions(call);
        ep_clear_discarded_host_predecessors(call);
        ep_detach_host_predecessor(call, false);
        bool host_exit = ep_is_host_exit();
        if (!host_exit && call->error) {
            /* A transport/runtime failure is authoritative over the incomplete host callback. */
            if (EG(exception)) { zend_clear_exception(); }
            ep_clear_parked_host_exceptions(call);
            call->host_exception = false;
        } else if (!host_exit && call->result
            && (call->host_exception || call->parked_host_exceptions)) {
            /* A return from finally overrides the pending host callback exception. */
            if (EG(exception)) { zend_clear_exception(); }
            ep_clear_parked_host_exceptions(call);
            call->host_exception = false;
        }
        if (!host_exit && call->exception_message && EG(exception)
            && call->active_host_predecessor) {
            ep_detach_host_predecessor(call, true);
        }
        if (!host_exit) { ep_chain_parked_host_exceptions(call); }
        bool restore_host_exception = !host_exit && call->exception_message && EG(exception);
        if (!host_exit && EG(exception)) {
            for (ep_exception_copy *copy = call->previous_exceptions; copy; copy = copy->next) {
                ep_insert_previous(call, copy, call->active_host_predecessor);
            }
        }
        if (restore_host_exception) {
            /* A native exception from finally replaces the host callback exception and keeps
             * that exception as its previous link, matching Zend's finally semantics. */
            zend_exception_save();
        }
        if (!EG(exception)) {
            if (call->error) {
                zend_throw_error(NULL, "%s", call->error);
            } else if (call->exception_message) {
                ep_materialize_exception(call->exception_name, call->exception_name_len,
                    call->exception_message, call->exception_message_len,
                    call->exception_code, &call->pending_exception);
                zend_object *tail = ep_materialize_previous_chain(
                    call, Z_OBJ(call->pending_exception),
                    call->exception_host_predecessor_key, call->exception_previous, NULL);
                (void)tail;
                zval thrown = call->pending_exception;
                ZVAL_UNDEF(&call->pending_exception);
                zend_throw_exception_object(&thrown);
            } else if (call->result) {
                ep_to_zend(call->result, return_value);
                if (integer_to_float && Z_TYPE_P(return_value) == IS_LONG) {
                    ZVAL_DOUBLE(return_value, (double)Z_LVAL_P(return_value));
                }
            } else {
                zend_throw_error(NULL, "elephc native call returned no value");
            }
        }
        if (restore_host_exception) { zend_exception_restore(); }
        ep_restore_ambient_prev_exception(call);
    } zend_catch {
        ep_restore_ambient_prev_exception(call);
        ep_call_destroy(call);
        zend_bailout();
    } zend_end_try();
    ep_call_destroy(call);
}

static PHP_GINIT_FUNCTION(@IDENT@)
{
    @IDENT@_globals->snapshot = NULL;
    @IDENT@_globals->active = NULL;
    @IDENT@_globals->initialized = false;
}

/* Fiber destruction can resume parked callbacks after every module RSHUTDOWN.
 * Zend blocks switching before this post-deactivate phase, so native images and
 * activation records become unreachable only here. */
static ZEND_MODULE_POST_ZEND_DEACTIVATE_D(@IDENT@)
{
    ep_call_abandon_chain(EP_GLOBAL(active));
    free(EP_GLOBAL(snapshot));
    EP_GLOBAL(snapshot) = NULL;
    EP_GLOBAL(active) = NULL;
    EP_GLOBAL(initialized) = false;
    return SUCCESS;
}

/* Also covers partial request teardown that never reaches post-deactivate. */
static PHP_GSHUTDOWN_FUNCTION(@IDENT@)
{
    ep_call_abandon_chain(@IDENT@_globals->active);
    free(@IDENT@_globals->snapshot);
    @IDENT@_globals->snapshot = NULL;
    @IDENT@_globals->active = NULL;
    @IDENT@_globals->initialized = false;
}
