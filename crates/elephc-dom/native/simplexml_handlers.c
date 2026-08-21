/*
 * Native helpers for the SimpleXMLElement object-handler surface.
 * Mirrors the PHP 8.5.8 simplexml.c handler logic for cast/compare/count/
 * get_iterator/has_dimension/has_property/read_dimension/read_property/
 * unset_dimension/unset_property/write_dimension/write_property.
 *
 * Every helper accepts the receiver's node pointer, document pointer, and
 * the SimpleXML iterator state (iter_type, nsprefix, isprefix) carried by
 * the bridge wrapper. The bridge owns wrapper lifetime, so this file only
 * releases its own allocations.
 */

#include <limits.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <libxml/parser.h>
#include <libxml/tree.h>

typedef struct {
    uint8_t *pointer;
    size_t length;
} native_buffer;

typedef struct {
    uint8_t *pointer;
    size_t length;
    int32_t error_code;
    int32_t reserved;
} native_buffer_result;

typedef struct {
    void *pointer;
    int32_t error_code;
    int32_t reserved;
} native_pointer_result;

typedef struct {
    int32_t value;
    int32_t reserved;
} native_bool_result;

typedef struct {
    int32_t value;
    int32_t reserved;
} native_int_result;

static int handler_match_ns(xmlNodePtr node, const xmlChar *name, int is_prefix) {
    if (name == NULL && (node->ns == NULL || node->ns->prefix == NULL)) {
        return 1;
    }
    if (node->ns && xmlStrEqual(is_prefix ? node->ns->prefix : node->ns->href, name)) {
        return 1;
    }
    return 0;
}

static xmlChar *handler_dup_bytes(const uint8_t *bytes, size_t length) {
    if (bytes == NULL) {
        return NULL;
    }
    if (length > INT_MAX) {
        return NULL;
    }
    return xmlStrndup((const xmlChar *) bytes, (int) length);
}

static xmlNodePtr handler_iterator_fetch(xmlNodePtr node, int iter_type,
                                         const xmlChar *iter_name,
                                         const xmlChar *namespace_or_prefix,
                                         int is_prefix) {
    while (node) {
        if (iter_type == 3) {
            if (node->type == XML_ATTRIBUTE_NODE
                && (iter_name == NULL || xmlStrEqual(node->name, iter_name))
                && handler_match_ns(node, namespace_or_prefix, is_prefix)) {
                return node;
            }
        } else if (node->type == XML_ELEMENT_NODE
                   && (iter_type != 1 || (iter_name != NULL && xmlStrEqual(node->name, iter_name)))
                   && handler_match_ns(node, namespace_or_prefix, is_prefix)) {
            return node;
        }
        node = node->next;
    }
    return NULL;
}

static xmlNodePtr handler_reset_iterator(xmlNodePtr node, int iter_type,
                                         const xmlChar *iter_name,
                                         const xmlChar *namespace_or_prefix,
                                         int is_prefix) {
    if (node == NULL) {
        return NULL;
    }
    xmlNodePtr first = iter_type == 3 ? (xmlNodePtr) node->properties : node->children;
    return handler_iterator_fetch(first, iter_type, iter_name, namespace_or_prefix, is_prefix);
}

static xmlNodePtr handler_view_first(xmlNodePtr node, int iter_type,
                                     const xmlChar *iter_name,
                                     const xmlChar *namespace_or_prefix,
                                     int is_prefix) {
    if (iter_type == 0) {
        return node;
    }
    return handler_reset_iterator(node, iter_type, iter_name, namespace_or_prefix, is_prefix);
}

static xmlNodePtr handler_view_offset(xmlNodePtr node, int iter_type,
                                      const xmlChar *iter_name,
                                      const xmlChar *namespace_or_prefix,
                                      int is_prefix, int64_t offset) {
    if (node == NULL) {
        return NULL;
    }
    if (iter_type == 0) {
        return offset == 0 ? node : NULL;
    }
    xmlNodePtr current = handler_reset_iterator(
        node, iter_type, iter_name, namespace_or_prefix, is_prefix
    );
    int64_t index = 0;
    while (current && index < offset) {
        current = handler_iterator_fetch(
            current->next, iter_type, iter_name, namespace_or_prefix, is_prefix
        );
        index++;
    }
    return current;
}

/*
 * Mirrors php-src's sxe_prop_is_empty() without mutating iterator state.
 * This is deliberately not PHP string truthiness: a direct node containing
 * the text "0" is non-empty, and any selected attribute is non-empty even
 * when its value is empty or "0".
 */
