/* Native embedding host for the real mb_parse_str coordinator and PHP reference runtime. */
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#ifndef FILTERED
#define FILTERED 0
#endif
#ifndef CONFIGURED
#define CONFIGURED 1
#endif
#ifndef ARG_COUNT
#define ARG_COUNT 2
#endif
#ifndef STRICT_TYPES
#define STRICT_TYPES 0
#endif
#ifndef CORE_POLICY
#define CORE_POLICY 0
#endif

struct host_string { const unsigned char *bytes; uint64_t length; void *owner; };
struct configuration { struct host_string separators; int64_t max_variables, max_nesting; uint64_t display_errors; };
struct filtered { uint64_t accepted; struct host_string value; };
struct capture { uint64_t mode; void *discarded; };
typedef int32_t (*configuration_fn)(void *, uint32_t, struct configuration *);
typedef int32_t (*filter_fn)(void *, const unsigned char *, uint64_t, const unsigned char *, uint64_t, struct filtered *);
struct native_query { struct capture capture; void *context; configuration_fn configuration; filter_fn filter; };
struct policy { uint64_t marker; int64_t nesting; uint64_t phases[3]; };

_Static_assert(sizeof(struct native_query) == 40, "native query ABI");
_Static_assert(offsetof(struct native_query, context) == 16, "independent policy context");
_Static_assert(offsetof(struct native_query, configuration) == 24, "configuration callback");
_Static_assert(offsetof(struct native_query, filter) == 32, "optional filter callback");
_Static_assert(sizeof(struct configuration) == 48, "configuration result ABI");
_Static_assert(sizeof(struct filtered) == 32, "filter result ABI");

extern int64_t native_invoke(uint32_t, const void *const *, uint64_t, uint32_t, void *, struct native_query *)
    __asm__("__rt_mbstring_query_native");
extern int64_t status_invoke(uint32_t, const void *const *, uint64_t, uint32_t, void *, struct native_query *)
    __asm__("_query_fixture_invoke_status");
static uint64_t observed;

#if CORE_POLICY
extern int32_t elephc_mbstring_query_configuration_v1(void *, uint32_t, struct configuration *);
struct ini_argument { uint64_t kind; int64_t value; const unsigned char *bytes; uint64_t length; };
struct ini_result { uint64_t words[6]; };
typedef int32_t (*diagnostic_fn)(void *, uint32_t, const unsigned char *, uint64_t);
struct ini_host { uint32_t version, size; void *context; diagnostic_fn diagnostic; };
extern int32_t elephc_mbstring_core_ini_v1(uint32_t, const struct ini_argument *, uint64_t, const struct ini_host *, struct ini_result *);
extern void elephc_mbstring_release_v1(struct ini_result *);

/* A valid protected sink; the display_errors handler does not issue diagnostics. */
static int32_t ignore_diagnostic(void *context, uint32_t level, const unsigned char *bytes, uint64_t length) {
    (void)context; (void)level; (void)bytes; (void)length;
    return 0;
}

/* Mutate the actual request's Core setting from a callback before the nesting diagnostic phase. */
static int32_t disable_display(void) {
    const struct ini_argument arguments[] = {
        { 2, 0, (const unsigned char *)"display_errors", 14 },
        { 2, 0, (const unsigned char *)"0", 1 }
    };
    const struct ini_host host = { 1, sizeof(struct ini_host), NULL, ignore_diagnostic };
    struct ini_result result = { {0} };
    int32_t status = elephc_mbstring_core_ini_v1(2, arguments, 2, &host, &result);
    elephc_mbstring_release_v1(&result);
    return status;
}
#endif

/* Read current host policy at every requested phase; the separator lease is static. */
static int32_t configuration(void *context, uint32_t phase, struct configuration *out) {
    struct policy *policy = context;
    if (!policy || policy->marker != UINT64_C(0x7175657279) || phase > 2) return 1;
    policy->phases[phase]++;
#if CORE_POLICY
    return elephc_mbstring_query_configuration_v1(NULL, phase, out);
#else
    *out = (struct configuration){ { (const unsigned char *)"&;", 2, NULL }, 1000, policy->nesting, 1 };
    return 0;
#endif
}

/* Model an installed SAPI filter with rejection, binary forwarding, and live nesting policy. */
static int32_t filter(void *context, const unsigned char *name, uint64_t name_length,
    const unsigned char *value, uint64_t value_length, struct filtered *out) {
    struct policy *policy = context;
    if (!policy || policy->marker != UINT64_C(0x7175657279)) return 1;
    *out = (struct filtered){ 1, { value, value_length, NULL } };
    if (name_length == 4 && !memcmp(name, "skip", 4)) out->accepted = 0;
    if (name_length == 7 && !memcmp(name, "replace", 7)) {
        out->value = (struct host_string){ (const unsigned char *)"filtered", 8, NULL };
    }
    if (name_length == 5 && !memcmp(name, "limit", 5)) {
#if CORE_POLICY
        if (disable_display() != 0) return 1;
#else
        policy->nesting = 1;
#endif
    }
    return 0;
}

int64_t query_fixture(const void *source, void **child) __asm__("_query_invoke_fixture");
/* Source owners survive in the outer PHP frame; this host borrows their existing cells. */
int64_t query_fixture(const void *source, void **child) {
    struct policy policy = { UINT64_C(0x7175657279), 64, {0, 0, 0} };
    struct native_query state = { {0, NULL}, &policy, CONFIGURED ? configuration : NULL, FILTERED ? filter : NULL };
    const void *args[3] = { source, (unsigned char *)child - 8, source };
    observed = 0;
    int64_t result = CONFIGURED
        ? native_invoke(82, args, ARG_COUNT, STRICT_TYPES, NULL, &state)
        : status_invoke(82, args, ARG_COUNT, STRICT_TYPES, NULL, &state);
    observed = policy.phases[0] * 10000 + policy.phases[1] * 100 + policy.phases[2];
    return result;
}

int64_t query_observe(void *mode) __asm__("_query_invoke_observe");
/* Expose only phase counts after the completed invocation has relinquished native ownership. */
int64_t query_observe(void *mode) { (void)mode; return (int64_t)observed; }
