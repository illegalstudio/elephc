/*
 * Stable native entry points around php-src's pinned libxml2 selector
 * adapter. Results own only flat C allocations and never expose Lexbor state.
 */

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "lexbor/css/parser.h"
#include "dom/lexbor/selectors-adapted/selectors.h"

typedef struct {
    void **pointers;
    size_t count;
    int32_t matched;
    int32_t error_code;
    uint8_t *message;
    size_t message_length;
} elephc_dom_native_selector_result;

typedef struct {
    void **pointers;
    size_t count;
    size_t capacity;
    int32_t failed;
} elephc_dom_selector_nodes;

typedef struct {
    const xmlNode *reference;
    int32_t matched;
} elephc_dom_selector_match_context;

static _Thread_local int32_t elephc_dom_selector_exception_code;
static _Thread_local const char *elephc_dom_selector_exception_message;

int32_t elephc_dom_selector_has_exception(void)
{
    return elephc_dom_selector_exception_code != 0;
}

void elephc_dom_selector_throw_error(
    int32_t code,
    const char *message,
    int32_t strict
)
{
    (void) strict;
    if (elephc_dom_selector_exception_code == 0) {
        elephc_dom_selector_exception_code = code;
        elephc_dom_selector_exception_message = message;
    }
}

static int32_t elephc_dom_selector_copy_error(
    elephc_dom_native_selector_result *result,
    int32_t code,
    const char *prefix,
    const uint8_t *detail,
    size_t detail_length
)
{
    size_t prefix_length = prefix == NULL ? 0 : strlen(prefix);

    result->error_code = code;
    if (prefix_length > SIZE_MAX - detail_length) {
        result->error_code = -1;
        return 0;
    }
    result->message_length = prefix_length + detail_length;
    if (result->message_length == 0) {
        return 1;
    }
    result->message = malloc(result->message_length);
    if (result->message == NULL) {
        result->message_length = 0;
        result->error_code = -1;
        return 0;
    }
    if (prefix_length != 0) {
        memcpy(result->message, prefix, prefix_length);
    }
    if (detail_length != 0) {
        memcpy(
            result->message + prefix_length,
            detail,
            detail_length
        );
    }
    return 1;
}

static lxb_status_t elephc_dom_selector_collect(
    const xmlNode *node,
    lxb_css_selector_specificity_t specificity,
    void *context
)
{
    elephc_dom_selector_nodes *nodes =
        (elephc_dom_selector_nodes *) context;
    void **replacement;
    size_t capacity;

    (void) specificity;
    if (nodes->count == nodes->capacity) {
        capacity = nodes->capacity == 0 ? 8 : nodes->capacity * 2;
        if (capacity < nodes->capacity
            || capacity > SIZE_MAX / sizeof(*replacement)) {
            nodes->failed = 1;
            return LXB_STATUS_ERROR_MEMORY_ALLOCATION;
        }
        replacement = realloc(
            nodes->pointers,
            capacity * sizeof(*replacement)
        );
        if (replacement == NULL) {
            nodes->failed = 1;
            return LXB_STATUS_ERROR_MEMORY_ALLOCATION;
        }
        nodes->pointers = replacement;
        nodes->capacity = capacity;
    }
    nodes->pointers[nodes->count++] = (void *) node;
    return LXB_STATUS_OK;
}

static lxb_status_t elephc_dom_selector_collect_first(
    const xmlNode *node,
    lxb_css_selector_specificity_t specificity,
    void *context
)
{
    lxb_status_t status = elephc_dom_selector_collect(
        node,
        specificity,
        context
    );

    return status == LXB_STATUS_OK ? LXB_STATUS_STOP : status;
}

static lxb_status_t elephc_dom_selector_match(
    const xmlNode *node,
    lxb_css_selector_specificity_t specificity,
    void *context
)
{
    elephc_dom_selector_match_context *match_context =
        (elephc_dom_selector_match_context *) context;

    (void) specificity;
    if (node == match_context->reference) {
        match_context->matched = 1;
        return LXB_STATUS_STOP;
    }
    return LXB_STATUS_OK;
}

static int32_t elephc_dom_selector_status_ok(lxb_status_t status)
{
    return status == LXB_STATUS_OK || status == LXB_STATUS_STOP;
}

