#include <stdint.h>
#include <stddef.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#define PCRE2_CODE_UNIT_WIDTH 8
#include <pcre2.h>
#include <pcre2posix.h>

/* Private continuation flag; PCRE2 10.47 handles UTF-8, CRLF and empty retries. */
#define ELEPHC_PCRE2_GLOBAL_NEXT (1u << 16)
/* Private compile flags for PHP modifiers pcre2posix has no bit for (REG_EXTENDED is 0).
 * They sit above every REG_* compile flag and are cleared before the POSIX record keeps
 * the remaining flags. */
#define ELEPHC_PCRE2_EXTENDED 0x2000
#define ELEPHC_PCRE2_ANCHORED 0x4000
#define ELEPHC_PCRE2_DOLLAR_ENDONLY 0x8000
#define ELEPHC_PCRE2_NO_AUTO_CAPTURE 0x20000
#define ELEPHC_PCRE2_DUPNAMES 0x40000
#define ELEPHC_PCRE2_CASELESS_RESTRICT 0x80000
/* Private /r bit on the filter bridge's raw-PCRE2-option ABI. It is not a POSIX cflag and
 * never reaches preg->re_cflags. */
#define ELEPHC_PCRE2_FILTER_CASELESS_RESTRICT 0x08000000
#define ELEPHC_PCRE2_PRIVATE_COMPILE_FLAGS (ELEPHC_PCRE2_EXTENDED | ELEPHC_PCRE2_ANCHORED \
    | ELEPHC_PCRE2_DOLLAR_ENDONLY | ELEPHC_PCRE2_NO_AUTO_CAPTURE | ELEPHC_PCRE2_DUPNAMES \
    | ELEPHC_PCRE2_CASELESS_RESTRICT)
/* Private exec statuses for PCRE2 match errors POSIX has no code for. They sit above
 * every REG_* value so PHP's PREG_RECURSION_LIMIT_ERROR and PREG_BAD_UTF8_OFFSET_ERROR
 * survive the opaque ABI. */
#define ELEPHC_PCRE2_DEPTH_LIMIT 0x103
#define ELEPHC_PCRE2_BAD_UTF_OFFSET 0x105

/* Maps a failed pcre2_match() result to the exec ABI status, keeping each category
 * php-src's pcre_handle_exec_error() tells apart: the match limit (REG_ESPACE), the
 * depth limit, malformed UTF-8 (REG_INVARG), a bad UTF-8 offset, and everything else,
 * heap limit and out-of-memory included, as an internal error (REG_ASSERT). */
static int32_t elephc_pcre2_match_status(int result)
{
    if (result == PCRE2_ERROR_NOMATCH) return REG_NOMATCH;
    if (result == PCRE2_ERROR_MATCHLIMIT) return REG_ESPACE;
    if (result == PCRE2_ERROR_DEPTHLIMIT) return ELEPHC_PCRE2_DEPTH_LIMIT;
    if (result == PCRE2_ERROR_BADUTFOFFSET) return ELEPHC_PCRE2_BAD_UTF_OFFSET;
    if (result <= PCRE2_ERROR_UTF8_ERR1 && result >= PCRE2_ERROR_UTF8_ERR21) return REG_INVARG;
    return REG_ASSERT;
}

/* Compiles one pattern with character tables for the current libc locale.
 *
 * PHP's pcre extension does not change LC_CTYPE; PCRE2 receives the locale's ctype
 * tables explicitly. pcre2_code_copy_with_tables gives the compiled code its own copy
 * so the temporary table block can be freed before returning. */
static pcre2_code *elephc_pcre2_compile_locale(
    const uint8_t *pattern,
    PCRE2_SIZE length,
    uint32_t options,
    uint32_t extra_options,
    int *errorcode,
    PCRE2_SIZE *erroffset)
{
    const uint8_t *tables = pcre2_maketables(NULL);
    pcre2_compile_context *context = pcre2_compile_context_create(NULL);
    pcre2_code *code;

    if (context != NULL && tables != NULL) {
        pcre2_set_character_tables(context, tables);
    }
    if (extra_options != 0) {
        if (context == NULL) {
            *errorcode = PCRE2_ERROR_NOMEMORY;
            *erroffset = 0;
            if (tables != NULL) {
                pcre2_maketables_free(NULL, tables);
            }
            return NULL;
        }
        pcre2_set_compile_extra_options(context, extra_options);
    }
    code = pcre2_compile((PCRE2_SPTR)pattern, length, options,
        errorcode, erroffset, context);
    if (context != NULL) {
        pcre2_compile_context_free(context);
    }
    if (code != NULL && tables != NULL) {
        pcre2_code *owned = pcre2_code_copy_with_tables(code);
        if (owned == NULL) {
            pcre2_code_free(code);
            code = NULL;
            *errorcode = PCRE2_ERROR_NOMEMORY;
        } else {
            pcre2_code_free(code);
            code = owned;
        }
    }
    if (tables != NULL) {
        pcre2_maketables_free(NULL, tables);
    }
    return code;
}

