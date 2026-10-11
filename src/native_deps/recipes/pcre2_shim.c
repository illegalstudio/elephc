#include <stdint.h>
#include <stddef.h>
#include <limits.h>
#include <stdlib.h>
#define PCRE2_CODE_UNIT_WIDTH 8
#include <pcre2.h>
#include <pcre2posix.h>

/* Private continuation flag; PCRE2 10.47 handles UTF-8, CRLF and empty retries. */
#define ELEPHC_PCRE2_GLOBAL_NEXT (1u << 16)

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
    int use_startend;
    int64_t start_offset = -1;
    int64_t end_offset = -1;
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
    use_startend = (eflags & REG_STARTEND) != 0 && effective_slots != 0;
    if ((eflags & ELEPHC_PCRE2_GLOBAL_NEXT) != 0 && !use_startend) {
        return (int32_t)REG_INVARG;
    }
    if (use_startend) {
        start_offset = offset_pairs[0];
        end_offset = offset_pairs[1];
        if (start_offset < 0 || end_offset < start_offset
            || start_offset > INT_MAX || end_offset > INT_MAX) {
            return (int32_t)REG_BADPAT;
        }
    }
    for (index = 0; index < slots; ++index) {
        offset_pairs[index * 2] = -1;
        offset_pairs[index * 2 + 1] = -1;
    }
    /* PHP's iteration offset must retain the original subject for anchors,
     * word boundaries and lookbehind. POSIX regexec instead slices at rm_so. */
    if (use_startend) {
        uint32_t options = 0;
        uint32_t next_options = 0;
        PCRE2_SIZE next_offset = (PCRE2_SIZE)start_offset;
        PCRE2_SIZE *vector;
        size_t returned_slots;
        if ((eflags & REG_NOTBOL) != 0) options |= PCRE2_NOTBOL;
        if ((eflags & REG_NOTEOL) != 0) options |= PCRE2_NOTEOL;
        if ((eflags & REG_NOTEMPTY) != 0) options |= PCRE2_NOTEMPTY;
        if ((eflags & ELEPHC_PCRE2_GLOBAL_NEXT) != 0) {
#if PCRE2_MAJOR > 10 || (PCRE2_MAJOR == 10 && PCRE2_MINOR >= 47)
            if (!pcre2_next_match((pcre2_match_data *)handle->regex.re_match_data,
                    &next_offset, &next_options)) {
                return REG_NOMATCH;
            }
#else
            /* Host-only test providers can use older PCRE2. Managed production
             * pins 10.47; no lookaround-BSK extension is enabled in either path. */
            vector = pcre2_get_ovector_pointer((pcre2_match_data *)handle->regex.re_match_data);
            next_offset = vector[1];
            if (vector[0] == vector[1]) {
                if (next_offset >= (PCRE2_SIZE)end_offset) return REG_NOMATCH;
                next_options = PCRE2_NOTEMPTY_ATSTART;
            }
#endif
            options |= next_options;
        }
        result = pcre2_match((const pcre2_code *)handle->regex.re_pcre2_code,
            (PCRE2_SPTR)subject_z, (PCRE2_SIZE)end_offset,
            next_offset, options,
            (pcre2_match_data *)handle->regex.re_match_data, NULL);
        if (result < 0) {
            if (result == PCRE2_ERROR_NOMATCH) return REG_NOMATCH;
            if (result == PCRE2_ERROR_NOMEMORY || result == PCRE2_ERROR_HEAPLIMIT
                || result == PCRE2_ERROR_MATCHLIMIT) return REG_ESPACE;
            return REG_INVARG;
        }
        if ((handle->regex.re_cflags & REG_NOSUB) != 0) return 0;
        vector = pcre2_get_ovector_pointer((pcre2_match_data *)handle->regex.re_match_data);
        returned_slots = result == 0 ? handle->slot_count : (size_t)result;
        if (returned_slots > effective_slots) returned_slots = effective_slots;
        for (index = 0; index < returned_slots; ++index) {
            offset_pairs[index * 2] = vector[index * 2] == PCRE2_UNSET
                ? -1 : (int64_t)vector[index * 2];
            offset_pairs[index * 2 + 1] = vector[index * 2 + 1] == PCRE2_UNSET
                ? -1 : (int64_t)vector[index * 2 + 1];
        }
        return 0;
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

/* Compiles PHP mbstring's caseless MIME selector through the native PCRE2 API.
 * Returns zero on success, a positive compile error, or a negative ABI failure.
 * The caller owns a successful handle until mime_free; no PHP callbacks run. */
int32_t elephc_pcre2_v1_mime_compile(
    void **handle_out,
    const uint8_t *pattern,
    uint64_t length,
    uint64_t *error_offset_out
) {
    pcre2_code *code;
    int error = 0;
    PCRE2_SIZE offset = 0;
    if (handle_out != NULL) {
        *handle_out = NULL;
    }
    if (error_offset_out != NULL) {
        *error_offset_out = 0;
    }
    if (handle_out == NULL || error_offset_out == NULL
        || (pattern == NULL && length != 0) || length > PTRDIFF_MAX) {
        return PCRE2_ERROR_BADDATA;
    }
    code = pcre2_compile(pattern == NULL ? (const uint8_t *)"" : pattern,
        (PCRE2_SIZE)length, PCRE2_CASELESS, &error, &offset, NULL);
    if (code == NULL) {
        *error_offset_out = (uint64_t)offset;
        return (int32_t)error;
    }
    *handle_out = code;
    return 0;
}

/* Returns one for a match, zero for no match, and a negative PCRE2 error otherwise.
 * Byte lengths are explicit; the host applies PHP's C-string boundaries first. */
int32_t elephc_pcre2_v1_mime_match(void *handle, const uint8_t *subject, uint64_t length) {
    pcre2_match_data *data;
    int result;
    if (handle == NULL || (subject == NULL && length != 0) || length > PTRDIFF_MAX) {
        return PCRE2_ERROR_BADDATA;
    }
    data = pcre2_match_data_create_from_pattern((pcre2_code *)handle, NULL);
    if (data == NULL) {
        return PCRE2_ERROR_NOMEMORY;
    }
    result = pcre2_match((pcre2_code *)handle, subject == NULL ? (const uint8_t *)"" : subject,
        (PCRE2_SIZE)length, 0, 0, data, NULL);
    pcre2_match_data_free(data);
    return result >= 0 ? 1 : (result == PCRE2_ERROR_NOMATCH ? 0 : (int32_t)result);
}

/* Releases a native MIME handle through the same allocator that compiled it. */
void elephc_pcre2_v1_mime_free(void *handle) {
    pcre2_code_free((pcre2_code *)handle);
}

/* Copies a NUL-terminated PCRE2 diagnostic and returns its length excluding NUL.
 * A short buffer returns PCRE2_ERROR_NOMEMORY and remains NUL-terminated. */
int32_t elephc_pcre2_v1_error_message(int32_t error, uint8_t *buffer, uint64_t capacity) {
    if (buffer == NULL || capacity > PTRDIFF_MAX) {
        return PCRE2_ERROR_BADDATA;
    }
    if (capacity == 0) {
        return PCRE2_ERROR_NOMEMORY;
    }
    return (int32_t)pcre2_get_error_message((int)error, buffer, (PCRE2_SIZE)capacity);
}