elephc_dom_native_selector_result
elephc_dom_native_selector_query(
    void *root,
    const uint8_t *input,
    size_t input_length,
    int32_t operation,
    int32_t quirks
)
{
    elephc_dom_native_selector_result result =
        {NULL, 0, 0, 0, NULL, 0};
    elephc_dom_selector_nodes nodes = {NULL, 0, 0, 0};
    lxb_css_parser_t parser;
    lxb_selectors_t selectors;
    lxb_css_selector_list_t *list = NULL;
    lxb_status_t status;
    int32_t parser_initialized = 0;
    int32_t selectors_initialized = 0;

    if (root == NULL || (input == NULL && input_length != 0)
        || operation < 0 || operation > 3) {
        result.error_code = -1;
        return result;
    }
    elephc_dom_selector_exception_code = 0;
    elephc_dom_selector_exception_message = NULL;
    memset(&parser, 0, sizeof(parser));
    status = lxb_css_parser_init(&parser, NULL);
    if (status != LXB_STATUS_OK) {
        result.error_code = -1;
        goto cleanup;
    }
    parser_initialized = 1;
    memset(&selectors, 0, sizeof(selectors));
    status = lxb_selectors_init(&selectors);
    if (status != LXB_STATUS_OK) {
        result.error_code = -1;
        goto cleanup;
    }
    selectors_initialized = 1;
    lxb_selectors_opt_set(
        &selectors,
        (operation == 1 ? LXB_SELECTORS_OPT_DEFAULT
            : LXB_SELECTORS_OPT_MATCH_FIRST)
            | (quirks != 0 ? LXB_SELECTORS_OPT_QUIRKS_MODE : 0)
    );
    list = lxb_css_selectors_parse(
        &parser,
        (const lxb_char_t *) input,
        input_length
    );
    if (list == NULL) {
        size_t message_count =
            lexbor_array_obj_length(&parser.log->messages);

        if (message_count == 0) {
            elephc_dom_selector_copy_error(
                &result,
                12,
                NULL,
                (const uint8_t *) "Invalid selector",
                sizeof("Invalid selector") - 1
            );
        } else {
            lxb_css_log_message_t *message = lexbor_array_obj_get(
                &parser.log->messages,
                0
            );
            elephc_dom_selector_copy_error(
                &result,
                12,
                "Invalid selector (",
                message->text.data,
                message->text.length
            );
            if (result.error_code == 12) {
                if (result.message_length == SIZE_MAX) {
                    free(result.message);
                    result.message = NULL;
                    result.message_length = 0;
                    result.error_code = -1;
                    goto cleanup;
                }
                uint8_t *replacement = realloc(
                    result.message,
                    result.message_length + 1
                );
                if (replacement == NULL) {
                    free(result.message);
                    result.message = NULL;
                    result.message_length = 0;
                    result.error_code = -1;
                } else {
                    result.message = replacement;
                    result.message[result.message_length++] = ')';
                }
            }
        }
        goto cleanup;
    }

    if (operation == 0 || operation == 1) {
        status = lxb_selectors_find(
            &selectors,
            (const xmlNode *) root,
            list,
            operation == 0
                ? elephc_dom_selector_collect_first
                : elephc_dom_selector_collect,
            &nodes
        );
    } else if (operation == 2) {
        elephc_dom_selector_match_context match_context = {
            (const xmlNode *) root,
            0
        };

        status = lxb_selectors_match_node(
            &selectors,
            (const xmlNode *) root,
            list,
            elephc_dom_selector_match,
            &match_context
        );
        result.matched = match_context.matched;
    } else {
        const xmlNode *current = (const xmlNode *) root;

        status = LXB_STATUS_OK;
        while (current != NULL) {
            elephc_dom_selector_match_context match_context = {
                current,
                0
            };

            status = lxb_selectors_match_node(
                &selectors,
                current,
                list,
                elephc_dom_selector_match,
                &match_context
            );
            if (!elephc_dom_selector_status_ok(status)
                || match_context.matched != 0) {
                if (match_context.matched != 0) {
                    result.matched = 1;
                    status = elephc_dom_selector_collect_first(
                        current,
                        0,
                        &nodes
                    );
                }
                break;
            }
            current = current->parent;
        }
    }
    if (elephc_dom_selector_exception_code != 0) {
        const char *message = elephc_dom_selector_exception_message;

        elephc_dom_selector_copy_error(
            &result,
            elephc_dom_selector_exception_code,
            NULL,
            (const uint8_t *) message,
            message == NULL ? 0 : strlen(message)
        );
    } else if (!elephc_dom_selector_status_ok(status)
        || nodes.failed != 0) {
        result.error_code = nodes.failed != 0 ? -1 : 100;
    } else {
        result.pointers = nodes.pointers;
        result.count = nodes.count;
        nodes.pointers = NULL;
    }

cleanup:
    free(nodes.pointers);
    if (list != NULL) {
        lxb_css_selector_list_destroy_memory(list);
    }
    if (selectors_initialized != 0) {
        lxb_selectors_destroy(&selectors);
    }
    if (parser_initialized != 0) {
        (void) lxb_css_parser_destroy(&parser, false);
    }
    return result;
}

void elephc_dom_native_selector_result_free(
    void **pointers,
    uint8_t *message
)
{
    free(pointers);
    free(message);
}
