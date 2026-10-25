/* Purpose: opaque tree and XPath operations for the SimpleXML bridge.
 * Called from: elephc-xml; embedded into the managed libxml2 shim recipe.
 * Key details: only this shim accesses libxml2 structure layouts. Returned
 * xmlChar buffers belong to the caller; borrowed node pointers belong to docs.
 */
#include <libxml/parser.h>
#include <libxml/tree.h>
#include <libxml/xpath.h>
#include <libxml/xpathInternals.h>
#include <libxml/xmlerror.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>

/* Capture libxml diagnostics per operation, without replacing any global handler. */
struct sx_diagnostics { xmlBufferPtr text; int options; int formatted; };
/* Concatenate printf fragments before PHP splits complete warning lines. */
static void sx_diagnostic_fragment(void *raw, const char *format, ...) {
    struct sx_diagnostics *capture = raw;
    if (!capture->text) return;
    va_list args, copy;
    va_start(args, format);
    va_copy(copy, args);
    int length = vsnprintf(NULL, 0, format, copy);
    va_end(copy);
    if (length >= 0) {
        char *text = xmlMalloc((size_t)length + 1);
        if (text) {
            vsnprintf(text, (size_t)length + 1, format, args);
            xmlBufferAdd(capture->text, (const xmlChar *)text, length);
            xmlFree(text);
        }
    }
    va_end(args);
}
/* Parser options filter diagnostics; XPath uses the message without parser context lines. */
static void sx_diagnostic_error(void *raw, const xmlError *error) {
    struct sx_diagnostics *capture = raw;
    if (!capture->text || !error) return;
    if ((error->level == XML_ERR_WARNING && (capture->options & XML_PARSE_NOWARNING)) ||
        (error->level >= XML_ERR_ERROR && (capture->options & XML_PARSE_NOERROR))) return;
    if (capture->formatted) xmlFormatError(error, sx_diagnostic_fragment, capture);
    else if (error->message) xmlBufferCat(capture->text, (const xmlChar *)error->message);
}
/* Transfer an owned buffer to Rust, including an empty buffer when no warning occurred. */
static void sx_diagnostic_finish(struct sx_diagnostics *capture, char **diagnostics) {
    *diagnostics = capture->text ? (char *)xmlStrdup(xmlBufferContent(capture->text)) : NULL;
    if (capture->text) xmlBufferFree(capture->text);
}
/* Parse bytes while preserving the formatter's description, source and caret lines. */
void *elephc_libxml2_v1_sx_parse(const char *data, int len, int options, const char *url,
        char **diagnostics) {
    *diagnostics = NULL;
    xmlParserCtxtPtr context = xmlNewParserCtxt();
    if (!context) return NULL;
    struct sx_diagnostics capture = { xmlBufferCreate(), options, 1 };
    xmlCtxtSetErrorHandler(context, sx_diagnostic_error, &capture);
    xmlDocPtr doc = xmlCtxtReadMemory(context, data, len, url && *url ? url : NULL, NULL, options);
    sx_diagnostic_finish(&capture, diagnostics);
    xmlFreeParserCtxt(context);
    return doc;
}
void elephc_libxml2_v1_sx_doc_free(void *doc) { xmlFreeDoc(doc); }
void *elephc_libxml2_v1_sx_root(void *doc) { return xmlDocGetRootElement(doc); }
void *elephc_libxml2_v1_sx_child(void *node) { return ((xmlNodePtr)node)->children; }
void *elephc_libxml2_v1_sx_next(void *node) { return ((xmlNodePtr)node)->next; }
void *elephc_libxml2_v1_sx_parent(void *node) { return ((xmlNodePtr)node)->parent; }
void *elephc_libxml2_v1_sx_attrs(void *node) { return ((xmlNodePtr)node)->properties; }
int elephc_libxml2_v1_sx_type(void *node) { return ((xmlNodePtr)node)->type; }
const char *elephc_libxml2_v1_sx_name(void *node) { return (const char *)((xmlNodePtr)node)->name; }
const char *elephc_libxml2_v1_sx_ns(void *node, int prefix) {
    xmlNsPtr ns = ((xmlNodePtr)node)->ns;
    return ns ? (const char *)(prefix ? ns->prefix : ns->href) : NULL;
}
/* SimpleXML string conversion includes direct text/CDATA, never descendant elements. */
char *elephc_libxml2_v1_sx_text(void *raw) {
    xmlNodePtr node = raw;
    if (node->type == XML_ATTRIBUTE_NODE || node->type == XML_COMMENT_NODE || node->type == XML_PI_NODE)
        return (char *)xmlNodeGetContent(node);
    xmlBufferPtr out = xmlBufferCreate();
    if (!out) return NULL;
    for (xmlNodePtr c = node->children; c; c = c->next) {
        if (c->type == XML_TEXT_NODE || c->type == XML_CDATA_SECTION_NODE || c->type == XML_ENTITY_REF_NODE) {
            xmlChar *text = xmlNodeGetContent(c);
            if (text) { xmlBufferCat(out, text); xmlFree(text); }
        }
    }
    xmlChar *result = xmlStrdup(xmlBufferContent(out));
    xmlBufferFree(out);
    return (char *)result;
}
void elephc_libxml2_v1_sx_string_free(void *value) { xmlFree(value); }
char *elephc_libxml2_v1_sx_dump(void *rawdoc, void *rawnode) {
    xmlDocPtr doc = rawdoc;
    xmlNodePtr node = rawnode;
    if (node == xmlDocGetRootElement(doc)) {
        xmlChar *out = NULL; int size = 0;
        xmlDocDumpMemory(doc, &out, &size);
        return (char *)out;
    }
    xmlBufferPtr out = xmlBufferCreate();
    if (!out) return NULL;
    xmlNodeDump(out, doc, node, 0, 0);
    xmlChar *result = xmlStrdup(xmlBufferContent(out));
    xmlBufferFree(out);
    return (char *)result;
}
/* Removed nodes are retained by the Rust document owner until the last view dies. */
void elephc_libxml2_v1_sx_unlink(void *node) { xmlUnlinkNode(node); }
void elephc_libxml2_v1_sx_node_free(void *node) {
    if (((xmlNodePtr)node)->type == XML_ATTRIBUTE_NODE) xmlFreeProp(node);
    else xmlFreeNode(node);
}
void elephc_libxml2_v1_sx_content(void *node, const char *value) {
    xmlNodeSetContent(node, (const xmlChar *)value);
}
/* PHP's property/dimension check_empty differs from a SimpleXML boolean cast. */
int elephc_libxml2_v1_sx_empty(void *raw) {
    xmlNodePtr node = raw;
    xmlNodePtr child = node->children;
    if (!child) return 1;
    if (node->type != XML_ATTRIBUTE_NODE && (child->type != XML_TEXT_NODE || child->next)) return 0;
    return !child->content || !child->content[0] || xmlStrEqual(child->content, BAD_CAST "0");
}
/* Text assignment escapes ampersands, whereas addChild follows PHP's entity parsing. */
void elephc_libxml2_v1_sx_assign(void *raw, const char *value) {
    xmlNodePtr node = raw;
    xmlNodePtr text = xmlNewDocText(node->doc, (const xmlChar *)value);
    if (text) xmlAddChild(node, text);
}
void *elephc_libxml2_v1_sx_add(void *raw, const char *qualified, const char *value,
        const char *uri, int has_uri, int attribute, int has_value) {
    xmlNodePtr parent = raw;
    const char *colon = strchr(qualified, ':');
    const xmlChar *name = (const xmlChar *)(colon ? colon + 1 : qualified);
    xmlChar *prefix = colon ? xmlStrndup((const xmlChar *)qualified, (int)(colon - qualified)) : NULL;
    xmlNsPtr ns = NULL;
    if (has_uri && *uri) {
        ns = xmlSearchNsByHref(parent->doc, parent, (const xmlChar *)uri);
    } else if (!has_uri) {
        ns = attribute ? NULL : parent->ns;
    }
    xmlNodePtr result;
    if (attribute) {
        if (has_uri && *uri && !ns) ns = xmlNewNs(parent, (const xmlChar *)uri, prefix);
        result = (xmlNodePtr)xmlNewNsProp(parent, ns, name, (const xmlChar *)value);
    } else {
        result = xmlNewChild(parent, ns, name, has_value ? (const xmlChar *)value : NULL);
        if (result && has_uri && !ns) {
            ns = xmlNewNs(result, (const xmlChar *)uri, prefix);
            xmlSetNs(result, ns);
        }
    }
    xmlFree(prefix);
    return result;
}
void *elephc_libxml2_v1_sx_ns_defs(void *node) { return ((xmlNodePtr)node)->nsDef; }
void *elephc_libxml2_v1_sx_ns_next(void *ns) { return ((xmlNsPtr)ns)->next; }
const char *elephc_libxml2_v1_sx_ns_value(void *ns, int prefix) {
    return (const char *)(prefix ? ((xmlNsPtr)ns)->prefix : ((xmlNsPtr)ns)->href);
}
void *elephc_libxml2_v1_sx_xpath_context(void *doc, void *node) {
    xmlXPathContextPtr ctx = xmlXPathNewContext(doc);
    if (ctx) {
        ctx->node = node;
        for (xmlNodePtr p = node; p && p->type == XML_ELEMENT_NODE; p = p->parent) {
            for (xmlNsPtr ns = p->nsDef; ns; ns = ns->next) {
                if (ns->prefix && !xmlXPathNsLookup(ctx, ns->prefix))
                    xmlXPathRegisterNs(ctx, ns->prefix, ns->href);
            }
        }
    }
    return ctx;
}
int elephc_libxml2_v1_sx_xpath_register(void *ctx, const char *prefix, const char *uri) {
    return xmlXPathRegisterNs(ctx, (const xmlChar *)prefix, (const xmlChar *)uri);
}
void *elephc_libxml2_v1_sx_xpath_eval(void *ctx, const char *expression, char **diagnostics) {
    struct sx_diagnostics capture = { xmlBufferCreate(), 0, 0 };
    xmlXPathSetErrorHandler(ctx, sx_diagnostic_error, &capture);
    xmlXPathObjectPtr result = xmlXPathEvalExpression((const xmlChar *)expression, ctx);
    xmlXPathSetErrorHandler(ctx, NULL, NULL);
    sx_diagnostic_finish(&capture, diagnostics);
    return result;
}
int elephc_libxml2_v1_sx_xpath_count(void *result) {
    xmlXPathObjectPtr obj = result;
    return obj && obj->type == XPATH_NODESET ? (obj->nodesetval ? obj->nodesetval->nodeNr : 0) : -1;
}
int elephc_libxml2_v1_sx_xpath_type(void *result) {
    xmlXPathObjectPtr obj = result;
    return obj ? (int)obj->type : 0;
}
void *elephc_libxml2_v1_sx_xpath_node(void *result, int index) {
    return ((xmlXPathObjectPtr)result)->nodesetval->nodeTab[index];
}
void elephc_libxml2_v1_sx_xpath_free(void *result) { xmlXPathFreeObject(result); }
void elephc_libxml2_v1_sx_xpath_context_free(void *ctx) { xmlXPathFreeContext(ctx); }