/* Compiles a PHP pattern while filling the same POSIX regex_t pcre2_regcomp uses.
 * The private flags carry PHP modifiers as compile options, like php-src:
 * /x PCRE2_EXTENDED, /A PCRE2_ANCHORED, /D PCRE2_DOLLAR_ENDONLY,
 * /n PCRE2_NO_AUTO_CAPTURE, /J PCRE2_DUPNAMES and /r PCRE2_EXTRA_CASELESS_RESTRICT.
 * An option group inserted before the pattern would hide (*UTF), (*ANYCRLF), (*CRLF),
 * and (*LIMIT_...) verbs, which PCRE2 accepts only at the very start. */
static int elephc_pcre2_compile_php_bounded(
    regex_t *preg, const uint8_t *pattern, PCRE2_SIZE length, int cflags)
{
    uint32_t options = 0;
    uint32_t extra_options = 0;
    uint32_t capture_count = 0;
    int errorcode = 0;
    PCRE2_SIZE erroffset = 0;
    pcre2_code *code;

    if ((cflags & REG_ICASE) != 0) options |= PCRE2_CASELESS;
    if ((cflags & REG_NEWLINE) != 0) options |= PCRE2_MULTILINE;
    if ((cflags & REG_DOTALL) != 0) options |= PCRE2_DOTALL;
    if ((cflags & REG_NOSPEC) != 0) options |= PCRE2_LITERAL;
    if ((cflags & REG_UTF) != 0) options |= PCRE2_UTF;
    if ((cflags & REG_UCP) != 0) options |= PCRE2_UCP;
    if ((cflags & REG_UNGREEDY) != 0) options |= PCRE2_UNGREEDY;
    if ((cflags & ELEPHC_PCRE2_EXTENDED) != 0) options |= PCRE2_EXTENDED;
    if ((cflags & ELEPHC_PCRE2_ANCHORED) != 0) options |= PCRE2_ANCHORED;
    if ((cflags & ELEPHC_PCRE2_DOLLAR_ENDONLY) != 0) options |= PCRE2_DOLLAR_ENDONLY;
    if ((cflags & ELEPHC_PCRE2_NO_AUTO_CAPTURE) != 0) options |= PCRE2_NO_AUTO_CAPTURE;
    if ((cflags & ELEPHC_PCRE2_DUPNAMES) != 0) options |= PCRE2_DUPNAMES;
#ifdef PCRE2_EXTRA_CASELESS_RESTRICT
    /* php-src maps /r only when PCRE2 has the option (10.43+); the pinned 10.47 does. */
    if ((cflags & ELEPHC_PCRE2_CASELESS_RESTRICT) != 0) {
        extra_options |= PCRE2_EXTRA_CASELESS_RESTRICT;
    }
#endif

    preg->re_cflags = cflags & ~ELEPHC_PCRE2_PRIVATE_COMPILE_FLAGS;
    preg->re_pcre2_code = NULL;
    preg->re_match_data = NULL;
    code = elephc_pcre2_compile_locale(pattern, length,
        options, extra_options, &errorcode, &erroffset);
    preg->re_erroffset = erroffset;
    if (code == NULL) {
        (void)errorcode;
        return REG_BADPAT;
    }
    if (pcre2_pattern_info(code, PCRE2_INFO_CAPTURECOUNT, &capture_count) != 0
        || capture_count == UINT32_MAX) {
        pcre2_code_free(code);
        return REG_BADPAT;
    }
    preg->re_nsub = (size_t)capture_count;
    preg->re_pcre2_code = code;
    preg->re_match_data = pcre2_match_data_create(capture_count + 1, NULL);
    if (preg->re_match_data == NULL) {
        pcre2_code_free(code);
        preg->re_pcre2_code = NULL;
        return REG_ESPACE;
    }
    preg->re_erroffset = (size_t)(-1);
    return 0;
}

/* C-string compatibility entry point for the preg helpers that still pass a NUL-terminated
 * pattern. New code that owns an explicit length uses elephc_pcre2_compile_php_bounded. */
static int elephc_pcre2_compile_php(regex_t *preg, const char *pattern, int cflags)
{
    return elephc_pcre2_compile_php_bounded(
        preg, (const uint8_t *)pattern, PCRE2_ZERO_TERMINATED, cflags);
}

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
    result = elephc_pcre2_compile_php(&handle->regex, pattern_z, (int)cflags);
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

/* Compiles one pattern body with an authoritative byte length. Embedded NUL bytes are part
 * of the pattern, so this path never falls back to strlen. */
