/*
 * Modern Document metadata adapters derived from PHP 8.5.8 ext/dom.
 * Keeps HTML head/body/title and WHATWG encoding behavior out of engine.c.
 */

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include <libxml/tree.h>

#include "lexbor/encoding/encoding.h"

typedef struct {
    uint8_t *pointer;
    size_t length;
} elephc_dom_native_buffer;

static const xmlChar elephc_dom_html_namespace[] =
    "http://www.w3.org/1999/xhtml";
static const xmlChar elephc_dom_svg_namespace[] =
    "http://www.w3.org/2000/svg";

static int32_t elephc_dom_metadata_has_namespace(
    const xmlNode *node,
    const xmlChar *namespace_uri
)
{
    return node != NULL
        && node->ns != NULL
        && xmlStrEqual(node->ns->href, namespace_uri);
}

static int32_t elephc_dom_metadata_accept_body(const xmlNode *node)
{
    return elephc_dom_metadata_has_namespace(
            node,
            elephc_dom_html_namespace
        )
        && (xmlStrEqual(node->name, (const xmlChar *) "body")
            || xmlStrEqual(node->name, (const xmlChar *) "frameset"));
}

static xmlNodePtr elephc_dom_metadata_html_child(
    const xmlDoc *document,
    int32_t body
)
{
    xmlNodePtr root = xmlDocGetRootElement(document);
    xmlNodePtr child;

    if (!elephc_dom_metadata_has_namespace(
            root,
            elephc_dom_html_namespace
        )
        || !xmlStrEqual(root->name, (const xmlChar *) "html")) {
        return NULL;
    }
    child = root->children;
    while (child != NULL) {
        if (child->type == XML_ELEMENT_NODE
            && (body
                ? elephc_dom_metadata_accept_body(child)
                : elephc_dom_metadata_has_namespace(
                        child,
                        elephc_dom_html_namespace
                    )
                    && xmlStrEqual(
                        child->name,
                        (const xmlChar *) "head"
                    ))) {
            return child;
        }
        child = child->next;
    }
    return NULL;
}

static xmlNodePtr elephc_dom_metadata_next(
    xmlNodePtr node,
    const xmlNode *root
)
{
    if (node->children != NULL) {
        return node->children;
    }
    while (node != root && node->next == NULL) {
        node = node->parent;
    }
    return node == root ? NULL : node->next;
}

static xmlNodePtr elephc_dom_metadata_html_title(const xmlDoc *document)
{
    xmlNodePtr root = (xmlNodePtr) document;
    xmlNodePtr node = document->children;

    while (node != NULL) {
        if (node->type == XML_ELEMENT_NODE
            && elephc_dom_metadata_has_namespace(
                node,
                elephc_dom_html_namespace
            )
            && xmlStrEqual(node->name, (const xmlChar *) "title")) {
            return node;
        }
        node = elephc_dom_metadata_next(node, root);
    }
    return NULL;
}

static xmlNodePtr elephc_dom_metadata_svg_title(xmlNodePtr root)
{
    xmlNodePtr child = root->children;

    while (child != NULL) {
        if (child->type == XML_ELEMENT_NODE
            && elephc_dom_metadata_has_namespace(
                child,
                elephc_dom_svg_namespace
            )
            && xmlStrEqual(child->name, (const xmlChar *) "title")) {
            return child;
        }
        child = child->next;
    }
    return NULL;
}

static xmlNodePtr elephc_dom_metadata_title(const xmlDoc *document)
{
    xmlNodePtr root = xmlDocGetRootElement(document);

    if (root == NULL) {
        return NULL;
    }
    if (elephc_dom_metadata_has_namespace(root, elephc_dom_svg_namespace)
        && xmlStrEqual(root->name, (const xmlChar *) "svg")) {
        return elephc_dom_metadata_svg_title(root);
    }
    return elephc_dom_metadata_html_title(document);
}

