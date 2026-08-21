/*
 * Native helpers for the PHP 8.5 SimpleXML method surface.
 *
 * The bridge owns wrapper identity and document lifetimes in Rust.  This file
 * only performs the libxml2 operations that cannot be expressed through the
 * existing DOM adapter without losing SimpleXML's QName or namespace rules.
 */

#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <libxml/tree.h>
#include <libxml/xmlIO.h>

typedef struct {
    void *pointer;
    int32_t error_code;
    int32_t reserved;
} elephc_dom_native_pointer_result;

typedef struct {
    uint8_t *pointer;
    size_t length;
} elephc_dom_native_simplexml_buffer;

typedef struct {
    const uint8_t *prefix;
    size_t prefix_length;
    const uint8_t *namespace_uri;
    size_t namespace_uri_length;
} elephc_dom_native_simplexml_namespace;

typedef struct {
    elephc_dom_native_simplexml_namespace *items;
    size_t count;
    int32_t allocation_failed;
    int32_t reserved;
} elephc_dom_native_simplexml_namespace_result;

static uint8_t *simplexml_copy_bytes(const xmlChar *bytes, size_t length)
{
    uint8_t *copy;

    if (length == 0) {
        return NULL;
    }
    copy = malloc(length);
    if (copy != NULL) {
        memcpy(copy, bytes, length);
    }
    return copy;
}

static int simplexml_namespace_append(
    elephc_dom_native_simplexml_namespace_result *result,
    size_t *capacity,
    const xmlChar *prefix,
    const xmlChar *namespace_uri
)
{
    size_t prefix_length = prefix == NULL ? 0 : (size_t) xmlStrlen(prefix);
    size_t namespace_uri_length = namespace_uri == NULL
        ? 0
        : (size_t) xmlStrlen(namespace_uri);
    elephc_dom_native_simplexml_namespace *items;
    size_t index;

    for (index = 0; index < result->count; index++) {
        const elephc_dom_native_simplexml_namespace *item = &result->items[index];
        if (item->prefix_length == prefix_length
            && (prefix_length == 0
                || memcmp(item->prefix, prefix, prefix_length) == 0)) {
            return 1;
        }
    }

    if (result->count == *capacity) {
        size_t next_capacity = *capacity == 0 ? 8 : *capacity * 2;
        if (next_capacity < *capacity
            || next_capacity > SIZE_MAX / sizeof(*result->items)) {
            return 0;
        }
        items = realloc(result->items, next_capacity * sizeof(*result->items));
        if (items == NULL) {
            return 0;
        }
        result->items = items;
        *capacity = next_capacity;
    }

    result->items[result->count].prefix =
        simplexml_copy_bytes(prefix, prefix_length);
    result->items[result->count].prefix_length = prefix_length;
    result->items[result->count].namespace_uri =
        simplexml_copy_bytes(namespace_uri, namespace_uri_length);
    result->items[result->count].namespace_uri_length = namespace_uri_length;
    if ((prefix_length != 0 && result->items[result->count].prefix == NULL)
        || (namespace_uri_length != 0
            && result->items[result->count].namespace_uri == NULL)) {
        free((void *) result->items[result->count].prefix);
        free((void *) result->items[result->count].namespace_uri);
        return 0;
    }
    result->count++;
    return 1;
}

static int simplexml_buffer_write(void *context, const char *bytes, int length)
{
    if (context == NULL || bytes == NULL || length < 0) {
        return -1;
    }
    return xmlBufferAdd(
        (xmlBufferPtr) context,
        (const xmlChar *) bytes,
        length
    ) == 0 ? length : -1;
}

static int simplexml_buffer_close(void *context)
{
    return context == NULL ? -1 : 0;
}

