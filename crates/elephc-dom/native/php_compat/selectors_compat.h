/*
 * Minimal Zend/PHP compatibility surface for php-src's libxml2 selector
 * adapter. The adapter itself remains compiled byte-for-byte from the pinned
 * PHP 8.5.8 archive; only its five PHP runtime dependencies are supplied here.
 */

#ifndef ELEPHC_DOM_SELECTORS_COMPAT_H
#define ELEPHC_DOM_SELECTORS_COMPAT_H

#include <assert.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#include <libxml/tree.h>
#include <libxml/xmlstring.h>

#define NAMESPACE_COMPAT_H
#define DOM_EXCEPTION_H
#define PHP_DOM_H

#define NOT_SUPPORTED_ERR 9
#define SYNTAX_ERR 12

#define ZEND_ASSERT(condition) assert(condition)
#define ZEND_STRL(value) value, sizeof(value) - 1
#define zend_always_inline inline __attribute__((always_inline))
#define EMPTY_SWITCH_DEFAULT_CASE() default: abort()

static inline int zend_tolower_ascii(int value)
{
    return value >= 'A' && value <= 'Z' ? value + ('a' - 'A') : value;
}

int32_t elephc_dom_selector_has_exception(void);
void elephc_dom_selector_throw_error(
    int32_t code,
    const char *message,
    int32_t strict
);

#define EG(member) elephc_dom_selector_has_exception()
#define php_dom_throw_error_with_message(code, message, strict) \
    elephc_dom_selector_throw_error(code, message, strict)

static inline bool php_dom_ns_is_fast(
    const xmlNode *node,
    const void *token
)
{
    static const xmlChar html_namespace[] =
        "http://www.w3.org/1999/xhtml";

    (void) token;
    return node != NULL
        && node->ns != NULL
        && xmlStrEqual(node->ns->href, html_namespace);
}

#define php_dom_ns_is_html_magic_token ((const void *) 1)

static inline bool php_dom_ns_is_html_and_document_is_html(
    const xmlNode *node
)
{
    return node != NULL
        && node->doc != NULL
        && node->doc->type == XML_HTML_DOCUMENT_NODE
        && php_dom_ns_is_fast(
            node,
            php_dom_ns_is_html_magic_token
        );
}

static inline xmlChar *php_libxml_attr_value(
    const xmlAttr *attribute,
    bool *should_free
)
{
    *should_free = false;
    if (attribute->children == NULL
        || (attribute->children->type == XML_TEXT_NODE
            && attribute->children->next == NULL
            && attribute->children->content == NULL)) {
        return (xmlChar *) "";
    }
    if (attribute->children->type == XML_TEXT_NODE
        && attribute->children->next == NULL) {
        return attribute->children->content;
    }
    xmlChar *value = xmlNodeGetContent((const xmlNode *) attribute);
    if (value == NULL) {
        return (xmlChar *) "";
    }
    *should_free = true;
    return value;
}

static inline bool dom_compare_value(
    const xmlAttr *attribute,
    const xmlChar *expected
)
{
    bool should_free;
    xmlChar *value =
        php_libxml_attr_value(attribute, &should_free);
    bool result = xmlStrEqual(value, expected);

    if (should_free) {
        xmlFree(value);
    }
    return result;
}

#endif
