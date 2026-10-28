/* Purpose: legacy DOM qualified attribute lookup over SimpleXML-owned XML trees.
 * Called from: the SimpleXML bridge's DOM adapters.
 * Key details: only regular attributes escape as write targets; Rust retires their
 * old content before replacement so existing node views remain valid.
 */
#include <libxml/tree.h>
#include <string.h>

/* PHP resolves prefixes to namespace URIs and treats local declarations specially. */
static xmlNodePtr sx_dom_attribute_lookup(xmlNodePtr node, const xmlChar *name) {
    if (!node || node->type != XML_ELEMENT_NODE) return NULL;
    int length;
    const xmlChar *local = xmlSplitQName3(name, &length);
    if (local) {
        if (length == 5 && !memcmp(name, "xmlns:", 6)) {
            for (xmlNsPtr ns = node->nsDef; ns; ns = ns->next) {
                if (xmlStrEqual(ns->prefix, local)) return (xmlNodePtr)ns;
            }
            return NULL;
        }
        xmlChar *prefix = xmlStrndup(name, length);
        if (!prefix) return NULL;
        xmlNsPtr ns = xmlSearchNs(node->doc, node, prefix);
        xmlFree(prefix);
        if (ns) return (xmlNodePtr)xmlHasNsProp(node, local, ns->href);
    } else if (xmlStrEqual(name, BAD_CAST "xmlns")) {
        for (xmlNsPtr ns = node->nsDef; ns; ns = ns->next) {
            if (!ns->prefix) return (xmlNodePtr)ns;
        }
        return NULL;
    }
    return (xmlNodePtr)xmlHasNsProp(node, name, NULL);
}

/* Copy namespace declarations and DTD defaults without exposing their different layouts. */
char *elephc_libxml2_v1_sx_dom_attribute_text(void *raw, const char *name) {
    xmlNodePtr attr = sx_dom_attribute_lookup(raw, (const xmlChar *)name);
    if (!attr) return (char *)xmlStrdup(BAD_CAST "");
    if (attr->type == XML_ATTRIBUTE_NODE) return (char *)xmlNodeListGetString(attr->doc, attr->children, 1);
    if (attr->type == XML_NAMESPACE_DECL) return (char *)xmlStrdup(((xmlNsPtr)attr)->href);
    if (attr->type == XML_ATTRIBUTE_DECL) return (char *)xmlStrdup(((xmlAttributePtr)attr)->defaultValue);
    return NULL;
}

/* Resolve or create a write target, leaving old attribute children for Rust to retire. */
void *elephc_libxml2_v1_sx_dom_attribute_target(void *raw, const char *name,
        const char *value, int *status) {
    xmlNodePtr node = raw;
    *status = 0;
    if (!node || node->type != XML_ELEMENT_NODE || !*name || xmlValidateName(BAD_CAST name, 0)) return NULL;
    xmlNodePtr attr = sx_dom_attribute_lookup(node, (const xmlChar *)name);
    if (attr && attr->type == XML_NAMESPACE_DECL) return NULL;
    if (xmlStrEqual(BAD_CAST name, BAD_CAST "xmlns")) {
        *status = xmlNewNs(node, BAD_CAST value, NULL) != NULL;
        return NULL;
    }
    if (!attr || attr->type != XML_ATTRIBUTE_NODE) {
        attr = (xmlNodePtr)xmlSetProp(node, BAD_CAST name, NULL);
    } else {
        int length;
        const xmlChar *local = xmlSplitQName3(BAD_CAST name, &length);
        if (local) {
            xmlChar *prefix = xmlStrndup(BAD_CAST name, length);
            if (!prefix) return NULL;
            xmlNsPtr ns = xmlSearchNs(node->doc, node, prefix);
            xmlFree(prefix);
            if (ns) attr->ns = ns;
        }
    }
    *status = attr != NULL;
    return attr;
}
