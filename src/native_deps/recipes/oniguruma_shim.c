/* Opaque Oniguruma 6.9.10 transport for the shared Rust mbstring engine.
 * Encoding IDs and syntax IDs belong to this ABI, never to Oniguruma's structs.
 * Native callbacks do not call PHP. Regex and region owners have separate frees.
 */
#include "elephc_oniguruma.h"
#include <oniguruma.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>

#if ONIGURUMA_VERSION_MAJOR != 6 || ONIGURUMA_VERSION_MINOR != 9 || ONIGURUMA_VERSION_TEENY != 10
#error "The mbregex provider requires the pinned Oniguruma 6.9.10 source"
#endif

enum { ELEPHC_ONIG_INVALID = -100000 };
typedef struct { const OnigUChar *begin, *end; } elephc_onig_name;
typedef struct {
    OnigRegex regex;
    elephc_onig_name *names;
    unsigned int count;
} elephc_onig_regex;

/* Keep the canonical PHP encoding order stable, including its UTF-16/UCS-4 names. */
static OnigEncoding encoding(uint32_t id) {
    OnigEncoding encodings[] = {
        ONIG_ENCODING_EUC_JP, ONIG_ENCODING_UTF8, ONIG_ENCODING_UTF16_BE,
        ONIG_ENCODING_UTF16_LE, ONIG_ENCODING_UTF32_BE, ONIG_ENCODING_UTF32_LE,
        ONIG_ENCODING_SJIS, ONIG_ENCODING_BIG5, ONIG_ENCODING_EUC_CN,
        ONIG_ENCODING_EUC_TW, ONIG_ENCODING_EUC_KR, ONIG_ENCODING_KOI8_R,
        ONIG_ENCODING_ISO_8859_1, ONIG_ENCODING_ISO_8859_2, ONIG_ENCODING_ISO_8859_3,
        ONIG_ENCODING_ISO_8859_4, ONIG_ENCODING_ISO_8859_5, ONIG_ENCODING_ISO_8859_6,
        ONIG_ENCODING_ISO_8859_7, ONIG_ENCODING_ISO_8859_8, ONIG_ENCODING_ISO_8859_9,
        ONIG_ENCODING_ISO_8859_10, ONIG_ENCODING_ISO_8859_11, ONIG_ENCODING_ISO_8859_13,
        ONIG_ENCODING_ISO_8859_14, ONIG_ENCODING_ISO_8859_15, ONIG_ENCODING_ISO_8859_16,
        ONIG_ENCODING_ASCII
    };
    return id < sizeof(encodings) / sizeof(encodings[0]) ? encodings[id] : NULL;
}

static OnigSyntaxType *syntax(uint32_t id) {
    OnigSyntaxType *syntaxes[] = { ONIG_SYNTAX_RUBY, ONIG_SYNTAX_JAVA,
        ONIG_SYNTAX_GNU_REGEX, ONIG_SYNTAX_GREP, ONIG_SYNTAX_EMACS,
        ONIG_SYNTAX_PERL, ONIG_SYNTAX_POSIX_BASIC, ONIG_SYNTAX_POSIX_EXTENDED };
    return id < sizeof(syntaxes) / sizeof(syntaxes[0]) ? syntaxes[id] : NULL;
}

/* Initialization is serialized once by the Rust process-lifetime provider registry. */
static int32_t initialize(void) {
    if (strcmp(onig_version(), "6.9.10") != 0) return ELEPHC_ONIG_INVALID;
    return onig_initialize(NULL, 0);
}

static void free_regex(void *handle) {
    elephc_onig_regex *owner = handle;
    if (!owner) return;
    if (owner->regex) onig_free(owner->regex);
    free(owner->names);
    free(owner);
}

/* Names borrow the compiled pattern metadata, whose owner outlives every access. */
static int collect_name(const OnigUChar *begin, const OnigUChar *end,
        int count, int *groups, OnigRegex regex, void *context) {
    elephc_onig_regex *owner = context;
    (void)count; (void)groups; (void)regex;
    owner->names[owner->count++] = (elephc_onig_name){ begin, end };
    return 0;
}

static int32_t copy_error(int32_t code, OnigErrorInfo *info, uint8_t *buffer, uint64_t capacity) {
    OnigUChar message[ONIG_MAX_ERROR_MESSAGE_LEN];
    if (!buffer || capacity == 0) return ELEPHC_ONIG_INVALID;
    int length = info ? onig_error_code_to_str(message, code, info) : onig_error_code_to_str(message, code);
    if (length < 0 || (uint64_t)length >= capacity) return ELEPHC_ONIG_INVALID;
    memcpy(buffer, message, (size_t)length);
    buffer[length] = 0;
    return length;
}

static int32_t compile(const elephc_onig_compile_v1 *request, void **output,
        uint8_t *error, uint64_t capacity) {
    if (!output) return ELEPHC_ONIG_INVALID;
    *output = NULL;
    if (!request || !error || capacity < ONIG_MAX_ERROR_MESSAGE_LEN || request->reserved
            || request->length > INT_MAX || (!request->pattern && request->length)
            || request->options > 63) return ELEPHC_ONIG_INVALID;
    OnigEncoding enc = encoding(request->encoding);
    OnigSyntaxType *syn = syntax(request->syntax);
    if (!enc || !syn) return ELEPHC_ONIG_INVALID;
    const OnigUChar *pattern = request->pattern ? request->pattern : (const uint8_t *)"";
    if (!ONIGENC_IS_VALID_MBC_STRING(enc, pattern, pattern + request->length)) {
        return ONIGERR_INVALID_WIDE_CHAR_VALUE;
    }
    elephc_onig_regex *owner = calloc(1, sizeof(*owner));
    if (!owner) return ONIGERR_MEMORY;
    OnigErrorInfo info;
    int status = onig_new(&owner->regex, pattern, pattern + request->length,
        request->options, enc, syn, &info);
    if (status != ONIG_NORMAL) {
        if (copy_error(status, &info, error, capacity) < 0) status = ELEPHC_ONIG_INVALID;
        free_regex(owner);
        return status;
    }
    int count = onig_number_of_names(owner->regex);
    if (count > 0) {
        owner->names = calloc((size_t)count, sizeof(*owner->names));
        if (!owner->names) { free_regex(owner); return ONIGERR_MEMORY; }
        onig_foreach_name(owner->regex, collect_name, owner);
    }
    *output = owner;
    return ONIG_NORMAL;
}

