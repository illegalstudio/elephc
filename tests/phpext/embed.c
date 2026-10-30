/* Real PHP embed SAPI: repeated requests, with overlapping ZTS requests on eight threads. */
#include <sapi/embed/php_embed.h>
#include <Zend/zend_exceptions.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#if defined(__SANITIZE_ADDRESS__)
#define EP_SANITIZED_ALLOCATOR 1
#elif defined(__clang__)
#if __has_feature(address_sanitizer)
#define EP_SANITIZED_ALLOCATOR 1
#endif
#endif

#if defined(EP_SANITIZED_ALLOCATOR)
#include <sanitizer/allocator_interface.h>
#elif defined(__GLIBC__)
#include <malloc.h>
#if defined(HAVE_VALGRIND)
#include <valgrind/valgrind.h>
/* Valgrind emulates mallinfo, but not mallinfo2. This bounded fixture fits its legacy fields. */
extern struct mallinfo emulated_malloc_info(void) __asm__("mallinfo");
#endif
#elif defined(__APPLE__)
#include <mach/mach.h>
#include <malloc/malloc.h>
#else
#error "The PHP lifecycle test requires live malloc statistics on this platform"
#endif

#if defined(__x86_64__) && defined(__linux__)
/* PHP's supplied Valgrind replacement targets the CLI's NONE soname. Embed
 * libphp.so needs the same bounded comparison registered for its own soname.
 * Keep all Memcheck diagnostics enabled, including extension memory accesses.
 * See php-src/Zend/zend_string.c, I_REPLACE_SONAME_FNNAME_ZU. */
bool ZEND_FASTCALL _vgr00000ZU_libphpZdso_zend_string_equal_val(const zend_string *a, const zend_string *b)
{
    return memcmp(ZSTR_VAL(a), ZSTR_VAL(b), ZSTR_LEN(a)) == 0;
}
#endif

static atomic_int failures;
static atomic_uint request_ids;

#if defined(__APPLE__) && !defined(EP_SANITIZED_ALLOCATOR)
/* Apple aggregate statistics include medium blocks already returned to per-CPU free caches. */
static size_t live_apple_malloc_bytes(void);
#endif

/* Count retained native allocations even when a parked fatal Fiber still references them. */
static size_t live_malloc_bytes(void)
{
#if defined(EP_SANITIZED_ALLOCATOR)
    return __sanitizer_get_current_allocated_bytes();
#elif defined(__GLIBC__)
#if defined(HAVE_VALGRIND)
    if (RUNNING_ON_VALGRIND) {
        struct mallinfo stats = emulated_malloc_info();
        if (stats.uordblks < 0 || stats.hblkhd < 0) {
            fprintf(stderr, "Valgrind live malloc statistics overflowed\n");
            atomic_fetch_add(&failures, 1);
            return 0;
        }
        return (size_t)stats.uordblks + (size_t)stats.hblkhd;
    }
#endif
    struct mallinfo2 stats = mallinfo2();
    return stats.uordblks + stats.hblkhd;
#else
    return live_apple_malloc_bytes();
#endif
}

#if defined(__APPLE__) && !defined(EP_SANITIZED_ALLOCATOR)
typedef struct {
    vm_address_t zone;
    vm_address_t addresses[64], zones[64];
    vm_size_t sizes[64];
    unsigned count;
} large_malloc_ranges;

/* Introspection reads this same process while the fatal phase has no worker threads. */
static kern_return_t read_local_memory(task_t task, vm_address_t address, vm_size_t size, void **memory)
{
    (void)task;
    (void)size;
    *memory = (void *)address;
    return KERN_SUCCESS;
}

typedef struct {
    size_t bytes;
    bool overflow;
} live_malloc_total;

/* The in-use enumerator excludes cached free blocks while retaining every live user allocation. */
static void sum_live_malloc_ranges(task_t task, void *context, unsigned type,
    vm_range_t *ranges, unsigned count)
{
    (void)task;
    if (!(type & MALLOC_PTR_IN_USE_RANGE_TYPE)) { return; }
    live_malloc_total *total = context;
    for (unsigned i = 0; i < count; i++) {
        if (ranges[i].size > SIZE_MAX - total->bytes) { total->overflow = true; return; }
        total->bytes += ranges[i].size;
    }
}