static int handler_prop_is_empty(xmlNodePtr base, int iter_type,
                                 const xmlChar *iter_name,
                                 const xmlChar *namespace_or_prefix,
                                 int is_prefix) {
    xmlNodePtr node = base;
    int use_iter = 0;

    if (node == NULL) {
        return 1;
    }

    if (iter_type == 1) {
        node = handler_reset_iterator(
            base, iter_type, iter_name, namespace_or_prefix, is_prefix
        );
    }
    if (node && node->type != XML_ENTITY_DECL) {
        xmlAttrPtr attr = node->properties;
        int test_name = iter_name != NULL && iter_type == 3;
        while (attr) {
            if ((!test_name || xmlStrEqual(attr->name, iter_name))
                && handler_match_ns(
                    (xmlNodePtr) attr, namespace_or_prefix, is_prefix
                )) {
                return 0;
            }
            attr = attr->next;
        }
    }

    node = handler_view_first(
        base, iter_type, iter_name, namespace_or_prefix, is_prefix
    );
    if (node == NULL || iter_type == 3) {
        return 1;
    }
    if (node->type == XML_ATTRIBUTE_NODE) {
        return 0;
    }
    if (iter_type != 2) {
        if (iter_type == 0 || node->children == NULL || node->parent == NULL
            || node->children->next != NULL || node->children->children != NULL
            || node->parent->children == node->parent->last) {
            node = node->children;
        } else {
            node = handler_reset_iterator(
                base, iter_type, iter_name, namespace_or_prefix, is_prefix
            );
            use_iter = 1;
        }
    }

    while (node) {
        if (node->children != NULL || node->prev != NULL || node->next != NULL) {
            if (node->type == XML_TEXT_NODE) {
                goto next_iter;
            }
        } else if (node->type == XML_TEXT_NODE) {
            const xmlChar *content = node->content;
            if (content != NULL && content[0] != 0) {
                return 0;
            }
            goto next_iter;
        }

        if (node->type == XML_ELEMENT_NODE
            && !handler_match_ns(node, namespace_or_prefix, is_prefix)) {
            goto next_iter;
        }
        if (node->name != NULL) {
            return 0;
        }

next_iter:
        if (use_iter) {
            node = handler_iterator_fetch(
                node->next, iter_type, iter_name, namespace_or_prefix, is_prefix
            );
        } else {
            node = node->next;
        }
    }
    return 1;
}

/*
 * Extract text content for int/float/string cast handling via
 * xmlNodeListGetString(), matching simplexml.c cast_object().
 */
static int handler_cast(xmlDocPtr document, xmlNodePtr node,
                        native_buffer_result *out) {
    xmlChar *contents = NULL;
    if (node) {
        if (node->type == XML_COMMENT_NODE || node->type == XML_PI_NODE) {
            contents = (xmlChar *) node->content;
        } else {
            contents = xmlNodeListGetString(document, node->children, 1);
        }
    }
    if (contents) {
        size_t length = xmlStrlen(contents);
        out->pointer = (uint8_t *) malloc(length);
        if (!out->pointer) {
            xmlFree(contents);
            out->length = 0;
            out->error_code = 1;
            return 1;
        }
        memcpy(out->pointer, contents, length);
        out->length = length;
        if (node->type == XML_COMMENT_NODE || node->type == XML_PI_NODE) {
            /* content not owned by xmlNodeListGetString; do not free */
        } else {
            xmlFree(contents);
        }
    } else {
        out->pointer = NULL;
        out->length = 0;
    }
    out->error_code = 0;
    return 0;
}

/* -- public helper exports ------------------------------------------------- */

native_pointer_result elephc_dom_native_simplexml_handler_view_first(
        xmlNodePtr node, int iter_type,
        const uint8_t *iter_name, size_t iter_name_len,
        const uint8_t *namespace_or_prefix, size_t namespace_or_prefix_len,
        int is_prefix) {
    native_pointer_result result = {NULL, 0, 0};
    xmlChar *name = handler_dup_bytes(iter_name, iter_name_len);
    xmlChar *namespace_name = handler_dup_bytes(namespace_or_prefix, namespace_or_prefix_len);
    result.pointer = handler_view_first(node, iter_type, name, namespace_name, is_prefix);
    if (name) xmlFree(name);
    if (namespace_name) xmlFree(namespace_name);
    return result;
}

