/*
 * HTML5 parser and Lexbor-to-libxml2 bridge derived from PHP 8.5.8 ext/dom.
 * The PHP-visible wrappers continue to use libxml2 nodes after HTML parsing.
 */

#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libxml/HTMLtree.h>
#include <libxml/tree.h>
#include <libxml/valid.h>

#include "lexbor/core/array_obj.h"
#include "lexbor/encoding/encoding.h"
#include "lexbor/dom/interfaces/comment.h"
#include "lexbor/dom/interfaces/document_type.h"
#include "lexbor/dom/interfaces/text.h"
#include "lexbor/html/encoding.h"
#include "lexbor/html/interfaces/document.h"
#include "lexbor/html/interfaces/element.h"
#include "lexbor/html/interfaces/template_element.h"
#include "lexbor/html/parser.h"
#include "lexbor/html/tokenizer/error.h"
#include "lexbor/html/tree/error.h"

#define ELEPHC_DOM_HTML_NOERROR 32U
#define ELEPHC_DOM_HTML_NOIMPLIED 8192U
#define ELEPHC_DOM_HTML_NO_DEFAULT_NS 2147483648U
#define ELEPHC_DOM_HTML_WORK_LIST_SIZE 128
#define ELEPHC_DOM_HTML_CODEPOINT_BUFFER_SIZE 4096
#define ELEPHC_DOM_HTML_OUTPUT_BUFFER_SIZE 4096

typedef struct {
    int32_t level;
    int32_t domain;
    int32_t code;
    int32_t line;
    int32_t column;
    int32_t reserved;
    uint8_t *message;
    size_t message_length;
    uint8_t *file;
    size_t file_length;
} elephc_dom_native_error;

typedef struct {
    void *document;
    elephc_dom_native_error *errors;
    size_t error_count;
    int32_t allocation_failed;
    int32_t reserved;
} elephc_dom_native_parse_result;

typedef struct {
    lxb_dom_node_t *node;
    uintptr_t active_namespace;
    xmlNodePtr parent;
    xmlNsPtr namespace;
} elephc_dom_html_work_item;

typedef struct {
    elephc_dom_native_error *errors;
    size_t count;
    size_t capacity;
} elephc_dom_html_error_list;

void elephc_dom_native_document_free(void *document);

static const xmlChar elephc_dom_html_namespace[] =
    "http://www.w3.org/1999/xhtml";
static const xmlChar elephc_dom_svg_namespace[] =
    "http://www.w3.org/2000/svg";
static const xmlChar elephc_dom_mathml_namespace[] =
    "http://www.w3.org/1998/Math/MathML";
static const xmlChar elephc_dom_xml_namespace[] =
    "http://www.w3.org/XML/1998/namespace";
static const xmlChar elephc_dom_xmlns_namespace[] =
    "http://www.w3.org/2000/xmlns/";
static const xmlChar elephc_dom_xlink_namespace[] =
    "http://www.w3.org/1999/xlink";
static const uint8_t elephc_dom_no_quirks_marker = 0;
static const uint8_t elephc_dom_limited_quirks_marker = 0;
static const uint8_t elephc_dom_quirks_marker = 0;

int32_t elephc_dom_native_html_encoding_is_valid(
    const uint8_t *encoding,
    size_t encoding_length
)
{
    return encoding != NULL
        && lxb_encoding_data_by_name(encoding, encoding_length) != NULL;
}

static const lxb_encoding_data_t *elephc_dom_html_detect_encoding(
    const uint8_t **source,
    size_t *length,
    const uint8_t *override_encoding,
    size_t override_encoding_length
)
{
    const lxb_encoding_data_t *encoding;

    if (override_encoding != NULL || override_encoding_length != 0) {
        return lxb_encoding_data_by_name(
            (const lxb_char_t *) override_encoding,
            override_encoding_length
        );
    }
    if (*length >= 3 && (*source)[0] == 0xEF
        && (*source)[1] == 0xBB && (*source)[2] == 0xBF) {
        *source += 3;
        *length -= 3;
        return lxb_encoding_data(LXB_ENCODING_UTF_8);
    }
    if (*length >= 2 && (*source)[0] == 0xFE && (*source)[1] == 0xFF) {
        *source += 2;
        *length -= 2;
        return lxb_encoding_data(LXB_ENCODING_UTF_16BE);
    }
    if (*length >= 2 && (*source)[0] == 0xFF && (*source)[1] == 0xFE) {
        *source += 2;
        *length -= 2;
        return lxb_encoding_data(LXB_ENCODING_UTF_16LE);
    }

    {
        lxb_html_encoding_t detector;
        size_t scan_length = *length > 1024 ? 1024 : *length;

        if (lxb_html_encoding_init(&detector) == LXB_STATUS_OK) {
            if (lxb_html_encoding_determine(
                    &detector,
                    (const lxb_char_t *) *source,
                    (const lxb_char_t *) *source + scan_length
                ) == LXB_STATUS_OK) {
                lxb_html_encoding_entry_t *entry =
                    lxb_html_encoding_meta_entry(&detector, 0);
                if (entry != NULL) {
                    encoding = lxb_encoding_data_by_pre_name(
                        entry->name,
                        (size_t) (entry->end - entry->name)
                    );
                    if (encoding != NULL) {
                        lxb_html_encoding_destroy(&detector, 0);
                        return encoding;
                    }
                }
            }
            lxb_html_encoding_destroy(&detector, 0);
        }
    }
    return lxb_encoding_data(LXB_ENCODING_UTF_8);
}

