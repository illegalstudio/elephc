/* Purpose: import the built-in HTML DOM without XML parsing or namespace rewriting.
 * Called from: the SimpleXML bridge's internal tree-builder ABI.
 * Key details: document-owned nodes keep literal HTML names and decoded values.
 */
#include <libxml/HTMLtree.h>
#include <libxml/tree.h>

/* HTML document identity makes SimpleXML serialize its root as a node, as PHP does. */
void *elephc_libxml2_v1_sx_html_document(void) {
    return htmlNewDocNoDtD(NULL, NULL);
}

/* Append without coalescing adjacent text nodes, preserving DOM binding correspondence. */
void *elephc_libxml2_v1_sx_html_append(void *rawdoc, void *rawparent, int kind,
        const char *name, const char *value) {
    xmlDocPtr doc = rawdoc;
    xmlNodePtr parent = rawparent ? rawparent : (xmlNodePtr)doc;
    if (!doc || !parent || !name || !value) return NULL;
    if (kind == 2) {
        if (parent->type != XML_ELEMENT_NODE) return NULL;
        return xmlNewProp(parent, (const xmlChar *)name, (const xmlChar *)value);
    }
    xmlNodePtr node = NULL;
    switch (kind) {
        case 1: node = xmlNewDocNode(doc, NULL, (const xmlChar *)name, NULL); break;
        case 3: node = xmlNewDocText(doc, (const xmlChar *)value); break;
        case 4: node = xmlNewCDataBlock(doc, (const xmlChar *)value, xmlStrlen((const xmlChar *)value)); break;
        case 5: node = xmlNewDocComment(doc, (const xmlChar *)value); break;
        case 6: node = xmlNewDocPI(doc, (const xmlChar *)name, (const xmlChar *)value); break;
        default: return NULL;
    }
    if (!node) return NULL;
    xmlNodePtr added = parent->last ? xmlAddNextSibling(parent->last, node) : xmlAddChild(parent, node);
    if (!added) xmlFreeNode(node);
    return added;
}
