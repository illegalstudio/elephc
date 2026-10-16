/* Native execution uses no Zend API. The mutex protects complete request snapshots. */
#include <pthread.h>
#include <Zend/zend_exceptions.h>

extern unsigned char elephc_state_begin[], elephc_state_end[];
extern const uintptr_t ep_class_name_count __asm__("_class_name_count");
extern const uintptr_t ep_class_name_entries[] __asm__("_class_name_entries");
extern uintptr_t ep_native_stack_limit __asm__("_stack_limit");
extern uintptr_t ep_native_stack_limit_main __asm__("_stack_limit_main");

typedef void (*ep_native_function)(const zval *const *);
static pthread_mutex_t ep_native_lock = PTHREAD_MUTEX_INITIALIZER;
static unsigned char *ep_template;
static size_t ep_state_size;
static ep_call *ep_current;

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
ZEND_END_MODULE_GLOBALS(@IDENT@)
ZEND_DECLARE_MODULE_GLOBALS(@IDENT@)
#define EP_GLOBAL(member) ZEND_MODULE_GLOBALS_ACCESSOR(@IDENT@, member)

/* Native warning lines become length-aware messages for the host error handler. */
static void ep_issue_warning(ep_call *call)
{
    const char *message = call->output;
    size_t length = call->output_len;
    if (length >= 9 && memcmp(message, "Warning: ", 9) == 0) { message += 9; length -= 9; }
    while (length && message[length - 1] == '\n') { length--; }
    call->warning = zend_string_init(message, length, false);
    zend_error_zstr(E_WARNING, call->warning);
}