static const char *elephc_dom_html_tokenizer_error_name(
    lxb_html_tokenizer_error_id_t id
)
{
    switch (id) {
        case LXB_HTML_TOKENIZER_ERROR_ABCLOFEMCO:
            return "abrupt-closing-of-empty-comment";
        case LXB_HTML_TOKENIZER_ERROR_ABDOPUID:
            return "abrupt-doctype-public-identifier";
        case LXB_HTML_TOKENIZER_ERROR_ABDOSYID:
            return "abrupt-doctype-system-identifier";
        case LXB_HTML_TOKENIZER_ERROR_ABOFDIINNUCHRE:
            return "absence-of-digits-in-numeric-character-reference";
        case LXB_HTML_TOKENIZER_ERROR_CDINHTCO:
            return "cdata-in-html-content";
        case LXB_HTML_TOKENIZER_ERROR_CHREOUUNRA:
            return "character-reference-outside-unicode-range";
        case LXB_HTML_TOKENIZER_ERROR_COCHININST:
            return "control-character-in-input-stream";
        case LXB_HTML_TOKENIZER_ERROR_COCHRE:
            return "control-character-reference";
        case LXB_HTML_TOKENIZER_ERROR_ENTAWIAT:
            return "end-tag-with-attributes";
        case LXB_HTML_TOKENIZER_ERROR_DUAT:
            return "duplicate-attribute";
        case LXB_HTML_TOKENIZER_ERROR_ENTAWITRSO:
            return "end-tag-with-trailing-solidus";
        case LXB_HTML_TOKENIZER_ERROR_EOBETANA:
            return "eof-before-tag-name";
        case LXB_HTML_TOKENIZER_ERROR_EOINCD:
            return "eof-in-cdata";
        case LXB_HTML_TOKENIZER_ERROR_EOINCO:
            return "eof-in-comment";
        case LXB_HTML_TOKENIZER_ERROR_EOINDO:
            return "eof-in-doctype";
        case LXB_HTML_TOKENIZER_ERROR_EOINSCHTCOLITE:
            return "eof-in-script-html-comment-like-text";
        case LXB_HTML_TOKENIZER_ERROR_EOINTA:
            return "eof-in-tag";
        case LXB_HTML_TOKENIZER_ERROR_INCLCO:
            return "incorrectly-closed-comment";
        case LXB_HTML_TOKENIZER_ERROR_INOPCO:
            return "incorrectly-opened-comment";
        case LXB_HTML_TOKENIZER_ERROR_INCHSEAFDONA:
            return "invalid-character-sequence-after-doctype-name";
        case LXB_HTML_TOKENIZER_ERROR_INFICHOFTANA:
            return "invalid-first-character-of-tag-name";
        case LXB_HTML_TOKENIZER_ERROR_MIATVA:
            return "missing-attribute-value";
        case LXB_HTML_TOKENIZER_ERROR_MIDONA:
            return "missing-doctype-name";
        case LXB_HTML_TOKENIZER_ERROR_MIDOPUID:
            return "missing-doctype-public-identifier";
        case LXB_HTML_TOKENIZER_ERROR_MIDOSYID:
            return "missing-doctype-system-identifier";
        case LXB_HTML_TOKENIZER_ERROR_MIENTANA:
            return "missing-end-tag-name";
        case LXB_HTML_TOKENIZER_ERROR_MIQUBEDOPUID:
            return "missing-quote-before-doctype-public-identifier";
        case LXB_HTML_TOKENIZER_ERROR_MIQUBEDOSYID:
            return "missing-quote-before-doctype-system-identifier";
        case LXB_HTML_TOKENIZER_ERROR_MISEAFCHRE:
            return "missing-semicolon-after-character-reference";
        case LXB_HTML_TOKENIZER_ERROR_MIWHAFDOPUKE:
            return "missing-whitespace-after-doctype-public-keyword";
        case LXB_HTML_TOKENIZER_ERROR_MIWHAFDOSYKE:
            return "missing-whitespace-after-doctype-system-keyword";
        case LXB_HTML_TOKENIZER_ERROR_MIWHBEDONA:
            return "missing-whitespace-before-doctype-name";
        case LXB_HTML_TOKENIZER_ERROR_MIWHBEAT:
            return "missing-whitespace-between-attributes";
        case LXB_HTML_TOKENIZER_ERROR_MIWHBEDOPUANSYID:
            return "missing-whitespace-between-doctype-public-and-system-identifiers";
        case LXB_HTML_TOKENIZER_ERROR_NECO:
            return "nested-comment";
        case LXB_HTML_TOKENIZER_ERROR_NOCHRE:
            return "noncharacter-character-reference";
        case LXB_HTML_TOKENIZER_ERROR_NOININST:
            return "noncharacter-in-input-stream";
        case LXB_HTML_TOKENIZER_ERROR_NOVOHTELSTTAWITRSO:
            return "non-void-html-element-start-tag-with-trailing-solidus";
        case LXB_HTML_TOKENIZER_ERROR_NUCHRE:
            return "null-character-reference";
        case LXB_HTML_TOKENIZER_ERROR_SUCHRE:
            return "surrogate-character-reference";
        case LXB_HTML_TOKENIZER_ERROR_SUININST:
            return "surrogate-in-input-stream";
        case LXB_HTML_TOKENIZER_ERROR_UNCHAFDOSYID:
            return "unexpected-character-after-doctype-system-identifier";
        case LXB_HTML_TOKENIZER_ERROR_UNCHINATNA:
            return "unexpected-character-in-attribute-name";
        case LXB_HTML_TOKENIZER_ERROR_UNCHINUNATVA:
            return "unexpected-character-in-unquoted-attribute-value";
        case LXB_HTML_TOKENIZER_ERROR_UNEQSIBEATNA:
            return "unexpected-equals-sign-before-attribute-name";
        case LXB_HTML_TOKENIZER_ERROR_UNNUCH:
            return "unexpected-null-character";
        case LXB_HTML_TOKENIZER_ERROR_UNQUMAINOFTANA:
            return "unexpected-question-mark-instead-of-tag-name";
        case LXB_HTML_TOKENIZER_ERROR_UNSOINTA:
            return "unexpected-solidus-in-tag";
        case LXB_HTML_TOKENIZER_ERROR_UNNACHRE:
            return "unknown-named-character-reference";
        default:
            return "unknown error";
    }
}