native_pointer_result elephc_dom_native_simplexml_handler_view_offset(
        xmlNodePtr node, int iter_type,
        const uint8_t *iter_name, size_t iter_name_len,
        const uint8_t *namespace_or_prefix, size_t namespace_or_prefix_len,
        int is_prefix, int64_t offset) {
    native_pointer_result result = {NULL, 0, 0};
    xmlChar *name = handler_dup_bytes(iter_name, iter_name_len);
    xmlChar *namespace_name = handler_dup_bytes(namespace_or_prefix, namespace_or_prefix_len);
    result.pointer = handler_view_offset(
        node, iter_type, name, namespace_name, is_prefix, offset
    );
    if (name) xmlFree(name);
    if (namespace_name) xmlFree(namespace_name);
    return result;
}

native_int_result elephc_dom_native_simplexml_handler_view_count(
        xmlNodePtr node, int iter_type,
        const uint8_t *iter_name, size_t iter_name_len,
        const uint8_t *namespace_or_prefix, size_t namespace_or_prefix_len,
        int is_prefix) {
    native_int_result result = {0, 0};
    xmlChar *name = handler_dup_bytes(iter_name, iter_name_len);
    xmlChar *namespace_name = handler_dup_bytes(namespace_or_prefix, namespace_or_prefix_len);
    xmlNodePtr current = handler_reset_iterator(
        node, iter_type, name, namespace_name, is_prefix
    );
    while (current) {
        result.value++;
        current = handler_iterator_fetch(
            current->next, iter_type, name, namespace_name, is_prefix
        );
    }
    if (name) xmlFree(name);
    if (namespace_name) xmlFree(namespace_name);
    return result;
}

native_int_result elephc_dom_native_simplexml_handler_selected_is_empty(xmlNodePtr node) {
    native_int_result result = {1, 0};
    if (node == NULL) {
        return result;
    }
    if (node->type == XML_ATTRIBUTE_NODE) {
        if (node->children && node->children->content && node->children->content[0]
            && !xmlStrEqual(node->children->content, (const xmlChar *) "0")) {
            result.value = 0;
        }
        return result;
    }
    if (node->children == NULL) {
        return result;
    }
    if (node->children->type == XML_TEXT_NODE && node->children->next == NULL
        && (node->children->content == NULL || node->children->content[0] == 0
            || xmlStrEqual(node->children->content, (const xmlChar *) "0"))) {
        return result;
    }
    result.value = 0;
    return result;
}

native_int_result elephc_dom_native_simplexml_handler_compare(xmlNodePtr node1, xmlNodePtr node2,
                                            xmlDocPtr doc1, xmlDocPtr doc2) {
    native_int_result result = {0, 0};
    if (node1 && node2) {
        result.value = (node1 == node2) ? 0 : 1 /* ZEND_UNCOMPARABLE */;
    } else if (!node1 && !node2) {
        result.value = (doc1 == doc2) ? 0 : 1;
    } else {
        result.value = 1;
    }
    return result;
}

native_buffer_result elephc_dom_native_simplexml_handler_cast_string(xmlDocPtr document, xmlNodePtr node, int iter_type) {
    (void) iter_type;
    native_buffer_result result = {0, 0, 0, 0};
    handler_cast(document, node, &result);
    return result;
}

native_int_result elephc_dom_native_simplexml_handler_cast_bool(
        xmlDocPtr document, xmlNodePtr node, int iter_type,
        const uint8_t *iter_name, size_t iter_name_len,
        const uint8_t *namespace_or_prefix, size_t namespace_or_prefix_len,
        int is_prefix) {
    (void) document;
    native_int_result result = {0, 0};
    xmlChar *name = handler_dup_bytes(iter_name, iter_name_len);
    xmlChar *namespace_name = handler_dup_bytes(namespace_or_prefix, namespace_or_prefix_len);
    xmlNodePtr first = iter_type == 0 ? NULL : handler_reset_iterator(
        node, iter_type, name, namespace_name, is_prefix
    );
    result.value = first != NULL
        || !handler_prop_is_empty(node, iter_type, name, namespace_name, is_prefix);
    if (name) xmlFree(name);
    if (namespace_name) xmlFree(namespace_name);
    return result;
}

/* -- dimension handlers (attribute access via []) -------------------------- */

static xmlAttrPtr handler_first_matching_attr(xmlNodePtr node, const xmlChar *name,
                                              const xmlChar *ns, int is_prefix) {
    xmlAttrPtr attr = node ? node->properties : NULL;
    while (attr) {
        if (handler_match_ns((xmlNodePtr) attr, ns, is_prefix)) {
            if (name == NULL || xmlStrEqual(attr->name, name)) {
                return attr;
            }
        }
        attr = attr->next;
    }
    return NULL;
}