static void free_region(void *handle) { if (handle) onig_region_free(handle, 1); }

static int64_t search(void *handle, const elephc_onig_search_v1 *request, void **output) {
    if (!output) return ELEPHC_ONIG_INVALID;
    *output = NULL;
    if (!handle || !request || request->length > INT_MAX || request->offset > request->length
            || (!request->subject && (request->length || (request->flags & ELEPHC_ONIG_SUBJECT_PADDED)))
            || request->anchored > 1 || request->flags > 7
            || ((request->flags & 1) && request->stack_limit > UINT_MAX)
            || ((request->flags & 2) && request->retry_limit > ULONG_MAX)) return ELEPHC_ONIG_INVALID;
    elephc_onig_regex *owner = handle;
    const OnigUChar *subject = request->subject ? request->subject : (const uint8_t *)"";
    const OnigUChar *end = subject + request->length;
    uint8_t *padded = NULL;
    /* Byte offsets can split valid encoded characters. Shared Rust subjects retain their padding;
     * callers without that guarantee receive one bounded copy before native decoder lookahead. */
    if (!(request->flags & ELEPHC_ONIG_SUBJECT_PADDED)) {
        padded = calloc((size_t)request->length + ELEPHC_ONIG_SUBJECT_PADDING, 1);
        if (!padded) return ONIGERR_MEMORY;
        memcpy(padded, subject, (size_t)request->length);
        subject = padded;
        end = subject + request->length;
    }
    OnigRegion *region = onig_region_new();
    OnigMatchParam *params = onig_new_match_param();
    if (!region || !params) {
        free_region(region);
        if (params) onig_free_match_param(params);
        free(padded);
        return ONIGERR_MEMORY;
    }
    if (request->flags & 1) onig_set_match_stack_limit_size_of_match_param(params, (unsigned int)request->stack_limit);
    if (request->flags & 2) onig_set_retry_limit_in_match_of_match_param(params, (unsigned long)request->retry_limit);
    int status = request->anchored
        ? onig_match_with_param(owner->regex, subject, end, subject + request->offset, region, ONIG_OPTION_NONE, params)
        : onig_search_with_param(owner->regex, subject, end, subject + request->offset, end, region, ONIG_OPTION_NONE, params);
    onig_free_match_param(params);
    free(padded);
    if (status < 0) { free_region(region); return status; }
    *output = region;
    return status;
}

static uint64_t region_count(void *handle) {
    return handle ? (uint64_t)((OnigRegion *)handle)->num_regs : 0;
}

static int32_t region_get(void *handle, uint64_t index, int64_t *begin, int64_t *end) {
    if (!handle || !begin || !end || index >= region_count(handle)) return ELEPHC_ONIG_INVALID;
    OnigRegion *region = handle;
    *begin = region->beg[index];
    *end = region->end[index];
    return 0;
}

static uint64_t name_count(void *handle) {
    return handle ? ((elephc_onig_regex *)handle)->count : 0;
}

static int32_t name_get(void *handle, void *region, uint64_t index, elephc_onig_name_v1 *output) {
    if (!handle || !region || !output || index >= name_count(handle)) return ELEPHC_ONIG_INVALID;
    elephc_onig_regex *owner = handle;
    elephc_onig_name name = owner->names[index];
    output->bytes = name.begin;
    output->length = (uint64_t)(name.end - name.begin);
    output->group = onig_name_to_backref_number(owner->regex, name.begin, name.end, region);
    return 0;
}

static int32_t name_lookup(void *handle, void *region, const uint8_t *name, uint64_t length) {
    if (!handle || !region || !name || length > INT_MAX) return ELEPHC_ONIG_INVALID;
    return onig_name_to_backref_number(((elephc_onig_regex *)handle)->regex, name, name + length, region);
}

static int32_t error_message(int32_t code, uint8_t *buffer, uint64_t capacity) {
    if (code == ELEPHC_ONIG_INVALID || code >= 0) return ELEPHC_ONIG_INVALID;
    OnigUChar empty = 0;
    OnigErrorInfo info = { ONIG_ENCODING_UTF8, &empty, &empty };
    return copy_error(code, &info, buffer, capacity);
}

static int32_t numbered_backrefs(void *handle) {
    if (!handle) return ELEPHC_ONIG_INVALID;
    return onig_noname_group_capture_is_active(((elephc_onig_regex *)handle)->regex) ? 1 : 0;
}

/* Cache consumers compare the native mask, including options supplied by the selected syntax. */
static uint32_t compiled_options(void *handle) {
    return handle ? onig_get_options(((elephc_onig_regex *)handle)->regex) : UINT32_MAX;
}

const elephc_onig_provider_v1 *elephc_oniguruma_v1_provider(void) {
    static const elephc_onig_provider_v1 provider = {
        1, sizeof(elephc_onig_provider_v1), initialize, compile, free_regex, search,
        free_region, region_count, region_get, name_count, name_get, name_lookup, numbered_backrefs, error_message,
        compiled_options
    };
    return &provider;
}
