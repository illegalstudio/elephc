#include <stdint.h>
#include <stddef.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define PCRE2_CODE_UNIT_WIDTH 8
#include <pcre2.h>
#include <pcre2posix.h>

/* Private continuation flag (an exec eflag); PCRE2 10.47 handles UTF-8, CRLF and empty retries.
   The compile cflags below start at 0x2000 and never meet it: eflags and cflags are separate
   words. */
#define ELEPHC_PCRE2_GLOBAL_NEXT (1u << 16)
#define ELEPHC_PCRE2_CFLAG_ANCHORED         0x2000U
#define ELEPHC_PCRE2_CFLAG_EXTENDED         0x4000U
#define ELEPHC_PCRE2_CFLAG_DOLLAR_ENDONLY   0x8000U
#define ELEPHC_PCRE2_CFLAG_DUPNAMES         0x10000U
#define ELEPHC_PCRE2_CFLAG_NO_AUTO_CAPTURE  0x20000U

typedef struct elephc_pcre2_v1_handle {
    regex_t regex;
    size_t slot_count;
    int anchored;
    /* 0 when the compiled pattern belongs to the cache below: freeing the handle then releases
       only its match data. */
    int owns_code;
} elephc_pcre2_v1_handle;

/* Compiled patterns kept for the life of the process, as php's own PCRE cache keeps them.
 *
 * Every preg_* call compiles its pattern, and a request of a real application calls preg_* on
 * the same few hundred patterns again and again: Symfony's router and Twig's lexer compiled the
 * same patterns on every request, which cost more than matching them did. A compiled pattern is
 * read-only while it matches, so one can serve every handle; each handle still gets its own
 * match data, so a pattern used again while a match on it is in progress (a preg_replace_callback
 * callback running the same pattern) never shares state with the outer call.
 *
 * Open addressing over the pattern bytes and the compile flags, at most half full. Once
 * ELEPHC_PCRE2_CACHE_LIMIT patterns are cached, further patterns compile for their handle alone,
 * which bounds the memory a program building patterns from its data can pin. A pattern that
 * fails to compile is never cached, so its warning is raised on every call, as php raises it. */
#define ELEPHC_PCRE2_CACHE_SLOTS 8192U
#define ELEPHC_PCRE2_CACHE_LIMIT 4096U

typedef struct elephc_pcre2_v1_cached {
    char *pattern;
    size_t length;
    uint32_t cflags;
    pcre2_code *code;
    uint32_t capture_count;
} elephc_pcre2_v1_cached;

static elephc_pcre2_v1_cached *elephc_pcre2_v1_cache = NULL;
static size_t elephc_pcre2_v1_cache_count = 0;

/* FNV-1a over the pattern bytes, then the flags, so one pattern under two modifier sets is two
   entries. */
static size_t elephc_pcre2_v1_cache_home(const char *pattern, size_t length, uint32_t cflags) {
    uint64_t hash = 0xcbf29ce484222325ULL;
    size_t index;

    for (index = 0; index < length; ++index) {
        hash = (hash ^ (unsigned char)pattern[index]) * 0x100000001b3ULL;
    }
    hash = (hash ^ cflags) * 0x100000001b3ULL;
    return (size_t)(hash & (ELEPHC_PCRE2_CACHE_SLOTS - 1U));
}

/* Returns the slot holding the pattern, or the empty slot that ends its probe, or NULL when the
   cache could not be allocated. */
static elephc_pcre2_v1_cached *elephc_pcre2_v1_cache_slot(const char *pattern, size_t length, uint32_t cflags) {
    size_t slot;

    if (elephc_pcre2_v1_cache == NULL) {
        elephc_pcre2_v1_cache = (elephc_pcre2_v1_cached *)calloc(ELEPHC_PCRE2_CACHE_SLOTS, sizeof(*elephc_pcre2_v1_cache));
        if (elephc_pcre2_v1_cache == NULL) {
            return NULL;
        }
    }
    slot = elephc_pcre2_v1_cache_home(pattern, length, cflags);
    while (elephc_pcre2_v1_cache[slot].code != NULL) {
        const elephc_pcre2_v1_cached *entry = &elephc_pcre2_v1_cache[slot];
        if (entry->cflags == cflags && entry->length == length && memcmp(entry->pattern, pattern, length) == 0) {
            break;
        }
        slot = (slot + 1U) & (ELEPHC_PCRE2_CACHE_SLOTS - 1U);
    }
    return &elephc_pcre2_v1_cache[slot];
}