static const char *elephc_dom_html_tree_error_name(
    lxb_html_tree_error_id_t id
)
{
    switch (id) {
        case LXB_HTML_RULES_ERROR_UNTO:
            return "unexpected-token";
        case LXB_HTML_RULES_ERROR_UNCLTO:
            return "unexpected-closed-token";
        case LXB_HTML_RULES_ERROR_NUCH:
            return "null-character";
        case LXB_HTML_RULES_ERROR_UNCHTO:
            return "unexpected-character-token";
        case LXB_HTML_RULES_ERROR_UNTOININMO:
            return "unexpected-token-in-initial-mode";
        case LXB_HTML_RULES_ERROR_BADOTOININMO:
            return "bad-doctype-token-in-initial-mode";
        case LXB_HTML_RULES_ERROR_DOTOINBEHTMO:
            return "doctype-token-in-before-html-mode";
        case LXB_HTML_RULES_ERROR_UNCLTOINBEHTMO:
            return "unexpected-closed-token-in-before-html-mode";
        case LXB_HTML_RULES_ERROR_DOTOINBEHEMO:
            return "doctype-token-in-before-head-mode";
        case LXB_HTML_RULES_ERROR_UNCLTOINBEHEMO:
            return "unexpected-closed_token-in-before-head-mode";
        case LXB_HTML_RULES_ERROR_DOTOINHEMO:
            return "doctype-token-in-head-mode";
        case LXB_HTML_RULES_ERROR_NOVOHTELSTTAWITRSO:
            return "non-void-html-element-start-tag-with-trailing-solidus";
        case LXB_HTML_RULES_ERROR_HETOINHEMO:
            return "head-token-in-head-mode";
        case LXB_HTML_RULES_ERROR_UNCLTOINHEMO:
            return "unexpected-closed-token-in-head-mode";
        case LXB_HTML_RULES_ERROR_TECLTOWIOPINHEMO:
            return "template-closed-token-without-opening-in-head-mode";
        case LXB_HTML_RULES_ERROR_TEELISNOCUINHEMO:
            return "template-element-is-not-current-in-head-mode";
        case LXB_HTML_RULES_ERROR_DOTOINHENOMO:
            return "doctype-token-in-head-noscript-mode";
        case LXB_HTML_RULES_ERROR_DOTOAFHEMO:
            return "doctype-token-after-head-mode";
        case LXB_HTML_RULES_ERROR_HETOAFHEMO:
            return "head-token-after-head-mode";
        case LXB_HTML_RULES_ERROR_DOTOINBOMO:
            return "doctype-token-in-body-mode";
        case LXB_HTML_RULES_ERROR_BAENOPELISWR:
            return "bad-ending-open-elements-is-wrong";
        case LXB_HTML_RULES_ERROR_OPELISWR:
            return "open-elements-is-wrong";
        case LXB_HTML_RULES_ERROR_UNELINOPELST:
            return "unexpected-element-in-open-elements-stack";
        case LXB_HTML_RULES_ERROR_MIELINOPELST:
            return "missing-element-in-open-elements-stack";
        case LXB_HTML_RULES_ERROR_NOBOELINSC:
            return "no-body-element-in-scope";
        case LXB_HTML_RULES_ERROR_MIELINSC:
            return "missing-element-in-scope";
        case LXB_HTML_RULES_ERROR_UNELINSC:
            return "unexpected-element-in-scope";
        case LXB_HTML_RULES_ERROR_UNELINACFOST:
            return "unexpected-element-in-active-formatting-stack";
        case LXB_HTML_RULES_ERROR_UNENOFFI:
            return "unexpected-end-of-file";
        case LXB_HTML_RULES_ERROR_CHINTATE:
            return "characters-in-table-text";
        case LXB_HTML_RULES_ERROR_DOTOINTAMO:
            return "doctype-token-in-table-mode";
        case LXB_HTML_RULES_ERROR_DOTOINSEMO:
            return "doctype-token-in-select-mode";
        case LXB_HTML_RULES_ERROR_DOTOAFBOMO:
            return "doctype-token-after-body-mode";
        case LXB_HTML_RULES_ERROR_DOTOINFRMO:
            return "doctype-token-in-frameset-mode";
        case LXB_HTML_RULES_ERROR_DOTOAFFRMO:
            return "doctype-token-after-frameset-mode";
        case LXB_HTML_RULES_ERROR_DOTOFOCOMO:
            return "doctype-token-foreign-content-mode";
        default:
            return "unknown error";
    }
}

static void elephc_dom_html_error_list_free(
    elephc_dom_html_error_list *list
)
{
    while (list->count != 0) {
        elephc_dom_native_error *error = &list->errors[--list->count];
        free(error->message);
        free(error->file);
    }
    free(list->errors);
    memset(list, 0, sizeof(*list));
}