/* Count actual allocations, not magazine accounting that deliberately retains freed blocks. */
static size_t live_apple_malloc_bytes(void)
{
    vm_address_t *zones;
    unsigned count;
    live_malloc_total total = {0};
    kern_return_t status = malloc_get_all_zones(mach_task_self(), read_local_memory, &zones, &count);
    if (status != KERN_SUCCESS) { goto failed; }
    for (unsigned i = 0; i < count; i++) {
        malloc_zone_t *zone = (malloc_zone_t *)zones[i];
        if (!zone->introspect || !zone->introspect->enumerator) { status = KERN_FAILURE; goto failed; }
        status = zone->introspect->enumerator(mach_task_self(), &total, MALLOC_PTR_IN_USE_RANGE_TYPE,
            zones[i], read_local_memory, sum_live_malloc_ranges);
        if (status != KERN_SUCCESS) { goto failed; }
        if (total.overflow) { status = KERN_FAILURE; goto failed; }
    }
    return total.bytes;
failed:
    fprintf(stderr, "live malloc allocation enumeration failed: %d\n", status);
    atomic_fetch_add(&failures, 1);
    return 0;
}

/* Record into stack storage so diagnostics cannot allocate while walking a malloc zone. */
static void record_large_malloc_ranges(task_t task, void *context, unsigned type,
    vm_range_t *ranges, unsigned count)
{
    (void)task;
    if (!(type & MALLOC_PTR_IN_USE_RANGE_TYPE)) { return; }
    large_malloc_ranges *record = context;
    for (unsigned i = 0; i < count && record->count < 64; i++) {
        if (ranges[i].size < 1024 * 1024) { continue; }
        unsigned slot = record->count++;
        record->addresses[slot] = ranges[i].address;
        record->sizes[slot] = ranges[i].size;
        record->zones[slot] = record->zone;
    }
}

/* Large native images are distinguishable from aggregate allocator accounting after a failure. */
static void report_large_malloc_ranges(void)
{
    vm_address_t *zones;
    unsigned count;
    kern_return_t status = malloc_get_all_zones(mach_task_self(), read_local_memory, &zones, &count);
    if (status != KERN_SUCCESS) {
        fprintf(stderr, "malloc zone enumeration failed: %d\n", status);
        return;
    }
    large_malloc_ranges record = {0};
    for (unsigned i = 0; i < count && record.count < 64; i++) {
        malloc_zone_t *zone = (malloc_zone_t *)zones[i];
        if (!zone->introspect) { continue; }
        if (zone->introspect->statistics) {
            malloc_statistics_t stats = {0};
            malloc_zone_statistics(zone, &stats);
            fprintf(stderr, "malloc zone %p (%s): %u blocks, %zu bytes in use, %zu bytes allocated\n",
                (void *)zone, zone->zone_name ? zone->zone_name : "unnamed", stats.blocks_in_use,
                stats.size_in_use, stats.size_allocated);
        }
        if (!zone->introspect->enumerator) { continue; }
        record.zone = zones[i];
        status = zone->introspect->enumerator(mach_task_self(), &record, MALLOC_PTR_IN_USE_RANGE_TYPE,
            zones[i], read_local_memory, record_large_malloc_ranges);
        if (status != KERN_SUCCESS) {
            fprintf(stderr, "malloc range enumeration failed: %d\n", status);
        }
    }
    for (unsigned i = 0; i < record.count; i++) {
        fprintf(stderr, "live malloc range %p: %zu bytes, zone %p\n",
            (void *)record.addresses[i], (size_t)record.sizes[i], (void *)record.zones[i]);
    }
}
#endif

static void *volatile malloc_probe_roots[3];

/* Independent ground truth ensures the counter detects retained images above the 4 MiB bound. */
static void check_malloc_accounting(void)
{
    const size_t bytes = 7 * 256 * 1024;
    size_t baseline = live_malloc_bytes();
    unsigned allocated = 0;
    for (; allocated < 3; allocated++) {
        void *buffer = malloc(bytes);
        malloc_probe_roots[allocated] = buffer;
        if (!buffer) { atomic_fetch_add(&failures, 1); break; }
        memset(buffer, 0xa5, bytes);
    }
    if (allocated == 3) {
        size_t held = live_malloc_bytes();
        if (held < baseline || held - baseline < 3 * bytes) {
            fprintf(stderr, "live malloc counter missed %zu independently held bytes\n", 3 * bytes);
            atomic_fetch_add(&failures, 1);
        }
    }
    for (unsigned i = 0; i < 3; i++) {
        free(malloc_probe_roots[i]);
        malloc_probe_roots[i] = NULL;
    }
    size_t released = live_malloc_bytes();
    /* Even one cached probe block must stop counting as live after free. */
    if (released > baseline && released - baseline >= bytes) {
        fprintf(stderr, "live malloc counter retained %zu already freed bytes\n", released - baseline);
        atomic_fetch_add(&failures, 1);
    }
}