native_pointer_result elephc_dom_native_simplexml_handler_view_attribute(
        xmlNodePtr node, int iter_type,
        const uint8_t *iter_name, size_t iter_name_len,
        const uint8_t *namespace_or_prefix, size_t namespace_or_prefix_len,
        int is_prefix, const uint8_t *attribute_name, size_t attribute_name_len) {
    native_pointer_result result = {NULL, 0, 0};
    xmlChar *name = handler_dup_bytes(iter_name, iter_name_len);
    xmlChar *namespace_name = handler_dup_bytes(namespace_or_prefix, namespace_or_prefix_len);
    xmlChar *attribute = handler_dup_bytes(attribute_name, attribute_name_len);
    if (iter_type == 3) {
        xmlNodePtr current = handler_reset_iterator(
            node, iter_type, name, namespace_name, is_prefix
        );
        while (current) {
            if (attribute != NULL && xmlStrEqual(current->name, attribute)) {
                result.pointer = current;
                break;
            }
            current = handler_iterator_fetch(
                current->next, iter_type, name, namespace_name, is_prefix
            );
        }
    } else if (iter_type != 2) {
        xmlNodePtr base = handler_view_first(
            node, iter_type, name, namespace_name, is_prefix
        );
        result.pointer = (void *) handler_first_matching_attr(
            base, attribute, namespace_name, is_prefix
        );
    }
    if (name) xmlFree(name);
    if (namespace_name) xmlFree(namespace_name);
    if (attribute) xmlFree(attribute);
    return result;
}

static void handler_unlink(xmlNodePtr target) {
    xmlUnlinkNode(target);
}

void elephc_dom_native_simplexml_handler_unlink_node(xmlNodePtr node) {
    if (node) {
        handler_unlink(node);
    }
}

static void handler_set_text(xmlNodePtr target, const uint8_t *value, size_t value_len) {
    xmlChar *terminated = handler_dup_bytes(value, value_len);
    if (value != NULL && terminated == NULL) {
        return;
    }
    /* Use xmlNodeSetContent to fully replace the children. */
    xmlNode *child = target->children;
    while (child) {
        xmlNode *next = child->next;
        xmlUnlinkNode(child);
        child = next;
    }
    xmlChar *encoded = xmlEncodeEntitiesReentrant(
        target->doc, terminated ? terminated : (const xmlChar *) ""
    );
    if (encoded) {
        xmlNodeSetContent(target, encoded);
        xmlFree(encoded);
    }
    if (terminated) xmlFree(terminated);
}

native_int_result elephc_dom_native_simplexml_handler_set_node_text(
        xmlNodePtr node, const uint8_t *value, size_t value_len) {
    native_int_result result = {0, 0};
    if (node == NULL) {
        result.value = -1;
        return result;
    }
    handler_set_text(node, value, value_len);
    return result;
}

native_int_result elephc_dom_native_simplexml_handler_write_dimension_attr(xmlDocPtr document, xmlNodePtr node,
                                                         const uint8_t *name, size_t name_len,
                                                         const uint8_t *value, size_t value_len,
                                                         int iter_type) {
    native_int_result result = {0, 0};
    if (node == NULL) {
        result.value = -1;
        return result;
    }
    if (name_len == 0) {
        result.value = -2;
        return result;
    }
    if (iter_type == 3) {
        xmlAttrPtr attr = (xmlAttrPtr) node;
        xmlChar *needle = xmlStrndup(name, (int) name_len);
        while (attr) {
            if (xmlStrEqual(attr->name, needle)) {
                xmlChar *terminated = handler_dup_bytes(value, value_len);
                xmlChar *encoded = xmlEncodeEntitiesReentrant(
                    document, terminated ? terminated : (const xmlChar *) ""
                );
                if (encoded) {
                    xmlNodeSetContent((xmlNodePtr) attr, encoded);
                    xmlFree(encoded);
                }
                if (terminated) xmlFree(terminated);
                break;
            }
            attr = attr->next;
        }
        if (needle) xmlFree(needle);
        return result;
    }
    xmlChar *needle = xmlStrndup(name, (int) name_len);
    xmlAttrPtr attr = handler_first_matching_attr(node, needle, NULL, 0);
    if (attr) {
        xmlChar *terminated = handler_dup_bytes(value, value_len);
        xmlChar *encoded = xmlEncodeEntitiesReentrant(
            document, terminated ? terminated : (const xmlChar *) ""
        );
        if (encoded) {
            xmlNodeSetContent((xmlNodePtr) attr, encoded);
            xmlFree(encoded);
        }
        if (terminated) xmlFree(terminated);
    } else {
        xmlChar *terminated = handler_dup_bytes(value, value_len);
        xmlNewProp(node, needle, terminated ? terminated : (const xmlChar *) "");
        if (terminated) xmlFree(terminated);
    }
    if (needle) xmlFree(needle);
    return result;
}
