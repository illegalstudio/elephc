#include <stdint.h>
#include <stddef.h>
#include <limits.h>
#include <stdlib.h>
#include <pcre2posix.h>

typedef struct elephc_pcre2_v1_handle {
    regex_t regex;
    size_t slot_count;
} elephc_pcre2_v1_handle;

int32_t elephc_pcre2_v1_compile(
    void **handle_out,
    const char *pattern_z,
    uint32_t cflags,
    uint64_t *match_slot_count_out
) {
    elephc_pcre2_v1_handle *handle;
    int result;

    if (handle_out != NULL) {
        *handle_out = NULL;
    }
    if (match_slot_count_out != NULL) {
        *match_slot_count_out = 0;
    }
    if (handle_out == NULL || match_slot_count_out == NULL || pattern_z == NULL || cflags > INT_MAX) {
        return (int32_t)REG_BADPAT;
    }

    handle = (elephc_pcre2_v1_handle *)calloc(1, sizeof(*handle));
    if (handle == NULL) {
        return (int32_t)REG_ESPACE;
    }
    result = pcre2_regcomp(&handle->regex, pattern_z, (int)cflags);
    if (result != 0) {
        free(handle);
        return (int32_t)result;
    }
    if (handle->regex.re_nsub == SIZE_MAX) {
        pcre2_regfree(&handle->regex);
        free(handle);
        return (int32_t)REG_ESPACE;
    }
    handle->slot_count = handle->regex.re_nsub + 1;
#if SIZE_MAX > UINT64_MAX
    if (handle->slot_count > UINT64_MAX) {
        pcre2_regfree(&handle->regex);
        free(handle);
        return (int32_t)REG_ESPACE;
    }
#endif
    *match_slot_count_out = (uint64_t)handle->slot_count;
    *handle_out = handle;
    return 0;
}

int32_t elephc_pcre2_v1_exec(
    void *opaque_handle,
    const char *subject_z,
    uint64_t requested_slots,
    int64_t *offset_pairs,
    uint32_t eflags
) {
    elephc_pcre2_v1_handle *handle = (elephc_pcre2_v1_handle *)opaque_handle;
    regmatch_t *matches = NULL;
    size_t slots;
    size_t effective_slots;
    size_t index;
    int result;

    if (handle == NULL || subject_z == NULL || (requested_slots != 0 && offset_pairs == NULL) || eflags > INT_MAX) {
        return (int32_t)REG_BADPAT;
    }
    if (requested_slots > SIZE_MAX || requested_slots > SIZE_MAX / sizeof(regmatch_t)
        || requested_slots > SIZE_MAX / (2 * sizeof(int64_t))) {
        return (int32_t)REG_ESPACE;
    }
    slots = (size_t)requested_slots;
    effective_slots = slots < handle->slot_count ? slots : handle->slot_count;
    for (index = 0; index < slots; ++index) {
        offset_pairs[index * 2] = -1;
        offset_pairs[index * 2 + 1] = -1;
    }
    if (effective_slots != 0) {
        matches = (regmatch_t *)malloc(effective_slots * sizeof(*matches));
        if (matches == NULL) {
            return (int32_t)REG_ESPACE;
        }
        for (index = 0; index < effective_slots; ++index) {
            matches[index].rm_so = -1;
            matches[index].rm_eo = -1;
        }
    }
    result = pcre2_regexec(&handle->regex, subject_z, effective_slots, matches, (int)eflags);
    if (result == 0) {
        for (index = 0; index < effective_slots; ++index) {
            offset_pairs[index * 2] = (int64_t)matches[index].rm_so;
            offset_pairs[index * 2 + 1] = (int64_t)matches[index].rm_eo;
        }
    }
    free(matches);
    return (int32_t)result;
}

void elephc_pcre2_v1_free(void *opaque_handle) {
    elephc_pcre2_v1_handle *handle = (elephc_pcre2_v1_handle *)opaque_handle;
    if (handle == NULL) {
        return;
    }
    pcre2_regfree(&handle->regex);
    free(handle);
}