/* php's preg_last_error() code for the most recent compile or match: 0 none, 1 internal,
   2 backtrack limit, 3 recursion limit, 4 bad UTF-8, 5 bad UTF-8 offset. A match on a subject
   that is not valid UTF-8 under /u fails in PCRE2 itself; the runtime helpers read that as
   "no more matches" and returned the subject, while php returns null (preg_replace) or false
   (preg_match). Callers typed for the failure read this after the call. */
static int32_t elephc_pcre2_v1_error = 0;

static int32_t elephc_pcre2_v1_error_from_match(int rc) {
    if (rc >= 0 || rc == PCRE2_ERROR_NOMATCH) {
        return 0;
    }
    if (rc <= PCRE2_ERROR_UTF8_ERR1 && rc >= PCRE2_ERROR_UTF8_ERR21) {
        return 4;
    }
    if (rc == PCRE2_ERROR_BADUTFOFFSET) {
        return 5;
    }
    if (rc == PCRE2_ERROR_MATCHLIMIT) {
        return 2;
    }
    if (rc == PCRE2_ERROR_DEPTHLIMIT || rc == PCRE2_ERROR_HEAPLIMIT) {
        return 3;
    }
    return 1;
}

/* Frees a handle and its match data, and its compiled pattern unless the cache holds it. */
static void elephc_pcre2_v1_release(elephc_pcre2_v1_handle *handle) {
    if (handle->owns_code) {
        pcre2_regfree(&handle->regex);
    } else {
        pcre2_match_data_free((pcre2_match_data *)handle->regex.re_match_data);
    }
    free(handle);
}

int32_t elephc_pcre2_v1_last_error(void) {
    return elephc_pcre2_v1_error;
}

/* Forgets the previous call's error. `preg_grep()` compiles once per ELEMENT, so over an empty
   array nothing would reset the code the call's own result is decided by. */
void elephc_pcre2_v1_clear_error(void) {
    elephc_pcre2_v1_error = 0;
}

/* php's preg_last_error_msg() text for each preg_last_error() code. */
void elephc_pcre2_v1_last_error_msg(const char **message_out, uint64_t *length_out) {
    static const char *const messages[] = {
        "No error",
        "Internal error",
        "Backtrack limit exhausted",
        "Recursion limit exhausted",
        "Malformed UTF-8 characters, possibly incorrectly encoded",
        "The offset did not correspond to the beginning of a valid UTF-8 code point",
        "JIT stack limit exhausted",
    };
    const char *message = "Internal error";

    if (elephc_pcre2_v1_error >= 0
        && (size_t)elephc_pcre2_v1_error < sizeof(messages) / sizeof(messages[0])) {
        message = messages[elephc_pcre2_v1_error];
    }
    if (message_out != NULL) {
        *message_out = message;
    }
    if (length_out != NULL) {
        *length_out = (uint64_t)strlen(message);
    }
}

/* Why the most recent compile failed, as PCRE2 words it, or an empty string when it did not.
   php reports a pattern PCRE2 rejects as `<function>(): Compilation failed: <message> at offset
   <n>` and the call returns false or null; the runtime asks for that text right after the call,
   so one static record of the last failure is enough. */
static int elephc_pcre2_v1_compile_failed = 0;
static int elephc_pcre2_v1_compile_errorcode = 0;
static PCRE2_SIZE elephc_pcre2_v1_compile_erroffset = 0;
static char elephc_pcre2_v1_warning[512];

/* Pattern errors php reports before PCRE2 ever sees the pattern. The runtime's delimiter
   stripper records one, and the compile that always follows it fails with that reason instead
   of compiling whatever bytes the stripper handed on. */