/* A shutdown-flushed output callback can park a Fiber after the destructor pass. */
static void run_late_userland_fiber_request(void)
{
    static const char code[] =
        "ob_start(function ($bytes) {"
        "  if ($bytes === '') { return ''; }"
        "  $GLOBALS['late_fiber'] = new Fiber(function () {"
        "    set_error_handler(function () { Fiber::suspend('parked'); return true; });"
        "    try { phpext_pause('late userland shutdown'); } catch (RuntimeException $error) {}"
        "  });"
        "  if ($GLOBALS['late_fiber']->start() !== 'parked') { throw new Exception('late Fiber did not park'); }"
        "  return '';"
        "}); echo 'late';";
    if (php_request_startup() == FAILURE) { atomic_fetch_add(&failures, 1); return; }
    if (zend_eval_string(code, NULL, "phpext late userland Fiber") == FAILURE || EG(exception)) {
        atomic_fetch_add(&failures, 1);
    }
    php_request_shutdown(NULL);
}

/* A fatal on the main context abandons one or two parked native activations. */
static void run_abandoned_requests(void)
{
    static const char code[] =
        "error_reporting(0); ini_set('display_errors', '0'); ini_set('log_errors', '0');"
        "$GLOBALS['fatal_fiber'] = new Fiber(function () use ($nested) {"
        "  set_error_handler(function () use ($nested) {"
        "    if ($nested) {"
        "      set_error_handler(function () { Fiber::suspend('parked'); return true; });"
        "      try { phpext_pause('nested abandoned'); } catch (RuntimeException $error) {}"
        "    } else { Fiber::suspend('parked'); } return true;"
        "  });"
        "  try { phpext_pause('abandoned'); } catch (RuntimeException $error) {}"
        "});"
        "if ($GLOBALS['fatal_fiber']->start() !== 'parked') { throw new Exception('Fiber did not park'); }"
        "elephc_late_hold($GLOBALS['fatal_fiber'], $resumed);"
        "set_error_handler(static function () { return false; });";
    php_request_shutdown(NULL);
    size_t baseline = live_malloc_bytes();
    size_t samples[9] = {baseline};
    unsigned completed = 0;
    for (unsigned request = 0; request < 8; request++) {
        if (php_request_startup() == FAILURE) { atomic_fetch_add(&failures, 1); break; }
        unsigned expected_resumes = (request / 4) * 2 + (request % 4 < 2 ? request % 4 : 2);
        char script[sizeof(code) + 160];
        snprintf(script, sizeof(script),
            "if (elephc_late_resume_count() !== %u) { throw new Exception('late resume missing'); } "
            "$nested = %s; $resumed = %s; %s", expected_resumes,
            request % 2 ? "true" : "false", request % 4 < 2 ? "true" : "false", code);
        volatile bool bailed = false;
        volatile bool evaluation_failed = false;
        zend_first_try {
            if (zend_eval_string(script, NULL, "phpext abandoned request") == FAILURE || EG(exception)) {
                evaluation_failed = true;
                atomic_fetch_add(&failures, 1);
                if (EG(exception)) { zend_exception_error(EG(exception), E_WARNING); }
            } else {
                /* Finish eval ownership before deliberately abandoning the executor. */
                zend_error_noreturn(E_USER_ERROR, "intentional fatal request");
            }
        } zend_catch {
            bailed = true;
        } zend_end_try();
        if (!bailed && !evaluation_failed) { atomic_fetch_add(&failures, 1); }
        php_request_shutdown(NULL);
        completed = request + 1;
        size_t current = live_malloc_bytes();
        samples[request + 1] = current;
        /* The fixture uses a 512 KiB native heap; abandoned images grow well past 4 MiB. */
        if (current > baseline && current - baseline > 4 * 1024 * 1024) {
            fprintf(stderr, "abandoned native calls retained %zu malloc bytes after %u requests\n",
                current - baseline, request + 1);
            for (unsigned i = 0; i <= request + 1; i++) {
                fprintf(stderr, "malloc bytes after %u requests: %zu\n", i, samples[i]);
            }
#if defined(__APPLE__) && !defined(EP_SANITIZED_ALLOCATOR)
            report_large_malloc_ranges();
#endif
            atomic_fetch_add(&failures, 1);
            break;
        }
    }
    if (php_request_startup() == FAILURE) {
        atomic_fetch_add(&failures, 1);
    } else if (completed == 8 && (zend_eval_string("if (elephc_late_resume_count() !== 4) { throw new Exception('final late resume missing'); }",
        NULL, "phpext final late resume") == FAILURE || EG(exception))) {
        atomic_fetch_add(&failures, 1);
        if (EG(exception)) { zend_exception_error(EG(exception), E_WARNING); }
    }
}

