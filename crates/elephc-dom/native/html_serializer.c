/*
 * HTML5 serialization derived from PHP 8.5.8 ext/dom/html5_serializer.c.
 * Writes into one libxml2-owned buffer returned through the native bridge ABI.
 */

#include <limits.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <libxml/tree.h>

#include "lexbor/encoding/encoding.h"

typedef struct {
    uint8_t *pointer;
    size_t length;
} elephc_dom_native_buffer;

typedef struct {
    xmlBufferPtr buffer;
    int32_t failed;
} elephc_dom_html_output;

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

static int32_t elephc_dom_html_write(
    elephc_dom_html_output *output,
    const char *bytes,
    size_t length
)
{
    if (output->failed || length > INT_MAX
        || xmlBufferAdd(
            output->buffer,
            (const xmlChar *) bytes,
            (int) length
        ) != 0) {
        output->failed = 1;
        return 0;
    }
    return 1;
}

static int32_t elephc_dom_html_write_string(
    elephc_dom_html_output *output,
    const xmlChar *value
)
{
    return value == NULL
        || elephc_dom_html_write(
            output,
            (const char *) value,
            xmlStrlen(value)
        );
}

static int32_t elephc_dom_html_has_namespace(
    const xmlNode *node,
    const xmlChar *namespace_uri
)
{
    return node != NULL && node->ns != NULL
        && xmlStrEqual(node->ns->href, namespace_uri);
}

static int32_t elephc_dom_html_namespace_is_builtin(const xmlNode *node)
{
    return elephc_dom_html_has_namespace(node, elephc_dom_html_namespace)
        || elephc_dom_html_has_namespace(node, elephc_dom_svg_namespace)
        || elephc_dom_html_has_namespace(node, elephc_dom_mathml_namespace);
}

static int32_t elephc_dom_html_escape(
    elephc_dom_html_output *output,
    const char *content,
    int32_t attribute
)
{
    const char *start = content;

    while (*content != '\0') {
        const char *replacement = NULL;
        size_t replacement_length = 0;
        size_t consumed = 1;

        if (*content == '&') {
            replacement = "&amp;";
            replacement_length = 5;
        } else if ((unsigned char) content[0] == 0xC2
            && (unsigned char) content[1] == 0xA0) {
            replacement = "&nbsp;";
            replacement_length = 6;
            consumed = 2;
        } else if (attribute && *content == '"') {
            replacement = "&quot;";
            replacement_length = 6;
        } else if (!attribute && *content == '<') {
            replacement = "&lt;";
            replacement_length = 4;
        } else if (!attribute && *content == '>') {
            replacement = "&gt;";
            replacement_length = 4;
        }
        if (replacement != NULL) {
            if (!elephc_dom_html_write(
                    output,
                    start,
                    (size_t) (content - start)
                )
                || !elephc_dom_html_write(
                    output,
                    replacement,
                    replacement_length
                )) {
                return 0;
            }
            content += consumed;
            start = content;
        } else {
            content++;
        }
    }
    return elephc_dom_html_write(
        output,
        start,
        (size_t) (content - start)
    );
}

static int32_t elephc_dom_html_void(const xmlNode *node)
{
    static const char *names[] = {
        "area", "base", "br", "col", "embed", "hr", "img", "input",
        "link", "meta", "source", "track", "wbr", "basefont", "bgsound",
        "frame", "keygen", "param",
    };
    size_t index;

    if (!elephc_dom_html_has_namespace(node, elephc_dom_html_namespace)) {
        return 0;
    }
    for (index = 0; index < sizeof(names) / sizeof(*names); index++) {
        if (xmlStrEqual(node->name, (const xmlChar *) names[index])) {
            return 1;
        }
    }
    return 0;
}

static int32_t elephc_dom_html_raw_text_parent(const xmlNode *parent)
{
    static const char *names[] = {
        "style", "script", "xmp", "iframe", "noembed", "noframes",
        "plaintext",
    };
    size_t index;

    if (!elephc_dom_html_has_namespace(
            parent,
            elephc_dom_html_namespace
        )) {
        return 0;
    }
    for (index = 0; index < sizeof(names) / sizeof(*names); index++) {
        if (xmlStrEqual(parent->name, (const xmlChar *) names[index])) {
            return 1;
        }
    }
    return 0;
}

static int32_t elephc_dom_html_write_tag_name(
    elephc_dom_html_output *output,
    const xmlNode *node
)
{
    if (node->ns != NULL && node->ns->prefix != NULL
        && !elephc_dom_html_namespace_is_builtin(node)) {
        if (!elephc_dom_html_write_string(output, node->ns->prefix)
            || !elephc_dom_html_write(output, ":", 1)) {
            return 0;
        }
    }
    return elephc_dom_html_write_string(output, node->name);
}