static int32_t elephc_dom_html_error_list_push(
    elephc_dom_html_error_list *list,
    int32_t line,
    int32_t column,
    const uint8_t *input_name,
    size_t input_name_length,
    const char *kind,
    const char *name,
    size_t range_length
)
{
    elephc_dom_native_error *error;
    elephc_dom_native_error *replacement;
    int message_length;

    if (list->count == list->capacity) {
        size_t next_capacity =
            list->capacity == 0 ? 8 : list->capacity * 2;
        if (next_capacity < list->capacity
            || next_capacity > SIZE_MAX / sizeof(*replacement)) {
            return 0;
        }
        replacement = realloc(
            list->errors,
            next_capacity * sizeof(*replacement)
        );
        if (replacement == NULL) {
            return 0;
        }
        list->errors = replacement;
        list->capacity = next_capacity;
    }
    error = &list->errors[list->count];
    memset(error, 0, sizeof(*error));
    error->level = 2;
    error->domain = 1;
    error->code = 1;
    error->line = line;
    error->column = column;
    error->file = malloc(input_name_length == 0 ? 1 : input_name_length);
    if (error->file == NULL) {
        return 0;
    }
    if (input_name_length != 0) {
        memcpy(error->file, input_name, input_name_length);
    }
    error->file_length = input_name_length;
    if (range_length <= 1) {
        message_length = snprintf(
            NULL,
            0,
            "%s error %s in %.*s, line: %d, column: %d",
            kind,
            name,
            (int) input_name_length,
            (const char *) input_name,
            line,
            column
        );
    } else {
        message_length = snprintf(
            NULL,
            0,
            "%s error %s in %.*s, line: %d, column: %d-%zu",
            kind,
            name,
            (int) input_name_length,
            (const char *) input_name,
            line,
            column,
            (size_t) column + range_length - 1
        );
    }
    if (message_length < 0) {
        free(error->file);
        memset(error, 0, sizeof(*error));
        return 0;
    }
    error->message = malloc((size_t) message_length + 1);
    if (error->message == NULL) {
        free(error->file);
        memset(error, 0, sizeof(*error));
        return 0;
    }
    if (range_length <= 1) {
        snprintf(
            (char *) error->message,
            (size_t) message_length + 1,
            "%s error %s in %.*s, line: %d, column: %d",
            kind,
            name,
            (int) input_name_length,
            (const char *) input_name,
            line,
            column
        );
    } else {
        snprintf(
            (char *) error->message,
            (size_t) message_length + 1,
            "%s error %s in %.*s, line: %d, column: %d-%zu",
            kind,
            name,
            (int) input_name_length,
            (const char *) input_name,
            line,
            column,
            (size_t) column + range_length - 1
        );
    }
    error->message_length = (size_t) message_length;
    list->count++;
    return 1;
}

static void elephc_dom_html_line_and_column(
    const uint8_t *input,
    size_t length,
    size_t offset,
    int32_t *line,
    int32_t *column
)
{
    size_t index;

    *line = 1;
    *column = 1;
    if (offset > length) {
        offset = length;
    }
    for (index = 0; index < offset; index++) {
        if (input[index] == '\n') {
            (*line)++;
            *column = 1;
        } else if ((input[index] & 0xC0) != 0x80) {
            (*column)++;
        }
    }
}

static int32_t elephc_dom_html_collect_errors(
    lxb_html_parser_t *parser,
    const uint8_t *input,
    size_t input_length,
    const uint8_t *input_name,
    size_t input_name_length,
    uint32_t options,
    elephc_dom_html_error_list *list,
    size_t *tokenizer_index,
    size_t *tree_index
)
{
    lexbor_array_obj_t *parse_errors;
    size_t index = *tokenizer_index;
    void *item;

    if ((options & ELEPHC_DOM_HTML_NOERROR) != 0) {
        return 1;
    }
    parse_errors = lxb_html_parser_tokenizer(parser)->parse_errors;
    while ((item = lexbor_array_obj_get(
        parse_errors,
        index
    )) != NULL) {
        lxb_html_tokenizer_error_t *token_error = item;
        size_t offset = input_length;
        int32_t line;
        int32_t column;

        if (token_error->pos >= input
            && token_error->pos <= input + input_length) {
            offset = (size_t) (token_error->pos - input);
        }
        elephc_dom_html_line_and_column(
            input,
            input_length,
            offset,
            &line,
            &column
        );
        if (!elephc_dom_html_error_list_push(
                list,
                line,
                column,
                input_name,
                input_name_length,
                "tokenizer",
                elephc_dom_html_tokenizer_error_name(token_error->id),
                1
            )) {
            return 0;
        }
        index++;
    }
    *tokenizer_index = index;
    parse_errors = lxb_html_parser_tree(parser)->parse_errors;
    index = *tree_index;
    while ((item = lexbor_array_obj_get(
        parse_errors,
        index
    )) != NULL) {
        lxb_html_tree_error_t *tree_error = item;
        int32_t line = (int32_t) tree_error->line + 1;
        int32_t column = (int32_t) tree_error->column + 1;

        if ((options & ELEPHC_DOM_HTML_NOIMPLIED) != 0
            && line == 1
            && tree_error->id
                == LXB_HTML_RULES_ERROR_UNTOININMO) {
            index++;
            continue;
        }
        if (!elephc_dom_html_error_list_push(
                list,
                line,
                column,
                input_name,
                input_name_length,
                "tree",
                elephc_dom_html_tree_error_name(tree_error->id),
                tree_error->length
            )) {
            return 0;
        }
        index++;
    }
    *tree_index = index;
    return 1;
}