#define ELEPHC_PCRE2_PATTERN_EMPTY 1
#define ELEPHC_PCRE2_PATTERN_DELIMITER 2
#define ELEPHC_PCRE2_PATTERN_NO_END 3
#define ELEPHC_PCRE2_PATTERN_MODIFIER 4
static int32_t elephc_pcre2_v1_pending_kind = 0;
static uint32_t elephc_pcre2_v1_pending_byte = 0;
static int32_t elephc_pcre2_v1_failure_kind = 0;
static uint32_t elephc_pcre2_v1_failure_byte = 0;

/* Records a delimiter or modifier error for the next compile to report. */
void elephc_pcre2_v1_record_pattern_error(int32_t kind, uint32_t byte) {
    elephc_pcre2_v1_pending_kind = kind;
    elephc_pcre2_v1_pending_byte = byte & 0xffU;
}

/* Builds php's whole compile-failure warning line (`Warning: ...\n`, the form the runtime and
   interpreter warning writers take) for `function_name` when the last compile failed.
   Returns 1 with the message, or 0 (and an empty message) when it succeeded. */
int32_t elephc_pcre2_v1_compile_warning(
    const char *function_name,
    uint64_t function_length,
    const char **message_out,
    uint64_t *length_out
) {
    PCRE2_UCHAR reason[256];
    int written;

    if (message_out != NULL) {
        *message_out = "";
    }
    if (length_out != NULL) {
        *length_out = 0;
    }
    if (!elephc_pcre2_v1_compile_failed || message_out == NULL || length_out == NULL) {
        return 0;
    }
    elephc_pcre2_v1_compile_failed = 0;
    if (function_length > 128) {
        function_length = 128;
    }
    if (function_name == NULL) {
        function_name = "";
        function_length = 0;
    }
    switch (elephc_pcre2_v1_failure_kind) {
    case ELEPHC_PCRE2_PATTERN_EMPTY:
        written = snprintf(elephc_pcre2_v1_warning, sizeof(elephc_pcre2_v1_warning),
                           "Warning: %.*s(): Empty regular expression\n",
                           (int)function_length, function_name);
        break;
    case ELEPHC_PCRE2_PATTERN_DELIMITER:
        written = snprintf(elephc_pcre2_v1_warning, sizeof(elephc_pcre2_v1_warning),
                           "Warning: %.*s(): Delimiter must not be alphanumeric, backslash, or NUL byte\n",
                           (int)function_length, function_name);
        break;
    case ELEPHC_PCRE2_PATTERN_NO_END:
        written = snprintf(elephc_pcre2_v1_warning, sizeof(elephc_pcre2_v1_warning),
                           "Warning: %.*s(): No ending delimiter '%c' found\n",
                           (int)function_length, function_name, (int)elephc_pcre2_v1_failure_byte);
        break;
    case ELEPHC_PCRE2_PATTERN_MODIFIER:
        written = snprintf(elephc_pcre2_v1_warning, sizeof(elephc_pcre2_v1_warning),
                           "Warning: %.*s(): Unknown modifier '%c'\n",
                           (int)function_length, function_name, (int)elephc_pcre2_v1_failure_byte);
        break;
    default:
        if (pcre2_get_error_message(elephc_pcre2_v1_compile_errorcode, reason, sizeof(reason)) < 0) {
            reason[0] = '\0';
        }
        written = snprintf(elephc_pcre2_v1_warning, sizeof(elephc_pcre2_v1_warning),
                           "Warning: %.*s(): Compilation failed: %s at offset %lu\n",
                           (int)function_length, function_name,
                           (const char *)reason, (unsigned long)elephc_pcre2_v1_compile_erroffset);
        break;
    }
    if (written < 0) {
        return 0;
    }
    if ((size_t)written >= sizeof(elephc_pcre2_v1_warning)) {
        written = (int)sizeof(elephc_pcre2_v1_warning) - 1;
    }
    *message_out = elephc_pcre2_v1_warning;
    *length_out = (uint64_t)written;
    return 1;
}