elephc_dom_native_simplexml_buffer
elephc_dom_native_simplexml_serialize_node(void *document, void *node)
{
    elephc_dom_native_simplexml_buffer result = {NULL, 0};
    xmlBufferPtr buffer;
    xmlOutputBufferPtr output;
    size_t length;

    if (document == NULL || node == NULL) {
        return result;
    }
    buffer = xmlBufferCreate();
    if (buffer == NULL) {
        return result;
    }
    output = xmlOutputBufferCreateIO(
        simplexml_buffer_write,
        simplexml_buffer_close,
        buffer,
        NULL
    );
    if (output == NULL) {
        xmlBufferFree(buffer);
        return result;
    }
    xmlNodeDumpOutput(
        output,
        (xmlDocPtr) document,
        (xmlNodePtr) node,
        0,
        0,
        (const char *) ((xmlDocPtr) document)->encoding
    );
    if (xmlOutputBufferFlush(output) < 0) {
        xmlOutputBufferClose(output);
        xmlBufferFree(buffer);
        return result;
    }
    if (xmlOutputBufferClose(output) < 0) {
        xmlBufferFree(buffer);
        return result;
    }
    length = xmlBufferLength(buffer);
    result.pointer = xmlMalloc(length == 0 ? 1 : length);
    if (result.pointer != NULL && length != 0) {
        memcpy(result.pointer, xmlBufferContent(buffer), length);
    }
    if (result.pointer != NULL) {
        result.length = length;
    }
    xmlBufferFree(buffer);
    return result;
}

elephc_dom_native_simplexml_buffer
elephc_dom_native_simplexml_node_list_content(void *node)
{
    elephc_dom_native_simplexml_buffer result = {NULL, 0};
    xmlNodePtr native_node = node;
    xmlChar *content = NULL;

    if (native_node == NULL || native_node->doc == NULL) {
        return result;
    }
    if (native_node->children != NULL) {
        content = xmlNodeListGetString(
            native_node->doc,
            native_node->children,
            1
        );
    } else if ((native_node->type == XML_COMMENT_NODE
        || native_node->type == XML_PI_NODE)
        && native_node->content != NULL) {
        content = xmlStrdup(native_node->content);
    }
    if (content == NULL) {
        content = xmlStrdup(BAD_CAST "");
    }
    if (content != NULL) {
        result.pointer = content;
        result.length = (size_t) xmlStrlen(content);
    }
    return result;
}

/* Returns libxml2's raw node name used by php-src's SimpleXML property hash.
 * Unlike DOM nodeName/localName, this deliberately exposes "comment" for an
 * XML comment and the processing-instruction target for an XML PI. */
elephc_dom_native_simplexml_buffer
elephc_dom_native_simplexml_node_name(void *node)
{
    elephc_dom_native_simplexml_buffer result = {NULL, 0};
    xmlNodePtr native_node = node;

    if (native_node != NULL && native_node->name != NULL) {
        result.pointer = (uint8_t *) native_node->name;
        result.length = (size_t) xmlStrlen(native_node->name);
    }
    return result;
}

static int simplexml_collect_used_namespaces(
    xmlNodePtr node,
    int recursive,
    elephc_dom_native_simplexml_namespace_result *result,
    size_t *capacity
)
{
    xmlAttrPtr attribute;
    xmlNodePtr child;

    if (node == NULL) {
        return 1;
    }
    if (node->ns != NULL
        && !simplexml_namespace_append(
            result,
            capacity,
            node->ns->prefix,
            node->ns->href
        )) {
        return 0;
    }
    for (attribute = node->properties; attribute != NULL; attribute = attribute->next) {
        if (attribute->ns != NULL
            && !simplexml_namespace_append(
                result,
                capacity,
                attribute->ns->prefix,
                attribute->ns->href
            )) {
            return 0;
        }
    }
    if (!recursive) {
        return 1;
    }
    for (child = node->children; child != NULL; child = child->next) {
        if (child->type == XML_ELEMENT_NODE
            && !simplexml_collect_used_namespaces(
                child,
                recursive,
                result,
                capacity
            )) {
            return 0;
        }
    }
    return 1;
}

static int simplexml_collect_declared_namespaces(
    xmlNodePtr node,
    int recursive,
    int include_xmlns_attributes,
    elephc_dom_native_simplexml_namespace_result *result,
    size_t *capacity
)
{
    static const xmlChar XMLNS_NAMESPACE[] =
        "http://www.w3.org/2000/xmlns/";
    xmlNsPtr namespace_definition;
    xmlAttrPtr attribute;
    xmlNodePtr child;

    if (node == NULL || node->type != XML_ELEMENT_NODE) {
        return 1;
    }
    for (namespace_definition = node->nsDef;
         namespace_definition != NULL;
         namespace_definition = namespace_definition->next) {
        if (!simplexml_namespace_append(
            result,
            capacity,
            namespace_definition->prefix,
            namespace_definition->href
        )) {
            return 0;
        }
    }
    if (include_xmlns_attributes) {
        for (attribute = node->properties;
             attribute != NULL;
             attribute = attribute->next) {
            xmlChar *value;
            const xmlChar *prefix;

            if (attribute->ns == NULL
                || !xmlStrEqual(attribute->ns->href, XMLNS_NAMESPACE)) {
                continue;
            }
            prefix = attribute->ns->prefix == NULL
                ? NULL
                : attribute->name;
            value = xmlNodeListGetString(node->doc, attribute->children, 1);
            if (!simplexml_namespace_append(
                result,
                capacity,
                prefix,
                value
            )) {
                xmlFree(value);
                return 0;
            }
            xmlFree(value);
        }
    }
    if (!recursive) {
        return 1;
    }
    for (child = node->children; child != NULL; child = child->next) {
        if (!simplexml_collect_declared_namespaces(
            child,
            recursive,
            include_xmlns_attributes,
            result,
            capacity
        )) {
            return 0;
        }
    }
    return 1;
}

