/*
 * Elephc-owned libxml2 shim, compiled by the libxml2 catalog recipe against the exact
 * libxml2 headers it just built and archived as lib/libelephc_libxml2_shim.a.
 *
 * WHY IT EXISTS. php-src's ext/xml/compat.c reaches into xmlParserCtxt (input cursor,
 * line/column, errNo, inSubset, instate, myDoc) and xmlEntity (etype, content, ids) to
 * reproduce expat's behavior on top of libxml2. Those struct layouts are private in
 * spirit and version-specific in practice; the Rust bridge (crates/elephc-xml) never
 * declares them. Every field access happens here, in C, against the pinned headers,
 * behind versioned `elephc_libxml2_v1_*` entry points with opaque pointers and plain
 * integers/strings. The bridge declares only these functions plus libxml2's public,
 * pointer-opaque APIs (xmlParseChunk, xmlStopParser, xmlTextWriter*).
 *
 * Nothing here allocates PHP values or touches Elephc's runtime; the shim is a pure
 * function of libxml2.
 */
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* compat.c touches the same deprecated context members; the pinned headers keep them. */
#if defined(__GNUC__) || defined(__clang__)
#pragma GCC diagnostic ignored "-Wdeprecated-declarations"
#endif

#include <libxml/parser.h>
#include <libxml/parserInternals.h>
#include <libxml/entities.h>
#include <libxml/dict.h>
#include <libxml/tree.h>
#include <libxml/xmlerror.h>
#include <libxml/xmlversion.h>

/* The SAX callbacks the bridge implements: the compat.c subset (SAX1 element events for
 * the non-namespace parser, SAX2 element events for the namespace parser, character
 * data, PIs, comments, entity lookup, notation and unparsed-entity declarations). Every
 * callback receives the `user` pointer given to elephc_libxml2_v1_parser_create. */
typedef struct elephc_libxml2_v1_sax {
    void (*start_element)(void *user, const xmlChar *name, const xmlChar **attributes);
    void (*end_element)(void *user, const xmlChar *name);
    void (*start_element_ns)(void *user, const xmlChar *localname, const xmlChar *prefix,
                             const xmlChar *uri, int nb_namespaces, const xmlChar **namespaces,
                             int nb_attributes, int nb_defaulted, const xmlChar **attributes);
    void (*end_element_ns)(void *user, const xmlChar *localname, const xmlChar *prefix,
                           const xmlChar *uri);
    void (*characters)(void *user, const xmlChar *data, int len);
    void (*processing_instruction)(void *user, const xmlChar *target, const xmlChar *data);
    void (*comment)(void *user, const xmlChar *value);
    xmlEntityPtr (*get_entity)(void *user, const xmlChar *name);
    void (*notation_decl)(void *user, const xmlChar *name, const xmlChar *public_id,
                          const xmlChar *system_id);
    void (*unparsed_entity_decl)(void *user, const xmlChar *name, const xmlChar *public_id,
                                 const xmlChar *system_id, const xmlChar *notation_name);
} elephc_libxml2_v1_sax;

typedef struct elephc_libxml2_v1_parser {
    xmlSAXHandler sax;
    xmlParserCtxtPtr ctxt;
} elephc_libxml2_v1_parser;

/* External resources (DTD subsets, external parsed entities) are never fetched: the
 * compiled runtime has no PHP stream layer to hand libxml2, so the loader answers
 * "no such entity", the outcome php-src produces for an unreachable SYSTEM id. */
static xmlParserErrors
elephc_libxml2_v1_refuse_resource(void *vctxt, const char *url, const char *public_id,
                                  xmlResourceType type, xmlParserInputFlags flags,
                                  xmlParserInput **out)
{
    (void)vctxt;
    (void)url;
    (void)public_id;
    (void)type;
    (void)flags;
    if (out != NULL) {
        *out = NULL;
    }
    return XML_IO_ENOENT;
}

/* libxml2 version this shim was compiled against (LIBXML_VERSION, e.g. 21503). */
int32_t elephc_libxml2_v1_version(void)
{
    return (int32_t)LIBXML_VERSION;
}

/* XML_ParserCreate_MM: a push parser with compat.c's handler set and options. `user` is
 * what every callback receives. Returns NULL when libxml2 cannot allocate the context. */