int32_t elephc_pcre2_v1_compile(
    void **handle_out,
    const char *pattern_z,
    uint32_t cflags,
    uint64_t *match_slot_count_out
) {
    elephc_pcre2_v1_handle *handle;
    uint32_t native_options;
    pcre2_code *code;
    pcre2_match_data *match_data;
    uint32_t capture_count;
    int errorcode;
    PCRE2_SIZE erroffset;
    size_t pattern_length;
    elephc_pcre2_v1_cached *cached;

    elephc_pcre2_v1_error = 0;
    elephc_pcre2_v1_compile_failed = 0;
    if (handle_out != NULL) {
        *handle_out = NULL;
    }
    if (match_slot_count_out != NULL) {
        *match_slot_count_out = 0;
    }
    if (handle_out == NULL || match_slot_count_out == NULL || pattern_z == NULL) {
        elephc_pcre2_v1_error = 1;
        return (int32_t)REG_BADPAT;
    }
    if (elephc_pcre2_v1_pending_kind != 0) {
        elephc_pcre2_v1_failure_kind = elephc_pcre2_v1_pending_kind;
        elephc_pcre2_v1_failure_byte = elephc_pcre2_v1_pending_byte;
        elephc_pcre2_v1_pending_kind = 0;
        elephc_pcre2_v1_compile_failed = 1;
        elephc_pcre2_v1_error = 1;
        return (int32_t)REG_BADPAT;
    }

    handle = (elephc_pcre2_v1_handle *)calloc(1, sizeof(*handle));
    if (handle == NULL) {
        return (int32_t)REG_ESPACE;
    }
    handle->anchored = (cflags & ELEPHC_PCRE2_CFLAG_ANCHORED) != 0;

    /* Compile through native pcre2_compile() instead of delegating to
       pcre2_regcomp(). The POSIX wrapper's fixed REG_* cflag set has no room
       for PHP's x/D/J/n modifiers: none of PCRE2_EXTENDED, PCRE2_DOLLAR_ENDONLY,
       PCRE2_DUPNAMES, or PCRE2_NO_AUTO_CAPTURE map through any bit
       pcre2posix.h defines (confirmed against the installed header: only
       CASELESS/MULTILINE/DOTALL/UTF/UNGREEDY/UCP/LITERAL have POSIX bits, and
       REG_NOSUB does NOT map to PCRE2_NO_AUTO_CAPTURE — empirically it forces
       regexec() to report zero match positions instead, which would have
       silently broken $matches[0]). Translating every existing bit ourselves
       and wrapping the result in the same regex_t shape pcre2_regexec() and
       pcre2_regfree() already expect keeps every previously-supported
       modifier's target PCRE2_* option identical to what pcre2_regcomp() set,
       while adding the four PHP modifiers Symfony's compiled routes need. */
    native_options = 0;
    if ((cflags & REG_ICASE) != 0) native_options |= PCRE2_CASELESS;
    if ((cflags & REG_NEWLINE) != 0) native_options |= PCRE2_MULTILINE;
    if ((cflags & REG_DOTALL) != 0) native_options |= PCRE2_DOTALL;
    if ((cflags & REG_UTF) != 0) native_options |= PCRE2_UTF;
    if ((cflags & REG_UNGREEDY) != 0) native_options |= PCRE2_UNGREEDY;
    if ((cflags & REG_UCP) != 0) native_options |= PCRE2_UCP;
    if ((cflags & ELEPHC_PCRE2_CFLAG_EXTENDED) != 0) native_options |= PCRE2_EXTENDED;
    if ((cflags & ELEPHC_PCRE2_CFLAG_DOLLAR_ENDONLY) != 0) native_options |= PCRE2_DOLLAR_ENDONLY;
    if ((cflags & ELEPHC_PCRE2_CFLAG_DUPNAMES) != 0) native_options |= PCRE2_DUPNAMES;
    if ((cflags & ELEPHC_PCRE2_CFLAG_NO_AUTO_CAPTURE) != 0) native_options |= PCRE2_NO_AUTO_CAPTURE;

    pattern_length = strlen(pattern_z);
    cached = elephc_pcre2_v1_cache_slot(pattern_z, pattern_length, cflags);
    if (cached != NULL && cached->code != NULL) {
        code = cached->code;
        capture_count = cached->capture_count;
    } else {
        code = pcre2_compile((PCRE2_SPTR)pattern_z, PCRE2_ZERO_TERMINATED, native_options, &errorcode, &erroffset, NULL);
        if (code == NULL) {
            free(handle);
            elephc_pcre2_v1_error = 1;
            elephc_pcre2_v1_compile_failed = 1;
            elephc_pcre2_v1_failure_kind = 0;
            elephc_pcre2_v1_compile_errorcode = errorcode;
            elephc_pcre2_v1_compile_erroffset = erroffset;
            return (int32_t)REG_BADPAT;
        }
        capture_count = 0;
        if (pcre2_pattern_info(code, PCRE2_INFO_CAPTURECOUNT, &capture_count) != 0) {
            pcre2_code_free(code);
            free(handle);
            return (int32_t)REG_ESPACE;
        }
        handle->owns_code = 1;
        if (cached != NULL && elephc_pcre2_v1_cache_count < ELEPHC_PCRE2_CACHE_LIMIT) {
            char *key = (char *)malloc(pattern_length + 1U);
            if (key != NULL) {
                memcpy(key, pattern_z, pattern_length + 1U);
                cached->pattern = key;
                cached->length = pattern_length;
                cached->cflags = cflags;
                cached->code = code;
                cached->capture_count = capture_count;
                elephc_pcre2_v1_cache_count += 1U;
                handle->owns_code = 0;
            }
        }
    }
    match_data = pcre2_match_data_create_from_pattern(code, NULL);
    if (match_data == NULL) {
        if (handle->owns_code) {
            pcre2_code_free(code);
        }
        free(handle);
        return (int32_t)REG_ESPACE;
    }

    handle->regex.re_pcre2_code = (void *)code;
    handle->regex.re_match_data = (void *)match_data;
    handle->regex.re_endp = NULL;
    handle->regex.re_nsub = (size_t)capture_count;
    handle->regex.re_erroffset = 0;
    handle->regex.re_cflags = 0;

    if (handle->regex.re_nsub == SIZE_MAX) {
        elephc_pcre2_v1_release(handle);
        return (int32_t)REG_ESPACE;
    }
    handle->slot_count = handle->regex.re_nsub + 1;
#if SIZE_MAX > UINT64_MAX
    if (handle->slot_count > UINT64_MAX) {
        elephc_pcre2_v1_release(handle);
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
    pcre2_code *code;
    pcre2_match_data *match_data;
    PCRE2_SIZE *ovector;
    PCRE2_SIZE subject_length;
    PCRE2_SIZE match_offset;
    uint32_t match_options;
    size_t slots;
    size_t pair_count;
    size_t index;
    int use_startend;
    int64_t start_offset = -1;
    int64_t end_offset = -1;
    int rc;

    if (handle == NULL || subject_z == NULL || (requested_slots != 0 && offset_pairs == NULL) || eflags > INT_MAX) {
        return (int32_t)REG_BADPAT;
    }
    if (requested_slots > SIZE_MAX || requested_slots > SIZE_MAX / (2 * sizeof(int64_t))) {
        return (int32_t)REG_ESPACE;
    }
    slots = (size_t)requested_slots;
    use_startend = (eflags & REG_STARTEND) != 0 && slots != 0;
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

    /* Match through native pcre2_match() rather than pcre2_regexec(). The POSIX wrapper
       implements REG_STARTEND by ADVANCING the subject pointer -- `pcre2_match(re, string + so,
       eo - so, 0, ...)` -- so every continuation of a global match saw its own start offset as
       the start of the subject, and `^` under the `m` modifier matched THERE. php's own preg
       passes the whole subject with a start offset instead, which is the only way `^` keeps
       meaning "line start". Measured: `preg_replace('/^./m', '    $0', "a\nb\n")` indented every
       character rather than the first of each line, which is exactly how Symfony's
       `CompiledUrlMatcherDumper` writes its routing cache -- the dumped file was unparsable PHP.
       pcre2_match() also reports offsets relative to the whole subject already, which is what
       this ABI's callers expect and what the POSIX wrapper had to re-add by hand. */
    code = (pcre2_code *)handle->regex.re_pcre2_code;
    match_data = (pcre2_match_data *)handle->regex.re_match_data;
    if (code == NULL || match_data == NULL) {
        return (int32_t)REG_BADPAT;
    }
    if (use_startend) {
        subject_length = (PCRE2_SIZE)end_offset;
        match_offset = (PCRE2_SIZE)start_offset;
    } else {
        subject_length = (PCRE2_SIZE)strlen(subject_z);
        match_offset = 0;
    }
    match_options = 0;
    if ((eflags & REG_NOTBOL) != 0) match_options |= PCRE2_NOTBOL;
    if ((eflags & REG_NOTEOL) != 0) match_options |= PCRE2_NOTEOL;
    if ((eflags & REG_NOTEMPTY) != 0) match_options |= PCRE2_NOTEMPTY;
    if ((eflags & ELEPHC_PCRE2_GLOBAL_NEXT) != 0) {
        /* Continue the previous match on this handle the way php's global loop does: PCRE2
           picks the next start offset and retry options (an empty match retries NOTEMPTY_ATSTART
           and steps a whole UTF-8 character or CRLF). The earlier match's data is still in
           `match_data`. */
        uint32_t next_options = 0;
#if PCRE2_MAJOR > 10 || (PCRE2_MAJOR == 10 && PCRE2_MINOR >= 47)
        if (!pcre2_next_match(match_data, &match_offset, &next_options)) {
            return (int32_t)REG_NOMATCH;
        }
#else
        /* Host-only test providers can use older PCRE2. Managed production pins 10.47; no
           lookaround-BSK extension is enabled in either path. */
        ovector = pcre2_get_ovector_pointer(match_data);
        match_offset = ovector[1];
        if (ovector[0] == ovector[1]) {
            if (match_offset >= subject_length) return (int32_t)REG_NOMATCH;
            next_options = PCRE2_NOTEMPTY_ATSTART;
        }
#endif
        match_options |= next_options;
    }
    rc = pcre2_match(code, (PCRE2_SPTR)subject_z, subject_length, match_offset, match_options,
                     match_data, NULL);
    elephc_pcre2_v1_error = elephc_pcre2_v1_error_from_match(rc);
    if (rc == PCRE2_ERROR_NOMATCH) {
        return (int32_t)REG_NOMATCH;
    }
    if (rc < 0) {
        return (int32_t)REG_BADPAT;
    }
    ovector = pcre2_get_ovector_pointer(match_data);
    if (ovector == NULL) {
        return (int32_t)REG_BADPAT;
    }
    /* rc == 0 means the ovector was too small to report every capture; the pairs it did fill
       are still valid, and the pattern's own slot count bounds them. */
    pair_count = (rc == 0) ? handle->slot_count : (size_t)rc;
    if (handle->anchored && ovector[0] != match_offset) {
        return (int32_t)REG_NOMATCH;
    }
    for (index = 0; index < slots && index < pair_count; ++index) {
        if (ovector[index * 2] == PCRE2_UNSET) {
            continue;
        }
        offset_pairs[index * 2] = (int64_t)ovector[index * 2];
        offset_pairs[index * 2 + 1] = (int64_t)ovector[index * 2 + 1];
    }
    return 0;
}

void elephc_pcre2_v1_free(void *opaque_handle) {
    elephc_pcre2_v1_handle *handle = (elephc_pcre2_v1_handle *)opaque_handle;
    if (handle == NULL) {
        return;
    }
    elephc_pcre2_v1_release(handle);
}

/* Reads the compiled pattern out of the POSIX wrapper without exposing PCRE2 layouts to callers. */
static const pcre2_code *elephc_pcre2_v1_code(void *opaque_handle) {
    const elephc_pcre2_v1_handle *handle = (const elephc_pcre2_v1_handle *)opaque_handle;

    if (handle == NULL) {
        return NULL;
    }
    return (const pcre2_code *)handle->regex.re_pcre2_code;
}


/* Returns the name set by the last MARK verb (`(*:name)`) the most recent match passed.
 *
 * PHP exposes this as `$matches['MARK']`, and Symfony's dumped `CompiledUrlMatcher` is built on
 * it: each alternative of the dynamic-route regexp ends in `(*:<offset>)` and the matcher reads
 * the mark to choose `$this->dynamicRoutes[(int) $matches['MARK']]`. Without it that index is 0
 * and every route with a placeholder fails.
 *
 * The caller only invokes this after a SUCCESSFUL match, which is the only case PHP reports a
 * mark for -- pcre2_get_mark() can also report a mark passed before a FAILING match, and PHP
 * does not surface that one.
 */
int32_t elephc_pcre2_v1_last_mark(
    void *opaque_handle,
    const char **mark_out,
    uint64_t *mark_len_out
) {
    elephc_pcre2_v1_handle *handle = (elephc_pcre2_v1_handle *)opaque_handle;
    pcre2_match_data *match_data;
    PCRE2_SPTR mark;
    size_t length;

    if (mark_out != NULL) {
        *mark_out = NULL;
    }
    if (mark_len_out != NULL) {
        *mark_len_out = 0;
    }
    if (handle == NULL || mark_out == NULL || mark_len_out == NULL) {
        return 1;
    }
    match_data = (pcre2_match_data *)handle->regex.re_match_data;
    if (match_data == NULL) {
        return 1;
    }
    mark = pcre2_get_mark(match_data);
    if (mark == NULL) {
        return 1;
    }
    /* pcre2 stores the name NUL-terminated in its own pattern storage, which outlives this call
       for as long as the compiled pattern does -- the same lifetime the name table relies on. */
    for (length = 0; mark[length] != '\0'; ++length) {
    }
    *mark_out = (const char *)mark;
    *mark_len_out = (uint64_t)length;
    return 0;
}

uint64_t elephc_pcre2_v1_name_count(void *opaque_handle) {
    const pcre2_code *code = elephc_pcre2_v1_code(opaque_handle);
    uint32_t count = 0;

    if (code == NULL) {
        return 0;
    }
    if (pcre2_pattern_info(code, PCRE2_INFO_NAMECOUNT, &count) != 0) {
        return 0;
    }
    return (uint64_t)count;
}

int32_t elephc_pcre2_v1_group_name(
    void *opaque_handle,
    uint64_t group,
    const char **name_out,
    uint64_t *name_len_out
) {
    const pcre2_code *code = elephc_pcre2_v1_code(opaque_handle);
    PCRE2_SPTR table = NULL;
    uint32_t count = 0;
    uint32_t entry_size = 0;
    uint32_t index;

    if (name_out != NULL) {
        *name_out = NULL;
    }
    if (name_len_out != NULL) {
        *name_len_out = 0;
    }
    if (code == NULL || name_out == NULL || name_len_out == NULL || group > 0xFFFFU) {
        return 1;
    }
    if (pcre2_pattern_info(code, PCRE2_INFO_NAMECOUNT, &count) != 0
        || pcre2_pattern_info(code, PCRE2_INFO_NAMEENTRYSIZE, &entry_size) != 0
        || pcre2_pattern_info(code, PCRE2_INFO_NAMETABLE, &table) != 0) {
        return 1;
    }
    if (count == 0 || entry_size < 3 || table == NULL) {
        return 1;
    }
    for (index = 0; index < count; ++index) {
        const unsigned char *entry = (const unsigned char *)table + (size_t)index * (size_t)entry_size;
        uint32_t entry_group = ((uint32_t)entry[0] << 8) | (uint32_t)entry[1];
        const char *name;
        size_t limit;
        size_t length;

        if (entry_group != (uint32_t)group) {
            continue;
        }
        /* Each entry is a 2-byte group number then a NUL-terminated name, so the name can never
           run past the entry. The bound is measured rather than assumed: strnlen() is POSIX, and
           this shim is compiled as strict C11. */
        name = (const char *)(entry + 2);
        limit = (size_t)entry_size - 2;
        for (length = 0; length < limit && name[length] != '\0'; ++length) {
        }
        *name_out = name;
        *name_len_out = (uint64_t)length;
        return 0;
    }
    return 1;
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
