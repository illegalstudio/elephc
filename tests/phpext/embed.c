/* Real PHP embed SAPI: repeated requests, with overlapping ZTS requests on eight threads. */
#include <sapi/embed/php_embed.h>
#include <Zend/zend_exceptions.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>

static atomic_int failures;

static const char request_code[] =
    "for ($i = 1, $limit = (int)(getenv('ELEPHC_EMBED_ITERATIONS') ?: 24); $i <= $limit; $i++) {"
    "  if (phpext_counter() !== $i) { throw new Exception('request static leaked'); }"
    "  $value = ['binary' => \"a\\0b\", 7 => [null, true, 1.5]];"
    "  if (phpext_mirror($value) !== $value) { throw new Exception('array corrupted'); }"
    "  if (phpext_label(\"x\\0y\") !== \"label:x\\0y\") { throw new Exception('string corrupted'); }"
    "  try { phpext_failure(); throw new Exception('missing native exception'); }"
    "  catch (RuntimeException $e) { if ($e->getCode() !== 42) { throw new Exception('wrong exception'); } }"
    "}"
    "try { phpext_recurse(100000); throw new Exception('missing native stack guard'); }"
    "catch (Error $e) { if (!str_contains($e->getMessage(), 'runtime failed')) { throw $e; } }"
    "if (phpext_counter() !== $limit + 1) { throw new Exception('failed stack recovery'); }";

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
            if (zend_eval_string(request_code, NULL, "phpext request isolation") == FAILURE || EG(exception)) {
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
#else
    php_request_shutdown(NULL);
    run_requests(NULL);
    if (php_request_startup() == FAILURE) { return 4; }
#endif
    php_embed_shutdown();
    if (atomic_load(&failures)) {
        fprintf(stderr, "PHP extension request failures: %d\n", atomic_load(&failures));
        return 1;
    }
    puts("request isolation ok");
    return 0;
}