static xmlNsPtr elephc_dom_metadata_unprefixed_namespace(
    xmlDocPtr document,
    const xmlChar *namespace_uri
)
{
    xmlNsPtr namespace;

    for (namespace = document->oldNs; namespace != NULL;
        namespace = namespace->next) {
        if (namespace->prefix == NULL
            && xmlStrEqual(namespace->href, namespace_uri)) {
            return namespace;
        }
    }
    namespace = xmlNewNs(NULL, namespace_uri, NULL);
    if (namespace != NULL) {
        namespace->next = document->oldNs;
        document->oldNs = namespace;
    }
    return namespace;
}

static int32_t elephc_dom_metadata_ascii_whitespace(uint8_t byte)
{
    return byte == ' ' || byte == '\t' || byte == '\n'
        || byte == '\f' || byte == '\r';
}

static elephc_dom_native_buffer
elephc_dom_metadata_collapsed_child_text(const xmlNode *element)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlBufferPtr buffer = xmlBufferCreate();
    xmlNodePtr child;
    const xmlChar *content;
    size_t length;
    size_t index;
    size_t output_length = 0;
    int32_t pending_space = 0;

    if (buffer == NULL) {
        return result;
    }
    if (element != NULL) {
        child = element->children;
        while (child != NULL) {
            if ((child->type == XML_TEXT_NODE
                    || child->type == XML_CDATA_SECTION_NODE)
                && child->content != NULL
                && xmlBufferAdd(
                    buffer,
                    child->content,
                    xmlStrlen(child->content)
                ) != 0) {
                xmlBufferFree(buffer);
                return result;
            }
            child = child->next;
        }
    }
    content = xmlBufferContent(buffer);
    length = xmlBufferLength(buffer);
    result.pointer = xmlMalloc(length == 0 ? 1 : length);
    if (result.pointer == NULL) {
        xmlBufferFree(buffer);
        return result;
    }
    for (index = 0; index < length; index++) {
        if (elephc_dom_metadata_ascii_whitespace(content[index])) {
            if (output_length != 0) {
                pending_space = 1;
            }
        } else {
            if (pending_space) {
                result.pointer[output_length++] = ' ';
                pending_space = 0;
            }
            result.pointer[output_length++] = content[index];
        }
    }
    result.length = output_length;
    xmlBufferFree(buffer);
    return result;
}

static char *elephc_dom_metadata_copy_string(
    const uint8_t *bytes,
    size_t length
)
{
    char *copy;

    if ((bytes == NULL && length != 0) || length == SIZE_MAX) {
        return NULL;
    }
    copy = malloc(length + 1);
    if (copy == NULL) {
        return NULL;
    }
    if (length != 0) {
        memcpy(copy, bytes, length);
    }
    copy[length] = '\0';
    return copy;
}

static int32_t elephc_dom_metadata_replace_title_text(
    xmlDocPtr document,
    xmlNodePtr title,
    const uint8_t *value,
    size_t value_length
)
{
    char *value_string =
        elephc_dom_metadata_copy_string(value, value_length);
    xmlNodePtr text;

    if (value_string == NULL) {
        return 0;
    }
    while (title->children != NULL) {
        xmlNodePtr child = title->children;
        xmlUnlinkNode(child);
        xmlFreeNode(child);
    }
    text = xmlNewDocText(document, (const xmlChar *) value_string);
    free(value_string);
    if (text == NULL || xmlAddChild(title, text) == NULL) {
        if (text != NULL) {
            xmlFreeNode(text);
        }
        return 0;
    }
    return 1;
}

int32_t elephc_dom_native_document_set_modern_encoding(
    void *document,
    const uint8_t *encoding,
    size_t encoding_length
)
{
    xmlDocPtr native_document = (xmlDocPtr) document;
    const lxb_encoding_data_t *encoding_data;
    xmlChar *replacement;

    if (native_document == NULL
        || (encoding == NULL && encoding_length != 0)) {
        return 0;
    }
    encoding_data = lxb_encoding_data_by_name(
        (const lxb_char_t *) encoding,
        encoding_length
    );
    if (encoding_data == NULL) {
        return -1;
    }
    replacement = xmlStrdup((const xmlChar *) encoding_data->name);
    if (replacement == NULL) {
        return 0;
    }
    xmlFree((xmlChar *) native_document->encoding);
    native_document->encoding = replacement;
    return 1;
}