elephc_dom_native_pointer_result elephc_dom_native_simplexml_add_child(
    void *document,
    void *node,
    const uint8_t *qualified_name,
    size_t qualified_name_length,
    const uint8_t *value,
    size_t value_length,
    int32_t has_value,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    int32_t has_namespace
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlDocPtr doc = document;
    xmlNodePtr parent = node;
    xmlNodePtr child = NULL;
    xmlNsPtr namespace_definition = NULL;
    xmlChar *qualified_name_copy = NULL;
    xmlChar *local_name = NULL;
    xmlChar *prefix = NULL;
    xmlChar *value_copy = NULL;
    xmlChar *namespace_uri_copy = NULL;
    int local_name_is_owned = 0;

    if (doc == NULL || parent == NULL || qualified_name == NULL
        || qualified_name_length == 0 || qualified_name_length > INT_MAX
        || value_length > INT_MAX || namespace_uri_length > INT_MAX) {
        result.error_code = -1;
        return result;
    }
    qualified_name_copy = xmlStrndup(
        qualified_name,
        (int) qualified_name_length
    );
    if (qualified_name_copy == NULL) {
        result.error_code = -1;
        return result;
    }
    local_name = xmlSplitQName2(qualified_name_copy, &prefix);
    if (local_name == NULL) {
        local_name = qualified_name_copy;
    } else {
        local_name_is_owned = 1;
    }
    if (has_value) {
        value_copy = xmlStrndup(value, (int) value_length);
        if (value_copy == NULL) {
            result.error_code = -1;
            goto cleanup;
        }
    }
    child = xmlNewChild(parent, NULL, local_name, value_copy);
    if (child == NULL) {
        result.error_code = -1;
        goto cleanup;
    }
    if (has_namespace) {
        namespace_uri_copy = xmlStrndup(
            namespace_uri,
            (int) namespace_uri_length
        );
        if (namespace_uri_copy == NULL) {
            xmlUnlinkNode(child);
            xmlFreeNode(child);
            child = NULL;
            result.error_code = -1;
            goto cleanup;
        }
        if (namespace_uri_length == 0) {
            child->ns = NULL;
            namespace_definition = xmlNewNs(
                child,
                namespace_uri_copy,
                prefix
            );
        } else {
            namespace_definition = xmlSearchNsByHref(
                doc,
                parent,
                namespace_uri_copy
            );
            if (namespace_definition == NULL) {
                namespace_definition = xmlNewNs(
                    child,
                    namespace_uri_copy,
                    prefix
                );
            }
            child->ns = namespace_definition;
        }
        if (namespace_definition == NULL) {
            xmlUnlinkNode(child);
            xmlFreeNode(child);
            child = NULL;
            result.error_code = -1;
            goto cleanup;
        }
    }
    result.pointer = child;

cleanup:
    xmlFree(namespace_uri_copy);
    xmlFree(value_copy);
    if (local_name_is_owned) {
        xmlFree(local_name);
    }
    xmlFree(prefix);
    xmlFree(qualified_name_copy);
    return result;
}