static const char request_code[] =
    "ob_start(function ($bytes) use ($token) {"
    "  if ($bytes === '') { return ''; }"
    "  if ($bytes !== $token || phpext_mirror([$token]) !== [$token]) { throw new Exception('output context corrupted'); }"
    "  return '';"
    "}, 1);"
    "for ($i = 1, $limit = (int)(getenv('ELEPHC_EMBED_ITERATIONS') ?: 24); $i <= $limit; $i++) {"
    "  if (phpext_counter() !== $i) { throw new Exception('request static leaked'); }"
    "  $value = ['binary' => \"a\\0b\", 7 => [null, true, 1.5]];"
    "  if (phpext_mirror($value) !== $value) { throw new Exception('array corrupted'); }"
    "  if (phpext_label(\"x\\0y\") !== \"label:x\\0y\") { throw new Exception('string corrupted'); }"
    "  if (phpext_output($token) !== $token) { throw new Exception('output result corrupted'); }"
    "  try { phpext_failure(); throw new Exception('missing native exception'); }"
    "  catch (RuntimeException $e) { if ($e->getCode() !== 42) { throw new Exception('wrong exception'); } }"
    "} ob_end_clean();"
    "try { phpext_recurse(100000); throw new Exception('missing native stack guard'); }"
    "catch (Error $e) { if (!str_contains($e->getMessage(), 'runtime failed')) { throw $e; } }"
    "if (phpext_counter() !== $limit + 1) { throw new Exception('failed stack recovery'); }"
    "require getenv('ELEPHC_EMBED_FIBERS');";

/* Each thread owns Zend globals and four complete request lifecycles. */
static void *run_requests(void *unused)
{
    (void)unused;
#ifdef ZTS
    ts_resource(0);
    ZEND_TSRMLS_CACHE_UPDATE();
#endif
    for (unsigned request = 0; request < 4; request++) {
        zend_first_try {
            if (php_request_startup() == FAILURE) {
                atomic_fetch_add(&failures, 1);
                break;
            }
            char code[sizeof(request_code) + 80];
            snprintf(code, sizeof(code), "$token = \"request:%u\\0bytes\"; %s",
                atomic_fetch_add(&request_ids, 1), request_code);
            if (zend_eval_string(code, NULL, "phpext request isolation") == FAILURE || EG(exception)) {
                atomic_fetch_add(&failures, 1);
                if (EG(exception)) { zend_exception_error(EG(exception), E_WARNING); }
            }
            php_request_shutdown(NULL);
        } zend_catch {
            atomic_fetch_add(&failures, 1);
        } zend_end_try();
    }
#ifdef ZTS
    ts_free_thread();
#endif
    return NULL;
}

/* The process initializes PHP once, then tests real RINIT/RSHUTDOWN and final MSHUTDOWN. */
int main(int argc, char **argv)
{
    if (argc != 2) { fprintf(stderr, "usage: embed-test path/to/php.ini\n"); return 2; }
    php_embed_module.php_ini_path_override = argv[1];
    php_embed_module.php_ini_ignore_cwd = 1;
    if (php_embed_init(1, argv) == FAILURE) { return 3; }
    if (getenv("ELEPHC_EMBED_ALLOCATOR_PROBE")) {
        check_malloc_accounting();
    } else if (getenv("ELEPHC_EMBED_FATAL_PHASE")) {
        run_abandoned_requests();
    } else {
#ifdef ZTS
        pthread_t threads[8];
        pthread_attr_t attributes;
        if (pthread_attr_init(&attributes) || pthread_attr_setstacksize(&attributes, 512 * 1024)) { return 5; }
        unsigned started = 0;
        for (; started < 8; started++) {
            if (pthread_create(&threads[started], &attributes, run_requests, NULL)) {
                atomic_fetch_add(&failures, 1);
                break;
            }
        }
        for (unsigned i = 0; i < started; i++) { pthread_join(threads[i], NULL); }
        pthread_attr_destroy(&attributes);
        php_request_shutdown(NULL);
#else
        php_request_shutdown(NULL);
        run_requests(NULL);
#endif
        run_late_userland_fiber_request();
        if (php_request_startup() == FAILURE) { return 4; }
    }
    php_embed_shutdown();
    int result = atomic_load(&failures) ? 1 : 0;
    if (result) {
        fprintf(stderr, "PHP extension request failures: %d\n", atomic_load(&failures));
    }
    puts(result ? "request isolation failed" : "request isolation ok");
    fflush(stdout);
    /* Let Apple's leaks inspect the process after all module/thread/request teardown. */
    if (getenv("ELEPHC_EMBED_LEAK_READY")) { (void)getchar(); }
    return result;
}
