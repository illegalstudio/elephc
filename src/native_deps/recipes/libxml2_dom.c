/* Legacy DOM support for the elephc_xml bridge: serialization exactly as php-src's default
 * libxml document handlers (ext/libxml/libxml.c) and the struct-field reads the Rust DOM
 * engine needs. Rust calls libxml2's public tree API directly for everything else. */
#include <libxml/globals.h>
#include <libxml/xmlsave.h>

/* Appends libxml output chunks to an xmlBuffer, as php_libxml_write_smart_str does. */
static int dom_write_buffer(void *context, const char *buffer, int length) {
    if (xmlBufferAdd((xmlBufferPtr)context, (const xmlChar *)buffer, length) != 0) return -1;
    return length;
}

/* Moves a buffer's bytes into an xmlStrndup copy the caller frees, reporting their length. */
static char *dom_take_buffer(xmlBufferPtr buffer, int *length) {
    *length = xmlBufferLength(buffer);
    char *result = (char *)xmlStrndup(xmlBufferContent(buffer), *length);
    xmlBufferFree(buffer);
    return result;
}

/* DOMDocument::saveXML(): the whole document (node == NULL, php's dump_doc_to_str with
 * XML_SAVE_AS_XML, XML_SAVE_NO_DECL when requested and XML_SAVE_FORMAT under formatOutput)
 * or one node (php's dump_node_to_str: xmlNodeDumpOutput without an output encoding).
 * no_empty_tags mirrors LIBXML_NOEMPTYTAG through the xmlSaveNoEmptyTags global. */
char *elephc_libxml2_v1_dom_save(void *rawdoc, void *rawnode, int format, int no_decl,
        int no_empty_tags, int *length) {
    xmlDocPtr doc = rawdoc;
    *length = 0;
    xmlBufferPtr buffer = xmlBufferCreate();
    if (!buffer) return NULL;
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
    int saved_no_empty = xmlSaveNoEmptyTags;
    xmlSaveNoEmptyTags = no_empty_tags ? 1 : 0;
#pragma GCC diagnostic pop
    int status = 0;
    if (!rawnode) {
        int options = XML_SAVE_AS_XML | (no_decl ? XML_SAVE_NO_DECL : 0) | (format ? XML_SAVE_FORMAT : 0);
        xmlSaveCtxtPtr save = xmlSaveToIO(dom_write_buffer, NULL, buffer, (const char *)doc->encoding, options);
        if (!save) status = -1;
        else {
            if (xmlSaveDoc(save, doc) < 0) status = -1;
            if (xmlSaveClose(save) < 0) status = -1;
        }
    } else {
        xmlOutputBufferPtr output = xmlOutputBufferCreateIO(dom_write_buffer, NULL, buffer, NULL);
        if (!output) status = -1;
        else {
            xmlNodeDumpOutput(output, doc, rawnode, 0, format, (const char *)doc->encoding);
            if (xmlOutputBufferFlush(output) < 0) status = -1;
            xmlOutputBufferClose(output);
        }
    }
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
    xmlSaveNoEmptyTags = saved_no_empty;
#pragma GCC diagnostic pop
    if (status < 0) { xmlBufferFree(buffer); return NULL; }
    return dom_take_buffer(buffer, length);
}

/* The document a node belongs to (xmlNode.doc). */
void *elephc_libxml2_v1_dom_node_document(void *node) { return ((xmlNodePtr)node)->doc; }

/* A document property php exposes: 0 version, 1 encoding, 2 URL (documentURI). NULL when unset. */
const char *elephc_libxml2_v1_dom_doc_string(void *rawdoc, int which) {
    xmlDocPtr doc = rawdoc;
    switch (which) {
        case 0: return (const char *)doc->version;
        case 1: return (const char *)doc->encoding;
        case 2: return (const char *)doc->URL;
        default: return NULL;
    }
}

/* The document's xmlStandalone flag (1 standalone="yes", 0 "no", -1 absent, -2 implicit). */
int elephc_libxml2_v1_dom_doc_standalone(void *rawdoc) { return ((xmlDocPtr)rawdoc)->standalone; }

/* The node's last child (xmlNode.last). */
void *elephc_libxml2_v1_dom_last(void *node) { return ((xmlNodePtr)node)->last; }

/* Appends an unlinked text node after a parent whose last child is text WITHOUT merging, as
 * php-src's legacy appendChild()/insertBefore(null) do: xmlAddChild would merge the two and
 * free the inserted node, which PHP still references. */
void elephc_libxml2_v1_dom_link_text_last(void *rawparent, void *rawchild) {
    xmlNodePtr parent = rawparent, child = rawchild;
    child->parent = parent;
    if (parent->children == NULL) {
        parent->children = child;
        parent->last = child;
    } else {
        xmlNodePtr last = parent->last;
        last->next = child;
        child->prev = last;
        parent->last = child;
    }
}

/* Inserts an unlinked text node before ref WITHOUT merging, as php-src's legacy insertBefore()
 * does when ref or its previous sibling is text. */
void elephc_libxml2_v1_dom_link_text_before(void *rawref, void *rawchild) {
    xmlNodePtr ref = rawref, child = rawchild;
    child->parent = ref->parent;
    child->next = ref;
    child->prev = ref->prev;
    ref->prev = child;
    if (child->prev != NULL) child->prev->next = child;
    if (child->parent != NULL && child->parent->children == ref) child->parent->children = child;
}

/* The value of a text-like node or an attribute's first text child, borrowed. */
const char *elephc_libxml2_v1_dom_node_content(void *node) {
    return (const char *)((xmlNodePtr)node)->content;
}