int32_t elephc_dom_native_simplexml_add_attribute(
    void *document,
    void *node,
    const uint8_t *qualified_name,
    size_t qualified_name_length,
    const uint8_t *value,
    size_t value_length,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    int32_t has_namespace
)
{
    xmlDocPtr doc = document;
    xmlNodePtr element = node;
    xmlAttrPtr existing;
    xmlAttrPtr created;
    xmlNsPtr namespace_definition = NULL;
    xmlChar *qualified_name_copy = NULL;
    xmlChar *local_name = NULL;
    xmlChar *prefix = NULL;
    xmlChar *value_copy = NULL;
    xmlChar *namespace_uri_copy = NULL;
    int local_name_is_owned = 0;
    int32_t status = -1;

    if (doc == NULL || element == NULL || element->type != XML_ELEMENT_NODE
        || qualified_name == NULL || qualified_name_length == 0
        || qualified_name_length > INT_MAX || value_length > INT_MAX
        || namespace_uri_length > INT_MAX) {
        return -1;
    }
    qualified_name_copy = xmlStrndup(
        qualified_name,
        (int) qualified_name_length
    );
    if (qualified_name_copy == NULL) {
        return -1;
    }
    local_name = xmlSplitQName2(qualified_name_copy, &prefix);
    if (local_name == NULL) {
        local_name = qualified_name_copy;
        if (namespace_uri_length > 0) {
            status = -2;
            goto cleanup;
        }
    } else {
        local_name_is_owned = 1;
    }
    if (has_namespace) {
        namespace_uri_copy = xmlStrndup(
            namespace_uri,
            (int) namespace_uri_length
        );
        if (namespace_uri_copy == NULL) {
            goto cleanup;
        }
    }
    existing = xmlHasNsProp(
        element,
        local_name,
        has_namespace ? namespace_uri_copy : NULL
    );
    if (existing != NULL && existing->type != XML_ATTRIBUTE_DECL) {
        status = -3;
        goto cleanup;
    }
    if (has_namespace) {
        namespace_definition = xmlSearchNsByHref(
            doc,
            element,
            namespace_uri_copy
        );
        if (namespace_definition == NULL) {
            namespace_definition = xmlNewNs(
                element,
                namespace_uri_copy,
                prefix
            );
        }
        if (namespace_definition == NULL) {
            goto cleanup;
        }
    }
    value_copy = xmlStrndup(value, (int) value_length);
    if (value_copy == NULL) {
        goto cleanup;
    }
    created = xmlNewNsProp(
        element,
        namespace_definition,
        local_name,
        value_copy
    );
    status = created == NULL ? -1 : 0;

cleanup:
    xmlFree(namespace_uri_copy);
    xmlFree(value_copy);
    if (local_name_is_owned) {
        xmlFree(local_name);
    }
    xmlFree(prefix);
    xmlFree(qualified_name_copy);
    return status;
}

elephc_dom_native_simplexml_namespace_result
elephc_dom_native_simplexml_get_namespaces(void *node, int32_t recursive)
{
    elephc_dom_native_simplexml_namespace_result result = {NULL, 0, 0, 0};
    xmlNodePtr native_node = node;
    size_t capacity = 0;
    int success = 1;

    if (native_node == NULL) {
        return result;
    }
    if (native_node->type == XML_ELEMENT_NODE) {
        success = simplexml_collect_used_namespaces(
            native_node,
            recursive != 0,
            &result,
            &capacity
        );
    } else if (native_node->type == XML_ATTRIBUTE_NODE
        && native_node->ns != NULL) {
        success = simplexml_namespace_append(
            &result,
            &capacity,
            native_node->ns->prefix,
            native_node->ns->href
        );
    }
    result.allocation_failed = !success;
    return result;
}

elephc_dom_native_simplexml_namespace_result
elephc_dom_native_simplexml_get_doc_namespaces(
    void *document,
    void *node,
    int32_t recursive,
    int32_t from_root,
    int32_t include_xmlns_attributes
)
{
    elephc_dom_native_simplexml_namespace_result result = {NULL, 0, 0, 0};
    xmlDocPtr doc = document;
    xmlNodePtr native_node = node;
    size_t capacity = 0;

    if (from_root != 0) {
        native_node = doc == NULL ? NULL : xmlDocGetRootElement(doc);
    }
    if (native_node == NULL) {
        return result;
    }
    if (!simplexml_collect_declared_namespaces(
        native_node,
        recursive != 0,
        include_xmlns_attributes != 0,
        &result,
        &capacity
    )) {
        result.allocation_failed = 1;
    }
    return result;
}

void elephc_dom_native_simplexml_namespace_result_free(
    elephc_dom_native_simplexml_namespace_result *result
)
{
    size_t index;

    if (result == NULL) {
        return;
    }
    for (index = 0; index < result->count; index++) {
        free((void *) result->items[index].prefix);
        free((void *) result->items[index].namespace_uri);
    }
    free(result->items);
    result->items = NULL;
    result->count = 0;
}
