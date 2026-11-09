/* Legacy DOM support for the elephc_xml bridge: serialization exactly as php-src's default
 * libxml document handlers (ext/libxml/libxml.c) and the struct-field reads the Rust DOM
 * engine needs. Rust calls libxml2's public tree API directly for everything else. */
#include <libxml/HTMLparser.h>
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

/* A DTD's external identifiers: 0 ExternalID (publicId), 1 SystemID (systemId). */
const char *elephc_libxml2_v1_dom_dtd_string(void *rawdtd, int which) {
    xmlDtdPtr dtd = rawdtd;
    switch (which) {
        case 0: return (const char *)dtd->ExternalID;
        case 1: return (const char *)dtd->SystemID;
        default: return NULL;
    }
}

/* DOMDocumentType::$internalSubset as php-src's dom_documenttype_internal_subset_read: the
 * document's internal subset declarations, each dumped as xmlNodeDumpOutput writes it, or NULL
 * when there is no internal subset. The caller frees the result. */
char *elephc_libxml2_v1_dom_internal_subset(void *rawdtd) {
    xmlDtdPtr dtd = rawdtd;
    xmlDtdPtr subset = dtd->doc ? xmlGetIntSubset(dtd->doc) : NULL;
    if (subset == NULL || subset->children == NULL) return NULL;
    xmlBufferPtr buffer = xmlBufferCreate();
    if (buffer == NULL) return NULL;
    xmlOutputBufferPtr output = xmlOutputBufferCreateBuffer(buffer, NULL);
    if (output == NULL) { xmlBufferFree(buffer); return NULL; }
    for (xmlNodePtr cur = subset->children; cur != NULL; cur = cur->next) {
        xmlNodeDumpOutput(output, NULL, cur, 0, 0, NULL);
    }
    xmlOutputBufferFlush(output);
    char *result = (char *)xmlStrdup(xmlBufferContent(buffer));
    xmlOutputBufferClose(output);
    xmlBufferFree(buffer);
    return result;
}

/* The document's xmlStandalone flag (1 standalone="yes", 0 "no", -1 absent, -2 implicit). */
int elephc_libxml2_v1_dom_doc_standalone(void *rawdoc) { return ((xmlDocPtr)rawdoc)->standalone; }

/* DOMDocument::loadHTML() / loadHTMLFile() as php-src's dom_load_html: an HTML parser
 * context whose fields php_libxml_sanitize_parse_ctxt_options resets, the caller's options
 * applied only when non-zero, and warnings captured in the DOM loader's diagnostic style.
 * filename != NULL parses that file, otherwise `len` bytes of `data`. */
void *elephc_libxml2_v1_dom_parse_html(const char *data, int len, const char *filename, int options,
        char **diagnostics) {
    *diagnostics = NULL;
    htmlParserCtxtPtr context = filename ? htmlCreateFileParserCtxt(filename, NULL)
        : htmlCreateMemoryParserCtxt(data, len);
    if (!context) return NULL;
    struct sx_diagnostics capture = { xmlBufferCreate(), options, 2, context };
    xmlCtxtSetErrorHandler(context, sx_diagnostic_error, &capture);
    sx_install_global_recorder();
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
    context->loadsubset = 0;
    context->validate = 0;
    context->pedantic = 0;
    context->replaceEntities = 0;
    context->linenumbers = 0;
    context->keepBlanks = 1;
    context->options = 0;
#pragma GCC diagnostic pop
    if (options) htmlCtxtUseOptions(context, options);
    htmlParseDocument(context);
    xmlDocPtr doc = context->myDoc;
    sx_diagnostic_finish(&capture, diagnostics);
    htmlFreeParserCtxt(context);
    return doc;
}

/* DOMDocument::saveHTML(): the whole document (node == NULL, htmlDocDumpMemoryFormat) or one
 * node, a fragment's children one by one (htmlNodeDumpFormatOutput without an encoding).
 * NULL when nothing was produced, as php returns false then. */
char *elephc_libxml2_v1_dom_save_html(void *rawdoc, void *rawnode, int format, int *length) {
    xmlDocPtr doc = rawdoc;
    *length = 0;
    if (!rawnode) {
        xmlChar *memory = NULL;
        int size = 0;
        htmlDocDumpMemoryFormat(doc, &memory, &size, format);
        if (!memory || !size) { if (memory) xmlFree(memory); return NULL; }
        *length = size;
        return (char *)memory;
    }
    xmlBufferPtr buffer = xmlBufferCreate();
    if (!buffer) return NULL;
    xmlOutputBufferPtr output = xmlOutputBufferCreateBuffer(buffer, NULL);
    if (!output) { xmlBufferFree(buffer); return NULL; }
    xmlNodePtr node = rawnode;
    if (node->type == XML_DOCUMENT_FRAG_NODE) {
        for (node = node->children; node; node = node->next) {
            htmlNodeDumpFormatOutput(output, doc, node, NULL, format);
            if (output->error) break;
        }
    } else {
        htmlNodeDumpFormatOutput(output, doc, node, NULL, format);
    }
    int failed = output->error;
    if (!failed) xmlOutputBufferFlush(output);
    xmlOutputBufferClose(output);
    if (failed) { xmlBufferFree(buffer); return NULL; }
    return dom_take_buffer(buffer, length);
}

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