void *elephc_libxml2_v1_parser_create(const elephc_libxml2_v1_sax *callbacks, void *user,
                                      int32_t use_namespaces)
{
    elephc_libxml2_v1_parser *parser;
    xmlParserCtxtPtr ctxt;

    if (callbacks == NULL) {
        return NULL;
    }
    parser = (elephc_libxml2_v1_parser *)calloc(1, sizeof(*parser));
    if (parser == NULL) {
        return NULL;
    }
    parser->sax.getEntity = callbacks->get_entity;
    parser->sax.notationDecl = callbacks->notation_decl;
    parser->sax.unparsedEntityDecl = callbacks->unparsed_entity_decl;
    parser->sax.startElement = callbacks->start_element;
    parser->sax.endElement = callbacks->end_element;
    parser->sax.characters = callbacks->characters;
    parser->sax.processingInstruction = callbacks->processing_instruction;
    parser->sax.comment = callbacks->comment;
    parser->sax.cdataBlock = callbacks->characters;
    parser->sax.initialized = XML_SAX2_MAGIC;
    parser->sax.startElementNs = callbacks->start_element_ns;
    parser->sax.endElementNs = callbacks->end_element_ns;

    ctxt = xmlCreatePushParserCtxt(&parser->sax, user, NULL, 0, NULL);
    if (ctxt == NULL) {
        free(parser);
        return NULL;
    }
    parser->ctxt = ctxt;

    /* php_libxml_sanitize_parse_ctxt_options() */
    ctxt->loadsubset = 0;
    ctxt->validate = 0;
    ctxt->pedantic = 0;
    ctxt->replaceEntities = 0;
    ctxt->linenumbers = 0;
    ctxt->keepBlanks = 1;
    ctxt->options = 0;
    xmlCtxtUseOptions(ctxt, XML_PARSE_OLDSAX | XML_PARSE_NOENT);
    ctxt->wellFormed = 0;
    if (!use_namespaces) {
        /* compat.c: the SAX2 magic is needed by xmlCreatePushParserCtxt, then reset so
         * the parser dispatches the SAX1 element callbacks. */
        ctxt->sax->initialized = 1;
    }
    xmlCtxtSetResourceLoader(ctxt, elephc_libxml2_v1_refuse_resource, NULL);
    return parser;
}

/* XML_ParserFree. */
void elephc_libxml2_v1_parser_free(void *handle)
{
    elephc_libxml2_v1_parser *parser = (elephc_libxml2_v1_parser *)handle;
    if (parser == NULL) {
        return;
    }
    if (parser->ctxt != NULL) {
        if (parser->ctxt->myDoc != NULL) {
            xmlFreeDoc(parser->ctxt->myDoc);
            parser->ctxt->myDoc = NULL;
        }
        xmlFreeParserCtxt(parser->ctxt);
    }
    free(parser);
}

/* xml_parse_helper's XML_OPTION_PARSE_HUGE application, done before every chunk. */
void elephc_libxml2_v1_set_huge(void *handle, int32_t huge)
{
    elephc_libxml2_v1_parser *parser = (elephc_libxml2_v1_parser *)handle;
    if (huge) {
        parser->ctxt->options |= XML_PARSE_HUGE;
        xmlDictSetLimit(parser->ctxt->dict, 0);
    } else {
        parser->ctxt->options &= ~XML_PARSE_HUGE;
        xmlDictSetLimit(parser->ctxt->dict, XML_MAX_DICTIONARY_LIMIT);
    }
}

/* XML_Parse: 1 when the chunk parsed without an error above warning level, else 0. */
int32_t elephc_libxml2_v1_parse_chunk(void *handle, const char *data, int32_t len,
                                      int32_t terminate)
{
    elephc_libxml2_v1_parser *parser = (elephc_libxml2_v1_parser *)handle;
    int error = xmlParseChunk(parser->ctxt, data, (int)len, terminate ? 1 : 0);
    if (!error) {
        const xmlError *error_data = xmlCtxtGetLastError(parser->ctxt);
        return (error_data == NULL || error_data->level <= XML_ERR_WARNING) ? 1 : 0;
    }
    return 0;
}

/* XML_GetErrorCode. */
int32_t elephc_libxml2_v1_error_code(void *handle)
{
    return (int32_t)((elephc_libxml2_v1_parser *)handle)->ctxt->errNo;
}