static int32_t elephc_dom_html_write_attribute_name(
    elephc_dom_html_output *output,
    const xmlAttr *attribute
)
{
    if (attribute->ns == NULL) {
        return elephc_dom_html_write_string(output, attribute->name);
    }
    if (xmlStrEqual(
            attribute->ns->href,
            elephc_dom_xml_namespace
        )) {
        return elephc_dom_html_write(output, "xml:", 4)
            && elephc_dom_html_write_string(output, attribute->name);
    }
    if (xmlStrEqual(
            attribute->ns->href,
            elephc_dom_xmlns_namespace
        )) {
        if (xmlStrEqual(
                attribute->name,
                (const xmlChar *) "xmlns"
            )) {
            return elephc_dom_html_write(output, "xmlns", 5);
        }
        return elephc_dom_html_write(output, "xmlns:", 6)
            && elephc_dom_html_write_string(output, attribute->name);
    }
    if (xmlStrEqual(
            attribute->ns->href,
            elephc_dom_xlink_namespace
        )) {
        return elephc_dom_html_write(output, "xlink:", 6)
            && elephc_dom_html_write_string(output, attribute->name);
    }
    if (attribute->ns->prefix != NULL) {
        return elephc_dom_html_write_string(
                output,
                attribute->ns->prefix
            )
            && elephc_dom_html_write(output, ":", 1)
            && elephc_dom_html_write_string(output, attribute->name);
    }
    return elephc_dom_html_write_string(output, attribute->name);
}

static int32_t elephc_dom_html_write_element_start(
    elephc_dom_html_output *output,
    const xmlNode *node
)
{
    const xmlAttr *attribute;

    if (!elephc_dom_html_write(output, "<", 1)
        || !elephc_dom_html_write_tag_name(output, node)) {
        return 0;
    }
    for (attribute = node->properties;
        attribute != NULL;
        attribute = attribute->next) {
        xmlNodePtr child;

        if (!elephc_dom_html_write(output, " ", 1)
            || !elephc_dom_html_write_attribute_name(output, attribute)
            || !elephc_dom_html_write(output, "=\"", 2)) {
            return 0;
        }
        for (child = attribute->children;
            child != NULL;
            child = child->next) {
            if (child->type == XML_TEXT_NODE && child->content != NULL) {
                if (!elephc_dom_html_escape(
                        output,
                        (const char *) child->content,
                        1
                    )) {
                    return 0;
                }
            } else if (child->type == XML_ENTITY_REF_NODE) {
                if (!elephc_dom_html_write(output, "&", 1)
                    || !elephc_dom_html_escape(
                        output,
                        (const char *) child->name,
                        1
                    )
                    || !elephc_dom_html_write(output, ";", 1)) {
                    return 0;
                }
            }
        }
        if (!elephc_dom_html_write(output, "\"", 1)) {
            return 0;
        }
    }
    return elephc_dom_html_write(output, ">", 1);
}

static int32_t elephc_dom_html_write_element_end(
    elephc_dom_html_output *output,
    const xmlNode *node
)
{
    return elephc_dom_html_void(node)
        || (elephc_dom_html_write(output, "</", 2)
            && elephc_dom_html_write_tag_name(output, node)
            && elephc_dom_html_write(output, ">", 1));
}

static int32_t elephc_dom_html_write_text(
    elephc_dom_html_output *output,
    const xmlNode *node
)
{
    if (node->content == NULL) {
        return 1;
    }
    if (elephc_dom_html_raw_text_parent(node->parent)) {
        return elephc_dom_html_write_string(output, node->content);
    }
    return elephc_dom_html_escape(
        output,
        (const char *) node->content,
        0
    );
}

static const xmlNode *elephc_dom_html_children(const xmlNode *node)
{
    const xmlNode *fragment;

    if (node == NULL || node->type != XML_ELEMENT_NODE
        || !elephc_dom_html_has_namespace(
            node,
            elephc_dom_html_namespace
        )
        || !xmlStrEqual(node->name, (const xmlChar *) "template")
        || node->_private == NULL) {
        return node == NULL ? NULL : node->children;
    }
    fragment = (const xmlNode *) node->_private;
    return fragment->type == XML_DOCUMENT_FRAG_NODE
        && fragment->parent == node
            ? fragment->children
            : node->children;
}