static uint8_t *elephc_dom_html_decode_and_parse(
    lxb_html_document_t *document,
    const uint8_t *source,
    size_t length,
    const lxb_encoding_data_t *input_encoding,
    size_t *output_length,
    const uint8_t *input_name,
    size_t input_name_length,
    uint32_t options,
    elephc_dom_html_error_list *errors
)
{
    static const lxb_codepoint_t replacement_codepoint =
        LXB_ENCODING_REPLACEMENT_CODEPOINT;
    lxb_encoding_decode_t decoder;
    lxb_encoding_encode_t encoder;
    lxb_codepoint_t codepoints[ELEPHC_DOM_HTML_CODEPOINT_BUFFER_SIZE];
    lxb_char_t *output;
    size_t output_capacity;
    const lxb_encoding_data_t *utf8 =
        lxb_encoding_data(LXB_ENCODING_UTF_8);
    const lxb_char_t *input = (const lxb_char_t *) source;
    const lxb_char_t *input_end = input + length;
    lxb_status_t decode_status;
    size_t tokenizer_index = 0;
    size_t tree_index = 0;

    *output_length = 0;
    if (length > (SIZE_MAX - 4) / 3) {
        return NULL;
    }
    output_capacity = length * 3 + 4;
    output = malloc(output_capacity);
    if (output == NULL
        || lxb_encoding_decode_init(
            &decoder,
            input_encoding,
            codepoints,
            ELEPHC_DOM_HTML_CODEPOINT_BUFFER_SIZE
        ) != LXB_STATUS_OK
        || lxb_encoding_decode_replace_set(
            &decoder,
            &replacement_codepoint,
            1
        ) != LXB_STATUS_OK
        || lxb_encoding_encode_init(
            &encoder,
            utf8,
            output,
            output_capacity
        ) != LXB_STATUS_OK
        || lxb_encoding_encode_replace_set(
            &encoder,
            LXB_ENCODING_REPLACEMENT_BYTES,
            LXB_ENCODING_REPLACEMENT_SIZE
        ) != LXB_STATUS_OK
    ) {
        free(output);
        return NULL;
    }

    do {
        const lxb_codepoint_t *codepoint;
        const lxb_codepoint_t *codepoint_end;
        lxb_status_t encode_status;

        decode_status = input_encoding->decode(
            &decoder,
            &input,
            input_end
        );
        codepoint = codepoints;
        codepoint_end =
            codepoints + lxb_encoding_decode_buf_used(&decoder);
        encode_status = utf8->encode(
            &encoder,
            &codepoint,
            codepoint_end
        );
        if (encode_status != LXB_STATUS_OK) {
            free(output);
            return NULL;
        }
        lxb_encoding_decode_buf_used_set(&decoder, 0);
    } while (decode_status == LXB_STATUS_SMALL_BUFFER);
    if (decode_status != LXB_STATUS_OK
        && decode_status != LXB_STATUS_CONTINUE) {
        free(output);
        return NULL;
    }

    if (lxb_encoding_decode_finish(&decoder) != LXB_STATUS_OK) {
        free(output);
        return NULL;
    }
    if (lxb_encoding_decode_buf_used(&decoder) != 0) {
        const lxb_codepoint_t *codepoint = codepoints;
        const lxb_codepoint_t *codepoint_end =
            codepoints + lxb_encoding_decode_buf_used(&decoder);
        if (utf8->encode(&encoder, &codepoint, codepoint_end)
                != LXB_STATUS_OK) {
            free(output);
            return NULL;
        }
    }
    if (lxb_encoding_encode_finish(&encoder) != LXB_STATUS_OK) {
        free(output);
        return NULL;
    }
    *output_length = lxb_encoding_encode_buf_used(&encoder);
    if (lxb_html_document_parse_chunk_begin(document)
        != LXB_STATUS_OK) {
        free(output);
        *output_length = 0;
        return NULL;
    }
    {
        size_t offset = 0;
        while (offset < *output_length) {
            size_t chunk_length = *output_length - offset;
            if (chunk_length > ELEPHC_DOM_HTML_OUTPUT_BUFFER_SIZE) {
                chunk_length = ELEPHC_DOM_HTML_OUTPUT_BUFFER_SIZE;
            }
            if (lxb_html_document_parse_chunk(
                    document,
                    output + offset,
                    chunk_length
                ) != LXB_STATUS_OK) {
                free(output);
                *output_length = 0;
                return NULL;
            }
            if (!elephc_dom_html_collect_errors(
                    document->dom_document.parser,
                    output,
                    *output_length,
                    input_name,
                    input_name_length,
                    options,
                    errors,
                    &tokenizer_index,
                    &tree_index
                )) {
                free(output);
                *output_length = 0;
                return NULL;
            }
            offset += chunk_length;
        }
    }
    return output;
}

static const xmlChar *elephc_dom_html_namespace_uri(uintptr_t namespace_id)
{
    if (namespace_id == LXB_NS_SVG) {
        return elephc_dom_svg_namespace;
    }
    if (namespace_id == LXB_NS_MATH) {
        return elephc_dom_mathml_namespace;
    }
    return elephc_dom_html_namespace;
}

static xmlNsPtr elephc_dom_html_attribute_namespace(
    xmlDocPtr document,
    xmlNsPtr *slot,
    const xmlChar *prefix,
    const xmlChar *href
)
{
    if (*slot == NULL) {
        *slot = xmlNewNs(NULL, href, prefix);
        if (*slot != NULL) {
            (*slot)->next = document->oldNs;
            document->oldNs = *slot;
        }
    }
    return *slot;
}

static int32_t elephc_dom_html_convert_attribute(
    xmlDocPtr document,
    xmlNodePtr element,
    lxb_dom_attr_t *attribute,
    xmlNsPtr *xml_namespace,
    xmlNsPtr *xmlns_namespace,
    xmlNsPtr *xlink_namespace
)
{
    size_t name_length;
    size_t value_length;
    const lxb_char_t *qualified_name =
        lxb_dom_attr_qualified_name(attribute, &name_length);
    const lxb_char_t *value =
        lxb_dom_attr_value(attribute, &value_length);
    const lxb_char_t *local_name = qualified_name;
    xmlNsPtr namespace = NULL;
    xmlChar *name;
    xmlChar *content;
    xmlAttrPtr created;

    if (attribute->node.prefix != 0) {
        const lxb_char_t *colon = (const lxb_char_t *) memchr(
            qualified_name,
            ':',
            name_length
        );
        if (colon != NULL) {
            local_name = colon + 1;
            name_length -= (size_t) (local_name - qualified_name);
        }
    }
    if (name_length > INT_MAX || value_length > INT_MAX) {
        return 0;
    }
    name = xmlStrndup((const xmlChar *) local_name, (int) name_length);
    content = xmlStrndup((const xmlChar *) value, (int) value_length);
    if (name == NULL || content == NULL) {
        xmlFree(name);
        xmlFree(content);
        return 0;
    }
    if (attribute->node.ns == LXB_NS_XML) {
        namespace = elephc_dom_html_attribute_namespace(
            document,
            xml_namespace,
            (const xmlChar *) "xml",
            elephc_dom_xml_namespace
        );
    } else if (attribute->node.ns == LXB_NS_XMLNS) {
        namespace = elephc_dom_html_attribute_namespace(
            document,
            xmlns_namespace,
            (const xmlChar *) "xmlns",
            elephc_dom_xmlns_namespace
        );
    } else if (attribute->node.ns == LXB_NS_XLINK) {
        namespace = elephc_dom_html_attribute_namespace(
            document,
            xlink_namespace,
            (const xmlChar *) "xlink",
            elephc_dom_xlink_namespace
        );
    }
    created = namespace == NULL
        ? xmlNewProp(element, name, content)
        : xmlNewNsProp(element, namespace, name, content);
    xmlFree(name);
    xmlFree(content);
    if (created == NULL) {
        return 0;
    }
    if (name_length == 2 && local_name[0] == 'i' && local_name[1] == 'd'
        && attribute->node.ns == LXB_NS_HTML
        && xmlAddID(NULL, document, value, created) == 0) {
        created->atype = XML_ATTRIBUTE_ID;
    }
    return 1;
}