void *elephc_dom_native_document_head(void *document)
{
    return document == NULL
        ? NULL
        : elephc_dom_metadata_html_child((const xmlDoc *) document, 0);
}

void *elephc_dom_native_document_body(void *document)
{
    return document == NULL
        ? NULL
        : elephc_dom_metadata_html_child((const xmlDoc *) document, 1);
}

int32_t elephc_dom_native_node_is_html_body(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node != NULL
        && native_node->type == XML_ELEMENT_NODE
        && elephc_dom_metadata_accept_body(native_node);
}

int32_t elephc_dom_native_node_is_html_element(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node != NULL
        && native_node->type == XML_ELEMENT_NODE
        && elephc_dom_metadata_has_namespace(
            native_node,
            elephc_dom_html_namespace
        );
}

void *elephc_dom_native_document_title_element(void *document)
{
    return document == NULL
        ? NULL
        : elephc_dom_metadata_title((const xmlDoc *) document);
}

elephc_dom_native_buffer elephc_dom_native_document_title(void *document)
{
    if (document == NULL) {
        elephc_dom_native_buffer result = {NULL, 0};
        return result;
    }
    return elephc_dom_metadata_collapsed_child_text(
        elephc_dom_metadata_title((const xmlDoc *) document)
    );
}

int32_t elephc_dom_native_document_set_title(
    void *document,
    const uint8_t *value,
    size_t value_length
)
{
    xmlDocPtr native_document = (xmlDocPtr) document;
    xmlNodePtr root;
    xmlNodePtr title;

    if (native_document == NULL
        || (value == NULL && value_length != 0)) {
        return 0;
    }
    root = xmlDocGetRootElement(native_document);
    if (root == NULL) {
        return 1;
    }
    if (elephc_dom_metadata_has_namespace(root, elephc_dom_svg_namespace)
        && xmlStrEqual(root->name, (const xmlChar *) "svg")) {
        title = elephc_dom_metadata_svg_title(root);
        if (title == NULL) {
            xmlNsPtr namespace = root->ns;

            title = xmlNewDocNode(
                native_document,
                NULL,
                (const xmlChar *) "title",
                NULL
            );
            if (title == NULL) {
                return 0;
            }
            if (namespace->prefix != NULL) {
                namespace = elephc_dom_metadata_unprefixed_namespace(
                    native_document,
                    elephc_dom_svg_namespace
                );
                if (namespace == NULL) {
                    xmlFreeNode(title);
                    return 0;
                }
            }
            title->ns = namespace;
            title->parent = root;
            title->prev = NULL;
            title->next = root->children;
            if (root->children == NULL) {
                root->last = title;
            } else {
                root->children->prev = title;
            }
            root->children = title;
        }
    } else if (elephc_dom_metadata_has_namespace(
            root,
            elephc_dom_html_namespace
        )) {
        xmlNodePtr head = elephc_dom_metadata_html_child(
            native_document,
            0
        );
        title = elephc_dom_metadata_html_title(native_document);
        if (title == NULL && head == NULL) {
            return 1;
        }
        if (title == NULL) {
            title = xmlNewDocNode(
                native_document,
                head->ns,
                (const xmlChar *) "title",
                NULL
            );
            if (title == NULL || xmlAddChild(head, title) == NULL) {
                if (title != NULL) {
                    xmlFreeNode(title);
                }
                return 0;
            }
        }
    } else {
        return 1;
    }
    return elephc_dom_metadata_replace_title_text(
        native_document,
        title,
        value,
        value_length
    );
}