/* Clone mappings walk matching document trees and attribute chains in lockstep. */
void *elephc_libxml2_v1_sx_copy(void *doc) { return xmlCopyDoc(doc, 1); }
static xmlNodePtr sx_correspond(xmlNodePtr source, xmlNodePtr destination, xmlNodePtr node) {
    while (source && destination) {
        if (source == node) return destination;
        if (source->type == XML_ELEMENT_NODE) {
            xmlNodePtr mapped = sx_correspond((xmlNodePtr)source->properties,
                (xmlNodePtr)destination->properties, node);
            if (mapped) return mapped;
        }
        xmlNodePtr mapped = sx_correspond(source->children, destination->children, node);
        if (mapped) return mapped;
        source = source->next; destination = destination->next;
    }
    return NULL;
}
void *elephc_libxml2_v1_sx_correspond(void *source, void *destination, void *node) {
    return sx_correspond(((xmlDocPtr)source)->children, ((xmlDocPtr)destination)->children, node);
}

char *elephc_libxml2_v1_sx_full_text(void *node) { return (char *)xmlNodeGetContent(node); }
char *elephc_libxml2_v1_sx_dump_node(void *rawdoc, void *rawnode) {
    xmlBufferPtr out = xmlBufferCreate();
    if (!out) return NULL;
    xmlNodeDump(out, rawdoc, rawnode, 0, 0);
    xmlChar *result = xmlStrdup(xmlBufferContent(out));
    xmlBufferFree(out);
    return (char *)result;
}

/* Length-bearing serialization preserves legacy encodings and embedded NUL bytes. */
char *elephc_libxml2_v1_sx_dump_bytes(void *rawdoc, void *rawnode, int whole, int *length) {
    xmlDocPtr doc = rawdoc;
    xmlNodePtr node = rawnode;
    *length = 0;
    if (whole && node == xmlDocGetRootElement(doc)) {
        xmlChar *out = NULL;
        xmlDocDumpMemory(doc, &out, length);
        return (char *)out;
    }
    xmlBufferPtr out = xmlBufferCreate();
    if (!out) return NULL;
    if (xmlNodeDump(out, doc, node, 0, 0) < 0) { xmlBufferFree(out); return NULL; }
    *length = xmlBufferLength(out);
    xmlChar *result = xmlStrndup(xmlBufferContent(out), *length);
    xmlBufferFree(out);
    return (char *)result;
}