/* compat.c's external_entity_ref_handler failure path: stop, then override errNo. */
void elephc_libxml2_v1_stop(void *handle, int32_t error_code)
{
    elephc_libxml2_v1_parser *parser = (elephc_libxml2_v1_parser *)handle;
    xmlStopParser(parser->ctxt);
    parser->ctxt->errNo = (int)error_code;
}

/* ctxt->wellFormed. */
int32_t elephc_libxml2_v1_well_formed(void *handle)
{
    return ((elephc_libxml2_v1_parser *)handle)->ctxt->wellFormed ? 1 : 0;
}

/* XML_GetCurrentLineNumber. */
int32_t elephc_libxml2_v1_line(void *handle)
{
    xmlParserInputPtr input = ((elephc_libxml2_v1_parser *)handle)->ctxt->input;
    return input == NULL ? 0 : (int32_t)input->line;
}

/* XML_GetCurrentColumnNumber. */
int32_t elephc_libxml2_v1_column(void *handle)
{
    xmlParserInputPtr input = ((elephc_libxml2_v1_parser *)handle)->ctxt->input;
    return input == NULL ? 0 : (int32_t)input->col;
}

/* XML_GetCurrentByteIndex. */
int64_t elephc_libxml2_v1_byte_index(void *handle)
{
    xmlParserInputPtr input = ((elephc_libxml2_v1_parser *)handle)->ctxt->input;
    if (input == NULL) {
        return 0;
    }
    return (int64_t)input->consumed + (int64_t)(input->cur - input->base);
}

/* The source text of the tag being reported, as start_element_emit_default reads it:
 * from the `<` preceding the cursor to the cursor itself. Returns the length written to
 * *out_start (0 when no input is active). */
int64_t elephc_libxml2_v1_current_tag(void *handle, const char **out_start)
{
    xmlParserInputPtr input = ((elephc_libxml2_v1_parser *)handle)->ctxt->input;
    const xmlChar *cur;
    const xmlChar *end;

    if (input == NULL || input->cur == NULL || input->base == NULL) {
        *out_start = NULL;
        return 0;
    }
    cur = input->cur;
    end = cur;
    for (const xmlChar *base = input->base; cur > base && *cur != '<'; cur--) {
    }
    *out_start = (const char *)cur;
    return (int64_t)(end - cur + 1);
}

/* ctxt->inSubset (compat.c get_entity). */
int32_t elephc_libxml2_v1_in_subset(void *handle)
{
    return (int32_t)((elephc_libxml2_v1_parser *)handle)->ctxt->inSubset;
}

/* ctxt->instate == XML_PARSER_CONTENT (compat.c get_entity). */
int32_t elephc_libxml2_v1_in_content(void *handle)
{
    return ((elephc_libxml2_v1_parser *)handle)->ctxt->instate == XML_PARSER_CONTENT ? 1 : 0;
}

/* xmlGetPredefinedEntity, then xmlGetDocEntity on the parser's document. */
void *elephc_libxml2_v1_lookup_entity(void *handle, const xmlChar *name)
{
    elephc_libxml2_v1_parser *parser = (elephc_libxml2_v1_parser *)handle;
    xmlEntityPtr entity = xmlGetPredefinedEntity(name);
    if (entity == NULL) {
        entity = xmlGetDocEntity(parser->ctxt->myDoc, name);
    }
    return entity;
}

/* xmlEntity->etype (xmlEntityType numbering: 1 internal general, 2 external general
 * parsed, 3 external general unparsed, 4 internal parameter, 5 external parameter,
 * 6 internal predefined). */
int32_t elephc_libxml2_v1_entity_type(void *entity)
{
    return entity == NULL ? 0 : (int32_t)((xmlEntityPtr)entity)->etype;
}

const xmlChar *elephc_libxml2_v1_entity_name(void *entity)
{
    return entity == NULL ? NULL : ((xmlEntityPtr)entity)->name;
}

const xmlChar *elephc_libxml2_v1_entity_content(void *entity)
{
    return entity == NULL ? NULL : ((xmlEntityPtr)entity)->content;
}

const xmlChar *elephc_libxml2_v1_entity_system_id(void *entity)
{
    return entity == NULL ? NULL : ((xmlEntityPtr)entity)->SystemID;
}

const xmlChar *elephc_libxml2_v1_entity_external_id(void *entity)
{
    return entity == NULL ? NULL : ((xmlEntityPtr)entity)->ExternalID;
}