static int32_t elephc_dom_html_write_node(
    elephc_dom_html_output *output,
    const xmlNode *node,
    const xmlNode *bound
)
{
    while (node != NULL) {
        switch (node->type) {
            case XML_DTD_NODE:
                if (!elephc_dom_html_write(output, "<!DOCTYPE ", 10)
                    || !elephc_dom_html_write_string(output, node->name)
                    || !elephc_dom_html_write(output, ">", 1)) {
                    return 0;
                }
                break;
            case XML_CDATA_SECTION_NODE:
            case XML_TEXT_NODE:
                if (!elephc_dom_html_write_text(output, node)) {
                    return 0;
                }
                break;
            case XML_PI_NODE:
                if (!elephc_dom_html_write(output, "<?", 2)
                    || !elephc_dom_html_write_string(output, node->name)
                    || !elephc_dom_html_write(output, " ", 1)
                    || !elephc_dom_html_write_string(output, node->content)
                    || !elephc_dom_html_write(output, ">", 1)) {
                    return 0;
                }
                break;
            case XML_COMMENT_NODE:
                if (!elephc_dom_html_write(output, "<!--", 4)
                    || !elephc_dom_html_write_string(output, node->content)
                    || !elephc_dom_html_write(output, "-->", 3)) {
                    return 0;
                }
                break;
            case XML_ELEMENT_NODE: {
                const xmlNode *children =
                    elephc_dom_html_children(node);

                if (!elephc_dom_html_write_element_start(output, node)) {
                    return 0;
                }
                if (children != NULL && !elephc_dom_html_void(node)) {
                    node = children;
                    continue;
                }
                if (children == NULL
                    && !elephc_dom_html_write_element_end(output, node)) {
                    return 0;
                }
                break;
            }
            case XML_DOCUMENT_FRAG_NODE:
                if (node->children != NULL) {
                    node = node->children;
                    continue;
                }
                break;
            case XML_ENTITY_REF_NODE:
                if (!elephc_dom_html_write(output, "&", 1)
                    || !elephc_dom_html_write_string(output, node->name)
                    || !elephc_dom_html_write(output, ";", 1)) {
                    return 0;
                }
                break;
            default:
                break;
        }

        if (node->next != NULL) {
            node = node->next;
        } else {
            do {
                node = node->parent;
                if (node == bound) {
                    return 1;
                }
                if (node->type == XML_ELEMENT_NODE
                    && !elephc_dom_html_write_element_end(output, node)) {
                    return 0;
                }
            } while (node->next == NULL);
            node = node->next;
        }
    }
    return 1;
}

static elephc_dom_native_buffer elephc_dom_html_transcode(
    const xmlBuffer *utf8,
    const xmlDoc *document
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    const lxb_encoding_data_t *target_encoding;
    const lxb_encoding_data_t *utf8_encoding =
        lxb_encoding_data(LXB_ENCODING_UTF_8);
    lxb_encoding_encode_t encoder;
    lxb_encoding_decode_t decoder;
    lxb_char_t encoded[4096];
    lxb_codepoint_t codepoints[4096];
    const lxb_char_t *source;
    const lxb_char_t *source_end;
    xmlBufferPtr output;
    lxb_status_t decode_status;

    if (utf8 == NULL || document == NULL || document->encoding == NULL) {
        return result;
    }
    if (xmlBufferLength(utf8) == 0) {
        result.pointer = xmlMalloc(1);
        return result;
    }
    target_encoding = lxb_encoding_data_by_name(
        document->encoding,
        xmlStrlen(document->encoding)
    );
    if (target_encoding == NULL) {
        return result;
    }
    output = xmlBufferCreate();
    if (output == NULL) {
        return result;
    }
    (void) lxb_encoding_encode_init(
        &encoder,
        target_encoding,
        encoded,
        sizeof(encoded) / sizeof(*encoded)
    );
    (void) lxb_encoding_decode_init(
        &decoder,
        utf8_encoding,
        codepoints,
        sizeof(codepoints) / sizeof(*codepoints)
    );
    if (target_encoding->encoding == LXB_ENCODING_UTF_8) {
        (void) lxb_encoding_encode_replace_set(
            &encoder,
            LXB_ENCODING_REPLACEMENT_BYTES,
            LXB_ENCODING_REPLACEMENT_SIZE
        );
    } else {
        (void) lxb_encoding_encode_replace_set(
            &encoder,
            (const lxb_char_t *) "?",
            1
        );
    }
    (void) lxb_encoding_decode_replace_set(
        &decoder,
        LXB_ENCODING_REPLACEMENT_BUFFER,
        LXB_ENCODING_REPLACEMENT_BUFFER_LEN
    );
    source = xmlBufferContent(utf8);
    source_end = source + xmlBufferLength(utf8);
    do {
        const lxb_codepoint_t *codepoint;
        const lxb_codepoint_t *codepoint_end;
        lxb_status_t encode_status;

        decode_status =
            lxb_encoding_decode_utf_8(&decoder, &source, source_end);
        codepoint = codepoints;
        codepoint_end =
            codepoint + lxb_encoding_decode_buf_used(&decoder);
        do {
            encode_status = target_encoding->encode(
                &encoder,
                &codepoint,
                codepoint_end
            );
            if (encode_status == LXB_STATUS_ERROR
                || !elephc_dom_html_write(
                    &(elephc_dom_html_output) {output, 0},
                    (const char *) encoded,
                    lxb_encoding_encode_buf_used(&encoder)
                )) {
                xmlBufferFree(output);
                return result;
            }
            lxb_encoding_encode_buf_used_set(&encoder, 0);
        } while (encode_status == LXB_STATUS_SMALL_BUFFER);
        lxb_encoding_decode_buf_used_set(&decoder, 0);
    } while (decode_status == LXB_STATUS_SMALL_BUFFER);

    (void) lxb_encoding_decode_finish(&decoder);
    if (lxb_encoding_decode_buf_used(&decoder) != 0) {
        const lxb_codepoint_t *codepoint = codepoints;
        const lxb_codepoint_t *codepoint_end =
            codepoint + lxb_encoding_decode_buf_used(&decoder);
        if (target_encoding->encode(
                &encoder,
                &codepoint,
                codepoint_end
            ) == LXB_STATUS_ERROR
            || xmlBufferAdd(
                output,
                encoded,
                (int) lxb_encoding_encode_buf_used(&encoder)
            ) != 0) {
            xmlBufferFree(output);
            return result;
        }
        lxb_encoding_encode_buf_used_set(&encoder, 0);
    }
    (void) lxb_encoding_encode_finish(&encoder);
    if (lxb_encoding_encode_buf_used(&encoder) != 0
        && xmlBufferAdd(
            output,
            encoded,
            (int) lxb_encoding_encode_buf_used(&encoder)
        ) != 0) {
        xmlBufferFree(output);
        return result;
    }
    result.length = xmlBufferLength(output);
    result.pointer = xmlMalloc(result.length == 0 ? 1 : result.length);
    if (result.pointer != NULL && result.length != 0) {
        memcpy(
            result.pointer,
            xmlBufferContent(output),
            result.length
        );
    }
    xmlBufferFree(output);
    return result;
}