static int32_t elephc_dom_html_convert_children(
    lxb_dom_node_t *source,
    xmlDocPtr target,
    xmlNodePtr root,
    int32_t create_default_namespace
)
{
    lexbor_array_obj_t work_list;
    lxb_dom_node_t *node;
    xmlNsPtr xml_namespace = NULL;
    xmlNsPtr xmlns_namespace = NULL;
    xmlNsPtr xlink_namespace = NULL;
    int32_t success = 1;

    if (lexbor_array_obj_init(
            &work_list,
            ELEPHC_DOM_HTML_WORK_LIST_SIZE,
            sizeof(elephc_dom_html_work_item)
        ) != LXB_STATUS_OK) {
        return 0;
    }
    for (node = source->last_child;
        node != NULL;
        node = node->prev) {
        elephc_dom_html_work_item *item =
            lexbor_array_obj_push_wo_cls(&work_list);
        if (item == NULL) {
            success = 0;
            break;
        }
        item->node = node;
        item->active_namespace = LXB_NS__UNDEF;
        item->parent = root;
        item->namespace = NULL;
    }
    while (success) {
        elephc_dom_html_work_item *item =
            lexbor_array_obj_pop(&work_list);
        xmlNodePtr converted;

        if (item == NULL) {
            break;
        }
        node = item->node;
        if (node->type == LXB_DOM_NODE_TYPE_ELEMENT) {
            lxb_dom_element_t *element = lxb_dom_interface_element(node);
            const lxb_char_t *name =
                lxb_dom_element_qualified_name(element, NULL);
            uintptr_t namespace_id = element->node.ns;
            xmlNsPtr namespace = item->namespace;
            lxb_dom_node_t *child;
            lxb_dom_attr_t *attribute;
            xmlNodePtr child_parent;

            converted = xmlNewDocNode(target, NULL, name, NULL);
            if (converted == NULL
                || xmlAddChild(item->parent, converted) == NULL) {
                if (converted != NULL) {
                    xmlFreeNode(converted);
                }
                success = 0;
                break;
            }
            converted->line =
                node->line > USHRT_MAX ? USHRT_MAX : (unsigned short) node->line;
            if (create_default_namespace
                && namespace_id != item->active_namespace) {
                namespace = xmlNewNs(
                    converted,
                    elephc_dom_html_namespace_uri(namespace_id),
                    NULL
                );
                if (namespace == NULL) {
                    success = 0;
                    break;
                }
            }
            converted->ns = namespace;
            child_parent = converted;
            for (attribute = element->first_attr;
                attribute != NULL;
                attribute = attribute->next) {
                if (!elephc_dom_html_convert_attribute(
                        target,
                        converted,
                        attribute,
                        &xml_namespace,
                        &xmlns_namespace,
                        &xlink_namespace
                    )) {
                    success = 0;
                    break;
                }
            }
            if (!success) {
                break;
            }
            child = element->node.last_child;
            if (create_default_namespace
                && lxb_html_tree_node_is(
                    &element->node,
                    LXB_TAG_TEMPLATE
                )) {
                xmlNodePtr fragment = xmlNewDocFragment(target);
                lxb_html_template_element_t *template =
                    lxb_html_interface_template(&element->node);

                if (fragment == NULL) {
                    success = 0;
                    break;
                }
                fragment->parent = converted;
                converted->_private = fragment;
                child_parent = fragment;
                if (template->content != NULL) {
                    child = template->content->node.last_child;
                }
            }
            for (; child != NULL; child = child->prev) {
                elephc_dom_html_work_item *next =
                    lexbor_array_obj_push_wo_cls(&work_list);
                if (next == NULL) {
                    success = 0;
                    break;
                }
                next->node = child;
                next->active_namespace = namespace_id;
                next->parent = child_parent;
                next->namespace = namespace;
            }
        } else if (node->type == LXB_DOM_NODE_TYPE_TEXT) {
            lxb_dom_text_t *text = lxb_dom_interface_text(node);
            converted = xmlNewDocTextLen(
                target,
                text->char_data.data.data,
                (int) text->char_data.data.length
            );
            if (converted == NULL
                || xmlAddChild(item->parent, converted) == NULL) {
                if (converted != NULL) {
                    xmlFreeNode(converted);
                }
                success = 0;
            }
        } else if (node->type == LXB_DOM_NODE_TYPE_COMMENT) {
            lxb_dom_comment_t *comment = lxb_dom_interface_comment(node);
            converted = xmlNewDocComment(
                target,
                comment->char_data.data.data
            );
            if (converted == NULL
                || xmlAddChild(item->parent, converted) == NULL) {
                if (converted != NULL) {
                    xmlFreeNode(converted);
                }
                success = 0;
            }
        } else if (node->type == LXB_DOM_NODE_TYPE_DOCUMENT_TYPE) {
            lxb_dom_document_type_t *doctype =
                lxb_dom_interface_document_type(node);
            size_t public_length;
            size_t system_length;
            const lxb_char_t *name =
                lxb_dom_document_type_name(doctype, NULL);
            const lxb_char_t *public_id =
                lxb_dom_document_type_public_id(doctype, &public_length);
            const lxb_char_t *system_id =
                lxb_dom_document_type_system_id(doctype, &system_length);

            if (xmlCreateIntSubset(
                    target,
                    name,
                    public_length == 0 ? NULL : public_id,
                    system_length == 0 ? NULL : system_id
                ) == NULL) {
                success = 0;
            }
        }
    }
    lexbor_array_obj_destroy(&work_list, 0);
    return success;
}

