/* Purpose: opaque tree and XPath operations for the SimpleXML bridge.
 * Called from: elephc-xml; embedded into the managed libxml2 shim recipe.
 * Key details: only this shim accesses libxml2 structure layouts. Returned
 * xmlChar buffers belong to the caller; borrowed node pointers belong to docs.
 */
#include <libxml/parser.h>
#include <libxml/tree.h>
#include <libxml/xpath.h>
#include <libxml/xpathInternals.h>
#include <string.h>

/* Parse bytes without installing process-global error handlers. */
void *elephc_libxml2_v1_sx_parse(const char *data, int len, int options) {
    return xmlReadMemory(data, len, NULL, NULL, options | XML_PARSE_NOERROR | XML_PARSE_NOWARNING);
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
    if (node->type == XML_ATTRIBUTE_NODE) return (char *)xmlNodeGetContent(node);
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
        if (ns && prefix && (!ns->prefix || xmlStrcmp(prefix, ns->prefix))) ns = NULL;
    } else if (!has_uri) {
        ns = parent->ns;
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
void *elephc_libxml2_v1_sx_xpath_eval(void *ctx, const char *expression) {
    return xmlXPathEvalExpression((const xmlChar *)expression, ctx);
}
int elephc_libxml2_v1_sx_xpath_count(void *result) {
    xmlXPathObjectPtr obj = result;
    return obj && obj->type == XPATH_NODESET ? (obj->nodesetval ? obj->nodesetval->nodeNr : 0) : -1;
}
void *elephc_libxml2_v1_sx_xpath_node(void *result, int index) {
    return ((xmlXPathObjectPtr)result)->nodesetval->nodeTab[index];
}
void elephc_libxml2_v1_sx_xpath_free(void *result) { xmlXPathFreeObject(result); }
void elephc_libxml2_v1_sx_xpath_context_free(void *ctx) { xmlXPathFreeContext(ctx); }