elephc_dom_native_buffer elephc_dom_native_document_serialize_html5(
    void *document,
    void *node
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    elephc_dom_html_output output = {NULL, 0};
    xmlNodePtr target = node == NULL
        ? (xmlNodePtr) document
        : (xmlNodePtr) node;

    if (document == NULL || target == NULL) {
        return result;
    }
    output.buffer = xmlBufferCreate();
    if (output.buffer == NULL) {
        return result;
    }
    if (target->type == XML_DOCUMENT_NODE
        || target->type == XML_HTML_DOCUMENT_NODE
        || target->type == XML_DOCUMENT_FRAG_NODE) {
        if (target->children != NULL
            && !elephc_dom_html_write_node(
                &output,
                target->children,
                target
            )) {
            output.failed = 1;
        }
    } else {
        xmlNodePtr next = target->next;

        target->next = NULL;
        if (!elephc_dom_html_write_node(&output, target, target->parent)) {
            output.failed = 1;
        }
        target->next = next;
    }
    if (!output.failed) {
        result = elephc_dom_html_transcode(
            output.buffer,
            (const xmlDoc *) document
        );
    }
    xmlBufferFree(output.buffer);
    return result;
}

elephc_dom_native_buffer elephc_dom_native_element_serialize_html5(
    void *element,
    int32_t inner
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    elephc_dom_html_output output = {NULL, 0};
    xmlNodePtr target = (xmlNodePtr) element;

    if (target == NULL || target->type != XML_ELEMENT_NODE) {
        return result;
    }
    output.buffer = xmlBufferCreate();
    if (output.buffer == NULL) {
        return result;
    }
    if (inner != 0) {
        const xmlNode *children = elephc_dom_html_children(target);

        if (children != NULL
            && !elephc_dom_html_write_node(
                &output,
                children,
                children->parent
            )) {
            output.failed = 1;
        }
    } else {
        xmlNodePtr next = target->next;

        target->next = NULL;
        if (!elephc_dom_html_write_node(
                &output,
                target,
                target->parent
            )) {
            output.failed = 1;
        }
        target->next = next;
    }
    if (!output.failed) {
        result.length = xmlBufferLength(output.buffer);
        result.pointer = xmlMalloc(
            result.length == 0 ? 1 : result.length
        );
        if (result.pointer != NULL && result.length != 0) {
            memcpy(
                result.pointer,
                xmlBufferContent(output.buffer),
                result.length
            );
        }
    }
    xmlBufferFree(output.buffer);
    return result;
}
