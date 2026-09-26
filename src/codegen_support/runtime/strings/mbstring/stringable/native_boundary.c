/* Independent C ABI checks for the emitted protected Stringable callback boundary. */
#include <setjmp.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

struct host_string { const unsigned char *bytes; uint64_t len; void *owner; };
struct host_cell { uint64_t tag, lo, hi; };
extern int call_stringable(void *, const struct host_cell *, struct host_string *) __asm__("__rt_mbstring_stringable");
__attribute__((visibility("hidden"))) void *handler_top __asm__("_exc_handler_top");
__attribute__((visibility("hidden"))) void *call_frame_top __asm__("_exc_call_frame_top");
__attribute__((visibility("hidden"))) uint64_t suppression __asm__("_rt_diag_suppression");

static int mode, calls, failure;
static int context_marker, receiver_marker, previous_handler, previous_frame, nested_frame;
static struct host_cell receiver = {6, 0, 0};

/* Supplies conversion results or a PHP-style longjmp through an independently decoded record. */
void mb_test_convert(uint64_t tag, const struct host_cell *cell, void *context, struct host_string *out) {
    calls++;
    if (tag != 7 || cell != &receiver || context != &context_marker || !handler_top) { failure = __LINE__; return; }
    unsigned char *handler = handler_top;
    if (mode == 2) {
        struct host_string nested = {0};
        void *outer_handler = handler_top;
        void *outer_frame = call_frame_top;
        uint64_t outer_suppression = suppression;
        mode = 1;
        if (call_stringable(context, cell, &nested) != 2 || nested.bytes || nested.len || nested.owner) { failure = __LINE__; return; }
        if (handler_top != outer_handler || call_frame_top != outer_frame || suppression != outer_suppression) { failure = __LINE__; return; }
        mode = 2;
    }
    suppression = 99;
    call_frame_top = &nested_frame;
    if (mode == 1) { longjmp(*(jmp_buf *)(handler + 24), 1); }
    unsigned char *bytes = malloc(3);
    if (!bytes) { abort(); }
    memcpy(bytes, "a\0\xff", 3);
    *out = (struct host_string){bytes, 3, bytes};
}

/* Checks success, throw, nested throw, missing metadata, state restoration, and native ownership. */
int mb_test_stringable_boundary(void) {
    receiver.lo = (uint64_t)(uintptr_t)&receiver_marker;
    for (int current = 0; current < 3; current++) {
        mode = current;
        calls = 0;
        failure = 0;
        handler_top = &previous_handler;
        call_frame_top = &previous_frame;
        suppression = 7;
        struct host_string out = {(const unsigned char *)(uintptr_t)1, 42, (void *)(uintptr_t)2};
        int status = call_stringable(&context_marker, &receiver, &out);
        if (failure) { return failure; }
        if (handler_top != &previous_handler || call_frame_top != &previous_frame || suppression != 7) { return __LINE__; }
        if (calls != (current == 2 ? 2 : 1)) { return __LINE__; }
        if (current == 1) {
            if (status != 2 || out.bytes || out.len || out.owner) { return __LINE__; }
        } else {
            if (status || out.len != 3 || out.bytes != out.owner || memcmp(out.bytes, "a\0\xff", 3)) { return __LINE__; }
            free(out.owner);
        }
    }
    struct host_string out = {(const unsigned char *)(uintptr_t)1, 42, (void *)(uintptr_t)2};
    if (call_stringable(NULL, NULL, &out) != 1 || out.bytes || out.len || out.owner) { return __LINE__; }
    if (call_stringable(NULL, &receiver, NULL) != 1) { return __LINE__; }
    if (handler_top != &previous_handler || call_frame_top != &previous_frame || suppression != 7) { return __LINE__; }
    return 0;
}