static int32_t elephc_dom_html_convert_document(
    lxb_html_document_t *source,
    xmlDocPtr target,
    int32_t create_default_namespace
)
{
    return elephc_dom_html_convert_children(
        lxb_dom_interface_node(source),
        target,
        (xmlNodePtr) target,
        create_default_namespace
    );
}

static lxb_dom_document_cmode_t elephc_dom_html_document_mode(
    const xmlDoc *document
)
{
    if (document->_private
        == (void *) &elephc_dom_limited_quirks_marker) {
        return LXB_DOM_DOCUMENT_CMODE_LIMITED_QUIRKS;
    }
    if (document->_private == (void *) &elephc_dom_quirks_marker) {
        return LXB_DOM_DOCUMENT_CMODE_QUIRKS;
    }
    return LXB_DOM_DOCUMENT_CMODE_NO_QUIRKS;
}

int32_t elephc_dom_native_html_document_quirks_mode(void *document)
{
    if (document == NULL) {
        return 0;
    }
    return (int32_t) elephc_dom_html_document_mode(
        (const xmlDoc *) document
    );
}

static void elephc_dom_html_store_document_mode(
    xmlDocPtr document,
    lxb_dom_document_cmode_t mode
)
{
    if (mode == LXB_DOM_DOCUMENT_CMODE_LIMITED_QUIRKS) {
        document->_private =
            (void *) &elephc_dom_limited_quirks_marker;
    } else if (mode == LXB_DOM_DOCUMENT_CMODE_QUIRKS) {
        document->_private = (void *) &elephc_dom_quirks_marker;
    } else {
        document->_private = (void *) &elephc_dom_no_quirks_marker;
    }
}

void elephc_dom_native_html_copy_document_mode(
    void *source,
    void *target
)
{
    if (source != NULL && target != NULL) {
        elephc_dom_html_store_document_mode(
            (xmlDocPtr) target,
            elephc_dom_html_document_mode((const xmlDoc *) source)
        );
    }
}

static lxb_dom_node_t *elephc_dom_html_parse_fragment_utf8(
    lxb_html_document_t *document,
    lxb_dom_element_t *element,
    const uint8_t *input,
    size_t input_length
)
{
    const lxb_encoding_data_t *encoding =
        lxb_encoding_data(LXB_ENCODING_UTF_8);
    lxb_encoding_decode_t decoder;
    const lxb_char_t *current = input;
    const lxb_char_t *end = input + input_length;
    const lxb_char_t *last_output = current;
    lxb_status_t status;

    status = lxb_html_document_parse_fragment_chunk_begin(
        document,
        element
    );
    if (status != LXB_STATUS_OK || encoding == NULL) {
        return NULL;
    }
    lxb_encoding_decode_init_single(&decoder, encoding);
    while (current < end) {
        const lxb_char_t *candidate;
        lxb_codepoint_t codepoint;

        if (decoder.u.utf_8.need == 0 && *current < 0x80) {
            current++;
            continue;
        }
        candidate = current;
        codepoint = lxb_encoding_decode_utf_8_single(
            &decoder,
            &current,
            end
        );
        if (codepoint <= LXB_ENCODING_MAX_CODEPOINT) {
            continue;
        }
        status = lxb_html_document_parse_fragment_chunk(
            document,
            last_output,
            (size_t) (candidate - last_output)
        );
        if (status != LXB_STATUS_OK
            || lxb_html_document_parse_fragment_chunk(
                document,
                LXB_ENCODING_REPLACEMENT_BYTES,
                LXB_ENCODING_REPLACEMENT_SIZE
            ) != LXB_STATUS_OK) {
            return NULL;
        }
        last_output = current;
    }
    if (current != last_output
        && lxb_html_document_parse_fragment_chunk(
            document,
            last_output,
            (size_t) (current - last_output)
        ) != LXB_STATUS_OK) {
        return NULL;
    }
    return lxb_html_document_parse_fragment_chunk_end(document);
}