int32_t elephc_pcre2_v1_compile_len(
    void **handle_out,
    const uint8_t *pattern,
    uint64_t length,
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
    if (handle_out == NULL || match_slot_count_out == NULL
        || (pattern == NULL && length != 0) || length > PTRDIFF_MAX || cflags > INT_MAX) {
        return (int32_t)REG_BADPAT;
    }

    handle = (elephc_pcre2_v1_handle *)calloc(1, sizeof(*handle));
    if (handle == NULL) {
        return (int32_t)REG_ESPACE;
    }
    result = elephc_pcre2_compile_php_bounded(&handle->regex,
        pattern == NULL ? (const uint8_t *)"" : pattern, (PCRE2_SIZE)length, (int)cflags);
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
    size_t slots;
    size_t effective_slots;
    size_t index;
    int use_startend;
    int64_t start_offset = 0;
    int64_t end_offset;
    uint32_t options = 0;
    uint32_t next_options = 0;
    PCRE2_SIZE next_offset;
    PCRE2_SIZE *vector;
    size_t returned_slots;
    int result;

    if (handle == NULL || subject_z == NULL || (requested_slots != 0 && offset_pairs == NULL) || eflags > INT_MAX) {
        return (int32_t)REG_BADPAT;
    }
    if (requested_slots > SIZE_MAX || requested_slots > SIZE_MAX / (2 * sizeof(int64_t))) {
        return (int32_t)REG_ESPACE;
    }
    slots = (size_t)requested_slots;
    effective_slots = slots < handle->slot_count ? slots : handle->slot_count;
    use_startend = (eflags & REG_STARTEND) != 0 && effective_slots != 0;
    if ((eflags & ELEPHC_PCRE2_GLOBAL_NEXT) != 0 && !use_startend) {
        return (int32_t)REG_ASSERT;
    }
    if (use_startend) {
        start_offset = offset_pairs[0];
        end_offset = offset_pairs[1];
        if (start_offset < 0 || end_offset < start_offset
            || start_offset > INT_MAX || end_offset > INT_MAX) {
            return (int32_t)REG_BADPAT;
        }
    } else {
        end_offset = (int64_t)strlen(subject_z);
    }
    for (index = 0; index < slots; ++index) {
        offset_pairs[index * 2] = -1;
        offset_pairs[index * 2 + 1] = -1;
    }
    /* Every exec calls pcre2_match() directly, never pcre2_regexec(): POSIX folds the
     * depth limit into REG_ASSERT and the heap limit into REG_ESPACE, which would lose
     * PHP's preg_last_error() code. A REG_STARTEND search also keeps the original
     * subject for anchors, word boundaries and lookbehind, where POSIX regexec slices
     * at rm_so. */
    if ((eflags & REG_NOTBOL) != 0) options |= PCRE2_NOTBOL;
    if ((eflags & REG_NOTEOL) != 0) options |= PCRE2_NOTEOL;
    if ((eflags & REG_NOTEMPTY) != 0) options |= PCRE2_NOTEMPTY;
    next_offset = (PCRE2_SIZE)start_offset;
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
        return elephc_pcre2_match_status(result);
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
    code = elephc_pcre2_compile_locale(
        pattern == NULL ? (const uint8_t *)"" : pattern,
        (PCRE2_SIZE)length, PCRE2_CASELESS, 0, &error, &offset);
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

/* Compiles one PHP regexp body for the filter bridge's diagnostics. Returns zero on success,
 * a positive diagnostic length in `message`, or a negative PCRE2 error on an ABI failure.
 * The compile uses the same locale character tables as matching, so a pattern PCRE2 accepts
 * here cannot fail during the later match because of different ctype tables. */
int32_t elephc_pcre2_v1_php_compile_check(
    const uint8_t *pattern,
    uint64_t length,
    uint32_t options,
    uint8_t *message,
    uint64_t capacity,
    uint64_t *offset_out
) {
    int errorcode = 0;
    PCRE2_SIZE erroffset = 0;
    uint32_t extra_options = 0;
    pcre2_code *code;

    if (message == NULL || capacity > PTRDIFF_MAX || (pattern == NULL && length != 0)
        || length > PTRDIFF_MAX) {
        return PCRE2_ERROR_BADDATA;
    }
    if (offset_out != NULL) {
        *offset_out = 0;
    }
    if ((options & ELEPHC_PCRE2_FILTER_CASELESS_RESTRICT) != 0) {
        options &= ~ELEPHC_PCRE2_FILTER_CASELESS_RESTRICT;
#ifdef PCRE2_EXTRA_CASELESS_RESTRICT
        extra_options = PCRE2_EXTRA_CASELESS_RESTRICT;
#endif
    }
    code = elephc_pcre2_compile_locale(pattern == NULL ? (const uint8_t *)"" : pattern,
        (PCRE2_SIZE)length, options, extra_options, &errorcode, &erroffset);
    if (code != NULL) {
        pcre2_code_free(code);
        return 0;
    }
    if (offset_out != NULL) {
        *offset_out = (uint64_t)erroffset;
    }
    return (int32_t)pcre2_get_error_message(errorcode, message, (PCRE2_SIZE)capacity);
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