/* Park the native image around a guarded Zend callback, including nested module calls. */
static int ep_deliver(const char *bytes, size_t length, bool warning)
{
    ep_call *call = ep_current;
    if (call->error) { return 1; }
    if (!length) { return 0; }
    if (!call->checkpoint) {
        call->checkpoint = ep_alloc(call, ep_state_size);
        if (!call->checkpoint) { return 1; }
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
    memcpy(call->snapshot, elephc_state_begin, ep_state_size);
    memcpy(elephc_state_begin, ep_template, ep_state_size);
    ep_current = NULL;
    pthread_mutex_unlock(&ep_native_lock);
    zend_try {
        if (warning) {
            ep_issue_warning(call);
        } else {
            PHPWRITE(call->output, length);
        }
    } zend_catch {
        call->host_bailout = true;
    } zend_end_try();
    if (call->warning) { zend_string_release(call->warning); call->warning = NULL; }
    call->host_exception = EG(exception) != NULL;
    /* The module-local normal mutex remains initialized for every active request. */
    pthread_mutex_lock(&ep_native_lock);
    memcpy(elephc_state_begin, call->snapshot, ep_state_size);
    ep_native_stack_limit = ep_native_stack_limit_main = call->stack_floor;
    ep_current = call;
    return call->host_bailout || call->host_exception;
}

/* No Zend bailout escapes this C frame into the native stack. */
int elephc_php_write(const char *bytes, size_t length) { return ep_deliver(bytes, length, false); }

/* Warnings use the host error reporting mask and host PHP error handler. */
int elephc_php_warning(const char *bytes, size_t length) { return ep_deliver(bytes, length, true); }

/* The native result is copied before its heap owner is retired. */
void elephc_php_result(const zval *value)
{
    if (ep_current && !ep_current->error) {
        ep_current->result = ep_copy(ep_current, value, false, 0);
    }
}

/* Copy a compact native Throwable while its owned message remains live. */
int elephc_php_exception(const uintptr_t *object, uintptr_t status)
{
    ep_call *call = ep_current;
    if (!call) { return 1; }
    if (call->host_exception || call->host_bailout) { return 0; }
    if (call->error) { return 1; }
    if (!object) {
        call->error = status == 1 ? "PHP extension result exceeds the maximum nesting depth (64)" :
            (status == 3 ? "elephc heap allocation failed" : "elephc native runtime failed");
        return 1;
    }
    uintptr_t id = object[0];
    if (id < ep_class_name_count) {
        call->exception_name_len = ep_class_name_entries[id * 2 + 1];
        call->exception_name = ep_alloc(call, call->exception_name_len + 1);
        if (call->exception_name) {
            memcpy(call->exception_name, (void *)ep_class_name_entries[id * 2], call->exception_name_len);
        }
    }
    call->exception_message_len = object[2];
    call->exception_message = ep_alloc(call, call->exception_message_len + 1);
    if (call->exception_message) {
        memcpy(call->exception_message, (void *)object[1], call->exception_message_len);
    }
    call->exception_code = (zend_long)object[3];
    return call->error != NULL;
}

/* Resolve existing host Throwable classes without invoking user autoloaders. */
static zend_class_entry *ep_exception_class(ep_call *call)
{
    zend_class_entry *ce = NULL;
    if (call->exception_name) {
        for (size_t i = 0; i < call->exception_name_len; i++) {
            unsigned char c = (unsigned char)call->exception_name[i];
            if (c >= 'A' && c <= 'Z') { call->exception_name[i] = (char)(c + ('a' - 'A')); }
        }
        ce = zend_hash_str_find_ptr(CG(class_table), call->exception_name, call->exception_name_len);
    }
    if (!ce || ce->type != ZEND_INTERNAL_CLASS || !instanceof_function(ce, zend_ce_throwable)) {
        ce = zend_ce_exception;
    }
    return ce;
}

/* No Zend frame, allocation or callback lies below the native invocation. */
static void ep_invoke(INTERNAL_FUNCTION_PARAMETERS, ep_native_function function, zval *values, size_t count)
{
    (void)execute_data;
    if (!EP_GLOBAL(snapshot)) {
        zend_throw_error(NULL, "elephc PHP extension request is not initialized");
        return;
    }
    ep_call *call = calloc(1, sizeof(ep_call));
    if (!call) { zend_throw_error(NULL, "PHP extension activation allocation failed"); return; }
    call->stack_floor = ep_stack_floor();
    call->snapshot = EP_GLOBAL(snapshot);
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
            memcpy(elephc_state_begin, EP_GLOBAL(snapshot), ep_state_size);
            ep_current = call;
            function(inputs);
            ep_current = NULL;
            /* Infrastructure failures can leave partial native imports or results. Roll back
             * the entire image; ordinary PHP exceptions keep their completed state changes. */
            if (!call->error) { memcpy(call->snapshot, elephc_state_begin, ep_state_size); }
            else if (call->checkpoint) { memcpy(call->snapshot, call->checkpoint, ep_state_size); }
            memcpy(elephc_state_begin, ep_template, ep_state_size);
            pthread_mutex_unlock(&ep_native_lock);
        }
    }
    /* The call allocation is on the heap: setjmp cannot invalidate its cleanup fields. */
    zend_try {
        if (call->host_bailout) { zend_bailout(); }
        if (!EG(exception)) {
            if (call->error) {
                zend_throw_error(NULL, "%s", call->error);
            } else if (call->exception_message) {
                zend_class_entry *ce = ep_exception_class(call);
                object_init_ex(&call->pending_exception, ce);
                zend_object *exception = Z_OBJ(call->pending_exception);
                zend_update_property_stringl(ce, exception, "message", sizeof("message") - 1,
                    call->exception_message, call->exception_message_len);
                zend_update_property_long(ce, exception, "code", sizeof("code") - 1, call->exception_code);
                zval thrown = call->pending_exception;
                ZVAL_UNDEF(&call->pending_exception);
                zend_throw_exception_object(&thrown);
            } else if (call->result) {
                ep_to_zend(call->result, return_value);
            } else {
                zend_throw_error(NULL, "elephc native call returned no value");
            }
        }
    } zend_catch {
        ep_call_destroy(call);
        zend_bailout();
    } zend_end_try();
    ep_call_destroy(call);
}

static PHP_GINIT_FUNCTION(@IDENT@)
{
    @IDENT@_globals->snapshot = NULL;
}

/* Also covers partial request startup that never reaches this module's RSHUTDOWN. */
static PHP_GSHUTDOWN_FUNCTION(@IDENT@)
{
    free(@IDENT@_globals->snapshot);
    @IDENT@_globals->snapshot = NULL;
}