void *elephc_dom_native_parse_html_fragment(
    void *context,
    const uint8_t *input,
    size_t input_length
)
{
    xmlNodePtr context_node = (xmlNodePtr) context;
    lxb_html_document_t *source = NULL;
    lxb_dom_element_t *element = NULL;
    lxb_dom_node_t *parsed;
    const lxb_tag_data_t *tag;
    const lxb_ns_data_t *namespace;
    const lxb_char_t *namespace_uri;
    size_t namespace_length;
    xmlNodePtr fragment = NULL;

    if (context_node == NULL || context_node->doc == NULL
        || context_node->type != XML_ELEMENT_NODE
        || (input == NULL && input_length != 0)) {
        return NULL;
    }
    source = lxb_html_document_create();
    if (source == NULL) {
        return NULL;
    }
    source->dom_document.compat_mode =
        elephc_dom_html_document_mode(context_node->doc);
    element =
        lxb_dom_element_interface_create(&source->dom_document);
    if (element == NULL) {
        goto cleanup;
    }
    tag = lxb_tag_data_by_name(
        source->dom_document.tags,
        context_node->name,
        xmlStrlen(context_node->name)
    );
    element->node.local_name =
        tag == NULL ? LXB_TAG__UNDEF : tag->tag_id;
    if (context_node->ns == NULL || context_node->ns->href == NULL) {
        namespace_uri = (const lxb_char_t *) "";
        namespace_length = 0;
    } else {
        namespace_uri = context_node->ns->href;
        namespace_length = xmlStrlen(namespace_uri);
    }
    namespace = lxb_ns_data_by_link(
        source->dom_document.ns,
        namespace_uri,
        namespace_length
    );
    element->node.ns =
        namespace == NULL ? LXB_NS__UNDEF : namespace->ns_id;
    parsed = elephc_dom_html_parse_fragment_utf8(
        source,
        element,
        input,
        input_length
    );
    if (parsed == NULL) {
        goto cleanup;
    }
    fragment = xmlNewDocFragment(context_node->doc);
    if (fragment == NULL
        || !elephc_dom_html_convert_children(
            parsed,
            context_node->doc,
            fragment,
            1
        )) {
        if (fragment != NULL) {
            xmlFreeNode(fragment);
            fragment = NULL;
        }
    }

cleanup:
    lxb_html_document_destroy(source);
    return fragment;
}

static xmlNodePtr elephc_dom_html_find_child(
    xmlNodePtr parent,
    const xmlChar *name
)
{
    xmlNodePtr node = parent->children;
    while (node != NULL) {
        if (node->type == XML_ELEMENT_NODE
            && xmlStrEqual(node->name, name)) {
            return node;
        }
        node = node->next;
    }
    return NULL;
}

static void elephc_dom_html_hoist_children(
    xmlNodePtr parent,
    const xmlChar *name
)
{
    xmlNodePtr node = elephc_dom_html_find_child(parent, name);
    if (node == NULL) {
        return;
    }
    xmlUnlinkNode(node);
    while (node->children != NULL) {
        xmlNodePtr child = node->children;
        xmlUnlinkNode(child);
        xmlAddChild(parent, child);
    }
    xmlFreeNode(node);
}

elephc_dom_native_parse_result elephc_dom_native_document_parse_html5(
    const uint8_t *bytes,
    size_t length,
    uint32_t options,
    const uint8_t *override_encoding,
    size_t override_encoding_length,
    const uint8_t *input_name,
    size_t input_name_length
)
{
    elephc_dom_native_parse_result result = {NULL, NULL, 0, 0, 0};
    const uint8_t *source = bytes;
    const lxb_encoding_data_t *encoding;
    lxb_html_document_t *lexbor_document = NULL;
    lxb_html_parser_t *parser;
    xmlDocPtr document = NULL;
    uint8_t *decoded = NULL;
    size_t decoded_length = 0;
    elephc_dom_html_error_list errors = {NULL, 0, 0};

    if ((bytes == NULL && length != 0)
        || (override_encoding == NULL && override_encoding_length != 0)
        || input_name == NULL
        || input_name_length > INT_MAX
        || memchr(input_name, '\0', input_name_length) != NULL) {
        return result;
    }
    encoding = elephc_dom_html_detect_encoding(
        &source,
        &length,
        override_encoding,
        override_encoding_length
    );
    if (encoding == NULL) {
        result.reserved = 1;
        return result;
    }
    lexbor_document = lxb_html_document_create();
    if (lexbor_document == NULL) {
        result.allocation_failed = 1;
        goto cleanup;
    }
    decoded = elephc_dom_html_decode_and_parse(
            lexbor_document,
            source,
            length,
            encoding,
            &decoded_length,
            input_name,
            input_name_length,
            options,
            &errors
        );
    if (decoded == NULL) {
        result.allocation_failed = 1;
        goto cleanup;
    }
    parser = lexbor_document->dom_document.parser;
    if (lxb_html_document_parse_chunk_end(lexbor_document)
        != LXB_STATUS_OK) {
        result.allocation_failed = 1;
        goto cleanup;
    }
    document = htmlNewDocNoDtD(NULL, NULL);
    if (document == NULL
        || !elephc_dom_html_convert_document(
            lexbor_document,
            document,
            (options & ELEPHC_DOM_HTML_NO_DEFAULT_NS) == 0
        )) {
        result.allocation_failed = 1;
        goto cleanup;
    }
    if ((options & ELEPHC_DOM_HTML_NOIMPLIED) != 0) {
        xmlNodePtr html = elephc_dom_html_find_child(
            (xmlNodePtr) document,
            (const xmlChar *) "html"
        );
        if (html != NULL) {
            if (!parser->tree->has_explicit_head_tag) {
                elephc_dom_html_hoist_children(
                    html,
                    (const xmlChar *) "head"
                );
            }
            if (!parser->tree->has_explicit_body_tag) {
                elephc_dom_html_hoist_children(
                    html,
                    (const xmlChar *) "body"
                );
            }
            if (!parser->tree->has_explicit_html_tag) {
                elephc_dom_html_hoist_children(
                    (xmlNodePtr) document,
                    (const xmlChar *) "html"
                );
            }
        }
    }
    document->encoding = xmlStrdup((const xmlChar *) encoding->name);
    if (document->encoding == NULL) {
        result.allocation_failed = 1;
        goto cleanup;
    }
    elephc_dom_html_store_document_mode(
        document,
        lexbor_document->dom_document.compat_mode
    );
    result.document = document;
    result.errors = errors.errors;
    result.error_count = errors.count;
    errors.errors = NULL;
    errors.count = 0;
    errors.capacity = 0;
    document = NULL;

cleanup:
    if (document != NULL) {
        elephc_dom_native_document_free(document);
    }
    if (lexbor_document != NULL) {
        lxb_html_document_destroy(lexbor_document);
    }
    free(decoded);
    elephc_dom_html_error_list_free(&errors);
    return result;
}
