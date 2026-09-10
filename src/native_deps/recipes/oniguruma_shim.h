/* Versioned mbregex provider. Handles own their allocator and never execute PHP. */
#ifndef ELEPHC_ONIGURUMA_V1_H
#define ELEPHC_ONIGURUMA_V1_H
#include <stdint.h>
#define ELEPHC_ONIG_SUBJECT_PADDING 32
#define ELEPHC_ONIG_SUBJECT_PADDED 4

typedef struct {
    const uint8_t *pattern;
    uint64_t length;
    uint32_t encoding, options, syntax, reserved;
} elephc_onig_compile_v1;

typedef struct {
    const uint8_t *subject;
    uint64_t length, offset;
    /* Bits 0/1 select limits; bit 2 guarantees SUBJECT_PADDING readable zero bytes after length. */
    uint32_t anchored, flags;
    uint64_t stack_limit, retry_limit;
} elephc_onig_search_v1;

typedef struct {
    const uint8_t *bytes;
    uint64_t length;
    int64_t group;
} elephc_onig_name_v1;

typedef struct {
    uint32_t version, size;
    int32_t (*initialize)(void);
    int32_t (*compile)(const elephc_onig_compile_v1 *, void **, uint8_t *, uint64_t);
    void (*free_regex)(void *);
    int64_t (*search)(void *, const elephc_onig_search_v1 *, void **);
    void (*free_region)(void *);
    uint64_t (*region_count)(void *);
    int32_t (*region_get)(void *, uint64_t, int64_t *, int64_t *);
    uint64_t (*name_count)(void *);
    int32_t (*name_get)(void *, void *, uint64_t, elephc_onig_name_v1 *);
    int32_t (*name_lookup)(void *, void *, const uint8_t *, uint64_t);
    int32_t (*numbered_backrefs)(void *);
    int32_t (*error)(int32_t, uint8_t *, uint64_t);
    uint32_t (*compiled_options)(void *);
} elephc_onig_provider_v1;

const elephc_onig_provider_v1 *elephc_oniguruma_v1_provider(void);
#endif
