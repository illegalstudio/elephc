/*
 * Thin, panic-free probes over the exact native parser engines pinned by the
 * PHP 8.5.8 DOM compliance specification. Full object adapters build on these
 * entry points without exposing either engine's structures through the Rust ABI.
 */

#include <limits.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#include <libxml/HTMLparser.h>
#include <libxml/HTMLtree.h>
#include <libxml/c14n.h>
#include <libxml/chvalid.h>
#include <libxml/parser.h>
#include <libxml/encoding.h>
#include <libxml/tree.h>
#include <libxml/uri.h>
#include <libxml/hash.h>
#include <libxml/valid.h>
#include <libxml/xinclude.h>
#include <libxml/xmlerror.h>
#include <libxml/xpath.h>
#include <libxml/xpathInternals.h>
#include <libxml/relaxng.h>
#include <libxml/xmlsave.h>
#include <libxml/xmlschemas.h>
#include <libxml/xmlversion.h>

#include "lexbor/core/base.h"
#include "lexbor/html/interfaces/document.h"

/*
 * glibc hides PATH_MAX from <limits.h> under strict C11 unless a feature-test
 * macro is enabled. The supported Linux targets expose a 4096-byte PATH_MAX
 * when those declarations are enabled, so retain that bound when it is hidden.
 */
#ifndef PATH_MAX
#define PATH_MAX 4096
#endif

typedef struct {
    uint8_t *pointer;
    size_t length;
} elephc_dom_native_buffer;

typedef struct {
    uint8_t *pointer;
    size_t length;
    int32_t error_code;
    int32_t reserved;
} elephc_dom_native_buffer_result;

typedef struct {
    void *pointer;
    int32_t error_code;
    int32_t reserved;
} elephc_dom_native_pointer_result;

typedef struct {
    int32_t level;
    int32_t domain;
    int32_t code;
    int32_t line;
    int32_t column;
    int32_t reserved;
    uint8_t *message;
    size_t message_length;
    uint8_t *file;
    size_t file_length;
} elephc_dom_native_error;

typedef struct {
    void *document;
    elephc_dom_native_error *errors;
    size_t error_count;
    int32_t allocation_failed;
    int32_t host_status;
} elephc_dom_native_parse_result;

typedef struct {
    elephc_dom_native_error *errors;
    size_t error_count;
    int32_t allocation_failed;
    int32_t valid;
    int32_t status;
    int32_t host_status;
} elephc_dom_native_validation_result;

typedef struct {
    elephc_dom_native_error *errors;
    size_t error_count;
    void **invalidated;
    size_t invalidated_count;
    int32_t allocation_failed;
    int32_t substitutions;
    int32_t host_status;
} elephc_dom_native_xinclude_result;

typedef struct {
    const uint8_t *pointer;
    size_t length;
} elephc_dom_native_bytes;

typedef struct {
    uint8_t *bytes;
    size_t length;
    elephc_dom_native_error *errors;
    size_t error_count;
    int32_t allocation_failed;
    int32_t status;
} elephc_dom_native_c14n_result;

typedef struct {
    void **pointers;
    size_t pointer_count;
    uint8_t *bytes;
    size_t byte_count;
    elephc_dom_native_error *errors;
    size_t error_count;
    uint64_t *callback_leases;
    size_t callback_lease_count;
    double number;
    int32_t allocation_failed;
    int32_t kind;
    int32_t boolean_value;
    int32_t status;
    int32_t host_status;
} elephc_dom_native_xpath_result;

typedef struct {
    uint8_t *bytes;
    size_t length;
    uint64_t resource;
    int32_t kind;
    int32_t reserved;
} elephc_dom_host_loader_result;

typedef struct {
    elephc_dom_native_error *errors;
    size_t count;
    size_t capacity;
    int32_t allocation_failed;
} elephc_dom_error_list;

typedef struct {
    void **pointers;
    size_t count;
    size_t capacity;
    int32_t allocation_failed;
} elephc_dom_pointer_list;

typedef struct {
    void *element;
    const uint8_t *prefix;
    size_t prefix_length;
    const uint8_t *namespace_uri;
    size_t namespace_uri_length;
} elephc_dom_native_namespace_info;

typedef struct {
    elephc_dom_native_namespace_info *items;
    size_t count;
    int32_t allocation_failed;
    int32_t reserved;
} elephc_dom_native_namespace_info_result;

typedef struct {
    elephc_dom_error_list *errors;
    char *buffer;
    size_t length;
    size_t capacity;
    xmlGenericErrorFunc previous_handler;
    void *previous_context;
    int32_t installed;
} elephc_dom_generic_error_context;

typedef struct {
    int load_subset;
    int validate;
    int pedantic;
    int substitute;
    int line_numbers;
    int keep_blanks;
} elephc_dom_libxml_globals;

typedef struct elephc_dom_validation_ns_link {
    xmlNodePtr node;
    xmlNsPtr original_namespace;
    size_t added_count;
    int32_t restore_namespace;
    struct elephc_dom_validation_ns_link *next;
} elephc_dom_validation_ns_link;

typedef struct {
    elephc_dom_validation_ns_link *links;
    int32_t active;
    int32_t allocation_failed;
} elephc_dom_validation_ns_guard;

typedef struct {
    uint64_t context_id;
    xmlParserCtxtPtr parser;
    int32_t host_status;
} elephc_dom_resource_loader_context;

typedef struct {
    uint64_t lease_id;
    elephc_dom_resource_loader_context *loader;
} elephc_dom_stream_io_context;

typedef struct {
    int32_t kind;
    int32_t boolean_value;
    double number;
    const uint8_t *bytes;
    size_t length;
    void **nodes;
    size_t node_count;
} elephc_dom_xpath_callback_argument;

typedef struct {
    uint64_t context_id;
    uint64_t xpath_handle;
    int32_t host_status;
    uint64_t *leases;
    size_t lease_count;
    size_t lease_capacity;
    uint8_t *error_message;
    size_t error_length;
    int32_t error_kind;
} elephc_dom_xpath_callback_context;

extern uint32_t elephc_dom_host_external_entity_load(
    uint64_t context_id,
    const uint8_t *public_id,
    size_t public_id_length,
    const uint8_t *system_id,
    size_t system_id_length,
    const uint8_t *directory,
    size_t directory_length,
    const uint8_t *int_sub_name,
    size_t int_sub_name_length,
    const uint8_t *ext_sub_uri,
    size_t ext_sub_uri_length,
    const uint8_t *ext_sub_system,
    size_t ext_sub_system_length,
    elephc_dom_host_loader_result *out_result
);
extern uint32_t elephc_dom_host_resource_open(
    uint64_t context_id,
    const uint8_t *url,
    size_t url_length,
    elephc_dom_host_loader_result *out_result
);
extern void elephc_dom_host_loader_bytes_free(uint8_t *bytes, size_t length);
extern uint32_t elephc_dom_host_stream_read(
    uint64_t lease_id,
    uint8_t *buffer,
    size_t capacity,
    size_t *out_length
);
extern uint32_t elephc_dom_host_stream_close(uint64_t lease_id);
extern uint32_t elephc_dom_host_xpath_invoke(
    uint64_t context_id,
    uint64_t xpath_handle,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *name,
    size_t name_length,
    const elephc_dom_xpath_callback_argument *arguments,
    size_t argument_count,
    elephc_dom_host_loader_result *out_result
);
extern uint32_t elephc_dom_host_result_release(
    uint64_t context_id,
    uint64_t result_id
);

static const xmlChar elephc_dom_xml_namespace[] =
    "http://www.w3.org/XML/1998/namespace";
static const xmlChar elephc_dom_xmlns_namespace[] =
    "http://www.w3.org/2000/xmlns/";
static const xmlChar elephc_dom_html_namespace[] =
    "http://www.w3.org/1999/xhtml";
static const uint8_t elephc_dom_modern_xml_marker = 0;
static _Thread_local int elephc_dom_test_fail_xml_new_input_from_io = 0;

static xmlNodePtr elephc_dom_next_descendant(
    xmlNodePtr node,
    xmlNodePtr root
);
static xmlNodePtr elephc_dom_template_fragment(xmlNodePtr element);
void elephc_dom_native_html_copy_document_mode(
    void *source,
    void *target
);

static char *elephc_dom_copy_c_string(const uint8_t *bytes, size_t length)
{
    char *copy;

    if ((length != 0
            && (bytes == NULL || memchr(bytes, '\0', length) != NULL))
        || length == SIZE_MAX) {
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

static size_t elephc_dom_optional_c_string_length(const char *value)
{
    return value == NULL ? 0 : strlen(value);
}

static int elephc_dom_stream_io_read(void *opaque, char *buffer, int length)
{
    elephc_dom_stream_io_context *stream = opaque;
    size_t bytes_read = 0;
    uint32_t status;

    if (stream == NULL || buffer == NULL || length <= 0) {
        return -XML_ERR_ARGUMENT;
    }
    status = elephc_dom_host_stream_read(
        stream->lease_id,
        (uint8_t *) buffer,
        (size_t) length,
        &bytes_read
    );
    if (status != 0 || bytes_read > (size_t) INT_MAX) {
        if (stream->loader->host_status == 0) {
            stream->loader->host_status = status == 0 ? 3 : (int32_t) status;
        }
        return -XML_ERR_INTERNAL_ERROR;
    }
    return (int) bytes_read;
}

static int elephc_dom_stream_io_close(void *opaque)
{
    elephc_dom_stream_io_context *stream = opaque;
    uint32_t status;

    if (stream == NULL) {
        return -1;
    }
    status = elephc_dom_host_stream_close(stream->lease_id);
    if (status != 0 && stream->loader->host_status == 0) {
        stream->loader->host_status = (int32_t) status;
    }
    free(stream);
    return status == 0 ? 0 : -1;
}

static xmlParserErrors elephc_dom_resource_loader(
    void *opaque,
    const char *url,
    const char *public_id,
    xmlResourceType type,
    xmlParserInputFlags flags,
    xmlParserInput **out
)
{
    elephc_dom_resource_loader_context *loader = opaque;
    elephc_dom_host_loader_result result = {NULL, 0, 0, 3, 0};
    xmlParserCtxtPtr parser;
    uint32_t status;
    char *resolved = NULL;
    elephc_dom_stream_io_context *stream = NULL;
    xmlParserInput *input = NULL;
    xmlParserErrors error;

    if (loader == NULL || out == NULL) {
        return XML_ERR_ARGUMENT;
    }
    parser = loader->parser;
    (void) type;
    *out = NULL;
    status = elephc_dom_host_external_entity_load(
        loader->context_id,
        (const uint8_t *) public_id,
        elephc_dom_optional_c_string_length(public_id),
        (const uint8_t *) url,
        elephc_dom_optional_c_string_length(url),
        parser == NULL ? NULL : (const uint8_t *) parser->directory,
        parser == NULL
            ? 0
            : elephc_dom_optional_c_string_length(parser->directory),
        parser == NULL ? NULL : (const uint8_t *) parser->intSubName,
        parser == NULL
            ? 0
            : elephc_dom_optional_c_string_length(
                (const char *) parser->intSubName
            ),
        parser == NULL ? NULL : (const uint8_t *) parser->extSubURI,
        parser == NULL
            ? 0
            : elephc_dom_optional_c_string_length(
                (const char *) parser->extSubURI
            ),
        parser == NULL ? NULL : (const uint8_t *) parser->extSubSystem,
        parser == NULL
            ? 0
            : elephc_dom_optional_c_string_length(
                (const char *) parser->extSubSystem
            ),
        &result
    );
    if (status != 0) {
        loader->host_status = (int32_t) status;
        return XML_ERR_INTERNAL_ERROR;
    }
    if (result.kind == 3) {
        status = elephc_dom_host_resource_open(
            loader->context_id,
            (const uint8_t *) url,
            elephc_dom_optional_c_string_length(url),
            &result
        );
        if (status != 0) {
            loader->host_status = (int32_t) status;
            return XML_ERR_INTERNAL_ERROR;
        }
    }
    if (result.kind == 3) {
        return xmlNewInputFromUrl(url, flags, out);
    }
    if (result.kind == 0) {
        return XML_IO_ENOENT;
    }
    if (result.kind == 1) {
        resolved = elephc_dom_copy_c_string(result.bytes, result.length);
        elephc_dom_host_loader_bytes_free(result.bytes, result.length);
        if (resolved == NULL) {
            return XML_ERR_NO_MEMORY;
        }
        error = xmlNewInputFromUrl(resolved, flags, out);
        free(resolved);
        return error;
    }
    if (result.kind == 2 && result.resource != 0) {
        stream = malloc(sizeof(*stream));
        if (stream == NULL) {
            status = elephc_dom_host_stream_close(result.resource);
            if (status != 0) {
                loader->host_status = (int32_t) status;
            }
            return XML_ERR_NO_MEMORY;
        }
        stream->lease_id = result.resource;
        stream->loader = loader;
        input = elephc_dom_test_fail_xml_new_input_from_io != 0
            ? NULL
            : xmlNewInputFromIO(
                url,
                elephc_dom_stream_io_read,
                elephc_dom_stream_io_close,
                stream,
                flags
            );
        if (input == NULL) {
            (void) elephc_dom_stream_io_close(stream);
            return XML_ERR_NO_MEMORY;
        }
        *out = input;
        return XML_ERR_OK;
    }
    if (result.bytes != NULL || result.length != 0) {
        elephc_dom_host_loader_bytes_free(result.bytes, result.length);
    }
    loader->host_status = 3;
    return XML_ERR_INTERNAL_ERROR;
}

int elephc_dom_native_test_resource_loader_input_from_io_failure(
    uint64_t host_context
)
{
    elephc_dom_resource_loader_context loader = {
        host_context,
        NULL,
        0
    };
    xmlParserInput *input = NULL;
    xmlParserErrors status;

    elephc_dom_test_fail_xml_new_input_from_io = 1;
    status = elephc_dom_resource_loader(
        &loader,
        "elephc-test-resource",
        NULL,
        (xmlResourceType) 0,
        0,
        &input
    );
    elephc_dom_test_fail_xml_new_input_from_io = 0;
    if (input != NULL) {
        xmlFreeInputStream(input);
        return 0;
    }
    return status == XML_ERR_NO_MEMORY ? 1 : 0;
}

static void elephc_dom_set_memory_document_url(xmlDocPtr document)
{
    char resolved_path[PATH_MAX + 2];
    size_t length;

    if (document == NULL || document->URL != NULL
        || getcwd(resolved_path, PATH_MAX) == NULL) {
        return;
    }
    length = strlen(resolved_path);
    if (length == 0) {
        return;
    }
    if (resolved_path[length - 1] != '/') {
        resolved_path[length++] = '/';
        resolved_path[length] = '\0';
    }
    document->URL = xmlCanonicPath((const xmlChar *) resolved_path);
}

static int32_t elephc_dom_validate_and_split_qname(
    const char *namespace_uri,
    const char *qualified_name,
    int32_t modern,
    xmlChar **local_name,
    xmlChar **prefix
)
{
    static const xmlChar xml_namespace[] =
        "http://www.w3.org/XML/1998/namespace";
    static const xmlChar xmlns_namespace[] =
        "http://www.w3.org/2000/xmlns/";
    int invalid_code = modern != 0 ? 5 : 14;

    *local_name = NULL;
    *prefix = NULL;
    if (qualified_name == NULL
        || xmlValidateQName((const xmlChar *) qualified_name, 0) != 0) {
        return invalid_code;
    }
    *local_name = xmlSplitQName2(
        (const xmlChar *) qualified_name,
        prefix
    );
    if (*local_name == NULL) {
        *local_name = xmlStrdup((const xmlChar *) qualified_name);
        if (*local_name == NULL) {
            return 11;
        }
    }
    if (*prefix != NULL
        && (namespace_uri == NULL || namespace_uri[0] == '\0')) {
        return 14;
    }
    if (modern == 0) {
        return 0;
    }
    if (*prefix != NULL
        && xmlStrEqual(*prefix, (const xmlChar *) "xml")
        && !xmlStrEqual(
            (const xmlChar *) namespace_uri,
            xml_namespace
        )) {
        return 14;
    }
    if ((xmlStrEqual(
                (const xmlChar *) qualified_name,
                (const xmlChar *) "xmlns"
            )
            || (*prefix != NULL
                && xmlStrEqual(*prefix, (const xmlChar *) "xmlns")))
        && !xmlStrEqual(
            (const xmlChar *) namespace_uri,
            xmlns_namespace
        )) {
        return 14;
    }
    if (xmlStrEqual(
            (const xmlChar *) namespace_uri,
            xmlns_namespace
        )
        && !xmlStrEqual(
            (const xmlChar *) qualified_name,
            (const xmlChar *) "xmlns"
        )
        && (*prefix == NULL
            || !xmlStrEqual(*prefix, (const xmlChar *) "xmlns"))) {
        return 14;
    }
    return 0;
}

static int32_t elephc_dom_prefix_equal(
    const xmlChar *left,
    const xmlChar *right
)
{
    return (left == NULL && right == NULL)
        || (left != NULL
            && right != NULL
            && xmlStrEqual(left, right));
}

static int32_t elephc_dom_is_namespace_attribute(
    xmlAttrPtr attribute
)
{
    return attribute != NULL
        && attribute->ns != NULL
        && xmlStrEqual(
            attribute->ns->href,
            elephc_dom_xmlns_namespace
        );
}

static const xmlChar *elephc_dom_namespace_attribute_prefix(
    xmlAttrPtr attribute
)
{
    return attribute->ns->prefix == NULL ? NULL : attribute->name;
}

static const xmlChar *elephc_dom_namespace_attribute_uri(
    xmlAttrPtr attribute
)
{
    return attribute->children == NULL
        ? (const xmlChar *) ""
        : attribute->children->content;
}

static xmlAttrPtr elephc_dom_attribute_by_qualified_name(
    xmlNodePtr element,
    const xmlChar *qualified_name
)
{
    xmlAttrPtr attribute;
    size_t qualified_length;

    if (element == NULL || qualified_name == NULL) {
        return NULL;
    }
    qualified_length = xmlStrlen(qualified_name);
    for (attribute = element->properties; attribute != NULL;
        attribute = attribute->next) {
        const xmlChar *prefix =
            attribute->ns == NULL ? NULL : attribute->ns->prefix;

        if (prefix == NULL) {
            if (xmlStrEqual(attribute->name, qualified_name)) {
                return attribute;
            }
        } else {
            size_t prefix_length = xmlStrlen(prefix);
            size_t local_length = xmlStrlen(attribute->name);

            if (prefix_length + 1 + local_length == qualified_length
                && memcmp(
                    qualified_name,
                    prefix,
                    prefix_length
                ) == 0
                && qualified_name[prefix_length] == ':'
                && memcmp(
                    qualified_name + prefix_length + 1,
                    attribute->name,
                    local_length
                ) == 0) {
                return attribute;
            }
        }
    }
    return NULL;
}

static xmlNodePtr elephc_dom_namespace_lookup_element(xmlNodePtr node)
{
    if (node == NULL) {
        return NULL;
    }
    if (node->type == XML_DOCUMENT_NODE
        || node->type == XML_HTML_DOCUMENT_NODE) {
        node = ((xmlDocPtr) node)->children;
    } else if (node->type == XML_ATTRIBUTE_NODE) {
        node = node->parent;
    }
    while (node != NULL && node->type != XML_ELEMENT_NODE) {
        node = node->parent;
    }
    return node;
}

static xmlNsPtr elephc_dom_old_namespace(
    xmlDocPtr document,
    const xmlChar *prefix,
    const xmlChar *namespace_uri
)
{
    xmlNsPtr namespace;

    for (namespace = document->oldNs; namespace != NULL;
        namespace = namespace->next) {
        if (elephc_dom_prefix_equal(namespace->prefix, prefix)
            && xmlStrEqual(namespace->href, namespace_uri)) {
            return namespace;
        }
    }
    return NULL;
}

static xmlNsPtr elephc_dom_modern_lookup_namespace(
    xmlDocPtr document,
    xmlNodePtr node,
    const xmlChar *prefix
)
{
    xmlNodePtr current = elephc_dom_namespace_lookup_element(node);

    while (current != NULL) {
        xmlAttrPtr attribute;
        xmlNsPtr declaration;

        if (current->ns != NULL
            && elephc_dom_prefix_equal(
                current->ns->prefix,
                prefix
            )) {
            return current->ns;
        }
        for (attribute = current->properties;
            attribute != NULL;
            attribute = attribute->next) {
            if (elephc_dom_is_namespace_attribute(attribute)) {
                const xmlChar *declared_prefix =
                    elephc_dom_namespace_attribute_prefix(attribute);

                if (elephc_dom_prefix_equal(
                        declared_prefix,
                        prefix
                    )) {
                    return elephc_dom_old_namespace(
                        document,
                        declared_prefix,
                        elephc_dom_namespace_attribute_uri(attribute)
                    );
                }
            } else if (attribute->ns != NULL
                && elephc_dom_prefix_equal(
                    attribute->ns->prefix,
                    prefix
                )) {
                return attribute->ns;
            }
        }
        for (declaration = current->nsDef;
            declaration != NULL;
            declaration = declaration->next) {
            if (elephc_dom_prefix_equal(
                    declaration->prefix,
                    prefix
                )) {
                return declaration;
            }
        }
        current = current->parent;
        while (current != NULL
            && current->type != XML_ELEMENT_NODE) {
            current = current->parent;
        }
    }
    return NULL;
}

static xmlNsPtr elephc_dom_namespace_attribute_mapping(
    xmlAttrPtr attribute
)
{
    xmlNsPtr namespace;
    const xmlChar *prefix;
    const xmlChar *namespace_uri;

    if (!elephc_dom_is_namespace_attribute(attribute)
        || attribute->doc == NULL
        || attribute->doc->_private
            != (void *) &elephc_dom_modern_xml_marker) {
        return NULL;
    }
    prefix = elephc_dom_namespace_attribute_prefix(attribute);
    namespace_uri = elephc_dom_namespace_attribute_uri(attribute);
    namespace = (xmlNsPtr) attribute->_private;
    if (namespace != NULL
        && elephc_dom_prefix_equal(namespace->prefix, prefix)
        && xmlStrEqual(namespace->href, namespace_uri)) {
        return namespace;
    }
    return elephc_dom_old_namespace(
        attribute->doc,
        prefix,
        namespace_uri
    );
}

typedef struct {
    xmlNodePtr element;
    xmlNsPtr in_scope;
} elephc_dom_namespace_redefinition;

static int32_t elephc_dom_redefine_removed_namespace(
    xmlNodePtr root,
    xmlNsPtr removed_namespace
)
{
    size_t capacity = 128;
    size_t count = 1;
    elephc_dom_namespace_redefinition *worklist = malloc(
        capacity * sizeof(*worklist)
    );

    if (worklist == NULL) {
        return 0;
    }
    worklist[0].element = root;
    worklist[0].in_scope = NULL;
    while (count != 0) {
        elephc_dom_namespace_redefinition item = worklist[--count];
        xmlNsPtr in_scope = item.in_scope;
        xmlAttrPtr attribute;
        xmlNodePtr child;

        if (item.element->ns == removed_namespace) {
            if (in_scope == NULL) {
                in_scope = xmlNewNs(
                    item.element,
                    removed_namespace->href,
                    removed_namespace->prefix
                );
                if (in_scope == NULL) {
                    free(worklist);
                    return 0;
                }
            }
            item.element->ns = in_scope;
        }
        for (attribute = item.element->properties;
            attribute != NULL;
            attribute = attribute->next) {
            if (attribute->ns == removed_namespace) {
                if (in_scope == NULL) {
                    in_scope = xmlNewNs(
                        item.element,
                        removed_namespace->href,
                        removed_namespace->prefix
                    );
                    if (in_scope == NULL) {
                        free(worklist);
                        return 0;
                    }
                }
                attribute->ns = in_scope;
            }
        }
        for (child = item.element->children; child != NULL;
            child = child->next) {
            elephc_dom_namespace_redefinition *resized;

            if (child->type != XML_ELEMENT_NODE) {
                continue;
            }
            if (count == capacity) {
                if (capacity > SIZE_MAX / 2
                    || capacity * 2
                        > SIZE_MAX / sizeof(*worklist)) {
                    free(worklist);
                    return 0;
                }
                capacity *= 2;
                resized = realloc(
                    worklist,
                    capacity * sizeof(*worklist)
                );
                if (resized == NULL) {
                    free(worklist);
                    return 0;
                }
                worklist = resized;
            }
            worklist[count].element = child;
            worklist[count].in_scope = in_scope;
            count++;
        }
    }
    free(worklist);
    return 1;
}

static xmlNsPtr elephc_dom_document_namespace(
    xmlDocPtr document,
    xmlNodePtr owner,
    const char *namespace_uri,
    const xmlChar *prefix
)
{
    xmlNsPtr namespace;

    if (namespace_uri == NULL || namespace_uri[0] == '\0') {
        return NULL;
    }
    if (owner != NULL) {
        namespace = document->_private
                == (void *) &elephc_dom_modern_xml_marker
            ? elephc_dom_modern_lookup_namespace(
                document,
                owner,
                prefix
            )
            : xmlSearchNsByHref(
                document,
                owner,
                (const xmlChar *) namespace_uri
            );
        if (namespace != NULL
            && xmlStrEqual(
                namespace->href,
                (const xmlChar *) namespace_uri
            )
            && elephc_dom_prefix_equal(
                namespace->prefix,
                prefix
            )) {
            return namespace;
        }
        return xmlNewNs(
            owner,
            (const xmlChar *) namespace_uri,
            prefix
        );
    }
    namespace = elephc_dom_old_namespace(
        document,
        prefix,
        (const xmlChar *) namespace_uri
    );
    if (namespace != NULL) {
        return namespace;
    }
    namespace = xmlNewNs(
        NULL,
        (const xmlChar *) namespace_uri,
        prefix
    );
    if (namespace != NULL) {
        namespace->next = document->oldNs;
        document->oldNs = namespace;
    }
    return namespace;
}

static uint8_t *elephc_dom_copy_error_string(
    const char *source,
    size_t *length
)
{
    uint8_t *copy;

    *length = source == NULL ? 0 : strlen(source);
    if (*length == 0) {
        return NULL;
    }
    copy = malloc(*length);
    if (copy != NULL) {
        memcpy(copy, source, *length);
    }
    return copy;
}

static void elephc_dom_native_error_free(elephc_dom_native_error *error)
{
    free(error->message);
    free(error->file);
    memset(error, 0, sizeof(*error));
}

static void elephc_dom_capture_structured_error(
    void *user_data,
    const xmlError *error
)
{
    elephc_dom_error_list *list = user_data;
    elephc_dom_native_error *record;
    elephc_dom_native_error *resized;
    size_t new_capacity;

    if (list == NULL || error == NULL || list->allocation_failed != 0) {
        return;
    }
    if (list->count == list->capacity) {
        new_capacity = list->capacity == 0 ? 4 : list->capacity * 2;
        if (new_capacity < list->capacity
            || new_capacity > SIZE_MAX / sizeof(*list->errors)) {
            list->allocation_failed = 1;
            return;
        }
        resized = realloc(list->errors, new_capacity * sizeof(*list->errors));
        if (resized == NULL) {
            list->allocation_failed = 1;
            return;
        }
        list->errors = resized;
        list->capacity = new_capacity;
    }

    record = &list->errors[list->count];
    memset(record, 0, sizeof(*record));
    record->level = error->level;
    record->domain = error->domain;
    record->code = error->code;
    record->line = error->line;
    record->column = error->int2;
    record->message = elephc_dom_copy_error_string(
        error->message,
        &record->message_length
    );
    if (record->message_length != 0 && record->message == NULL) {
        list->allocation_failed = 1;
        return;
    }
    record->file = elephc_dom_copy_error_string(
        error->file,
        &record->file_length
    );
    if (record->file_length != 0 && record->file == NULL) {
        elephc_dom_native_error_free(record);
        list->allocation_failed = 1;
        return;
    }
    list->count++;
}

static void elephc_dom_capture_generic_error(
    void *user_data,
    const char *format,
    ...
)
{
    elephc_dom_generic_error_context *context = user_data;
    elephc_dom_error_list *errors;
    va_list arguments;
    va_list measurement;
    char *resized;
    size_t required;
    size_t message_length;
    int formatted_length;
    int32_t complete;
    xmlError error;

    if (context == NULL || context->errors == NULL || format == NULL) {
        return;
    }
    errors = context->errors;
    if (errors->allocation_failed != 0) {
        return;
    }

    va_start(arguments, format);
    va_copy(measurement, arguments);
    formatted_length = vsnprintf(NULL, 0, format, measurement);
    va_end(measurement);
    if (formatted_length < 0) {
        va_end(arguments);
        errors->allocation_failed = 1;
        return;
    }
    message_length = (size_t) formatted_length;
    if (message_length > SIZE_MAX - context->length - 1) {
        va_end(arguments);
        errors->allocation_failed = 1;
        return;
    }
    required = context->length + message_length + 1;
    if (required > context->capacity) {
        size_t capacity = context->capacity == 0 ? 128 : context->capacity;

        while (capacity < required) {
            if (capacity > SIZE_MAX / 2) {
                va_end(arguments);
                errors->allocation_failed = 1;
                return;
            }
            capacity *= 2;
        }
        resized = realloc(context->buffer, capacity);
        if (resized == NULL) {
            va_end(arguments);
            errors->allocation_failed = 1;
            return;
        }
        context->buffer = resized;
        context->capacity = capacity;
    }
    (void) vsnprintf(
        context->buffer + context->length,
        message_length + 1,
        format,
        arguments
    );
    va_end(arguments);
    context->length += message_length;
    complete = context->length != 0
        && context->buffer[context->length - 1] == '\n';
    if (complete == 0) {
        return;
    }
    while (context->length != 0
        && context->buffer[context->length - 1] == '\n') {
        context->length--;
    }
    context->buffer[context->length] = '\0';

    memset(&error, 0, sizeof(error));
    error.level = XML_ERR_ERROR;
    error.code = XML_ERR_INTERNAL_ERROR;
    error.message = context->buffer;
    elephc_dom_capture_structured_error(errors, &error);
    context->length = 0;
}

static void elephc_dom_generic_error_context_clear(
    elephc_dom_generic_error_context *context
)
{
    if (context == NULL) {
        return;
    }
    if (context->installed != 0) {
        xmlSetGenericErrorFunc(
            context->previous_context,
            context->previous_handler
        );
    }
    free(context->buffer);
    memset(context, 0, sizeof(*context));
}

static void elephc_dom_generic_error_context_install(
    elephc_dom_generic_error_context *context
)
{
    if (context == NULL || context->installed != 0) {
        return;
    }
    context->previous_handler = xmlGenericError;
    context->previous_context = xmlGenericErrorContext;
    context->installed = 1;
    xmlSetGenericErrorFunc(context, elephc_dom_capture_generic_error);
}

static void elephc_dom_pointer_list_append(
    elephc_dom_pointer_list *list,
    void *pointer
)
{
    void **resized;
    size_t capacity;

    if (list == NULL || pointer == NULL || list->allocation_failed != 0) {
        return;
    }
    if (list->count == list->capacity) {
        capacity = list->capacity == 0 ? 16 : list->capacity * 2;
        if (capacity < list->capacity
            || capacity > SIZE_MAX / sizeof(*list->pointers)) {
            list->allocation_failed = 1;
            return;
        }
        resized = realloc(
            list->pointers,
            capacity * sizeof(*list->pointers)
        );
        if (resized == NULL) {
            list->allocation_failed = 1;
            return;
        }
        list->pointers = resized;
        list->capacity = capacity;
    }
    list->pointers[list->count++] = pointer;
}

static void elephc_dom_collect_xinclude_subtree(
    elephc_dom_pointer_list *list,
    xmlNodePtr root
)
{
    xmlNodePtr current = root;

    while (current != NULL && list->allocation_failed == 0) {
        xmlAttrPtr attribute;

        elephc_dom_pointer_list_append(list, current);
        if (current->type == XML_ELEMENT_NODE) {
            for (attribute = current->properties; attribute != NULL;
                attribute = attribute->next) {
                xmlNodePtr child;

                elephc_dom_pointer_list_append(list, attribute);
                for (child = attribute->children; child != NULL;
                    child = child->next) {
                    elephc_dom_pointer_list_append(list, child);
                }
            }
        }
        current = elephc_dom_next_descendant(current, root);
    }
}

static void elephc_dom_collect_xinclude_targets(
    elephc_dom_pointer_list *list,
    xmlDocPtr document
)
{
    xmlNodePtr root;
    xmlNodePtr current;

    if (list == NULL || document == NULL) {
        return;
    }
    root = xmlDocGetRootElement(document);
    current = root;
    while (current != NULL && list->allocation_failed == 0) {
        if (current->type == XML_ELEMENT_NODE
            && current->ns != NULL
            && xmlStrEqual(current->name, XINCLUDE_NODE)
            && (xmlStrEqual(current->ns->href, XINCLUDE_NS)
                || xmlStrEqual(current->ns->href, XINCLUDE_OLD_NS))) {
            elephc_dom_collect_xinclude_subtree(list, current);
        }
        current = elephc_dom_next_descendant(current, root);
    }
}

static void elephc_dom_error_list_discard(elephc_dom_error_list *list)
{
    if (list == NULL) {
        return;
    }
    while (list->count != 0) {
        elephc_dom_native_error_free(&list->errors[--list->count]);
    }
    free(list->errors);
    memset(list, 0, sizeof(*list));
}

static elephc_dom_libxml_globals elephc_dom_sanitize_libxml_globals(void)
{
    elephc_dom_libxml_globals globals = {
        xmlLoadExtDtdDefaultValue,
        xmlDoValidityCheckingDefaultValue,
        xmlPedanticParserDefault(0),
        xmlSubstituteEntitiesDefault(0),
        xmlLineNumbersDefault(0),
        xmlKeepBlanksDefault(1)
    };

    xmlLoadExtDtdDefaultValue = 0;
    xmlDoValidityCheckingDefaultValue = 0;
    return globals;
}

static void elephc_dom_restore_libxml_globals(
    elephc_dom_libxml_globals globals
)
{
    xmlLoadExtDtdDefaultValue = globals.load_subset;
    xmlDoValidityCheckingDefaultValue = globals.validate;
    (void) xmlPedanticParserDefault(globals.pedantic);
    (void) xmlSubstituteEntitiesDefault(globals.substitute);
    (void) xmlLineNumbersDefault(globals.line_numbers);
    (void) xmlKeepBlanksDefault(globals.keep_blanks);
}

static elephc_dom_validation_ns_link *
elephc_dom_validation_ns_guard_add_node(
    elephc_dom_validation_ns_guard *guard,
    xmlNodePtr node
)
{
    elephc_dom_validation_ns_link *link = calloc(1, sizeof(*link));

    if (link == NULL) {
        guard->allocation_failed = 1;
        return NULL;
    }
    link->node = node;
    link->next = guard->links;
    guard->links = link;
    return link;
}

static xmlNsPtr elephc_dom_validation_ns_guard_add_namespace(
    elephc_dom_validation_ns_guard *guard,
    elephc_dom_validation_ns_link *link,
    const xmlChar *href,
    const xmlChar *prefix,
    xmlAttrPtr attribute
)
{
    xmlNsPtr namespace;

    if (href == NULL) {
        return NULL;
    }
    namespace = xmlMalloc(sizeof(*namespace));
    if (namespace == NULL) {
        guard->allocation_failed = 1;
        return NULL;
    }
    memset(namespace, 0, sizeof(*namespace));
    namespace->type = XML_LOCAL_NAMESPACE;
    namespace->href = xmlStrdup(href);
    namespace->prefix = prefix == NULL ? NULL : xmlStrdup(prefix);
    if (namespace->href == NULL
        || (prefix != NULL && namespace->prefix == NULL)) {
        xmlFreeNs(namespace);
        guard->allocation_failed = 1;
        return NULL;
    }
    namespace->_private = attribute;
    namespace->next = link->node->nsDef;
    link->node->nsDef = namespace;
    link->added_count++;
    return namespace;
}

static int32_t elephc_dom_validation_ns_guard_has_prefix(
    xmlNodePtr node,
    const xmlChar *prefix
)
{
    xmlNsPtr namespace;

    for (namespace = node->nsDef; namespace != NULL;
        namespace = namespace->next) {
        if (elephc_dom_prefix_equal(namespace->prefix, prefix)) {
            return 1;
        }
    }
    return 0;
}

static void elephc_dom_validation_ns_guard_relink_element(
    elephc_dom_validation_ns_guard *guard,
    xmlNodePtr node
)
{
    elephc_dom_validation_ns_link *link;
    xmlAttrPtr attribute;

    if (node == NULL || node->type != XML_ELEMENT_NODE
        || guard->allocation_failed != 0) {
        return;
    }
    link = elephc_dom_validation_ns_guard_add_node(guard, node);
    if (link == NULL) {
        return;
    }

    attribute = node->properties;
    while (attribute != NULL) {
        xmlAttrPtr next = attribute->next;

        if (elephc_dom_is_namespace_attribute(attribute)) {
            const xmlChar *prefix =
                elephc_dom_namespace_attribute_prefix(attribute);
            const xmlChar *href =
                elephc_dom_namespace_attribute_uri(attribute);

            if (elephc_dom_validation_ns_guard_add_namespace(
                    guard,
                    link,
                    href,
                    prefix,
                    attribute
                ) == NULL) {
                return;
            }
            if (attribute->prev != NULL) {
                attribute->prev->next = attribute->next;
            } else {
                node->properties = attribute->next;
            }
            if (attribute->next != NULL) {
                attribute->next->prev = attribute->prev;
            }
        }
        attribute = next;
    }

    if (node->ns != NULL && node->ns->prefix == NULL) {
        link->original_namespace = node->ns;
        link->restore_namespace = 1;
        node->ns = xmlSearchNs(node->doc, node, NULL);
    } else if (node->ns != NULL
        && elephc_dom_validation_ns_guard_add_namespace(
            guard,
            link,
            node->ns->href,
            node->ns->prefix,
            NULL
        ) == NULL) {
        return;
    }

    for (attribute = node->properties; attribute != NULL;
        attribute = attribute->next) {
        if (attribute->ns != NULL
            && !elephc_dom_is_namespace_attribute(attribute)
            && !elephc_dom_validation_ns_guard_has_prefix(
                node,
                attribute->ns->prefix
            )
            && elephc_dom_validation_ns_guard_add_namespace(
                guard,
                link,
                attribute->ns->href,
                attribute->ns->prefix,
                NULL
            ) == NULL) {
            return;
        }
    }
}

static void elephc_dom_validation_ns_guard_end(
    elephc_dom_validation_ns_guard *guard
)
{
    elephc_dom_validation_ns_link *link;

    if (guard == NULL) {
        return;
    }
    link = guard->links;
    while (link != NULL) {
        elephc_dom_validation_ns_link *next = link->next;

        if (link->restore_namespace != 0) {
            link->node->ns = link->original_namespace;
        }
        while (link->added_count != 0) {
            xmlNsPtr namespace = link->node->nsDef;
            xmlAttrPtr attribute;

            if (namespace == NULL) {
                break;
            }
            link->node->nsDef = namespace->next;
            attribute = (xmlAttrPtr) namespace->_private;
            if (attribute != NULL) {
                if (attribute->prev != NULL) {
                    attribute->prev->next = attribute;
                } else {
                    link->node->properties = attribute;
                }
                if (attribute->next != NULL) {
                    attribute->next->prev = attribute;
                }
            }
            xmlFreeNs(namespace);
            link->added_count--;
        }
        free(link);
        link = next;
    }
    memset(guard, 0, sizeof(*guard));
}

static int32_t elephc_dom_validation_ns_guard_begin(
    elephc_dom_validation_ns_guard *guard,
    xmlDocPtr document
)
{
    xmlNodePtr root;
    xmlNodePtr current;

    memset(guard, 0, sizeof(*guard));
    if (document == NULL
        || document->_private != (void *) &elephc_dom_modern_xml_marker) {
        return 1;
    }
    guard->active = 1;
    root = xmlDocGetRootElement(document);
    if (root == NULL) {
        return 1;
    }
    elephc_dom_validation_ns_guard_relink_element(guard, root);
    current = root->children;
    while (current != NULL && guard->allocation_failed == 0) {
        elephc_dom_validation_ns_guard_relink_element(guard, current);
        current = elephc_dom_next_descendant(current, root);
    }
    if (guard->allocation_failed != 0) {
        elephc_dom_validation_ns_guard_end(guard);
        return 0;
    }
    return 1;
}

static elephc_dom_native_validation_result
elephc_dom_validation_result_finish(
    elephc_dom_error_list *errors,
    int32_t valid,
    int32_t status
)
{
    elephc_dom_native_validation_result result = {
        NULL,
        0,
        0,
        valid,
        status,
        0
    };

    if (errors->allocation_failed != 0) {
        elephc_dom_error_list_discard(errors);
        result.allocation_failed = 1;
        return result;
    }
    result.errors = errors->errors;
    result.error_count = errors->count;
    memset(errors, 0, sizeof(*errors));
    return result;
}

static elephc_dom_native_xinclude_result
elephc_dom_xinclude_result_finish(
    elephc_dom_error_list *errors,
    elephc_dom_pointer_list *invalidated,
    int32_t substitutions,
    int32_t host_status
)
{
    elephc_dom_native_xinclude_result result = {
        NULL,
        0,
        NULL,
        0,
        0,
        substitutions,
        host_status
    };

    if (errors->allocation_failed != 0) {
        elephc_dom_error_list_discard(errors);
        result.allocation_failed = 1;
    } else {
        result.errors = errors->errors;
        result.error_count = errors->count;
        memset(errors, 0, sizeof(*errors));
    }
    if (invalidated->allocation_failed != 0) {
        free(invalidated->pointers);
        memset(invalidated, 0, sizeof(*invalidated));
        result.allocation_failed = 1;
    } else {
        result.invalidated = invalidated->pointers;
        result.invalidated_count = invalidated->count;
        memset(invalidated, 0, sizeof(*invalidated));
    }
    return result;
}

static int32_t elephc_dom_ascii_prefix_equal(
    const uint8_t *bytes,
    size_t length,
    const char *prefix,
    size_t prefix_length
)
{
    size_t index;

    if (bytes == NULL || length < prefix_length) {
        return 0;
    }
    for (index = 0; index < prefix_length; index++) {
        uint8_t actual = bytes[index];
        uint8_t expected = (uint8_t) prefix[index];

        if (actual >= 'A' && actual <= 'Z') {
            actual = (uint8_t) (actual + ('a' - 'A'));
        }
        if (expected >= 'A' && expected <= 'Z') {
            expected = (uint8_t) (expected + ('a' - 'A'));
        }
        if (actual != expected) {
            return 0;
        }
    }
    return 1;
}

static int32_t elephc_dom_validation_local_path_too_long(
    const uint8_t *path,
    size_t path_length
)
{
    size_t colon = 0;
    size_t local_length = path_length;
    int32_t has_scheme = 0;

    if (path == NULL) {
        return 0;
    }
    while (colon < path_length) {
        uint8_t byte = path[colon];

        if (byte == ':') {
            has_scheme = colon != 0;
            break;
        }
        if (byte == '/' || byte == '\\' || byte == '?' || byte == '#') {
            break;
        }
        colon++;
    }
    if (has_scheme != 0
        && !elephc_dom_ascii_prefix_equal(path, path_length, "file:", 5)) {
        return 0;
    }
    if (elephc_dom_ascii_prefix_equal(
            path,
            path_length,
            "file://localhost/",
            17
        )) {
        local_length = path_length - 16;
    } else if (elephc_dom_ascii_prefix_equal(
            path,
            path_length,
            "file:///",
            8
        )) {
        local_length = path_length - 7;
    } else if (has_scheme != 0) {
        return 0;
    }
    return local_length > PATH_MAX;
}

uint32_t elephc_dom_native_libxml_version(void)
{
    return LIBXML_VERSION;
}

const char *elephc_dom_native_libxml_version_string(void)
{
    return LIBXML_DOTTED_VERSION;
}

const char *elephc_dom_native_lexbor_version_string(void)
{
    return LEXBOR_VERSION_STRING;
}

int32_t elephc_dom_native_validate_name(
    const uint8_t *name,
    size_t name_length
)
{
    char *name_string = elephc_dom_copy_c_string(name, name_length);
    int32_t valid;

    if (name_string == NULL) {
        return 0;
    }
    valid = xmlValidateName((const xmlChar *) name_string, 0) == 0;
    free(name_string);
    return valid;
}

int32_t elephc_dom_native_validate_ncname(
    const uint8_t *name,
    size_t name_length
)
{
    char *name_string = elephc_dom_copy_c_string(name, name_length);
    int32_t valid;

    if (name_string == NULL) {
        return 0;
    }
    valid = xmlValidateNCName((const xmlChar *) name_string, 0) == 0;
    free(name_string);
    return valid;
}

int32_t elephc_dom_native_validate_qname(
    const uint8_t *name,
    size_t name_length
)
{
    char *name_string = elephc_dom_copy_c_string(name, name_length);
    int32_t valid;

    if (name_string == NULL) {
        return 0;
    }
    valid = xmlValidateQName((const xmlChar *) name_string, 0) == 0;
    free(name_string);
    return valid;
}

int32_t elephc_dom_native_parse_xml(const uint8_t *bytes, size_t length)
{
    xmlDocPtr document;

    if ((bytes == NULL && length != 0) || length > INT_MAX) {
        return -1;
    }

    xmlInitParser();
    document = xmlReadMemory(
        (const char *) bytes,
        (int) length,
        "elephc-memory.xml",
        NULL,
        XML_PARSE_NONET | XML_PARSE_NOERROR | XML_PARSE_NOWARNING
    );
    if (document == NULL) {
        return 0;
    }

    xmlFreeDoc(document);
    return 1;
}

int32_t elephc_dom_native_parse_html(const uint8_t *bytes, size_t length)
{
    lxb_html_document_t *document;
    lxb_status_t status;

    if (bytes == NULL && length != 0) {
        return -1;
    }

    document = lxb_html_document_create();
    if (document == NULL) {
        return -1;
    }

    status = lxb_html_document_parse(document, bytes, length);
    lxb_html_document_destroy(document);
    return status == LXB_STATUS_OK ? 1 : 0;
}

int32_t elephc_dom_native_encoding_is_valid(
    const uint8_t *encoding,
    size_t encoding_length
)
{
    xmlCharEncodingHandlerPtr handler;
    char *encoding_string =
        elephc_dom_copy_c_string(encoding, encoding_length);

    if (encoding_string == NULL) {
        return 0;
    }
    handler = xmlFindCharEncodingHandler(encoding_string);
    free(encoding_string);
    if (handler == NULL) {
        return 0;
    }
    xmlCharEncCloseFunc(handler);
    return 1;
}

void *elephc_dom_native_document_new(
    const uint8_t *version,
    size_t version_length,
    const uint8_t *encoding,
    size_t encoding_length
)
{
    char *version_string;
    char *encoding_string;
    xmlDocPtr document;

    version_string = elephc_dom_copy_c_string(version, version_length);
    if (version_string == NULL) {
        return NULL;
    }
    encoding_string = elephc_dom_copy_c_string(encoding, encoding_length);
    if (encoding_string == NULL) {
        free(version_string);
        return NULL;
    }

    document = xmlNewDoc((const xmlChar *) version_string);
    free(version_string);
    if (document != NULL && encoding_length != 0) {
        document->encoding = xmlStrdup((const xmlChar *) encoding_string);
        if (document->encoding == NULL) {
            xmlFreeDoc(document);
            document = NULL;
        }
    }
    free(encoding_string);
    return document;
}

void *elephc_dom_native_document_new_html(
    const uint8_t *title,
    size_t title_length
)
{
    static const xmlChar html_namespace[] =
        "http://www.w3.org/1999/xhtml";
    char *title_string = NULL;
    xmlDocPtr document;
    xmlDtdPtr doctype;
    xmlNodePtr html;
    xmlNodePtr head;
    xmlNodePtr title_node = NULL;
    xmlNodePtr body;
    xmlNsPtr namespace;

    if (title != NULL || title_length != 0) {
        title_string = elephc_dom_copy_c_string(title, title_length);
        if (title_string == NULL) {
            return NULL;
        }
    }
    document = htmlNewDocNoDtD(NULL, NULL);
    if (document == NULL) {
        free(title_string);
        return NULL;
    }
    document->encoding = xmlStrdup((const xmlChar *) "UTF-8");
    doctype = xmlCreateIntSubset(
        document,
        (const xmlChar *) "html",
        NULL,
        NULL
    );
    html = xmlNewDocRawNode(
        document,
        NULL,
        (const xmlChar *) "html",
        NULL
    );
    namespace = html == NULL
        ? NULL
        : xmlNewNs(html, html_namespace, NULL);
    if (html != NULL) {
        html->ns = namespace;
    }
    head = xmlNewDocRawNode(
        document,
        namespace,
        (const xmlChar *) "head",
        NULL
    );
    if (title_string != NULL) {
        title_node = xmlNewDocRawNode(
            document,
            namespace,
            (const xmlChar *) "title",
            (const xmlChar *) title_string
        );
    }
    body = xmlNewDocRawNode(
        document,
        namespace,
        (const xmlChar *) "body",
        NULL
    );
    free(title_string);
    if (document->encoding == NULL || doctype == NULL || html == NULL
        || namespace == NULL || head == NULL
        || ((title != NULL || title_length != 0) && title_node == NULL)
        || body == NULL) {
        if (head != NULL) {
            xmlFreeNode(head);
        }
        if (title_node != NULL) {
            xmlFreeNode(title_node);
        }
        if (body != NULL) {
            xmlFreeNode(body);
        }
        if (html != NULL) {
            xmlFreeNode(html);
        }
        xmlFreeDoc(document);
        return NULL;
    }
    xmlAddChild((xmlNodePtr) document, html);
    xmlAddChild(html, head);
    if (title_node != NULL) {
        xmlAddChild(head, title_node);
    }
    xmlAddChild(html, body);
    return document;
}

elephc_dom_native_parse_result elephc_dom_native_document_parse_xml(
    const uint8_t *bytes,
    size_t length,
    int32_t options,
    const uint8_t *override_encoding,
    size_t override_encoding_length,
    const uint8_t *input_name,
    size_t input_name_length,
    uint64_t host_context
)
{
    elephc_dom_native_parse_result result = {NULL, NULL, 0, 0, 0};
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    char *encoding_string = NULL;
    char *input_name_string = NULL;
    xmlParserCtxtPtr parser;
    xmlDocPtr document;
    elephc_dom_resource_loader_context loader = {
        host_context,
        NULL,
        0
    };

    if ((bytes == NULL && length != 0) || length > INT_MAX
        || (input_name == NULL && input_name_length != 0)) {
        return result;
    }
    if (override_encoding_length != 0) {
        encoding_string = elephc_dom_copy_c_string(
            override_encoding,
            override_encoding_length
        );
        if (encoding_string == NULL) {
            result.allocation_failed = 1;
            return result;
        }
    }
    if (input_name_length != 0) {
        input_name_string = elephc_dom_copy_c_string(
            input_name,
            input_name_length
        );
        if (input_name_string == NULL) {
            free(encoding_string);
            result.allocation_failed = 1;
            return result;
        }
    }
    xmlInitParser();
    parser = xmlNewParserCtxt();
    if (parser == NULL) {
        free(encoding_string);
        free(input_name_string);
        result.allocation_failed = 1;
        return result;
    }
    xmlCtxtSetErrorHandler(
        parser,
        elephc_dom_capture_structured_error,
        &errors
    );
    if (host_context != 0) {
        loader.parser = parser;
        xmlCtxtSetResourceLoader(
            parser,
            elephc_dom_resource_loader,
            &loader
        );
    }
    document = xmlCtxtReadMemory(
        parser,
        (const char *) bytes,
        (int) length,
        input_name_string,
        encoding_string,
        (options & ~(XML_PARSE_NOERROR | XML_PARSE_NOWARNING))
            | XML_PARSE_NONET
    );
    elephc_dom_set_memory_document_url(document);
    xmlFreeParserCtxt(parser);
    free(encoding_string);
    free(input_name_string);
    if (errors.allocation_failed != 0) {
        if (document != NULL) {
            xmlFreeDoc(document);
        }
        while (errors.count != 0) {
            elephc_dom_native_error_free(&errors.errors[--errors.count]);
        }
        free(errors.errors);
        result.allocation_failed = 1;
        return result;
    }
    result.document = document;
    result.errors = errors.errors;
    result.error_count = errors.count;
    result.host_status = loader.host_status;
    return result;
}

elephc_dom_native_parse_result elephc_dom_native_document_parse_html4(
    const uint8_t *bytes,
    size_t length,
    int32_t options,
    const uint8_t *input_name,
    size_t input_name_length
)
{
    elephc_dom_native_parse_result result = {NULL, NULL, 0, 0, 0};
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    char *input_name_string = NULL;
    htmlParserCtxtPtr parser;
    htmlDocPtr document;

    if ((bytes == NULL && length != 0) || length > INT_MAX
        || (input_name == NULL && input_name_length != 0)) {
        return result;
    }
    if (input_name_length != 0) {
        input_name_string = elephc_dom_copy_c_string(
            input_name,
            input_name_length
        );
        if (input_name_string == NULL) {
            result.allocation_failed = 1;
            return result;
        }
    }
    xmlInitParser();
    parser = htmlNewParserCtxt();
    if (parser == NULL) {
        free(input_name_string);
        result.allocation_failed = 1;
        return result;
    }
    xmlCtxtSetErrorHandler(
        (xmlParserCtxtPtr) parser,
        elephc_dom_capture_structured_error,
        &errors
    );
    document = htmlCtxtReadMemory(
        parser,
        (const char *) bytes,
        (int) length,
        input_name_string,
        NULL,
        options & ~(XML_PARSE_NOERROR | XML_PARSE_NOWARNING)
    );
    htmlFreeParserCtxt(parser);
    free(input_name_string);
    if (errors.allocation_failed != 0) {
        if (document != NULL) {
            xmlFreeDoc(document);
        }
        while (errors.count != 0) {
            elephc_dom_native_error_free(&errors.errors[--errors.count]);
        }
        free(errors.errors);
        result.allocation_failed = 1;
        return result;
    }
    result.document = document;
    result.errors = errors.errors;
    result.error_count = errors.count;
    return result;
}

static int32_t elephc_dom_mark_namespace_attributes(
    xmlDocPtr document,
    xmlNodePtr element
)
{
    xmlAttrPtr original_attributes;
    xmlAttrPtr last_added = NULL;
    xmlNsPtr namespace;

    if (element->nsDef == NULL) {
        return 1;
    }
    original_attributes = element->properties;
    element->properties = NULL;
    namespace = element->nsDef;
    while (namespace != NULL) {
        xmlNsPtr next = namespace->next;
        const xmlChar *attribute_name =
            namespace->prefix == NULL
                ? (const xmlChar *) "xmlns"
                : namespace->prefix;
        const xmlChar *attribute_prefix =
            namespace->prefix == NULL
                ? NULL
                : (const xmlChar *) "xmlns";
        xmlNsPtr attribute_namespace =
            elephc_dom_document_namespace(
                document,
                NULL,
                (const char *) elephc_dom_xmlns_namespace,
                attribute_prefix
            );

        if (attribute_namespace == NULL) {
            element->nsDef = namespace;
            if (last_added != NULL) {
                last_added->next = original_attributes;
                if (original_attributes != NULL) {
                    original_attributes->prev = last_added;
                }
            } else {
                element->properties = original_attributes;
            }
            return 0;
        }
        last_added = xmlSetNsProp(
            element,
            attribute_namespace,
            attribute_name,
            namespace->href
        );
        if (last_added == NULL) {
            element->nsDef = namespace;
            if (element->properties == NULL) {
                element->properties = original_attributes;
            } else {
                xmlAttrPtr tail = element->properties;
                while (tail->next != NULL) {
                    tail = tail->next;
                }
                tail->next = original_attributes;
                if (original_attributes != NULL) {
                    original_attributes->prev = tail;
                }
            }
            return 0;
        }
        last_added->_private = namespace;
        namespace->next = document->oldNs;
        document->oldNs = namespace;
        namespace = next;
    }
    if (last_added != NULL) {
        last_added->next = original_attributes;
        if (original_attributes != NULL) {
            original_attributes->prev = last_added;
        }
    } else {
        element->properties = original_attributes;
    }
    element->nsDef = NULL;
    return 1;
}

int32_t elephc_dom_native_document_convert_modern_xml(void *document)
{
    xmlDocPtr doc = (xmlDocPtr) document;
    xmlNodePtr current;
    xmlNodePtr root;

    if (doc == NULL) {
        return 0;
    }
    if (doc->_private == (void *) &elephc_dom_modern_xml_marker) {
        return 1;
    }
    if (doc->_private != NULL) {
        return 0;
    }
    if (doc->encoding == NULL) {
        doc->encoding = xmlStrdup((const xmlChar *) "UTF-8");
        if (doc->encoding == NULL) {
            return 0;
        }
    }
    root = (xmlNodePtr) doc;
    current = doc->children;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE
            && elephc_dom_mark_namespace_attributes(
                doc,
                current
            ) == 0) {
            return 0;
        }
        current = elephc_dom_next_descendant(current, root);
    }
    doc->_private = (void *) &elephc_dom_modern_xml_marker;
    return 1;
}

elephc_dom_native_xinclude_result elephc_dom_native_document_xinclude(
    void *document,
    int32_t flags,
    int32_t generic_errors,
    uint64_t host_context
)
{
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    elephc_dom_pointer_list invalidated = {NULL, 0, 0, 0};
    elephc_dom_generic_error_context generic = {
        &errors,
        NULL,
        0,
        0,
        NULL,
        NULL,
        0
    };
    elephc_dom_libxml_globals globals;
    elephc_dom_resource_loader_context loader = {
        host_context,
        NULL,
        0
    };
    xmlXIncludeCtxtPtr context;
    xmlNodePtr root;
    int substitutions;

    if (document == NULL) {
        return elephc_dom_xinclude_result_finish(
            &errors,
            &invalidated,
            -1,
            0
        );
    }
    elephc_dom_collect_xinclude_targets(
        &invalidated,
        (xmlDocPtr) document
    );
    if (invalidated.allocation_failed != 0) {
        return elephc_dom_xinclude_result_finish(
            &errors,
            &invalidated,
            -1,
            0
        );
    }

    xmlInitParser();
    context = xmlXIncludeNewContext((xmlDocPtr) document);
    root = xmlDocGetRootElement((xmlDocPtr) document);
    if (context == NULL || root == NULL) {
        if (context != NULL) {
            xmlXIncludeFreeContext(context);
        }
        return elephc_dom_xinclude_result_finish(
            &errors,
            &invalidated,
            -1,
            0
        );
    }
    if (generic_errors != 0) {
        elephc_dom_generic_error_context_install(&generic);
    } else {
        xmlXIncludeSetErrorHandler(
            context,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    if (host_context != 0) {
        xmlXIncludeSetResourceLoader(
            context,
            elephc_dom_resource_loader,
            &loader
        );
    }
    (void) xmlXIncludeSetFlags(
        context,
        flags | XML_PARSE_NOXINCNODE
    );
    globals = elephc_dom_sanitize_libxml_globals();
    substitutions = xmlXIncludeProcessNode(context, root);
    elephc_dom_restore_libxml_globals(globals);
    xmlXIncludeFreeContext(context);
    elephc_dom_generic_error_context_clear(&generic);
    return elephc_dom_xinclude_result_finish(
        &errors,
        &invalidated,
        substitutions,
        loader.host_status
    );
}

static int elephc_dom_c14n_parent_lookup(
    void *user_data,
    xmlNodePtr node,
    xmlNodePtr parent
)
{
    xmlNodePtr root = user_data;

    if (node == root) {
        return 1;
    }
    node = parent;
    while (node != NULL) {
        if (node == root) {
            return 1;
        }
        node = node->parent;
    }
    return 0;
}

static xmlChar *elephc_dom_c14n_string(
    const uint8_t *pointer,
    size_t length
)
{
    xmlChar *copy;

    if (length == SIZE_MAX) {
        return NULL;
    }
    copy = xmlMalloc(length + 1);
    if (copy == NULL) {
        return NULL;
    }
    if (length != 0) {
        if (pointer == NULL) {
            xmlFree(copy);
            return NULL;
        }
        memcpy(copy, pointer, length);
    }
    copy[length] = 0;
    return copy;
}

static void elephc_dom_c14n_prefixes_free(
    xmlChar **prefixes,
    size_t count
)
{
    while (count != 0) {
        xmlFree(prefixes[--count]);
    }
    free(prefixes);
}

elephc_dom_native_c14n_result elephc_dom_native_node_c14n(
    void *document,
    void *node,
    int32_t node_is_document,
    int32_t modern,
    int32_t exclusive,
    int32_t with_comments,
    int32_t has_xpath,
    const uint8_t *query,
    size_t query_length,
    const elephc_dom_native_bytes *namespace_prefixes,
    const elephc_dom_native_bytes *namespace_uris,
    size_t namespace_count,
    const elephc_dom_native_bytes *inclusive_prefixes,
    size_t inclusive_prefix_count,
    int32_t generic_errors
)
{
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    elephc_dom_generic_error_context generic = {
        &errors,
        NULL,
        0,
        0,
        NULL,
        NULL,
        0
    };
    elephc_dom_native_c14n_result result = {
        NULL,
        0,
        NULL,
        0,
        0,
        2
    };
    elephc_dom_validation_ns_guard guard = {NULL, 0, 0};
    elephc_dom_libxml_globals globals;
    xmlXPathContextPtr xpath_context = NULL;
    xmlXPathObjectPtr xpath_object = NULL;
    xmlNodeSetPtr nodeset = NULL;
    xmlOutputBufferPtr output = NULL;
    xmlChar **prefixes = NULL;
    xmlChar *query_copy = NULL;
    xmlDocPtr doc = document;
    xmlNodePtr target = node;
    size_t initialized_prefixes = 0;
    int canonicalized = -1;

    if (doc == NULL || target == NULL) {
        result.status = 3;
        return result;
    }
    if (modern != 0) {
        xmlNodePtr root = target;
        while (root->parent != NULL) {
            root = root->parent;
        }
        if (root->type != XML_DOCUMENT_NODE
            && root->type != XML_HTML_DOCUMENT_NODE) {
            result.status = 4;
            return result;
        }
    }

    xmlInitParser();
    globals = elephc_dom_sanitize_libxml_globals();
    if (generic_errors != 0) {
        elephc_dom_generic_error_context_install(&generic);
    } else {
        xmlSetStructuredErrorFunc(
            &errors,
            elephc_dom_capture_structured_error
        );
    }
    if (modern != 0
        && !elephc_dom_validation_ns_guard_begin(&guard, doc)) {
        errors.allocation_failed = 1;
        goto cleanup;
    }

    if (has_xpath != 0) {
        size_t index;

        query_copy = elephc_dom_c14n_string(query, query_length);
        xpath_context = xmlXPathNewContext(doc);
        if (query_copy == NULL || xpath_context == NULL) {
            errors.allocation_failed = 1;
            goto cleanup;
        }
        xpath_context->node = target;
        for (index = 0; index < namespace_count; index++) {
            xmlChar *prefix = elephc_dom_c14n_string(
                namespace_prefixes[index].pointer,
                namespace_prefixes[index].length
            );
            xmlChar *uri = elephc_dom_c14n_string(
                namespace_uris[index].pointer,
                namespace_uris[index].length
            );
            if (prefix == NULL || uri == NULL) {
                xmlFree(prefix);
                xmlFree(uri);
                errors.allocation_failed = 1;
                goto cleanup;
            }
            (void) xmlXPathRegisterNs(xpath_context, prefix, uri);
            xmlFree(prefix);
            xmlFree(uri);
        }
        xpath_object = xmlXPathEvalExpression(
            query_copy,
            xpath_context
        );
        xpath_context->node = NULL;
        if (xpath_object == NULL
            || xpath_object->type != XPATH_NODESET) {
            result.status = 1;
            goto cleanup;
        }
        nodeset = xpath_object->nodesetval;
    }

    if (exclusive != 0 && inclusive_prefix_count != 0) {
        size_t index;

        prefixes = calloc(
            inclusive_prefix_count + 1,
            sizeof(*prefixes)
        );
        if (prefixes == NULL) {
            errors.allocation_failed = 1;
            goto cleanup;
        }
        for (index = 0; index < inclusive_prefix_count; index++) {
            prefixes[index] = elephc_dom_c14n_string(
                inclusive_prefixes[index].pointer,
                inclusive_prefixes[index].length
            );
            if (prefixes[index] == NULL) {
                errors.allocation_failed = 1;
                goto cleanup;
            }
            initialized_prefixes++;
        }
    }

    output = xmlAllocOutputBuffer(NULL);
    if (output == NULL) {
        errors.allocation_failed = 1;
        goto cleanup;
    }
    if (has_xpath == 0 && node_is_document == 0) {
        canonicalized = xmlC14NExecute(
            doc,
            elephc_dom_c14n_parent_lookup,
            target,
            exclusive,
            prefixes,
            with_comments,
            output
        );
    } else {
        canonicalized = xmlC14NDocSaveTo(
            doc,
            nodeset,
            exclusive,
            prefixes,
            with_comments,
            output
        );
    }
    if (canonicalized < 0) {
        result.status = 2;
        goto cleanup;
    }
    result.length = xmlOutputBufferGetSize(output);
    if (result.length != 0) {
        const xmlChar *content = xmlOutputBufferGetContent(output);
        if (content == NULL) {
            errors.allocation_failed = 1;
            goto cleanup;
        }
        result.bytes = malloc(result.length);
        if (result.bytes == NULL) {
            errors.allocation_failed = 1;
            goto cleanup;
        }
        memcpy(result.bytes, content, result.length);
    }
    result.status = 0;

cleanup:
    if (output != NULL) {
        (void) xmlOutputBufferClose(output);
    }
    elephc_dom_c14n_prefixes_free(
        prefixes,
        initialized_prefixes
    );
    if (xpath_object != NULL) {
        xmlXPathFreeObject(xpath_object);
    }
    if (xpath_context != NULL) {
        xmlXPathFreeContext(xpath_context);
    }
    xmlFree(query_copy);
    if (guard.active != 0 || guard.links != NULL) {
        elephc_dom_validation_ns_guard_end(&guard);
    }
    if (generic_errors != 0) {
        elephc_dom_generic_error_context_clear(&generic);
    } else {
        xmlSetStructuredErrorFunc(NULL, NULL);
    }
    elephc_dom_restore_libxml_globals(globals);
    result.errors = errors.errors;
    result.error_count = errors.count;
    result.allocation_failed = errors.allocation_failed;
    return result;
}

static int elephc_dom_xpath_callback_lease_append(
    elephc_dom_xpath_callback_context *context,
    uint64_t lease
)
{
    uint64_t *resized;
    size_t capacity;

    if (context == NULL || lease == 0) {
        return 0;
    }
    if (context->lease_count == context->lease_capacity) {
        capacity = context->lease_capacity == 0
            ? 4
            : context->lease_capacity * 2;
        if (capacity < context->lease_capacity
            || capacity > SIZE_MAX / sizeof(*context->leases)) {
            return 0;
        }
        resized = realloc(
            context->leases,
            capacity * sizeof(*context->leases)
        );
        if (resized == NULL) {
            return 0;
        }
        context->leases = resized;
        context->lease_capacity = capacity;
    }
    context->leases[context->lease_count++] = lease;
    return 1;
}

static void elephc_dom_xpath_host_function(
    xmlXPathParserContextPtr parser_context,
    int argument_count
)
{
    elephc_dom_xpath_callback_context *callback_context;
    elephc_dom_xpath_callback_argument *arguments = NULL;
    xmlXPathObjectPtr *objects = NULL;
    xmlChar **converted_strings = NULL;
    elephc_dom_host_loader_result host_result = {NULL, 0, 0, 3, 0};
    xmlXPathObjectPtr returned = NULL;
    const xmlChar *function_uri;
    const xmlChar *function_name;
    int function_string = 0;
    uint32_t host_status;
    int index;

    if (parser_context == NULL
        || parser_context->context == NULL
        || argument_count < 0) {
        return;
    }
    callback_context = parser_context->context->userData;
    function_uri = parser_context->context->functionURI;
    function_name = parser_context->context->function;
    if (callback_context == NULL
        || function_uri == NULL
        || function_name == NULL) {
        xmlXPathErr(parser_context, XPATH_INVALID_CTXT);
        return;
    }
    function_string = xmlStrEqual(
        function_uri,
        BAD_CAST "http://php.net/xpath"
    ) && xmlStrEqual(function_name, BAD_CAST "functionString");
    if (argument_count != 0) {
        arguments = calloc(
            (size_t) argument_count,
            sizeof(*arguments)
        );
        objects = calloc((size_t) argument_count, sizeof(*objects));
        converted_strings = calloc(
            (size_t) argument_count,
            sizeof(*converted_strings)
        );
        if (arguments == NULL
            || objects == NULL
            || converted_strings == NULL) {
            free(arguments);
            free(objects);
            free(converted_strings);
            xmlXPathErr(parser_context, XPATH_MEMORY_ERROR);
            return;
        }
    }

    for (index = argument_count - 1; index >= 0; index--) {
        xmlXPathObjectPtr value = valuePop(parser_context);
        if (value == NULL) {
            callback_context->host_status = 3;
            goto cleanup;
        }
        objects[index] = value;
        switch (value->type) {
            case XPATH_UNDEFINED:
                arguments[index].kind = 0;
                break;
            case XPATH_BOOLEAN:
                arguments[index].kind = 1;
                arguments[index].boolean_value = value->boolval != 0;
                break;
            case XPATH_NUMBER:
                arguments[index].kind = 2;
                arguments[index].number = value->floatval;
                break;
            case XPATH_STRING:
                arguments[index].kind = 3;
                arguments[index].bytes = value->stringval;
                arguments[index].length = (size_t) xmlStrlen(value->stringval);
                break;
            case XPATH_NODESET:
                if (function_string != 0) {
                    converted_strings[index] =
                        xmlXPathCastToString(value);
                    if (converted_strings[index] == NULL) {
                        xmlXPathErr(
                            parser_context,
                            XPATH_MEMORY_ERROR
                        );
                        goto cleanup;
                    }
                    arguments[index].kind = 3;
                    arguments[index].bytes =
                        converted_strings[index];
                    arguments[index].length = (size_t) xmlStrlen(
                        converted_strings[index]
                    );
                } else {
                    arguments[index].kind = 4;
                    if (value->nodesetval != NULL) {
                        if (value->nodesetval->nodeNr < 0) {
                            callback_context->host_status = 3;
                            goto cleanup;
                        }
                        arguments[index].nodes =
                            (void **) value->nodesetval->nodeTab;
                        arguments[index].node_count =
                            (size_t) value->nodesetval->nodeNr;
                    }
                }
                break;
            default:
                callback_context->host_status = 3;
                goto cleanup;
        }
    }

    host_status = elephc_dom_host_xpath_invoke(
        callback_context->context_id,
        callback_context->xpath_handle,
        function_uri,
        (size_t) xmlStrlen(function_uri),
        function_name,
        (size_t) xmlStrlen(function_name),
        arguments,
        (size_t) argument_count,
        &host_result
    );
    if (host_status != 0
        || (host_result.kind != 1
            && host_result.kind != 4
            && host_result.kind != 5
            && host_result.kind != 6)) {
        callback_context->host_status = (int32_t) (
            host_status == 0 ? 3 : host_status
        );
        goto cleanup;
    }
    if (host_result.kind == 6) {
        uint8_t *copy;

        if (host_result.bytes == NULL
            || host_result.length == 0
            || (host_result.resource != 1
                && host_result.resource != 2)
            || host_result.reserved != 0) {
            if (host_result.bytes != NULL) {
                elephc_dom_host_loader_bytes_free(
                    host_result.bytes,
                    host_result.length
                );
                host_result.bytes = NULL;
            }
            callback_context->host_status = 3;
            goto cleanup;
        }
        copy = malloc(host_result.length);
        if (copy == NULL) {
            elephc_dom_host_loader_bytes_free(
                host_result.bytes,
                host_result.length
            );
            host_result.bytes = NULL;
            xmlXPathErr(parser_context, XPATH_MEMORY_ERROR);
            goto cleanup;
        }
        memcpy(copy, host_result.bytes, host_result.length);
        elephc_dom_host_loader_bytes_free(
            host_result.bytes,
            host_result.length
        );
        host_result.bytes = NULL;
        free(callback_context->error_message);
        callback_context->error_message = copy;
        callback_context->error_length = host_result.length;
        callback_context->error_kind =
            (int32_t) host_result.resource;
        goto cleanup;
    }
    if (host_result.kind == 5) {
        if (host_result.bytes == NULL
            || host_result.length == 0
            || host_result.resource == 0
            || host_result.reserved != 0) {
            if (host_result.length != 0) {
                (void) elephc_dom_host_result_release(
                    callback_context->context_id,
                    (uint64_t) host_result.length
                );
            }
            callback_context->host_status = 3;
            goto cleanup;
        }
        if (!elephc_dom_xpath_callback_lease_append(
                callback_context,
                (uint64_t) host_result.length
            )) {
            (void) elephc_dom_host_result_release(
                callback_context->context_id,
                (uint64_t) host_result.length
            );
            xmlXPathErr(parser_context, XPATH_MEMORY_ERROR);
            goto cleanup;
        }
        returned = xmlXPathNewNodeSet(
            (xmlNodePtr) host_result.bytes
        );
        if (returned == NULL) {
            xmlXPathErr(parser_context, XPATH_MEMORY_ERROR);
            goto cleanup;
        }
        valuePush(parser_context, returned);
        returned = NULL;
        goto cleanup;
    }
    if (host_result.kind == 4) {
        if (host_result.bytes != NULL
            || host_result.length != 0
            || host_result.resource > 1) {
            callback_context->host_status = 3;
            goto cleanup;
        }
        returned = xmlXPathNewBoolean(host_result.resource != 0);
        if (returned == NULL) {
            xmlXPathErr(parser_context, XPATH_MEMORY_ERROR);
            goto cleanup;
        }
        valuePush(parser_context, returned);
        returned = NULL;
        goto cleanup;
    }
    if (host_result.length > INT_MAX
        || (host_result.bytes == NULL && host_result.length != 0)) {
        callback_context->host_status = 3;
        goto cleanup;
    }
    if (host_result.length == 0) {
        returned = xmlXPathNewCString("");
    } else {
        xmlChar *copy = xmlStrndup(
            host_result.bytes,
            (int) host_result.length
        );
        if (copy != NULL) {
            returned = xmlXPathNewString(copy);
            xmlFree(copy);
        }
    }
    if (returned == NULL) {
        xmlXPathErr(parser_context, XPATH_MEMORY_ERROR);
        goto cleanup;
    }
    valuePush(parser_context, returned);
    returned = NULL;

cleanup:
    if (host_result.kind == 1 && host_result.bytes != NULL) {
        elephc_dom_host_loader_bytes_free(
            host_result.bytes,
            host_result.length
        );
    }
    if (objects != NULL) {
        for (index = 0; index < argument_count; index++) {
            if (objects[index] != NULL) {
                xmlXPathFreeObject(objects[index]);
            }
            xmlFree(converted_strings[index]);
        }
    }
    xmlXPathFreeObject(returned);
    free(objects);
    free(arguments);
    free(converted_strings);
    if (callback_context->host_status != 0
        || callback_context->error_message != NULL) {
        valuePush(parser_context, xmlXPathNewCString(""));
    }
}


/* Builds php-src's fake namespace-declaration node for one XPath namespace axis
 * result. The standalone allocation owns its own href/prefix copies and a child
 * text node carrying the namespace URI, so it survives libxml2 freeing the
 * duplicated XPath namespace entry. The bridge frees it via
 * elephc_dom_native_namespace_node_free when the DOMNameSpaceNode wrapper is
 * released. `parent` is the owning element retained by the document graph. */
static xmlNodePtr elephc_dom_create_fake_namespace_decl_node_ptr(
    xmlNodePtr parent,
    xmlNsPtr original
)
{
    xmlNsPtr curns;
    xmlNodePtr attrp;

    if (parent == NULL || parent->doc == NULL || original == NULL) {
        return NULL;
    }
    curns = xmlNewNs(NULL, original->href, NULL);
    if (curns == NULL) {
        return NULL;
    }
    if (original->prefix != NULL) {
        curns->prefix = xmlStrdup(original->prefix);
        if (curns->prefix == NULL) {
            xmlFreeNs(curns);
            return NULL;
        }
        attrp = xmlNewDocNode(
            parent->doc,
            NULL,
            BAD_CAST original->prefix,
            original->href
        );
    } else {
        attrp = xmlNewDocNode(
            parent->doc,
            NULL,
            BAD_CAST "xmlns",
            original->href
        );
    }
    if (attrp == NULL) {
        xmlFreeNs(curns);
        return NULL;
    }
    attrp->type = XML_NAMESPACE_DECL;
    attrp->parent = parent;
    attrp->ns = curns;
    return attrp;
}

/* Frees one standalone fake namespace-declaration node created by
 * elephc_dom_create_fake_namespace_decl_node_ptr. The node->ns binding is a
 * standalone xmlNs that xmlFreeNode would not release, so it is freed first.
 * The fake node keeps type XML_NAMESPACE_DECL, which would make xmlFreeNode
 * reinterpret it as an xmlNs and corrupt its children pointer, so the type is
 * switched back to XML_ELEMENT_NODE (matching php-src's libxml free path) to
 * let xmlFreeNode release the node, its name, and its child text node. */
void elephc_dom_native_namespace_node_free(void *node)
{
    xmlNodePtr attrp = (xmlNodePtr) node;
    xmlNsPtr curns;

    if (attrp == NULL || attrp->type != XML_NAMESPACE_DECL) {
        return;
    }
    curns = attrp->ns;
    if (curns != NULL) {
        attrp->ns = NULL;
        xmlFreeNs(curns);
    }
    attrp->parent = NULL;
    attrp->type = XML_ELEMENT_NODE;
    xmlFreeNode(attrp);
}

/* Clones one fake namespace-declaration node into a fresh standalone allocation
 * retaining the same parent element and namespace binding. Returns NULL when the
 * source is not a namespace declaration or allocation fails. */
void *elephc_dom_native_namespace_node_clone(void *node)
{
    xmlNodePtr attrp = (xmlNodePtr) node;

    if (attrp == NULL || attrp->type != XML_NAMESPACE_DECL) {
        return NULL;
    }
    return elephc_dom_create_fake_namespace_decl_node_ptr(
        attrp->parent,
        attrp->ns
    );
}

/* Returns the PHP node name for one standalone fake namespace-declaration node:
 * "xmlns:" + prefix when the namespace is prefixed, otherwise the node name
 * ("xmlns"). The result is a freshly xmlMalloc'd buffer the caller frees with
 * elephc_dom_native_buffer_free. */
elephc_dom_native_buffer elephc_dom_native_namespace_node_name(void *node)
{
    xmlNodePtr attrp = (xmlNodePtr) node;
    elephc_dom_native_buffer result = { NULL, 0 };

    if (attrp == NULL || attrp->type != XML_NAMESPACE_DECL) {
        return result;
    }
    if (attrp->ns != NULL && attrp->ns->prefix != NULL) {
        const xmlChar *prefix = attrp->ns->prefix;
        size_t prefix_len = xmlStrlen(prefix);
        size_t total = 5 + 1 + prefix_len;
        xmlChar *out = xmlMalloc(total + 1);
        if (out == NULL) {
            return result;
        }
        memcpy(out, BAD_CAST "xmlns", 5);
        out[5] = ':';
        memcpy(out + 6, prefix, prefix_len);
        out[total] = 0;
        result.pointer = out;
        result.length = total;
    } else if (attrp->name != NULL) {
        result.pointer = xmlStrdup(attrp->name);
        if (result.pointer != NULL) {
            result.length = xmlStrlen(result.pointer);
        }
    }
    return result;
}

/* Returns the PHP node value for one standalone fake namespace-declaration node:
 * the text content of its child text node (the namespace URI), or an empty
 * buffer when the node has no children. The result is a freshly allocated buffer
 * owned by the caller. */
elephc_dom_native_buffer elephc_dom_native_namespace_node_value(void *node)
{
    xmlNodePtr attrp = (xmlNodePtr) node;
    elephc_dom_native_buffer result = { NULL, 0 };

    if (attrp == NULL || attrp->type != XML_NAMESPACE_DECL) {
        return result;
    }
    xmlChar *content = xmlNodeGetContent(attrp->children);
    if (content != NULL) {
        result.pointer = content;
        result.length = xmlStrlen(content);
    }
    return result;
}

/* Returns the PHP local name for one standalone fake namespace-declaration node:
 * the node name (the prefix, or "xmlns" for the default namespace). The buffer is
 * borrowed from the live node and must not be freed by the caller. */
elephc_dom_native_buffer elephc_dom_native_namespace_node_local_name(void *node)
{
    xmlNodePtr attrp = (xmlNodePtr) node;
    elephc_dom_native_buffer result = { NULL, 0 };

    if (attrp != NULL
        && attrp->type == XML_NAMESPACE_DECL
        && attrp->name != NULL) {
        result.pointer = (uint8_t *) attrp->name;
        result.length = xmlStrlen(attrp->name);
    }
    return result;
}
elephc_dom_native_xpath_result elephc_dom_native_xpath_evaluate(
    void *document,
    void *node,
    int32_t modern,
    int32_t register_node_namespaces,
    int32_t force_nodeset,
    const uint8_t *expression,
    size_t expression_length,
    const elephc_dom_native_bytes *namespace_prefixes,
    const elephc_dom_native_bytes *namespace_uris,
    size_t namespace_count,
    uint64_t host_context,
    uint64_t xpath_handle,
    const elephc_dom_native_bytes *callback_namespaces,
    const elephc_dom_native_bytes *callback_names,
    size_t callback_count
)
{
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    elephc_dom_generic_error_context generic = {
        &errors,
        NULL,
        0,
        0,
        NULL,
        NULL,
        0
    };
    elephc_dom_native_xpath_result result = {
        NULL,
        0,
        NULL,
        0,
        NULL,
        0,
        NULL,
        0,
        0.0,
        0,
        0,
        0,
        1,
        0
    };
    elephc_dom_validation_ns_guard guard = {NULL, 0, 0};
    elephc_dom_libxml_globals globals;
    xmlDocPtr doc = document;
    xmlNodePtr context_node = node;
    xmlXPathContextPtr xpath_context = NULL;
    xmlXPathObjectPtr xpath_object = NULL;
    xmlNsPtr *in_scope_namespaces = NULL;
    xmlChar *expression_copy = NULL;
    elephc_dom_xpath_callback_context callback_context = {
        host_context,
        xpath_handle,
        0,
        NULL,
        0,
        0,
        NULL,
        0,
        0
    };
    size_t index;

    if (doc == NULL
        || (expression == NULL && expression_length != 0)
        || (namespace_count != 0
            && (namespace_prefixes == NULL || namespace_uris == NULL))
        || (callback_count != 0
            && (callback_namespaces == NULL || callback_names == NULL))) {
        return result;
    }
    if (context_node == NULL) {
        context_node = xmlDocGetRootElement(doc);
    }
    if (context_node != NULL
        && context_node != (xmlNodePtr) doc
        && context_node->doc != doc) {
        result.status = 2;
        return result;
    }

    xmlInitParser();
    globals = elephc_dom_sanitize_libxml_globals();
    elephc_dom_generic_error_context_install(&generic);
    if (modern != 0
        && !elephc_dom_validation_ns_guard_begin(&guard, doc)) {
        errors.allocation_failed = 1;
        goto cleanup;
    }

    expression_copy = elephc_dom_c14n_string(
        expression,
        expression_length
    );
    xpath_context = xmlXPathNewContext(doc);
    if (expression_copy == NULL || xpath_context == NULL) {
        errors.allocation_failed = 1;
        goto cleanup;
    }
    for (index = 0; index < namespace_count; index++) {
        xmlChar *prefix = elephc_dom_c14n_string(
            namespace_prefixes[index].pointer,
            namespace_prefixes[index].length
        );
        xmlChar *uri = elephc_dom_c14n_string(
            namespace_uris[index].pointer,
            namespace_uris[index].length
        );
        if (prefix == NULL || uri == NULL) {
            xmlFree(prefix);
            xmlFree(uri);
            errors.allocation_failed = 1;
            goto cleanup;
        }
        (void) xmlXPathRegisterNs(xpath_context, prefix, uri);
        xmlFree(prefix);
        xmlFree(uri);
    }
    for (index = 0; index < callback_count; index++) {
        xmlChar *namespace_uri = elephc_dom_c14n_string(
            callback_namespaces[index].pointer,
            callback_namespaces[index].length
        );
        xmlChar *name = elephc_dom_c14n_string(
            callback_names[index].pointer,
            callback_names[index].length
        );
        if (namespace_uri == NULL
            || name == NULL
            || xmlXPathRegisterFuncNS(
                xpath_context,
                name,
                namespace_uri,
                elephc_dom_xpath_host_function
            ) != 0) {
            xmlFree(namespace_uri);
            xmlFree(name);
            errors.allocation_failed = 1;
            goto cleanup;
        }
        xmlFree(namespace_uri);
        xmlFree(name);
    }

    xpath_context->node = context_node;
    xpath_context->userData = callback_count == 0
        ? NULL
        : &callback_context;
    if (register_node_namespaces != 0 && context_node != NULL) {
        in_scope_namespaces = xmlGetNsList(doc, context_node);
        xpath_context->namespaces = in_scope_namespaces;
        if (in_scope_namespaces != NULL) {
            while (in_scope_namespaces[xpath_context->nsNr] != NULL) {
                xpath_context->nsNr++;
            }
        }
    }
    xpath_object = xmlXPathEvalExpression(
        expression_copy,
        xpath_context
    );
    xpath_context->node = NULL;
    xpath_context->userData = NULL;
    xpath_context->namespaces = NULL;
    xpath_context->nsNr = 0;
    result.host_status = callback_context.host_status;
    if (result.host_status != 0) {
        result.status = 6;
        goto cleanup;
    }
    if (callback_context.error_message != NULL) {
        result.bytes = callback_context.error_message;
        result.byte_count = callback_context.error_length;
        callback_context.error_message = NULL;
        result.status = callback_context.error_kind == 2 ? 8 : 7;
        goto cleanup;
    }
    if (xpath_object == NULL) {
        result.status = 3;
        goto cleanup;
    }

    if (force_nodeset != 0 || xpath_object->type == XPATH_NODESET) {
        xmlNodeSetPtr nodeset = force_nodeset == 0
            || xpath_object->type == XPATH_NODESET
                ? xpath_object->nodesetval
                : NULL;
        result.kind = 1;
        if (nodeset != NULL && nodeset->nodeNr > 0) {
            result.pointer_count = (size_t) nodeset->nodeNr;
            if (result.pointer_count > SIZE_MAX / sizeof(*result.pointers)) {
                errors.allocation_failed = 1;
                goto cleanup;
            }
            result.pointers = malloc(
                result.pointer_count * sizeof(*result.pointers)
            );
            if (result.pointers == NULL) {
                errors.allocation_failed = 1;
                goto cleanup;
            }
            for (index = 0; index < result.pointer_count; index++) {
                xmlNodePtr item = nodeset->nodeTab[index];
                if (modern != 0 && item->type == XML_NAMESPACE_DECL) {
                    result.status = 4;
                    goto cleanup;
                }
                if (modern == 0 && item->type == XML_NAMESPACE_DECL) {
                    xmlNsPtr original = (xmlNsPtr) item;
                    xmlNodePtr nsparent = (xmlNodePtr) original->next;
                    xmlNodePtr fake =
                        elephc_dom_create_fake_namespace_decl_node_ptr(
                            nsparent,
                            original
                        );
                    if (fake == NULL) {
                        errors.allocation_failed = 1;
                        goto cleanup;
                    }
                    result.pointers[index] = fake;
                } else {
                    result.pointers[index] = item;
                }
            }
        }
    } else if (xpath_object->type == XPATH_BOOLEAN) {
        result.kind = 2;
        result.boolean_value = xpath_object->boolval != 0;
    } else if (xpath_object->type == XPATH_NUMBER) {
        result.kind = 3;
        result.number = xpath_object->floatval;
    } else if (xpath_object->type == XPATH_STRING) {
        result.kind = 4;
        result.byte_count = xmlStrlen(xpath_object->stringval);
        if (result.byte_count != 0) {
            result.bytes = malloc(result.byte_count);
            if (result.bytes == NULL) {
                errors.allocation_failed = 1;
                goto cleanup;
            }
            memcpy(
                result.bytes,
                xpath_object->stringval,
                result.byte_count
            );
        }
    } else {
        result.kind = 5;
    }
    result.status = 0;

cleanup:
    xmlFree(in_scope_namespaces);
    if (xpath_object != NULL) {
        xmlXPathFreeObject(xpath_object);
    }
    if (xpath_context != NULL) {
        xmlXPathFreeContext(xpath_context);
    }
    xmlFree(expression_copy);
    if (guard.active != 0 || guard.links != NULL) {
        elephc_dom_validation_ns_guard_end(&guard);
    }
    elephc_dom_generic_error_context_clear(&generic);
    elephc_dom_restore_libxml_globals(globals);
    result.errors = errors.errors;
    result.error_count = errors.count;
    result.callback_leases = callback_context.leases;
    result.callback_lease_count = callback_context.lease_count;
    free(callback_context.error_message);
    result.allocation_failed = errors.allocation_failed;
    return result;
}

elephc_dom_native_validation_result elephc_dom_native_document_validate(
    void *document
)
{
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    elephc_dom_validation_ns_guard guard;
    elephc_dom_libxml_globals globals;
    xmlValidCtxtPtr validator;
    int valid;

    if (document == NULL) {
        return elephc_dom_validation_result_finish(&errors, 0, -1);
    }
    xmlInitParser();
    globals = elephc_dom_sanitize_libxml_globals();
    validator = xmlNewValidCtxt();
    if (validator == NULL) {
        elephc_dom_restore_libxml_globals(globals);
        return elephc_dom_validation_result_finish(&errors, 0, 2);
    }
    xmlSetStructuredErrorFunc(&errors, elephc_dom_capture_structured_error);
    if (!elephc_dom_validation_ns_guard_begin(
            &guard,
            (xmlDocPtr) document
        )) {
        errors.allocation_failed = 1;
        valid = 0;
    } else {
        valid = xmlValidateDocument(validator, (xmlDocPtr) document);
        elephc_dom_validation_ns_guard_end(&guard);
    }
    xmlSetStructuredErrorFunc(NULL, NULL);
    xmlFreeValidCtxt(validator);
    elephc_dom_restore_libxml_globals(globals);
    return elephc_dom_validation_result_finish(
        &errors,
        valid != 0,
        0
    );
}

elephc_dom_native_validation_result
elephc_dom_native_document_schema_validate_source(
    void *document,
    const uint8_t *source,
    size_t source_length,
    int32_t flags,
    int32_t generic_errors,
    uint64_t host_context
)
{
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    elephc_dom_generic_error_context generic = {
        &errors,
        NULL,
        0,
        0,
        NULL,
        NULL,
        0
    };
    elephc_dom_validation_ns_guard guard;
    elephc_dom_libxml_globals globals;
    elephc_dom_resource_loader_context loader = {
        host_context,
        NULL,
        0
    };
    elephc_dom_native_validation_result result;
    xmlSchemaParserCtxtPtr parser;
    xmlSchemaPtr schema;
    xmlSchemaValidCtxtPtr validator;
    int valid;

    if (document == NULL || (source == NULL && source_length != 0)
        || source_length > INT_MAX) {
        return elephc_dom_validation_result_finish(&errors, 0, -1);
    }
    xmlInitParser();
    globals = elephc_dom_sanitize_libxml_globals();
    parser = xmlSchemaNewMemParserCtxt(
        (const char *) source,
        (int) source_length
    );
    if (parser == NULL) {
        elephc_dom_restore_libxml_globals(globals);
        return elephc_dom_validation_result_finish(&errors, 0, 2);
    }
    if (generic_errors != 0) {
        xmlSchemaSetParserErrors(
            parser,
            elephc_dom_capture_generic_error,
            elephc_dom_capture_generic_error,
            &generic
        );
    } else {
        xmlSchemaSetParserStructuredErrors(
            parser,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    if (host_context != 0) {
        xmlSchemaSetResourceLoader(
            parser,
            elephc_dom_resource_loader,
            &loader
        );
    }
    if (generic_errors != 0) {
        elephc_dom_generic_error_context_install(&generic);
    }
    schema = xmlSchemaParse(parser);
    xmlSchemaFreeParserCtxt(parser);
    elephc_dom_restore_libxml_globals(globals);
    if (schema == NULL) {
        elephc_dom_generic_error_context_clear(&generic);
        result = elephc_dom_validation_result_finish(&errors, 0, 1);
        result.host_status = loader.host_status;
        return result;
    }

    validator = xmlSchemaNewValidCtxt(schema);
    if (validator == NULL) {
        xmlSchemaFree(schema);
        elephc_dom_generic_error_context_clear(&generic);
        result = elephc_dom_validation_result_finish(&errors, 0, 2);
        result.host_status = loader.host_status;
        return result;
    }
    (void) xmlSchemaSetValidOptions(
        validator,
        flags & XML_SCHEMA_VAL_VC_I_CREATE
    );
    if (generic_errors != 0) {
        xmlSchemaSetValidErrors(
            validator,
            elephc_dom_capture_generic_error,
            elephc_dom_capture_generic_error,
            &generic
        );
    } else {
        xmlSchemaSetValidStructuredErrors(
            validator,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    globals = elephc_dom_sanitize_libxml_globals();
    if (!elephc_dom_validation_ns_guard_begin(
            &guard,
            (xmlDocPtr) document
        )) {
        errors.allocation_failed = 1;
        valid = 1;
    } else {
        valid = xmlSchemaValidateDoc(validator, (xmlDocPtr) document);
        elephc_dom_validation_ns_guard_end(&guard);
    }
    elephc_dom_restore_libxml_globals(globals);
    xmlSchemaFree(schema);
    xmlSchemaFreeValidCtxt(validator);
    elephc_dom_generic_error_context_clear(&generic);
    result = elephc_dom_validation_result_finish(
        &errors,
        valid == 0,
        0
    );
    result.host_status = loader.host_status;
    return result;
}

elephc_dom_native_validation_result
elephc_dom_native_document_schema_validate_file(
    void *document,
    const uint8_t *path,
    size_t path_length,
    int32_t flags,
    int32_t generic_errors,
    uint64_t host_context
)
{
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    elephc_dom_generic_error_context generic = {
        &errors,
        NULL,
        0,
        0,
        NULL,
        NULL,
        0
    };
    elephc_dom_validation_ns_guard guard;
    elephc_dom_libxml_globals globals;
    elephc_dom_resource_loader_context loader = {
        host_context,
        NULL,
        0
    };
    elephc_dom_native_validation_result result;
    xmlSchemaParserCtxtPtr parser;
    xmlSchemaPtr schema;
    xmlSchemaValidCtxtPtr validator;
    char *path_string;
    int valid;

    if (document == NULL || path == NULL || path_length == 0
        || memchr(path, '\0', path_length) != NULL) {
        return elephc_dom_validation_result_finish(&errors, 0, -1);
    }
    if (elephc_dom_validation_local_path_too_long(path, path_length)) {
        return elephc_dom_validation_result_finish(&errors, 0, 3);
    }
    path_string = elephc_dom_copy_c_string(path, path_length);
    if (path_string == NULL) {
        errors.allocation_failed = 1;
        return elephc_dom_validation_result_finish(&errors, 0, 0);
    }
    xmlInitParser();
    globals = elephc_dom_sanitize_libxml_globals();
    parser = xmlSchemaNewParserCtxt(path_string);
    free(path_string);
    if (parser == NULL) {
        elephc_dom_restore_libxml_globals(globals);
        return elephc_dom_validation_result_finish(&errors, 0, 2);
    }
    if (generic_errors != 0) {
        xmlSchemaSetParserErrors(
            parser,
            elephc_dom_capture_generic_error,
            elephc_dom_capture_generic_error,
            &generic
        );
    } else {
        xmlSchemaSetParserStructuredErrors(
            parser,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    if (host_context != 0) {
        xmlSchemaSetResourceLoader(
            parser,
            elephc_dom_resource_loader,
            &loader
        );
    }
    if (generic_errors != 0) {
        elephc_dom_generic_error_context_install(&generic);
    }
    schema = xmlSchemaParse(parser);
    xmlSchemaFreeParserCtxt(parser);
    elephc_dom_restore_libxml_globals(globals);
    if (schema == NULL) {
        elephc_dom_generic_error_context_clear(&generic);
        result = elephc_dom_validation_result_finish(&errors, 0, 1);
        result.host_status = loader.host_status;
        return result;
    }

    validator = xmlSchemaNewValidCtxt(schema);
    if (validator == NULL) {
        xmlSchemaFree(schema);
        elephc_dom_generic_error_context_clear(&generic);
        result = elephc_dom_validation_result_finish(&errors, 0, 2);
        result.host_status = loader.host_status;
        return result;
    }
    (void) xmlSchemaSetValidOptions(
        validator,
        flags & XML_SCHEMA_VAL_VC_I_CREATE
    );
    if (generic_errors != 0) {
        xmlSchemaSetValidErrors(
            validator,
            elephc_dom_capture_generic_error,
            elephc_dom_capture_generic_error,
            &generic
        );
    } else {
        xmlSchemaSetValidStructuredErrors(
            validator,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    globals = elephc_dom_sanitize_libxml_globals();
    if (!elephc_dom_validation_ns_guard_begin(
            &guard,
            (xmlDocPtr) document
        )) {
        errors.allocation_failed = 1;
        valid = 1;
    } else {
        valid = xmlSchemaValidateDoc(validator, (xmlDocPtr) document);
        elephc_dom_validation_ns_guard_end(&guard);
    }
    elephc_dom_restore_libxml_globals(globals);
    xmlSchemaFree(schema);
    xmlSchemaFreeValidCtxt(validator);
    elephc_dom_generic_error_context_clear(&generic);
    result = elephc_dom_validation_result_finish(
        &errors,
        valid == 0,
        0
    );
    result.host_status = loader.host_status;
    return result;
}

elephc_dom_native_validation_result
elephc_dom_native_document_relaxng_validate_source(
    void *document,
    const uint8_t *source,
    size_t source_length,
    int32_t generic_errors,
    uint64_t host_context
)
{
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    elephc_dom_generic_error_context generic = {
        &errors,
        NULL,
        0,
        0,
        NULL,
        NULL,
        0
    };
    elephc_dom_validation_ns_guard guard;
    elephc_dom_libxml_globals globals;
    elephc_dom_resource_loader_context loader = {
        host_context,
        NULL,
        0
    };
    elephc_dom_native_validation_result result;
    xmlRelaxNGParserCtxtPtr parser;
    xmlRelaxNGPtr schema;
    xmlRelaxNGValidCtxtPtr validator;
    int valid;

    if (document == NULL || (source == NULL && source_length != 0)
        || source_length > INT_MAX) {
        return elephc_dom_validation_result_finish(&errors, 0, -1);
    }
    xmlInitParser();
    globals = elephc_dom_sanitize_libxml_globals();
    parser = xmlRelaxNGNewMemParserCtxt(
        (const char *) source,
        (int) source_length
    );
    if (parser == NULL) {
        elephc_dom_restore_libxml_globals(globals);
        return elephc_dom_validation_result_finish(&errors, 0, 2);
    }
    if (generic_errors != 0) {
        xmlRelaxNGSetParserErrors(
            parser,
            elephc_dom_capture_generic_error,
            elephc_dom_capture_generic_error,
            &generic
        );
    } else {
        xmlRelaxNGSetParserStructuredErrors(
            parser,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    if (host_context != 0) {
        xmlRelaxNGSetResourceLoader(
            parser,
            elephc_dom_resource_loader,
            &loader
        );
    }
    if (generic_errors != 0) {
        elephc_dom_generic_error_context_install(&generic);
    }
    schema = xmlRelaxNGParse(parser);
    xmlRelaxNGFreeParserCtxt(parser);
    elephc_dom_restore_libxml_globals(globals);
    if (schema == NULL) {
        elephc_dom_generic_error_context_clear(&generic);
        result = elephc_dom_validation_result_finish(&errors, 0, 1);
        result.host_status = loader.host_status;
        return result;
    }

    validator = xmlRelaxNGNewValidCtxt(schema);
    if (validator == NULL) {
        xmlRelaxNGFree(schema);
        elephc_dom_generic_error_context_clear(&generic);
        result = elephc_dom_validation_result_finish(&errors, 0, 2);
        result.host_status = loader.host_status;
        return result;
    }
    if (generic_errors != 0) {
        xmlRelaxNGSetValidErrors(
            validator,
            elephc_dom_capture_generic_error,
            elephc_dom_capture_generic_error,
            &generic
        );
    } else {
        xmlRelaxNGSetValidStructuredErrors(
            validator,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    if (!elephc_dom_validation_ns_guard_begin(
            &guard,
            (xmlDocPtr) document
        )) {
        errors.allocation_failed = 1;
        valid = 1;
    } else {
        valid = xmlRelaxNGValidateDoc(validator, (xmlDocPtr) document);
        elephc_dom_validation_ns_guard_end(&guard);
    }
    xmlRelaxNGFree(schema);
    xmlRelaxNGFreeValidCtxt(validator);
    elephc_dom_generic_error_context_clear(&generic);
    result = elephc_dom_validation_result_finish(
        &errors,
        valid == 0,
        0
    );
    result.host_status = loader.host_status;
    return result;
}

elephc_dom_native_validation_result
elephc_dom_native_document_relaxng_validate_file(
    void *document,
    const uint8_t *path,
    size_t path_length,
    int32_t generic_errors,
    uint64_t host_context
)
{
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    elephc_dom_generic_error_context generic = {
        &errors,
        NULL,
        0,
        0,
        NULL,
        NULL,
        0
    };
    elephc_dom_validation_ns_guard guard;
    elephc_dom_libxml_globals globals;
    elephc_dom_resource_loader_context loader = {
        host_context,
        NULL,
        0
    };
    elephc_dom_native_validation_result result;
    xmlRelaxNGParserCtxtPtr parser;
    xmlRelaxNGPtr schema;
    xmlRelaxNGValidCtxtPtr validator;
    char *path_string;
    int valid;

    if (document == NULL || path == NULL || path_length == 0
        || memchr(path, '\0', path_length) != NULL) {
        return elephc_dom_validation_result_finish(&errors, 0, -1);
    }
    if (elephc_dom_validation_local_path_too_long(path, path_length)) {
        return elephc_dom_validation_result_finish(&errors, 0, 3);
    }
    path_string = elephc_dom_copy_c_string(path, path_length);
    if (path_string == NULL) {
        errors.allocation_failed = 1;
        return elephc_dom_validation_result_finish(&errors, 0, 0);
    }
    xmlInitParser();
    parser = xmlRelaxNGNewParserCtxt(path_string);
    free(path_string);
    if (parser == NULL) {
        return elephc_dom_validation_result_finish(&errors, 0, 2);
    }
    if (generic_errors != 0) {
        xmlRelaxNGSetParserErrors(
            parser,
            elephc_dom_capture_generic_error,
            elephc_dom_capture_generic_error,
            &generic
        );
    } else {
        xmlRelaxNGSetParserStructuredErrors(
            parser,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    if (host_context != 0) {
        xmlRelaxNGSetResourceLoader(
            parser,
            elephc_dom_resource_loader,
            &loader
        );
    }
    if (generic_errors != 0) {
        elephc_dom_generic_error_context_install(&generic);
    }
    globals = elephc_dom_sanitize_libxml_globals();
    schema = xmlRelaxNGParse(parser);
    xmlRelaxNGFreeParserCtxt(parser);
    elephc_dom_restore_libxml_globals(globals);
    if (schema == NULL) {
        elephc_dom_generic_error_context_clear(&generic);
        result = elephc_dom_validation_result_finish(&errors, 0, 1);
        result.host_status = loader.host_status;
        return result;
    }

    validator = xmlRelaxNGNewValidCtxt(schema);
    if (validator == NULL) {
        xmlRelaxNGFree(schema);
        elephc_dom_generic_error_context_clear(&generic);
        result = elephc_dom_validation_result_finish(&errors, 0, 2);
        result.host_status = loader.host_status;
        return result;
    }
    if (generic_errors != 0) {
        xmlRelaxNGSetValidErrors(
            validator,
            elephc_dom_capture_generic_error,
            elephc_dom_capture_generic_error,
            &generic
        );
    } else {
        xmlRelaxNGSetValidStructuredErrors(
            validator,
            elephc_dom_capture_structured_error,
            &errors
        );
    }
    if (!elephc_dom_validation_ns_guard_begin(
            &guard,
            (xmlDocPtr) document
        )) {
        errors.allocation_failed = 1;
        valid = 1;
    } else {
        valid = xmlRelaxNGValidateDoc(validator, (xmlDocPtr) document);
        elephc_dom_validation_ns_guard_end(&guard);
    }
    xmlRelaxNGFree(schema);
    xmlRelaxNGFreeValidCtxt(validator);
    elephc_dom_generic_error_context_clear(&generic);
    result = elephc_dom_validation_result_finish(
        &errors,
        valid == 0,
        0
    );
    result.host_status = loader.host_status;
    return result;
}

void elephc_dom_native_parse_result_free(
    elephc_dom_native_error *errors,
    size_t error_count
)
{
    if (errors == NULL) {
        return;
    }
    while (error_count != 0) {
        elephc_dom_native_error_free(&errors[--error_count]);
    }
    free(errors);
}

void elephc_dom_native_xinclude_result_free(
    elephc_dom_native_error *errors,
    size_t error_count,
    void **invalidated
)
{
    elephc_dom_native_parse_result_free(errors, error_count);
    free(invalidated);
}

void elephc_dom_native_c14n_result_free(
    uint8_t *bytes,
    elephc_dom_native_error *errors,
    size_t error_count
)
{
    free(bytes);
    elephc_dom_native_parse_result_free(errors, error_count);
}

void elephc_dom_native_xpath_result_free(
    void **pointers,
    uint8_t *bytes,
    elephc_dom_native_error *errors,
    size_t error_count,
    uint64_t *callback_leases
)
{
    free(pointers);
    free(bytes);
    free(callback_leases);
    elephc_dom_native_parse_result_free(errors, error_count);
}

static xmlNodePtr elephc_dom_template_fragment(xmlNodePtr element)
{
    xmlNodePtr fragment;

    if (element == NULL || element->type != XML_ELEMENT_NODE
        || element->_private == NULL) {
        return NULL;
    }
    fragment = (xmlNodePtr) element->_private;
    return fragment->type == XML_DOCUMENT_FRAG_NODE
        && fragment->parent == element
        && fragment->doc == element->doc
            ? fragment
            : NULL;
}

static void elephc_dom_free_template_fragments(xmlNodePtr node)
{
    while (node != NULL) {
        xmlNodePtr fragment;

        if (node->children != NULL) {
            elephc_dom_free_template_fragments(node->children);
        }
        fragment = elephc_dom_template_fragment(node);
        if (fragment != NULL) {
            elephc_dom_free_template_fragments(fragment->children);
            node->_private = NULL;
            xmlFreeNode(fragment);
        }
        node = node->next;
    }
}

void elephc_dom_native_document_free(void *document)
{
    if (document != NULL) {
        xmlDocPtr native_document = (xmlDocPtr) document;

        elephc_dom_free_template_fragments(native_document->children);
        xmlFreeDoc(native_document);
    }
}

static int elephc_dom_is_html_void_element(const xmlNode *element)
{
    const xmlChar *name = element->name;

    return xmlStrEqual(name, (const xmlChar *) "area")
        || xmlStrEqual(name, (const xmlChar *) "base")
        || xmlStrEqual(name, (const xmlChar *) "basefont")
        || xmlStrEqual(name, (const xmlChar *) "bgsound")
        || xmlStrEqual(name, (const xmlChar *) "br")
        || xmlStrEqual(name, (const xmlChar *) "col")
        || xmlStrEqual(name, (const xmlChar *) "embed")
        || xmlStrEqual(name, (const xmlChar *) "frame")
        || xmlStrEqual(name, (const xmlChar *) "hr")
        || xmlStrEqual(name, (const xmlChar *) "img")
        || xmlStrEqual(name, (const xmlChar *) "input")
        || xmlStrEqual(name, (const xmlChar *) "keygen")
        || xmlStrEqual(name, (const xmlChar *) "link")
        || xmlStrEqual(name, (const xmlChar *) "menuitem")
        || xmlStrEqual(name, (const xmlChar *) "meta")
        || xmlStrEqual(name, (const xmlChar *) "param")
        || xmlStrEqual(name, (const xmlChar *) "source")
        || xmlStrEqual(name, (const xmlChar *) "track")
        || xmlStrEqual(name, (const xmlChar *) "wbr");
}

static void elephc_dom_remove_serialization_markers(
    xmlNodePtr *markers,
    size_t count
)
{
    while (count != 0) {
        xmlNodePtr marker = markers[--count];
        xmlUnlinkNode(marker);
        xmlFreeNode(marker);
    }
    free(markers);
}

static xmlNodePtr *elephc_dom_add_serialization_markers(
    xmlDocPtr document,
    size_t *marker_count
)
{
    static const xmlChar html_namespace[] =
        "http://www.w3.org/1999/xhtml";
    xmlNodePtr *markers = NULL;
    size_t count = 0;
    size_t capacity = 0;
    xmlNodePtr root = (xmlNodePtr) document;
    xmlNodePtr current = document->children;

    *marker_count = 0;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE
            && current->children == NULL
            && current->ns != NULL
            && xmlStrEqual(current->ns->href, html_namespace)
            && !elephc_dom_is_html_void_element(current)) {
            if (count == capacity) {
                size_t new_capacity = capacity == 0 ? 4 : capacity * 2;
                xmlNodePtr *resized;

                if (new_capacity < capacity
                    || new_capacity > SIZE_MAX / sizeof(*markers)) {
                    elephc_dom_remove_serialization_markers(markers, count);
                    return NULL;
                }
                resized = realloc(
                    markers,
                    new_capacity * sizeof(*markers)
                );
                if (resized == NULL) {
                    elephc_dom_remove_serialization_markers(markers, count);
                    return NULL;
                }
                markers = resized;
                capacity = new_capacity;
            }
            markers[count] = xmlNewDocTextLen(
                document,
                (const xmlChar *) "",
                0
            );
            if (markers[count] == NULL
                || xmlAddChild(current, markers[count]) == NULL) {
                if (markers[count] != NULL) {
                    xmlFreeNode(markers[count]);
                }
                elephc_dom_remove_serialization_markers(markers, count);
                return NULL;
            }
            count++;
        }
        current = elephc_dom_next_descendant(current, root);
    }
    *marker_count = count;
    return markers;
}

typedef struct {
    xmlNodePtr *nodes;
    size_t count;
    xmlChar *attribute_name;
    char *serialized_attribute;
    size_t serialized_length;
} elephc_dom_html_void_markers;

typedef struct {
    xmlNodePtr element;
    xmlNodePtr fragment;
    xmlNodePtr fragment_children;
    xmlNodePtr fragment_last;
    xmlNodePtr element_children;
    xmlNodePtr element_last;
} elephc_dom_template_marker;

typedef struct {
    xmlNodePtr node;
    xmlNsPtr namespace;
} elephc_dom_root_namespace_marker;

static int32_t elephc_dom_add_root_namespace_marker(
    xmlNodePtr node,
    elephc_dom_root_namespace_marker *marker
)
{
    xmlNsPtr namespace;

    marker->node = NULL;
    marker->namespace = NULL;
    if (node == NULL || node->type != XML_ELEMENT_NODE
        || node->ns == NULL || node->ns->href == NULL) {
        return 1;
    }
    if (node->doc != NULL
        && node->doc->_private
            == (void *) &elephc_dom_modern_xml_marker) {
        return 1;
    }
    namespace = node->nsDef;
    while (namespace != NULL) {
        if (elephc_dom_prefix_equal(
                namespace->prefix,
                node->ns->prefix
            )
            && xmlStrEqual(namespace->href, node->ns->href)) {
            return 1;
        }
        namespace = namespace->next;
    }
    namespace = xmlNewNs(
        node,
        node->ns->href,
        node->ns->prefix
    );
    if (namespace == NULL) {
        return 0;
    }
    marker->node = node;
    marker->namespace = namespace;
    return 1;
}

static void elephc_dom_remove_root_namespace_marker(
    elephc_dom_root_namespace_marker *marker
)
{
    xmlNsPtr *link;

    if (marker->node == NULL || marker->namespace == NULL) {
        return;
    }
    link = &marker->node->nsDef;
    while (*link != NULL && *link != marker->namespace) {
        link = &(*link)->next;
    }
    if (*link == marker->namespace) {
        *link = marker->namespace->next;
        marker->namespace->next = NULL;
        xmlFreeNs(marker->namespace);
    }
    marker->node = NULL;
    marker->namespace = NULL;
}

static void elephc_dom_restore_template_content(
    elephc_dom_template_marker *markers,
    size_t count
)
{
    while (count != 0) {
        elephc_dom_template_marker *marker = &markers[--count];
        xmlNodePtr child = marker->fragment_children;

        marker->fragment->children = marker->fragment_children;
        marker->fragment->last = marker->fragment_last;
        while (child != NULL) {
            child->parent = marker->fragment;
            child = child->next;
        }
        marker->element->children = marker->element_children;
        marker->element->last = marker->element_last;
        child = marker->element_children;
        while (child != NULL) {
            child->parent = marker->element;
            child = child->next;
        }
    }
    free(markers);
}

static elephc_dom_template_marker *elephc_dom_expose_template_content(
    xmlDocPtr document,
    size_t *marker_count,
    int32_t *failed
)
{
    elephc_dom_template_marker *markers = NULL;
    size_t count = 0;
    size_t capacity = 0;
    xmlNodePtr root = (xmlNodePtr) document;
    xmlNodePtr current = document->children;

    *marker_count = 0;
    *failed = 0;
    while (current != NULL) {
        xmlNodePtr fragment = elephc_dom_template_fragment(current);

        if (fragment != NULL) {
            xmlNodePtr child;

            if (count == capacity) {
                size_t next_capacity = capacity == 0 ? 8 : capacity * 2;
                elephc_dom_template_marker *replacement;

                if (next_capacity < capacity
                    || next_capacity
                        > SIZE_MAX / sizeof(*replacement)) {
                    *failed = 1;
                    break;
                }
                replacement = realloc(
                    markers,
                    next_capacity * sizeof(*replacement)
                );
                if (replacement == NULL) {
                    *failed = 1;
                    break;
                }
                markers = replacement;
                capacity = next_capacity;
            }
            markers[count].element = current;
            markers[count].fragment = fragment;
            markers[count].fragment_children = fragment->children;
            markers[count].fragment_last = fragment->last;
            markers[count].element_children = current->children;
            markers[count].element_last = current->last;
            count++;
            current->children = fragment->children;
            current->last = fragment->last;
            fragment->children = NULL;
            fragment->last = NULL;
            child = current->children;
            while (child != NULL) {
                child->parent = current;
                child = child->next;
            }
        }
        current = elephc_dom_next_descendant(current, root);
    }
    if (*failed != 0) {
        elephc_dom_restore_template_content(markers, count);
        return NULL;
    }
    *marker_count = count;
    return markers;
}

static int32_t elephc_dom_html_marker_name_in_use(
    xmlDocPtr document,
    const xmlChar *name
)
{
    xmlNodePtr root = (xmlNodePtr) document;
    xmlNodePtr current = document->children;

    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE
            && xmlHasProp(current, name) != NULL) {
            return 1;
        }
        current = elephc_dom_next_descendant(current, root);
    }
    return 0;
}

static void elephc_dom_remove_html_void_markers(
    elephc_dom_html_void_markers *markers
)
{
    size_t index;

    for (index = 0; index < markers->count; index++) {
        xmlUnsetProp(markers->nodes[index], markers->attribute_name);
    }
    free(markers->nodes);
    xmlFree(markers->attribute_name);
    free(markers->serialized_attribute);
    memset(markers, 0, sizeof(*markers));
}

static int32_t elephc_dom_add_html_void_markers(
    xmlDocPtr document,
    elephc_dom_html_void_markers *markers
)
{
    static const xmlChar html_namespace[] =
        "http://www.w3.org/1999/xhtml";
    char candidate[96];
    size_t suffix = 0;
    size_t count = 0;
    size_t index = 0;
    xmlNodePtr root = (xmlNodePtr) document;
    xmlNodePtr current = document->children;

    memset(markers, 0, sizeof(*markers));
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE
            && current->children == NULL
            && current->ns != NULL
            && xmlStrEqual(current->ns->href, html_namespace)
            && elephc_dom_is_html_void_element(current)) {
            count++;
        }
        current = elephc_dom_next_descendant(current, root);
    }
    if (count == 0) {
        return 1;
    }
    do {
        int written = snprintf(
            candidate,
            sizeof(candidate),
            "data-elephc-internal-html-void-%zu",
            suffix++
        );
        if (written < 0 || (size_t) written >= sizeof(candidate)) {
            return 0;
        }
    } while (elephc_dom_html_marker_name_in_use(
        document,
        (const xmlChar *) candidate
    ));
    markers->attribute_name =
        xmlStrdup((const xmlChar *) candidate);
    markers->nodes = malloc(count * sizeof(*markers->nodes));
    markers->serialized_length = strlen(candidate) + 4;
    markers->serialized_attribute =
        malloc(markers->serialized_length + 1);
    if (markers->attribute_name == NULL || markers->nodes == NULL
        || markers->serialized_attribute == NULL) {
        elephc_dom_remove_html_void_markers(markers);
        return 0;
    }
    snprintf(
        markers->serialized_attribute,
        markers->serialized_length + 1,
        " %s=\"\"",
        candidate
    );
    current = document->children;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE
            && current->children == NULL
            && current->ns != NULL
            && xmlStrEqual(current->ns->href, html_namespace)
            && elephc_dom_is_html_void_element(current)) {
            if (xmlSetProp(
                    current,
                    markers->attribute_name,
                    (const xmlChar *) ""
                ) == NULL) {
                markers->count = index;
                elephc_dom_remove_html_void_markers(markers);
                return 0;
            }
            markers->nodes[index++] = current;
        }
        current = elephc_dom_next_descendant(current, root);
    }
    markers->count = index;
    return 1;
}

static void elephc_dom_rewrite_html_void_markers(
    xmlChar *bytes,
    size_t *length,
    const elephc_dom_html_void_markers *markers
)
{
    size_t read_offset = 0;
    size_t write_offset = 0;

    if (markers->serialized_attribute == NULL) {
        return;
    }
    while (read_offset < *length) {
        if (*length - read_offset >= markers->serialized_length
            && memcmp(
                bytes + read_offset,
                markers->serialized_attribute,
                markers->serialized_length
            ) == 0) {
            bytes[write_offset++] = ' ';
            read_offset += markers->serialized_length;
        } else {
            bytes[write_offset++] = bytes[read_offset++];
        }
    }
    *length = write_offset;
}

typedef struct {
    xmlNodePtr node;
    xmlNsPtr original;
    xmlNodePtr declaration_owner;
    xmlNsPtr declaration;
} elephc_dom_serialization_namespace_marker;

typedef struct {
    xmlNodePtr element;
    xmlAttrPtr attribute;
    xmlAttrPtr previous;
    xmlAttrPtr next;
} elephc_dom_serialization_attribute_marker;

static void elephc_dom_restore_serialization_attributes(
    elephc_dom_serialization_attribute_marker *markers,
    size_t count
)
{
    while (count != 0) {
        elephc_dom_serialization_attribute_marker marker =
            markers[--count];

        marker.attribute->prev = marker.previous;
        marker.attribute->next = marker.next;
        if (marker.previous == NULL) {
            marker.element->properties = marker.attribute;
        } else {
            marker.previous->next = marker.attribute;
        }
        if (marker.next != NULL) {
            marker.next->prev = marker.attribute;
        }
    }
    free(markers);
}

static int32_t elephc_dom_namespace_uri_equal(
    const xmlChar *left,
    const xmlChar *right
)
{
    if (left == NULL || left[0] == '\0') {
        return right == NULL || right[0] == '\0';
    }
    return right != NULL && xmlStrEqual(left, right);
}

static elephc_dom_serialization_attribute_marker *
elephc_dom_suppress_conflicting_default_namespaces(
    xmlNodePtr root,
    size_t *marker_count,
    int32_t *failed
)
{
    elephc_dom_serialization_attribute_marker *markers;
    xmlNodePtr current =
        root->type == XML_ELEMENT_NODE ? root : root->children;
    size_t capacity = 0;
    size_t count = 0;

    *marker_count = 0;
    *failed = 0;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE) {
            xmlAttrPtr attribute;

            for (attribute = current->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (elephc_dom_is_namespace_attribute(attribute)
                    && elephc_dom_namespace_attribute_prefix(
                        attribute
                    ) == NULL
                    && !elephc_dom_namespace_uri_equal(
                        current->ns == NULL
                            ? NULL
                            : current->ns->href,
                        elephc_dom_namespace_attribute_uri(attribute)
                    )) {
                    capacity++;
                }
            }
        }
        current = elephc_dom_next_descendant(current, root);
    }
    if (capacity == 0) {
        return NULL;
    }
    if (capacity > SIZE_MAX / sizeof(*markers)) {
        *failed = 1;
        return NULL;
    }
    markers = malloc(capacity * sizeof(*markers));
    if (markers == NULL) {
        *failed = 1;
        return NULL;
    }
    current = root->type == XML_ELEMENT_NODE ? root : root->children;
    while (current != NULL) {
        xmlNodePtr next = elephc_dom_next_descendant(current, root);

        if (current->type == XML_ELEMENT_NODE) {
            xmlAttrPtr attribute = current->properties;

            while (attribute != NULL) {
                xmlAttrPtr following = attribute->next;

                if (elephc_dom_is_namespace_attribute(attribute)
                    && elephc_dom_namespace_attribute_prefix(
                        attribute
                    ) == NULL
                    && !elephc_dom_namespace_uri_equal(
                        current->ns == NULL
                            ? NULL
                            : current->ns->href,
                        elephc_dom_namespace_attribute_uri(attribute)
                    )) {
                    markers[count].element = current;
                    markers[count].attribute = attribute;
                    markers[count].previous = attribute->prev;
                    markers[count].next = attribute->next;
                    if (attribute->prev == NULL) {
                        current->properties = attribute->next;
                    } else {
                        attribute->prev->next = attribute->next;
                    }
                    if (attribute->next != NULL) {
                        attribute->next->prev = attribute->prev;
                    }
                    attribute->prev = NULL;
                    attribute->next = NULL;
                    count++;
                }
                attribute = following;
            }
        }
        current = next;
    }
    *marker_count = count;
    return markers;
}

static void elephc_dom_restore_serialization_namespaces(
    elephc_dom_serialization_namespace_marker *markers,
    size_t count
)
{
    while (count != 0) {
        elephc_dom_serialization_namespace_marker marker =
            markers[--count];
        xmlNsPtr *link;

        marker.node->ns = marker.original;
        if (marker.declaration_owner == NULL
            || marker.declaration == NULL) {
            continue;
        }
        link = &marker.declaration_owner->nsDef;
        while (*link != NULL && *link != marker.declaration) {
            link = &(*link)->next;
        }
        if (*link == marker.declaration) {
            *link = marker.declaration->next;
            marker.declaration->next = NULL;
            xmlFreeNs(marker.declaration);
        }
    }
    free(markers);
}

static int32_t elephc_dom_serialization_namespace_by_prefix(
    xmlDocPtr document,
    xmlNodePtr node,
    xmlNodePtr root,
    const xmlChar *prefix,
    const xmlChar **namespace_uri,
    xmlNsPtr *mapping
)
{
    xmlNodePtr current = node;

    *namespace_uri = NULL;
    *mapping = NULL;
    if (prefix != NULL
        && xmlStrEqual(prefix, (const xmlChar *) "xml")) {
        *namespace_uri = elephc_dom_xml_namespace;
        return 1;
    }
    while (current != NULL) {
        xmlAttrPtr attribute;
        xmlNsPtr namespace;

        if (current->type == XML_ELEMENT_NODE) {
            for (attribute = current->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (elephc_dom_is_namespace_attribute(attribute)
                    && elephc_dom_prefix_equal(
                        elephc_dom_namespace_attribute_prefix(attribute),
                        prefix
                    )) {
                    *namespace_uri =
                        elephc_dom_namespace_attribute_uri(attribute);
                    *mapping =
                        elephc_dom_namespace_attribute_mapping(attribute);
                    return 1;
                }
            }
            for (namespace = current->nsDef; namespace != NULL;
                namespace = namespace->next) {
                if (elephc_dom_prefix_equal(
                        namespace->prefix,
                        prefix
                    )) {
                    *namespace_uri = namespace->href;
                    *mapping = namespace;
                    return 1;
                }
            }
        }
        if (current == root) {
            break;
        }
        current = current->parent;
    }
    (void) document;
    return 0;
}

static xmlNsPtr elephc_dom_serialization_namespace_by_uri(
    xmlDocPtr document,
    xmlNodePtr node,
    xmlNodePtr root,
    const xmlChar *namespace_uri
)
{
    xmlNodePtr current = node;

    while (current != NULL) {
        xmlAttrPtr attribute;
        xmlNsPtr namespace;

        if (current->type == XML_ELEMENT_NODE) {
            for (attribute = current->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (elephc_dom_is_namespace_attribute(attribute)
                    && elephc_dom_namespace_attribute_prefix(attribute)
                        != NULL
                    && xmlStrEqual(
                        elephc_dom_namespace_attribute_uri(attribute),
                        namespace_uri
                    )) {
                    namespace =
                        elephc_dom_namespace_attribute_mapping(attribute);
                    if (namespace != NULL) {
                        return namespace;
                    }
                    return elephc_dom_document_namespace(
                        document,
                        NULL,
                        (const char *) namespace_uri,
                        elephc_dom_namespace_attribute_prefix(attribute)
                    );
                }
            }
            for (namespace = current->nsDef; namespace != NULL;
                namespace = namespace->next) {
                if (namespace->prefix != NULL
                    && xmlStrEqual(namespace->href, namespace_uri)) {
                    return namespace;
                }
            }
        }
        if (current == root) {
            break;
        }
        current = current->parent;
    }
    return NULL;
}

static int32_t elephc_dom_record_serialization_namespace(
    elephc_dom_serialization_namespace_marker *markers,
    size_t *count,
    xmlNodePtr node,
    xmlNsPtr namespace,
    xmlNodePtr declaration_owner,
    xmlNsPtr declaration
)
{
    markers[*count].node = node;
    markers[*count].original = node->ns;
    markers[*count].declaration_owner = declaration_owner;
    markers[*count].declaration = declaration;
    node->ns = namespace;
    (*count)++;
    return 1;
}

static int32_t elephc_dom_generate_serialization_namespace(
    xmlDocPtr document,
    xmlNodePtr node,
    xmlNodePtr root,
    xmlNodePtr owner,
    const xmlChar *namespace_uri,
    size_t *prefix_index,
    elephc_dom_serialization_namespace_marker *markers,
    size_t *count
)
{
    char candidate[32];
    const xmlChar *declared_uri;
    xmlNsPtr mapping;
    xmlNsPtr namespace;
    int written;

    do {
        written = snprintf(
            candidate,
            sizeof(candidate),
            "ns%zu",
            (*prefix_index)++
        );
        if (written < 0 || (size_t) written >= sizeof(candidate)) {
            return 0;
        }
    } while (elephc_dom_serialization_namespace_by_prefix(
        document,
        owner,
        root,
        (const xmlChar *) candidate,
        &declared_uri,
        &mapping
    ));
    namespace = xmlNewNs(
        owner,
        namespace_uri,
        (const xmlChar *) candidate
    );
    if (namespace == NULL) {
        return 0;
    }
    return elephc_dom_record_serialization_namespace(
        markers,
        count,
        node,
        namespace,
        owner,
        namespace
    );
}

static int32_t elephc_dom_prepare_serialization_namespace(
    xmlDocPtr document,
    xmlNodePtr node,
    xmlNodePtr owner,
    xmlNodePtr root,
    size_t *prefix_index,
    elephc_dom_serialization_namespace_marker *markers,
    size_t *count
)
{
    const xmlChar *namespace_uri;
    xmlNsPtr mapping;
    xmlNsPtr replacement;
    const xmlChar *prefix = node->ns->prefix;

    if (prefix == NULL && node->type == XML_ATTRIBUTE_NODE) {
        replacement = elephc_dom_serialization_namespace_by_uri(
            document,
            owner,
            root,
            node->ns->href
        );
        if (replacement != NULL) {
            return elephc_dom_record_serialization_namespace(
                markers,
                count,
                node,
                replacement,
                NULL,
                NULL
            );
        }
        return elephc_dom_generate_serialization_namespace(
            document,
            node,
            root,
            owner,
            node->ns->href,
            prefix_index,
            markers,
            count
        );
    }
    if (prefix == NULL && node->type == XML_ELEMENT_NODE) {
        replacement = elephc_dom_serialization_namespace_by_uri(
            document,
            owner,
            root,
            node->ns->href
        );
        if (replacement != NULL && replacement != node->ns) {
            return elephc_dom_record_serialization_namespace(
                markers,
                count,
                node,
                replacement,
                NULL,
                NULL
            );
        }
        return 1;
    }
    if (elephc_dom_serialization_namespace_by_prefix(
            document,
            owner,
            root,
            prefix,
            &namespace_uri,
            &mapping
        )) {
        if (xmlStrEqual(namespace_uri, node->ns->href)) {
            if (mapping != NULL && mapping != node->ns) {
                return elephc_dom_record_serialization_namespace(
                    markers,
                    count,
                    node,
                    mapping,
                    NULL,
                    NULL
                );
            }
            return 1;
        }
        replacement = elephc_dom_serialization_namespace_by_uri(
            document,
            owner,
            root,
            node->ns->href
        );
        if (replacement != NULL) {
            return elephc_dom_record_serialization_namespace(
                markers,
                count,
                node,
                replacement,
                NULL,
                NULL
            );
        }
        return elephc_dom_generate_serialization_namespace(
            document,
            node,
            root,
            owner,
            node->ns->href,
            prefix_index,
            markers,
            count
        );
    }
    replacement = xmlNewNs(owner, node->ns->href, prefix);
    if (replacement == NULL) {
        return 0;
    }
    return elephc_dom_record_serialization_namespace(
        markers,
        count,
        node,
        replacement,
        owner,
        replacement
    );
}

static elephc_dom_serialization_namespace_marker *
elephc_dom_apply_serialization_namespaces(
    xmlDocPtr document,
    xmlNodePtr root,
    size_t *marker_count,
    int32_t *failed
)
{
    elephc_dom_serialization_namespace_marker *markers;
    xmlNodePtr current =
        root->type == XML_ELEMENT_NODE ? root : root->children;
    size_t count = 0;
    size_t capacity = 0;
    size_t prefix_index = 1;

    *marker_count = 0;
    *failed = 0;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE) {
            xmlAttrPtr attribute;

            if (current->ns != NULL && current->ns->href != NULL) {
                capacity++;
            }
            for (attribute = current->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (attribute->ns != NULL
                    && attribute->ns->href != NULL
                    && !elephc_dom_is_namespace_attribute(attribute)) {
                    capacity++;
                }
            }
        }
        current = elephc_dom_next_descendant(current, root);
    }
    if (capacity == 0) {
        return NULL;
    }
    if (capacity > SIZE_MAX / sizeof(*markers)) {
        *failed = 1;
        return NULL;
    }
    markers = malloc(capacity * sizeof(*markers));
    if (markers == NULL) {
        *failed = 1;
        return NULL;
    }
    current = root->type == XML_ELEMENT_NODE ? root : root->children;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE) {
            xmlAttrPtr attribute;

            if (current->ns != NULL
                && current->ns->href != NULL
                && !elephc_dom_prepare_serialization_namespace(
                    document,
                    current,
                    current,
                    root,
                    &prefix_index,
                    markers,
                    &count
                )) {
                *failed = 1;
                elephc_dom_restore_serialization_namespaces(
                    markers,
                    count
                );
                return NULL;
            }
            for (attribute = current->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (attribute->ns != NULL
                    && attribute->ns->href != NULL
                    && !elephc_dom_is_namespace_attribute(attribute)
                    && !elephc_dom_prepare_serialization_namespace(
                        document,
                        (xmlNodePtr) attribute,
                        current,
                        root,
                        &prefix_index,
                        markers,
                        &count
                    )) {
                    *failed = 1;
                    elephc_dom_restore_serialization_namespaces(
                        markers,
                        count
                    );
                    return NULL;
                }
            }
        }
        current = elephc_dom_next_descendant(current, root);
    }
    *marker_count = count;
    return markers;
}

elephc_dom_native_buffer elephc_dom_native_document_serialize(
    void *document,
    const uint8_t *encoding,
    size_t encoding_length,
    int32_t format,
    int32_t modern,
    int32_t options
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    char *encoding_string = NULL;
    xmlBufferPtr buffer = NULL;
    xmlSaveCtxtPtr save_context = NULL;
    int save_options = XML_SAVE_AS_XML;
    int save_status = -1;
    xmlNodePtr *markers = NULL;
    size_t marker_count = 0;
    elephc_dom_serialization_namespace_marker *namespace_markers = NULL;
    size_t namespace_marker_count = 0;
    int32_t namespace_failed = 0;
    elephc_dom_serialization_attribute_marker *attribute_markers = NULL;
    size_t attribute_marker_count = 0;
    int32_t attribute_failed = 0;
    elephc_dom_template_marker *template_markers = NULL;
    size_t template_marker_count = 0;
    int32_t template_failed = 0;
    elephc_dom_html_void_markers html_void_markers = {
        NULL, 0, NULL, NULL, 0
    };
    size_t output_length = 0;

    if (document == NULL) {
        return result;
    }
    if (encoding_length != 0) {
        encoding_string = elephc_dom_copy_c_string(encoding, encoding_length);
        if (encoding_string == NULL) {
            return result;
        }
    }
    if (modern == 2) {
        template_markers = elephc_dom_expose_template_content(
            (xmlDocPtr) document,
            &template_marker_count,
            &template_failed
        );
        if (template_failed != 0) {
            free(encoding_string);
            return result;
        }
    }
    if (modern != 0) {
        markers = elephc_dom_add_serialization_markers(
            (xmlDocPtr) document,
            &marker_count
        );
        if (markers == NULL && marker_count == 0) {
            xmlNodePtr current = ((xmlDocPtr) document)->children;
            while (current != NULL) {
                if (current->type == XML_ELEMENT_NODE
                    && current->children == NULL
                    && current->ns != NULL
                    && xmlStrEqual(
                        current->ns->href,
                        (const xmlChar *)
                            "http://www.w3.org/1999/xhtml"
                    )
                    && !elephc_dom_is_html_void_element(current)) {
                    elephc_dom_restore_template_content(
                        template_markers,
                        template_marker_count
                    );
                    free(encoding_string);
                    return result;
                }
                current = elephc_dom_next_descendant(
                    current,
                    (xmlNodePtr) document
                );
            }
        }
    }
    if (modern != 0
        && (options & XML_SAVE_NO_EMPTY) == 0
        && !elephc_dom_add_html_void_markers(
            (xmlDocPtr) document,
            &html_void_markers
        )) {
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        free(encoding_string);
        return result;
    }
    if (modern != 0) {
        attribute_markers =
            elephc_dom_suppress_conflicting_default_namespaces(
                (xmlNodePtr) document,
                &attribute_marker_count,
                &attribute_failed
            );
    }
    if (attribute_failed) {
        elephc_dom_remove_html_void_markers(&html_void_markers);
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        free(encoding_string);
        return result;
    }
    namespace_markers = elephc_dom_apply_serialization_namespaces(
        (xmlDocPtr) document,
        (xmlNodePtr) document,
        &namespace_marker_count,
        &namespace_failed
    );
    if (namespace_failed) {
        elephc_dom_restore_serialization_attributes(
            attribute_markers,
            attribute_marker_count
        );
        elephc_dom_remove_html_void_markers(&html_void_markers);
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        free(encoding_string);
        return result;
    }
    buffer = xmlBufferCreate();
    if (buffer != NULL) {
        if (format != 0) {
            save_options |= XML_SAVE_FORMAT;
        }
        if ((options & XML_SAVE_NO_DECL) != 0) {
            save_options |= XML_SAVE_NO_DECL;
        }
        if ((options & XML_SAVE_NO_EMPTY) != 0) {
            save_options |= XML_SAVE_NO_EMPTY;
        }
        save_context = xmlSaveToBuffer(
            buffer,
            encoding_string,
            save_options
        );
    }
    if (save_context != NULL) {
        save_status = xmlSaveDoc(
            save_context,
            (xmlDocPtr) document
        );
        if (xmlSaveClose(save_context) < 0) {
            save_status = -1;
        }
    }
    if (save_status >= 0) {
        output_length = xmlBufferLength(buffer);
        result.pointer = xmlMalloc(
            output_length == 0 ? 1 : output_length
        );
        if (result.pointer != NULL && output_length != 0) {
            memcpy(
                result.pointer,
                xmlBufferContent(buffer),
                output_length
            );
        }
    }
    if (result.pointer != NULL) {
        elephc_dom_rewrite_html_void_markers(
            result.pointer,
            &output_length,
            &html_void_markers
        );
    }
    if (buffer != NULL) {
        xmlBufferFree(buffer);
    }
    elephc_dom_remove_html_void_markers(&html_void_markers);
    elephc_dom_restore_serialization_namespaces(
        namespace_markers,
        namespace_marker_count
    );
    elephc_dom_restore_serialization_attributes(
        attribute_markers,
        attribute_marker_count
    );
    elephc_dom_remove_serialization_markers(markers, marker_count);
    elephc_dom_restore_template_content(
        template_markers,
        template_marker_count
    );
    free(encoding_string);
    if (result.pointer == NULL) {
        return result;
    }
    result.length = output_length;
    return result;
}

elephc_dom_native_buffer elephc_dom_native_document_serialize_node(
    void *document,
    void *node,
    int32_t format,
    int32_t modern,
    int32_t options
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlBufferPtr buffer;
    xmlSaveCtxtPtr save_context = NULL;
    int save_options = XML_SAVE_AS_XML;
    int save_status = -1;
    xmlNodePtr *markers = NULL;
    size_t marker_count = 0;
    elephc_dom_html_void_markers html_void_markers = {
        NULL, 0, NULL, NULL, 0
    };
    elephc_dom_template_marker *template_markers = NULL;
    size_t template_marker_count = 0;
    int32_t template_failed = 0;
    elephc_dom_serialization_namespace_marker *namespace_markers = NULL;
    size_t namespace_marker_count = 0;
    int32_t namespace_failed = 0;
    elephc_dom_serialization_attribute_marker *attribute_markers = NULL;
    size_t attribute_marker_count = 0;
    int32_t attribute_failed = 0;
    elephc_dom_root_namespace_marker root_namespace_marker = {
        NULL, NULL
    };

    if (document == NULL || node == NULL) {
        return result;
    }
    if (modern == 2) {
        template_markers = elephc_dom_expose_template_content(
            (xmlDocPtr) document,
            &template_marker_count,
            &template_failed
        );
        if (template_failed != 0) {
            return result;
        }
    }
    if (modern != 0) {
        markers = elephc_dom_add_serialization_markers(
            (xmlDocPtr) document,
            &marker_count
        );
    }
    if (modern != 0
        && (options & XML_SAVE_NO_EMPTY) == 0
        && !elephc_dom_add_html_void_markers(
            (xmlDocPtr) document,
            &html_void_markers
        )) {
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        return result;
    }
    if (modern != 0
        && !elephc_dom_add_root_namespace_marker(
            (xmlNodePtr) node,
            &root_namespace_marker
        )) {
        elephc_dom_remove_html_void_markers(&html_void_markers);
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        return result;
    }
    buffer = xmlBufferCreate();
    if (buffer == NULL) {
        elephc_dom_remove_root_namespace_marker(
            &root_namespace_marker
        );
        elephc_dom_remove_html_void_markers(&html_void_markers);
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        return result;
    }
    if (modern != 0) {
        attribute_markers =
            elephc_dom_suppress_conflicting_default_namespaces(
                (xmlNodePtr) node,
                &attribute_marker_count,
                &attribute_failed
            );
    }
    if (attribute_failed) {
        xmlBufferFree(buffer);
        elephc_dom_remove_root_namespace_marker(
            &root_namespace_marker
        );
        elephc_dom_remove_html_void_markers(&html_void_markers);
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        return result;
    }
    namespace_markers = elephc_dom_apply_serialization_namespaces(
        (xmlDocPtr) document,
        (xmlNodePtr) node,
        &namespace_marker_count,
        &namespace_failed
    );
    if (namespace_failed) {
        elephc_dom_restore_serialization_attributes(
            attribute_markers,
            attribute_marker_count
        );
        xmlBufferFree(buffer);
        elephc_dom_remove_root_namespace_marker(
            &root_namespace_marker
        );
        elephc_dom_remove_html_void_markers(&html_void_markers);
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        return result;
    }
    if (format != 0) {
        save_options |= XML_SAVE_FORMAT;
    }
    if ((options & XML_SAVE_NO_EMPTY) != 0) {
        save_options |= XML_SAVE_NO_EMPTY;
    }
    save_context = xmlSaveToBuffer(buffer, NULL, save_options);
    if (save_context != NULL) {
        save_status = xmlSaveTree(
            save_context,
            (xmlNodePtr) node
        );
        if (xmlSaveClose(save_context) < 0) {
            save_status = -1;
        }
    }
    elephc_dom_restore_serialization_namespaces(
        namespace_markers,
        namespace_marker_count
    );
    elephc_dom_restore_serialization_attributes(
        attribute_markers,
        attribute_marker_count
    );
    elephc_dom_remove_root_namespace_marker(
        &root_namespace_marker
    );
    if (save_status < 0) {
        xmlBufferFree(buffer);
        elephc_dom_remove_html_void_markers(&html_void_markers);
        elephc_dom_remove_serialization_markers(markers, marker_count);
        elephc_dom_restore_template_content(
            template_markers,
            template_marker_count
        );
        return result;
    }
    result.length = xmlBufferLength(buffer);
    result.pointer = xmlMalloc(
        result.length == 0 ? 1 : result.length
    );
    if (result.pointer != NULL && result.length != 0) {
        memcpy(
            result.pointer,
            xmlBufferContent(buffer),
            result.length
        );
        elephc_dom_rewrite_html_void_markers(
            result.pointer,
            &result.length,
            &html_void_markers
        );
    }
    xmlBufferFree(buffer);
    elephc_dom_remove_html_void_markers(&html_void_markers);
    elephc_dom_remove_serialization_markers(markers, marker_count);
    elephc_dom_restore_template_content(
        template_markers,
        template_marker_count
    );
    return result;
}

static int32_t elephc_dom_xml_chars_are_well_formed(
    const xmlChar *content
)
{
    const xmlChar *current = content;

    if (current == NULL) {
        return 1;
    }
    while (*current != '\0') {
        int length = 4;
        int codepoint = xmlGetUTF8Char(current, &length);

        if (codepoint < 0 || !xmlIsCharQ(codepoint)) {
            return 0;
        }
        current += length;
    }
    return 1;
}

static int32_t elephc_dom_xml_attribute_is_well_formed(
    xmlAttrPtr attribute
)
{
    xmlAttrPtr previous;
    xmlNodePtr child;

    if (attribute == NULL
        || xmlValidateNCName(attribute->name, 0) != 0
        || (attribute->ns == NULL
            && xmlStrEqual(
                attribute->name,
                (const xmlChar *) "xmlns"
            ))) {
        return 0;
    }
    for (previous = attribute->parent->properties;
        previous != attribute;
        previous = previous->next) {
        const xmlChar *previous_uri =
            previous->ns == NULL ? NULL : previous->ns->href;
        const xmlChar *attribute_uri =
            attribute->ns == NULL ? NULL : attribute->ns->href;

        if (elephc_dom_prefix_equal(previous_uri, attribute_uri)
            && xmlStrEqual(previous->name, attribute->name)) {
            return 0;
        }
    }
    for (child = attribute->children; child != NULL;
        child = child->next) {
        if (child->content != NULL
            && !elephc_dom_xml_chars_are_well_formed(
                child->content
            )) {
            return 0;
        }
    }
    if (elephc_dom_is_namespace_attribute(attribute)) {
        const xmlChar *value =
            elephc_dom_namespace_attribute_uri(attribute);

        if (xmlStrEqual(value, elephc_dom_xmlns_namespace)
            || (value[0] == '\0'
                && elephc_dom_namespace_attribute_prefix(
                    attribute
                ) != NULL)) {
            return 0;
        }
    }
    return 1;
}

static int32_t elephc_dom_xml_node_is_well_formed(xmlNodePtr node)
{
    xmlNodePtr current = node;

    while (current != NULL) {
        switch (current->type) {
            case XML_ELEMENT_NODE: {
                xmlAttrPtr attribute;

                if (xmlValidateNCName(current->name, 0) != 0
                    || (current->ns != NULL
                        && xmlStrEqual(
                            current->ns->prefix,
                            (const xmlChar *) "xmlns"
                        ))) {
                    return 0;
                }
                for (attribute = current->properties;
                    attribute != NULL;
                    attribute = attribute->next) {
                    if (!elephc_dom_xml_attribute_is_well_formed(
                            attribute
                        )) {
                        return 0;
                    }
                }
                break;
            }
            case XML_TEXT_NODE:
                if (!elephc_dom_xml_chars_are_well_formed(
                        current->content
                    )) {
                    return 0;
                }
                break;
            case XML_COMMENT_NODE:
                if (!elephc_dom_xml_chars_are_well_formed(
                        current->content
                    )
                    || (current->content != NULL
                        && strstr(
                            (const char *) current->content,
                            "--"
                        ) != NULL)
                    || (current->content != NULL
                        && current->content[0] != '\0'
                        && current->content[
                            xmlStrlen(current->content) - 1
                        ] == '-')) {
                    return 0;
                }
                break;
            case XML_PI_NODE:
                if (xmlValidateNCName(current->name, 0) != 0
                    || xmlStrcasecmp(
                        current->name,
                        (const xmlChar *) "xml"
                    ) == 0
                    || !elephc_dom_xml_chars_are_well_formed(
                        current->content
                    )
                    || (current->content != NULL
                        && strstr(
                            (const char *) current->content,
                            "?>"
                        ) != NULL)) {
                    return 0;
                }
                break;
            default:
                break;
        }
        current = elephc_dom_next_descendant(current, node);
    }
    return 1;
}

int32_t elephc_dom_native_element_xml_is_well_formed(
    void *element,
    int32_t inner
)
{
    xmlNodePtr target = (xmlNodePtr) element;
    xmlNodePtr current;

    if (target == NULL || target->type != XML_ELEMENT_NODE) {
        return 0;
    }
    if (inner == 0) {
        return elephc_dom_xml_node_is_well_formed(target);
    }
    for (current = target->children; current != NULL;
        current = current->next) {
        if (!elephc_dom_xml_node_is_well_formed(current)) {
            return 0;
        }
    }
    return 1;
}

elephc_dom_native_buffer elephc_dom_native_element_serialize_xml(
    void *element,
    int32_t inner
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr target = (xmlNodePtr) element;
    xmlBufferPtr buffer;
    xmlNodePtr child;

    if (target == NULL || target->type != XML_ELEMENT_NODE
        || target->doc == NULL) {
        return result;
    }
    if (inner == 0) {
        return elephc_dom_native_document_serialize_node(
            target->doc,
            target,
            0,
            1,
            0
        );
    }
    buffer = xmlBufferCreate();
    if (buffer == NULL) {
        return result;
    }
    child = target->children;
    while (child != NULL) {
        elephc_dom_native_buffer serialized =
            elephc_dom_native_document_serialize_node(
                target->doc,
                child,
                0,
                1,
                0
            );

        if (serialized.pointer == NULL
            || serialized.length > INT_MAX
            || xmlBufferAdd(
                buffer,
                serialized.pointer,
                (int) serialized.length
            ) != 0) {
            xmlFree(serialized.pointer);
            xmlBufferFree(buffer);
            return result;
        }
        xmlFree(serialized.pointer);
        child = child->next;
    }
    result.length = xmlBufferLength(buffer);
    result.pointer = xmlMalloc(
        result.length == 0 ? 1 : result.length
    );
    if (result.pointer != NULL && result.length != 0) {
        memcpy(
            result.pointer,
            xmlBufferContent(buffer),
            result.length
        );
    }
    xmlBufferFree(buffer);
    return result;
}

elephc_dom_native_buffer elephc_dom_native_document_serialize_html4(
    void *document,
    void *node,
    int32_t format
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlDocPtr doc = (xmlDocPtr) document;
    xmlNodePtr current = (xmlNodePtr) node;
    xmlBufferPtr buffer;
    xmlOutputBufferPtr output;
    const xmlChar *content;
    int length = 0;

    if (doc == NULL) {
        return result;
    }
    if (current == NULL) {
        htmlDocDumpMemoryFormat(
            doc,
            (xmlChar **) &result.pointer,
            &length,
            format != 0
        );
        if (result.pointer == NULL || length <= 0) {
            if (result.pointer != NULL) {
                xmlFree(result.pointer);
            }
            result.pointer = NULL;
            return result;
        }
        result.length = (size_t) length;
        return result;
    }

    buffer = xmlBufferCreate();
    if (buffer == NULL) {
        return result;
    }
    output = xmlOutputBufferCreateBuffer(buffer, NULL);
    if (output == NULL) {
        xmlBufferFree(buffer);
        return result;
    }
    if (current->type == XML_DOCUMENT_FRAG_NODE) {
        for (current = current->children;
             current != NULL && output->error == 0;
             current = current->next) {
            htmlNodeDumpFormatOutput(
                output,
                doc,
                current,
                NULL,
                format != 0
            );
        }
    } else {
        htmlNodeDumpFormatOutput(
            output,
            doc,
            current,
            NULL,
            format != 0
        );
    }
    if (output->error == 0) {
        xmlOutputBufferFlush(output);
    }
    content = xmlBufferContent(buffer);
    if (output->error == 0 && content != NULL) {
        result.length = xmlBufferLength(buffer);
        result.pointer = xmlMalloc(result.length == 0 ? 1 : result.length);
        if (result.pointer != NULL && result.length != 0) {
            memcpy(result.pointer, content, result.length);
        }
    }
    xmlOutputBufferClose(output);
    xmlBufferFree(buffer);
    return result;
}

void elephc_dom_native_buffer_free(uint8_t *pointer)
{
    if (pointer != NULL) {
        xmlFree(pointer);
    }
}

elephc_dom_native_buffer elephc_dom_native_document_version(void *document)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlDocPtr doc = (xmlDocPtr) document;

    if (doc != NULL && doc->version != NULL) {
        result.pointer = (uint8_t *) doc->version;
        result.length = xmlStrlen(doc->version);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_document_encoding(void *document)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlDocPtr doc = (xmlDocPtr) document;

    if (doc != NULL && doc->encoding != NULL) {
        result.pointer = (uint8_t *) doc->encoding;
        result.length = xmlStrlen(doc->encoding);
    }
    return result;
}

void *elephc_dom_native_document_doctype(void *document)
{
    return document == NULL
        ? NULL
        : xmlGetIntSubset((xmlDocPtr) document);
}

void *elephc_dom_native_document_type_new(
    const uint8_t *qualified_name,
    size_t qualified_name_length,
    const uint8_t *public_id,
    size_t public_id_length,
    const uint8_t *system_id,
    size_t system_id_length
)
{
    char *name = elephc_dom_copy_c_string(
        qualified_name,
        qualified_name_length
    );
    char *public_copy = public_id_length == 0
        ? NULL
        : elephc_dom_copy_c_string(public_id, public_id_length);
    char *system_copy = system_id_length == 0
        ? NULL
        : elephc_dom_copy_c_string(system_id, system_id_length);
    xmlDtdPtr doctype;

    if (name == NULL
        || (public_id_length != 0 && public_copy == NULL)
        || (system_id_length != 0 && system_copy == NULL)) {
        free(system_copy);
        free(public_copy);
        free(name);
        return NULL;
    }
    doctype = xmlCreateIntSubset(
        NULL,
        (const xmlChar *) name,
        (const xmlChar *) public_copy,
        (const xmlChar *) system_copy
    );
    free(system_copy);
    free(public_copy);
    free(name);
    return doctype;
}

elephc_dom_native_pointer_result
elephc_dom_native_document_create_implementation_root(
    void *document,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *qualified_name,
    size_t qualified_name_length,
    int32_t modern
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlDocPtr doc = (xmlDocPtr) document;
    char *namespace_string = NULL;
    char *name_string = NULL;
    xmlChar *local_name = NULL;
    xmlChar *prefix = NULL;
    xmlNsPtr namespace = NULL;
    xmlNodePtr node = NULL;

    if (doc == NULL) {
        return result;
    }
    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            result.error_code = 11;
            goto done;
        }
    }
    name_string = elephc_dom_copy_c_string(
        qualified_name,
        qualified_name_length
    );
    if (name_string == NULL) {
        result.error_code = 11;
        goto done;
    }
    result.error_code = elephc_dom_validate_and_split_qname(
        namespace_string,
        name_string,
        modern,
        &local_name,
        &prefix
    );
    if (result.error_code == 14 && modern == 0
        && prefix != NULL
        && (namespace_string == NULL || namespace_string[0] == '\0')) {
        result.error_code = 0;
    }
    if (result.error_code != 0) {
        goto done;
    }
    node = xmlNewDocNode(doc, NULL, local_name, NULL);
    if (node == NULL) {
        result.error_code = 11;
        goto done;
    }
    namespace = elephc_dom_document_namespace(
        doc,
        node,
        namespace_string,
        prefix
    );
    if (namespace_string != NULL
        && namespace_string[0] != '\0'
        && namespace == NULL) {
        xmlFreeNode(node);
        node = NULL;
        result.error_code = 11;
        goto done;
    }
    node->ns = namespace;
    result.pointer = node;

done:
    xmlFree(local_name);
    xmlFree(prefix);
    free(namespace_string);
    free(name_string);
    return result;
}

int32_t elephc_dom_native_document_attach_doctype(
    void *document,
    void *doctype,
    int32_t allow_adoption
)
{
    xmlDocPtr target = (xmlDocPtr) document;
    xmlDtdPtr type = (xmlDtdPtr) doctype;
    xmlDocPtr previous;

    if (target == NULL || type == NULL || type->type != XML_DTD_NODE
        || target->children != NULL || target->intSubset != NULL) {
        return -1;
    }
    previous = type->doc;
    if (previous != NULL && allow_adoption == 0) {
        return 4;
    }
    if (previous != NULL && previous->intSubset == type) {
        previous->intSubset = NULL;
    }
    xmlUnlinkNode((xmlNodePtr) type);
    xmlSetTreeDoc((xmlNodePtr) type, target);
    type->parent = target;
    type->prev = NULL;
    type->next = NULL;
    type->doc = target;
    target->children = (xmlNodePtr) type;
    target->last = (xmlNodePtr) type;
    target->intSubset = type;
    return 0;
}

elephc_dom_native_buffer elephc_dom_native_document_url(void *document)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlDocPtr native_document = (xmlDocPtr) document;

    if (native_document != NULL && native_document->URL != NULL) {
        result.pointer = (uint8_t *) native_document->URL;
        result.length = xmlStrlen(native_document->URL);
    }
    return result;
}

int32_t elephc_dom_native_document_set_url(
    void *document,
    const uint8_t *url,
    size_t url_length
)
{
    xmlDocPtr native_document = (xmlDocPtr) document;
    char *url_string;

    if (native_document == NULL) {
        return 0;
    }
    url_string = elephc_dom_copy_c_string(url, url_length);
    if (url_string == NULL) {
        return 0;
    }
    xmlFree((xmlChar *) native_document->URL);
    native_document->URL = xmlStrdup((const xmlChar *) url_string);
    free(url_string);
    return native_document->URL != NULL;
}

int32_t elephc_dom_native_document_set_version(
    void *document,
    const uint8_t *version,
    size_t version_length
)
{
    xmlDocPtr native_document = (xmlDocPtr) document;
    char *version_string;
    xmlChar *replacement;

    if (native_document == NULL) {
        return 0;
    }
    version_string = elephc_dom_copy_c_string(version, version_length);
    if (version_string == NULL) {
        return 0;
    }
    replacement = xmlStrdup((const xmlChar *) version_string);
    free(version_string);
    if (replacement == NULL) {
        return 0;
    }
    xmlFree((xmlChar *) native_document->version);
    native_document->version = replacement;
    return 1;
}

int32_t elephc_dom_native_document_set_encoding(
    void *document,
    const uint8_t *encoding,
    size_t encoding_length
)
{
    xmlDocPtr native_document = (xmlDocPtr) document;
    xmlCharEncodingHandlerPtr handler;
    char *encoding_string;
    xmlChar *replacement;

    if (native_document == NULL) {
        return 0;
    }
    encoding_string = elephc_dom_copy_c_string(encoding, encoding_length);
    if (encoding_string == NULL) {
        return 0;
    }
    handler = xmlFindCharEncodingHandler(encoding_string);
    if (handler == NULL) {
        free(encoding_string);
        return -1;
    }
    xmlCharEncCloseFunc(handler);
    replacement = xmlStrdup((const xmlChar *) encoding_string);
    free(encoding_string);
    if (replacement == NULL) {
        return 0;
    }
    xmlFree((xmlChar *) native_document->encoding);
    native_document->encoding = replacement;
    return 1;
}

int32_t elephc_dom_native_document_standalone(void *document)
{
    xmlDocPtr doc = (xmlDocPtr) document;
    return doc == NULL ? -2 : doc->standalone;
}

int32_t elephc_dom_native_document_set_standalone(
    void *document,
    int32_t standalone
)
{
    xmlDocPtr doc = (xmlDocPtr) document;
    if (doc == NULL || standalone < -1 || standalone > 1) {
        return 0;
    }
    doc->standalone = standalone;
    return 1;
}

void *elephc_dom_native_document_create_element(
    void *document,
    const uint8_t *name,
    size_t name_length,
    const uint8_t *value,
    size_t value_length,
    int32_t html
)
{
    static const xmlChar html_namespace[] =
        "http://www.w3.org/1999/xhtml";
    xmlDocPtr doc = (xmlDocPtr) document;
    char *name_string;
    char *value_string = NULL;
    xmlNodePtr node;
    xmlNsPtr namespace = NULL;
    size_t index;

    if (doc == NULL) {
        return NULL;
    }
    name_string = elephc_dom_copy_c_string(name, name_length);
    if (name_string == NULL
        || xmlValidateName((const xmlChar *) name_string, 0) != 0) {
        free(name_string);
        return NULL;
    }
    if (value != NULL || value_length != 0) {
        value_string = elephc_dom_copy_c_string(value, value_length);
        if (value_string == NULL) {
            free(name_string);
            return NULL;
        }
    }
    if (html != 0) {
        xmlNodePtr root = xmlDocGetRootElement(doc);

        for (index = 0; index < name_length && name_string[index] != '\0';
            index++) {
            if (name_string[index] >= 'A' && name_string[index] <= 'Z') {
                name_string[index] =
                    (char) (name_string[index] - 'A' + 'a');
            }
        }
        if (root != NULL) {
            namespace = xmlSearchNsByHref(doc, root, html_namespace);
        }
        node = xmlNewDocRawNode(
            doc,
            namespace,
            (const xmlChar *) name_string,
            NULL
        );
        if (node != NULL && namespace == NULL) {
            namespace = xmlNewNs(node, html_namespace, NULL);
            if (namespace == NULL) {
                xmlFreeNode(node);
                node = NULL;
            } else {
                node->ns = namespace;
            }
        }
    } else {
        node = xmlNewDocNode(
            doc,
            NULL,
            (const xmlChar *) name_string,
            (const xmlChar *) value_string
        );
    }
    free(value_string);
    free(name_string);
    return node;
}

elephc_dom_native_pointer_result
elephc_dom_native_document_create_element_ns(
    void *document,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *qualified_name,
    size_t qualified_name_length,
    const uint8_t *value,
    size_t value_length,
    int32_t modern
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlDocPtr doc = (xmlDocPtr) document;
    char *namespace_string = NULL;
    char *name_string = NULL;
    char *value_string = NULL;
    xmlChar *local_name = NULL;
    xmlChar *prefix = NULL;
    xmlNsPtr namespace = NULL;
    xmlNodePtr node = NULL;

    if (doc == NULL) {
        return result;
    }
    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            result.error_code = 11;
            goto done;
        }
    }
    name_string = elephc_dom_copy_c_string(
        qualified_name,
        qualified_name_length
    );
    if (value != NULL || value_length != 0) {
        value_string = elephc_dom_copy_c_string(value, value_length);
    }
    if (name_string == NULL
        || ((value != NULL || value_length != 0) && value_string == NULL)) {
        result.error_code = 11;
        goto done;
    }
    result.error_code = elephc_dom_validate_and_split_qname(
        namespace_string,
        name_string,
        modern,
        &local_name,
        &prefix
    );
    if (result.error_code != 0) {
        goto done;
    }
    node = xmlNewDocNode(
        doc,
        NULL,
        local_name,
        (const xmlChar *) value_string
    );
    if (node == NULL) {
        result.error_code = 11;
        goto done;
    }
    namespace = elephc_dom_document_namespace(
        doc,
        node,
        namespace_string,
        prefix
    );
    if (namespace_string != NULL
        && namespace_string[0] != '\0'
        && namespace == NULL) {
        xmlFreeNode(node);
        node = NULL;
        result.error_code = 11;
        goto done;
    }
    node->ns = namespace;
    result.pointer = node;

done:
    xmlFree(local_name);
    xmlFree(prefix);
    free(namespace_string);
    free(name_string);
    free(value_string);
    return result;
}

void *elephc_dom_native_document_create_attribute(
    void *document,
    const uint8_t *name,
    size_t name_length
)
{
    xmlDocPtr doc = (xmlDocPtr) document;
    char *name_string;
    xmlAttrPtr attribute;

    if (doc == NULL) {
        return NULL;
    }
    name_string = elephc_dom_copy_c_string(name, name_length);
    if (name_string == NULL
        || xmlValidateName((const xmlChar *) name_string, 0) != 0) {
        free(name_string);
        return NULL;
    }
    attribute = xmlNewDocProp(
        doc,
        (const xmlChar *) name_string,
        NULL
    );
    free(name_string);
    return attribute;
}

elephc_dom_native_pointer_result
elephc_dom_native_document_create_attribute_ns(
    void *document,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *qualified_name,
    size_t qualified_name_length,
    int32_t modern
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlDocPtr doc = (xmlDocPtr) document;
    char *namespace_string = NULL;
    char *name_string = NULL;
    xmlChar *local_name = NULL;
    xmlChar *prefix = NULL;
    xmlNsPtr namespace = NULL;
    xmlAttrPtr attribute = NULL;

    if (doc == NULL) {
        return result;
    }
    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            result.error_code = 11;
            goto done;
        }
    }
    name_string = elephc_dom_copy_c_string(
        qualified_name,
        qualified_name_length
    );
    if (name_string == NULL) {
        result.error_code = 11;
        goto done;
    }
    result.error_code = elephc_dom_validate_and_split_qname(
        namespace_string,
        name_string,
        modern,
        &local_name,
        &prefix
    );
    if (result.error_code != 0) {
        goto done;
    }
    attribute = xmlNewDocProp(doc, local_name, NULL);
    if (attribute == NULL) {
        result.error_code = 11;
        goto done;
    }
    namespace = elephc_dom_document_namespace(
        doc,
        NULL,
        namespace_string,
        prefix
    );
    if (namespace_string != NULL
        && namespace_string[0] != '\0'
        && namespace == NULL) {
        xmlFreeProp(attribute);
        attribute = NULL;
        result.error_code = 11;
        goto done;
    }
    attribute->ns = namespace;
    result.pointer = attribute;

done:
    xmlFree(local_name);
    xmlFree(prefix);
    free(namespace_string);
    free(name_string);
    return result;
}

void *elephc_dom_native_document_create_text(
    void *document,
    const uint8_t *value,
    size_t value_length
)
{
    xmlDocPtr doc = (xmlDocPtr) document;
    char *value_string;
    xmlNodePtr node;

    if (doc == NULL) {
        return NULL;
    }
    value_string = elephc_dom_copy_c_string(value, value_length);
    if (value_string == NULL) {
        return NULL;
    }
    node = xmlNewDocText(doc, (const xmlChar *) value_string);
    free(value_string);
    return node;
}

void *elephc_dom_native_document_create_cdata(
    void *document,
    const uint8_t *value,
    size_t value_length
)
{
    xmlDocPtr doc = (xmlDocPtr) document;

    if (doc == NULL || (value == NULL && value_length != 0)) {
        return NULL;
    }
    return xmlNewCDataBlock(doc, (const xmlChar *) value, (int) value_length);
}

void *elephc_dom_native_document_create_comment(
    void *document,
    const uint8_t *value,
    size_t value_length
)
{
    xmlDocPtr doc = (xmlDocPtr) document;
    char *value_string;
    xmlNodePtr node;

    if (doc == NULL) {
        return NULL;
    }
    value_string = elephc_dom_copy_c_string(value, value_length);
    if (value_string == NULL) {
        return NULL;
    }
    node = xmlNewDocComment(doc, (const xmlChar *) value_string);
    free(value_string);
    return node;
}

void *elephc_dom_native_document_create_fragment(void *document)
{
    return document == NULL
        ? NULL
        : xmlNewDocFragment((xmlDocPtr) document);
}

elephc_dom_native_parse_result elephc_dom_native_fragment_append_xml(
    void *fragment,
    const uint8_t *input,
    size_t input_length
)
{
    elephc_dom_native_parse_result result = {NULL, NULL, 0, 0, 0};
    elephc_dom_error_list errors = {NULL, 0, 0, 0};
    xmlNodePtr node = (xmlNodePtr) fragment;
    xmlNodePtr list = NULL;
    char *source;
    int old_load_subset;
    int old_validate;
    int old_pedantic;
    int old_substitute;
    int old_line_numbers;
    int old_keep_blanks;
    int parse_error;

    if (node == NULL || node->type != XML_DOCUMENT_FRAG_NODE
        || node->doc == NULL || (input == NULL && input_length != 0)
        || input_length == SIZE_MAX) {
        result.host_status = -1;
        return result;
    }
    source = malloc(input_length + 1);
    if (source == NULL) {
        result.allocation_failed = 1;
        return result;
    }
    if (input_length != 0) {
        memcpy(source, input, input_length);
    }
    source[input_length] = '\0';

    xmlInitParser();
    old_load_subset = xmlLoadExtDtdDefaultValue;
    xmlLoadExtDtdDefaultValue = 0;
    old_validate = xmlDoValidityCheckingDefaultValue;
    xmlDoValidityCheckingDefaultValue = 0;
    old_pedantic = xmlPedanticParserDefault(0);
    old_substitute = xmlSubstituteEntitiesDefault(0);
    old_line_numbers = xmlLineNumbersDefault(0);
    old_keep_blanks = xmlKeepBlanksDefault(1);
    xmlSetStructuredErrorFunc(&errors, elephc_dom_capture_structured_error);
    parse_error = xmlParseBalancedChunkMemory(
        node->doc,
        NULL,
        NULL,
        0,
        (const xmlChar *) source,
        &list
    );
    xmlSetStructuredErrorFunc(NULL, NULL);
    xmlLoadExtDtdDefaultValue = old_load_subset;
    xmlDoValidityCheckingDefaultValue = old_validate;
    (void) xmlPedanticParserDefault(old_pedantic);
    (void) xmlSubstituteEntitiesDefault(old_substitute);
    (void) xmlLineNumbersDefault(old_line_numbers);
    (void) xmlKeepBlanksDefault(old_keep_blanks);
    free(source);

    if (errors.allocation_failed != 0) {
        if (list != NULL) {
            xmlFreeNodeList(list);
        }
        while (errors.count != 0) {
            elephc_dom_native_error_free(&errors.errors[--errors.count]);
        }
        free(errors.errors);
        result.allocation_failed = 1;
        return result;
    }
    if (parse_error != 0) {
        if (list != NULL) {
            xmlFreeNodeList(list);
        }
        result.errors = errors.errors;
        result.error_count = errors.count;
        return result;
    }

    xmlAddChildList(node, list);
    result.document = fragment;
    result.errors = errors.errors;
    result.error_count = errors.count;
    return result;
}

typedef struct {
    const xmlChar *prefix;
    const xmlChar *namespace_uri;
} elephc_dom_fragment_namespace;

static int32_t elephc_dom_fragment_namespace_add(
    elephc_dom_fragment_namespace *namespaces,
    size_t *count,
    const xmlChar *prefix,
    const xmlChar *namespace_uri
)
{
    size_t index;

    if (namespace_uri == NULL
        || (prefix != NULL
            && xmlStrEqual(prefix, (const xmlChar *) "xml"))) {
        return 1;
    }
    for (index = 0; index < *count; index++) {
        if (elephc_dom_prefix_equal(
                namespaces[index].prefix,
                prefix
            )) {
            return 1;
        }
    }
    namespaces[*count].prefix = prefix;
    namespaces[*count].namespace_uri = namespace_uri;
    (*count)++;
    return 1;
}

static elephc_dom_fragment_namespace *
elephc_dom_fragment_namespaces(
    xmlNodePtr context,
    size_t *namespace_count
)
{
    elephc_dom_fragment_namespace *namespaces;
    xmlNodePtr current = context;
    size_t capacity = 1;
    size_t count = 0;

    *namespace_count = 0;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE) {
            xmlAttrPtr attribute;
            xmlNsPtr namespace;

            capacity++;
            for (attribute = current->properties;
                attribute != NULL;
                attribute = attribute->next) {
                capacity++;
            }
            for (namespace = current->nsDef;
                namespace != NULL;
                namespace = namespace->next) {
                capacity++;
            }
        }
        current = current->parent;
    }
    if (capacity > SIZE_MAX / sizeof(*namespaces)) {
        return NULL;
    }
    namespaces = malloc(capacity * sizeof(*namespaces));
    if (namespaces == NULL) {
        return NULL;
    }
    current = context;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE) {
            xmlAttrPtr attribute;
            xmlNsPtr namespace;

            if (current->ns != NULL) {
                elephc_dom_fragment_namespace_add(
                    namespaces,
                    &count,
                    current->ns->prefix,
                    current->ns->href
                );
            }
            for (attribute = current->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (elephc_dom_is_namespace_attribute(attribute)) {
                    elephc_dom_fragment_namespace_add(
                        namespaces,
                        &count,
                        elephc_dom_namespace_attribute_prefix(attribute),
                        elephc_dom_namespace_attribute_uri(attribute)
                    );
                }
            }
            for (namespace = current->nsDef;
                namespace != NULL;
                namespace = namespace->next) {
                elephc_dom_fragment_namespace_add(
                    namespaces,
                    &count,
                    namespace->prefix,
                    namespace->href
                );
            }
            for (attribute = current->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (attribute->ns != NULL
                    && !elephc_dom_is_namespace_attribute(attribute)) {
                    elephc_dom_fragment_namespace_add(
                        namespaces,
                        &count,
                        attribute->ns->prefix,
                        attribute->ns->href
                    );
                }
            }
        }
        current = current->parent;
    }
    *namespace_count = count;
    return namespaces;
}

static int32_t elephc_dom_fragment_buffer_add(
    xmlBufferPtr buffer,
    const xmlChar *bytes,
    size_t length
)
{
    return length == 0
        || (bytes != NULL
            && length <= INT_MAX
            && xmlBufferAdd(buffer, bytes, (int) length) == 0);
}

static int32_t elephc_dom_fragment_buffer_add_string(
    xmlBufferPtr buffer,
    const xmlChar *bytes
)
{
    return bytes == NULL
        || elephc_dom_fragment_buffer_add(
            buffer,
            bytes,
            xmlStrlen(bytes)
        );
}

static int32_t elephc_dom_fragment_write_tag_name(
    xmlBufferPtr buffer,
    const xmlNode *context
)
{
    return (context->ns == NULL || context->ns->prefix == NULL
            || (elephc_dom_fragment_buffer_add_string(
                    buffer,
                    context->ns->prefix
                )
                && elephc_dom_fragment_buffer_add(
                    buffer,
                    (const xmlChar *) ":",
                    1
                )))
        && elephc_dom_fragment_buffer_add_string(
            buffer,
            context->name
        );
}

static int32_t elephc_dom_fragment_write_namespace(
    xmlDocPtr document,
    xmlBufferPtr buffer,
    const elephc_dom_fragment_namespace *namespace
)
{
    xmlChar *escaped = xmlEncodeSpecialChars(
        document,
        namespace->namespace_uri
    );
    int32_t success;

    if (escaped == NULL) {
        return 0;
    }
    success = elephc_dom_fragment_buffer_add(
            buffer,
            namespace->prefix == NULL
                ? (const xmlChar *) " xmlns=\""
                : (const xmlChar *) " xmlns:",
            namespace->prefix == NULL ? 8 : 7
        )
        && (namespace->prefix == NULL
            || (elephc_dom_fragment_buffer_add_string(
                    buffer,
                    namespace->prefix
                )
                && elephc_dom_fragment_buffer_add(
                    buffer,
                    (const xmlChar *) "=\"",
                    2
                )))
        && elephc_dom_fragment_buffer_add_string(buffer, escaped)
        && elephc_dom_fragment_buffer_add(
            buffer,
            (const xmlChar *) "\"",
            1
        );
    xmlFree(escaped);
    return success;
}

static int32_t elephc_dom_fragment_mark_namespaces(
    xmlDocPtr document,
    xmlNodePtr root
)
{
    xmlNodePtr current = root;

    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE
            && !elephc_dom_mark_namespace_attributes(
                document,
                current
            )) {
            return 0;
        }
        current = elephc_dom_next_descendant(current, root);
    }
    return 1;
}

static xmlNsPtr elephc_dom_fragment_in_scope_namespace(
    xmlDocPtr document,
    xmlNodePtr context,
    xmlNodePtr root,
    xmlNodePtr element,
    const xmlChar *prefix
)
{
    xmlNodePtr current = element->parent;

    while (current != NULL) {
        xmlNsPtr namespace;

        for (namespace = current->nsDef;
            namespace != NULL;
            namespace = namespace->next) {
            if (elephc_dom_prefix_equal(namespace->prefix, prefix)) {
                return namespace;
            }
        }
        if (current->ns != NULL
            && elephc_dom_prefix_equal(
                current->ns->prefix,
                prefix
            )) {
            return current->ns;
        }
        if (current == root) {
            break;
        }
        current = current->parent;
    }
    return elephc_dom_modern_lookup_namespace(
        document,
        context,
        prefix
    );
}

static void elephc_dom_fragment_rebind_namespace(
    xmlNodePtr root,
    xmlNsPtr source,
    xmlNsPtr replacement
)
{
    xmlNodePtr current = root;

    while (current != NULL) {
        xmlAttrPtr attribute;

        if (current->ns == source) {
            current->ns = replacement;
        }
        for (attribute = current->properties;
            attribute != NULL;
            attribute = attribute->next) {
            if (attribute->ns == source) {
                attribute->ns = replacement;
            }
        }
        current = elephc_dom_next_descendant(current, root);
    }
}

static void elephc_dom_fragment_remove_redundant_namespaces(
    xmlDocPtr document,
    xmlNodePtr context,
    xmlNodePtr root
)
{
    xmlNodePtr current = root;

    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE) {
            xmlNsPtr *link = &current->nsDef;

            while (*link != NULL) {
                xmlNsPtr namespace = *link;
                xmlNsPtr in_scope =
                    elephc_dom_fragment_in_scope_namespace(
                        document,
                        context,
                        root,
                        current,
                        namespace->prefix
                    );

                if (in_scope != NULL
                    && xmlStrEqual(
                        in_scope->href,
                        namespace->href
                    )) {
                    *link = namespace->next;
                    namespace->next = NULL;
                    elephc_dom_fragment_rebind_namespace(
                        current,
                        namespace,
                        in_scope
                    );
                    xmlFreeNs(namespace);
                } else {
                    link = &namespace->next;
                }
            }
        }
        current = elephc_dom_next_descendant(current, root);
    }
}

elephc_dom_native_pointer_result
elephc_dom_native_parse_xml_fragment(
    void *context,
    const uint8_t *input,
    size_t input_length
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlNodePtr context_node = (xmlNodePtr) context;
    elephc_dom_fragment_namespace *namespaces = NULL;
    size_t namespace_count = 0;
    size_t index;
    xmlBufferPtr source = NULL;
    xmlDocPtr parsed = NULL;
    xmlNodePtr wrapper;
    xmlNodePtr fragment = NULL;
    xmlNodePtr child;

    if (context_node == NULL || context_node->doc == NULL
        || context_node->type != XML_ELEMENT_NODE
        || (input == NULL && input_length != 0)
        || input_length > INT_MAX) {
        result.error_code = -1;
        return result;
    }
    namespaces = elephc_dom_fragment_namespaces(
        context_node,
        &namespace_count
    );
    source = xmlBufferCreate();
    if (namespaces == NULL || source == NULL
        || !elephc_dom_fragment_buffer_add(
            source,
            (const xmlChar *) "<",
            1
        )
        || !elephc_dom_fragment_write_tag_name(source, context_node)) {
        result.error_code = 11;
        goto cleanup;
    }
    for (index = 0; index < namespace_count; index++) {
        if (!elephc_dom_fragment_write_namespace(
                context_node->doc,
                source,
                &namespaces[index]
            )) {
            result.error_code = 11;
            goto cleanup;
        }
    }
    if (!elephc_dom_fragment_buffer_add(
            source,
            (const xmlChar *) ">",
            1
        )
        || !elephc_dom_fragment_buffer_add(
            source,
            input,
            input_length
        )
        || !elephc_dom_fragment_buffer_add(
            source,
            (const xmlChar *) "</",
            2
        )
        || !elephc_dom_fragment_write_tag_name(source, context_node)
        || !elephc_dom_fragment_buffer_add(
            source,
            (const xmlChar *) ">",
            1
        )) {
        result.error_code = 11;
        goto cleanup;
    }
    parsed = xmlReadMemory(
        (const char *) xmlBufferContent(source),
        (int) xmlBufferLength(source),
        NULL,
        "UTF-8",
        XML_PARSE_NONET | XML_PARSE_NOERROR | XML_PARSE_NOWARNING
    );
    wrapper = parsed == NULL ? NULL : xmlDocGetRootElement(parsed);
    if (wrapper == NULL || wrapper->next != NULL) {
        result.error_code = 12;
        goto cleanup;
    }
    fragment = xmlNewDocFragment(context_node->doc);
    if (fragment == NULL) {
        result.error_code = 11;
        goto cleanup;
    }
    for (child = wrapper->children; child != NULL; child = child->next) {
        xmlNodePtr copy = NULL;
        int32_t copy_error = 0;

        if (child->type == XML_ELEMENT_NODE) {
            copy_error = xmlDOMWrapCloneNode(
                NULL,
                parsed,
                child,
                &copy,
                context_node->doc,
                context_node,
                1,
                0
            );
        } else {
            copy = xmlDocCopyNode(child, context_node->doc, 1);
        }
        if (copy_error == 0 && copy != NULL
            && copy->type == XML_ELEMENT_NODE) {
            elephc_dom_fragment_remove_redundant_namespaces(
                context_node->doc,
                context_node,
                copy
            );
        }
        if (copy_error != 0 || copy == NULL
            || xmlAddChild(fragment, copy) == NULL
            || !elephc_dom_fragment_mark_namespaces(
                context_node->doc,
                copy
            )) {
            if (copy != NULL && copy->parent == NULL) {
                xmlFreeNode(copy);
            }
            result.error_code = 11;
            goto cleanup;
        }
    }
    result.pointer = fragment;
    fragment = NULL;

cleanup:
    if (fragment != NULL) {
        xmlFreeNode(fragment);
    }
    if (parsed != NULL) {
        xmlFreeDoc(parsed);
    }
    if (source != NULL) {
        xmlBufferFree(source);
    }
    free(namespaces);
    return result;
}

void *elephc_dom_native_document_create_pi(
    void *document,
    const uint8_t *target,
    size_t target_length,
    const uint8_t *data,
    size_t data_length
)
{
    xmlDocPtr doc = (xmlDocPtr) document;
    char *target_string;
    char *data_string;
    xmlNodePtr node;

    if (doc == NULL) {
        return NULL;
    }
    target_string = elephc_dom_copy_c_string(target, target_length);
    data_string = elephc_dom_copy_c_string(data, data_length);
    if (target_string == NULL || data_string == NULL
        || xmlValidateName((const xmlChar *) target_string, 0) != 0) {
        free(target_string);
        free(data_string);
        return NULL;
    }
    node = xmlNewDocPI(
        doc,
        (const xmlChar *) target_string,
        (const xmlChar *) data_string
    );
    free(target_string);
    free(data_string);
    return node;
}

void *elephc_dom_native_document_create_entity_reference(
    void *document,
    const uint8_t *name,
    size_t name_length
)
{
    xmlDocPtr doc = (xmlDocPtr) document;
    char *name_string;
    xmlNodePtr node;

    if (doc == NULL) {
        return NULL;
    }
    name_string = elephc_dom_copy_c_string(name, name_length);
    if (name_string == NULL
        || xmlValidateName((const xmlChar *) name_string, 0) != 0) {
        free(name_string);
        return NULL;
    }
    node = xmlNewReference(doc, (const xmlChar *) name_string);
    free(name_string);
    return node;
}

void *elephc_dom_native_document_element(void *document)
{
    return document == NULL
        ? NULL
        : xmlDocGetRootElement((xmlDocPtr) document);
}

void *elephc_dom_native_node_append_child(void *parent, void *child)
{
    xmlNodePtr native_parent = (xmlNodePtr) parent;
    xmlNodePtr native_child = (xmlNodePtr) child;

    if (parent == NULL || child == NULL) {
        return NULL;
    }
    xmlUnlinkNode(native_child);
    native_child->parent = native_parent;
    native_child->prev = native_parent->last;
    native_child->next = NULL;
    if (native_parent->last == NULL) {
        native_parent->children = native_child;
    } else {
        native_parent->last->next = native_child;
    }
    native_parent->last = native_child;
    return native_child;
}

void *elephc_dom_native_node_parent(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node == NULL ? NULL : native_node->parent;
}

void *elephc_dom_native_node_document(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node == NULL ? NULL : native_node->doc;
}

void *elephc_dom_native_node_parent_element(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlNodePtr parent = native_node == NULL ? NULL : native_node->parent;

    return parent != NULL && parent->type == XML_ELEMENT_NODE ? parent : NULL;
}

static xmlEntityPtr elephc_dom_sync_entity_reference(xmlNodePtr node)
{
    xmlEntityPtr entity;

    if (node == NULL || node->type != XML_ENTITY_REF_NODE) {
        return NULL;
    }
    entity = node->doc == NULL
        ? NULL
        : xmlGetDocEntity(node->doc, node->name);
    node->children = (xmlNodePtr) entity;
    node->last = (xmlNodePtr) entity;
    node->content = entity == NULL ? NULL : entity->content;
    return entity;
}

static void elephc_dom_update_entity_reference_links(
    xmlNodePtr root,
    int32_t synchronize
)
{
    xmlNodePtr current = root;

    while (current != NULL) {
        if (current->type == XML_ENTITY_REF_NODE) {
            if (synchronize != 0) {
                elephc_dom_sync_entity_reference(current);
            } else {
                current->children = NULL;
                current->last = NULL;
                current->content = NULL;
            }
        }
        current = elephc_dom_next_descendant(current, root);
    }
}

void *elephc_dom_native_node_first_child(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;

    if (native_node != NULL && native_node->type == XML_ENTITY_REF_NODE) {
        elephc_dom_sync_entity_reference(native_node);
    }
    return native_node == NULL ? NULL : native_node->children;
}

void *elephc_dom_native_element_content_container(
    void *element,
    int32_t ensure
)
{
    xmlNodePtr native_element = (xmlNodePtr) element;
    xmlNodePtr fragment;

    if (native_element == NULL
        || native_element->type != XML_ELEMENT_NODE
        || native_element->doc == NULL
        || native_element->ns == NULL
        || !xmlStrEqual(
            native_element->ns->href,
            elephc_dom_html_namespace
        )
        || !xmlStrEqual(
            native_element->name,
            (const xmlChar *) "template"
        )) {
        return native_element;
    }
    fragment = elephc_dom_template_fragment(native_element);
    if (fragment == NULL && ensure != 0) {
        fragment = xmlNewDocFragment(native_element->doc);
        if (fragment != NULL) {
            fragment->parent = native_element;
            native_element->_private = fragment;
        }
    }
    return fragment;
}

void *elephc_dom_native_node_last_child(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;

    if (native_node != NULL && native_node->type == XML_ENTITY_REF_NODE) {
        elephc_dom_sync_entity_reference(native_node);
    }
    return native_node == NULL ? NULL : native_node->last;
}

void *elephc_dom_native_node_previous_sibling(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node == NULL ? NULL : native_node->prev;
}

void *elephc_dom_native_node_next_sibling(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node == NULL ? NULL : native_node->next;
}

void *elephc_dom_native_node_root(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;

    if (native_node == NULL) {
        return NULL;
    }
    while (native_node->parent != NULL) {
        native_node = native_node->parent;
    }
    return native_node;
}

int32_t elephc_dom_native_node_is_connected(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;

    while (native_node != NULL) {
        if (native_node->type == XML_DOCUMENT_NODE
            || native_node->type == XML_HTML_DOCUMENT_NODE) {
            return 1;
        }
        native_node = native_node->parent;
    }
    return 0;
}

int32_t elephc_dom_native_node_has_children(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node != NULL && native_node->children != NULL;
}

int32_t elephc_dom_native_node_contains(void *node, void *other)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlNodePtr native_other = (xmlNodePtr) other;

    if (native_node == NULL || native_other == NULL) {
        return 0;
    }
    do {
        if (native_other == native_node) {
            return 1;
        }
        native_other = native_other->parent;
    } while (native_other != NULL);
    return 0;
}

int32_t elephc_dom_native_node_unlink_child(void *parent, void *child)
{
    xmlNodePtr native_parent = (xmlNodePtr) parent;
    xmlNodePtr native_child = (xmlNodePtr) child;

    if (native_parent == NULL
        || native_child == NULL
        || native_child->parent != native_parent) {
        return 0;
    }
    xmlUnlinkNode(native_child);
    return 1;
}

void *elephc_dom_native_node_insert_before(
    void *parent,
    void *child,
    void *reference)
{
    xmlNodePtr native_parent = (xmlNodePtr) parent;
    xmlNodePtr native_child = (xmlNodePtr) child;
    xmlNodePtr native_reference = (xmlNodePtr) reference;

    if (native_parent == NULL || native_child == NULL) {
        return NULL;
    }
    if (native_reference == NULL) {
        return elephc_dom_native_node_append_child(parent, child);
    }
    if (native_reference->parent != native_parent) {
        return NULL;
    }
    if (native_reference == native_child) {
        return native_child;
    }
    xmlUnlinkNode(native_child);
    native_child->parent = native_parent;
    native_child->prev = native_reference->prev;
    native_child->next = native_reference;
    if (native_reference->prev == NULL) {
        native_parent->children = native_child;
    } else {
        native_reference->prev->next = native_child;
    }
    native_reference->prev = native_child;
    return native_child;
}

int32_t elephc_dom_native_node_rename(
    void *node,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *qualified_name,
    size_t qualified_name_length
)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    char *namespace_string = NULL;
    char *name_string = NULL;
    xmlChar *local_name = NULL;
    xmlChar *prefix = NULL;
    xmlNsPtr namespace = NULL;
    const xmlChar *replacement;
    int32_t result = 0;

    if (native_node == NULL
        || native_node->doc == NULL
        || (native_node->type != XML_ELEMENT_NODE
            && native_node->type != XML_ATTRIBUTE_NODE)) {
        return -1;
    }
    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            return 11;
        }
    }
    name_string = elephc_dom_copy_c_string(
        qualified_name,
        qualified_name_length
    );
    if (name_string == NULL) {
        result = 11;
        goto done;
    }
    result = elephc_dom_validate_and_split_qname(
        namespace_string,
        name_string,
        1,
        &local_name,
        &prefix
    );
    if (result != 0) {
        goto done;
    }

    if (native_node->type == XML_ATTRIBUTE_NODE) {
        xmlAttrPtr existing = native_node->parent == NULL
            ? NULL
            : xmlHasNsProp(
                native_node->parent,
                local_name,
                namespace_string != NULL && namespace_string[0] != '\0'
                    ? (const xmlChar *) namespace_string
                    : NULL
            );
        if (existing != NULL && existing != (xmlAttrPtr) native_node) {
            result = 1301;
            goto done;
        }
    } else {
        int32_t currently_html = native_node->ns != NULL
            && xmlStrEqual(
                native_node->ns->href,
                elephc_dom_html_namespace
            );
        int32_t will_be_html = namespace_string != NULL
            && xmlStrEqual(
                (const xmlChar *) namespace_string,
                elephc_dom_html_namespace
            );
        if (currently_html != will_be_html) {
            result = currently_html != 0 ? 1302 : 1303;
            goto done;
        }
        if (currently_html != 0
            && xmlStrEqual(native_node->name, (const xmlChar *) "template")
            && !xmlStrEqual(local_name, (const xmlChar *) "template")) {
            result = 1304;
            goto done;
        }
    }

    namespace = elephc_dom_document_namespace(
        native_node->doc,
        NULL,
        namespace_string,
        prefix
    );
    if (namespace_string != NULL
        && namespace_string[0] != '\0'
        && namespace == NULL) {
        result = 11;
        goto done;
    }
    replacement = xmlDictLookup(native_node->doc->dict, local_name, -1);
    if (replacement == NULL) {
        result = 11;
        goto done;
    }
    native_node->ns = namespace;
    if (xmlDictOwns(native_node->doc->dict, native_node->name) != 1) {
        xmlFree((xmlChar *) native_node->name);
    }
    native_node->name = replacement;

done:
    xmlFree(local_name);
    xmlFree(prefix);
    free(namespace_string);
    free(name_string);
    return result;
}

void *elephc_dom_native_node_replace_child(
    void *parent,
    void *child,
    void *replaced)
{
    xmlNodePtr native_parent = (xmlNodePtr) parent;
    xmlNodePtr native_child = (xmlNodePtr) child;
    xmlNodePtr native_replaced = (xmlNodePtr) replaced;

    if (native_parent == NULL
        || native_child == NULL
        || native_replaced == NULL
        || native_replaced->parent != native_parent) {
        return NULL;
    }
    if (native_child == native_replaced) {
        return native_replaced;
    }
    xmlUnlinkNode(native_child);
    native_child->parent = native_parent;
    native_child->prev = native_replaced->prev;
    native_child->next = native_replaced->next;
    if (native_replaced->prev == NULL) {
        native_parent->children = native_child;
    } else {
        native_replaced->prev->next = native_child;
    }
    if (native_replaced->next == NULL) {
        native_parent->last = native_child;
    } else {
        native_replaced->next->prev = native_child;
    }
    native_replaced->parent = NULL;
    native_replaced->prev = NULL;
    native_replaced->next = NULL;
    return native_replaced;
}

uint32_t elephc_dom_native_node_type(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    if (native_node == NULL) {
        return 0;
    }
    return native_node->type == XML_DTD_NODE
        ? XML_DOCUMENT_TYPE_NODE
        : (uint32_t) native_node->type;
}

uint32_t elephc_dom_native_node_storage_type(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node == NULL ? 0 : (uint32_t) native_node->type;
}

elephc_dom_native_buffer elephc_dom_native_node_name(void *node)
{
    static const uint8_t cdata_name[] = "#cdata-section";
    static const uint8_t comment_name[] = "#comment";
    static const uint8_t document_name[] = "#document";
    static const uint8_t fragment_name[] = "#document-fragment";
    static const uint8_t text_name[] = "#text";
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;
    const xmlChar *plain_name = NULL;

    if (native_node == NULL) {
        return result;
    }
    switch (native_node->type) {
        case XML_CDATA_SECTION_NODE:
            plain_name = (const xmlChar *) cdata_name;
            break;
        case XML_COMMENT_NODE:
            plain_name = (const xmlChar *) comment_name;
            break;
        case XML_HTML_DOCUMENT_NODE:
        case XML_DOCUMENT_NODE:
            plain_name = (const xmlChar *) document_name;
            break;
        case XML_DOCUMENT_FRAG_NODE:
            plain_name = (const xmlChar *) fragment_name;
            break;
        case XML_ENTITY_DECL:
        case XML_NOTATION_NODE:
            /*
             * `xmlEntity`-shaped declarations only share the leading node
             * fields through `doc`. Their next field is `orig`, not
             * `xmlNode::ns`; treating either declaration as a generic node
             * would dereference entity metadata as an `xmlNs` pointer.
             */
            plain_name = native_node->name;
            break;
        case XML_TEXT_NODE:
            plain_name = (const xmlChar *) text_name;
            break;
        default:
            if (native_node->name != NULL) {
                if (native_node->ns != NULL
                    && native_node->ns->prefix != NULL) {
                    result.pointer = xmlBuildQName(
                        native_node->name,
                        native_node->ns->prefix,
                        NULL,
                        0
                    );
                } else {
                    plain_name = native_node->name;
                }
            }
            break;
    }
    if (result.pointer == NULL && plain_name != NULL) {
        result.pointer = xmlStrdup(plain_name);
    }
    if (result.pointer != NULL) {
        result.length = xmlStrlen(result.pointer);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_node_content(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlChar *content;

    if (node == NULL) {
        return result;
    }
    content = xmlNodeGetContent((xmlNodePtr) node);
    if (content != NULL) {
        result.pointer = content;
        result.length = xmlStrlen(content);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_node_namespace_uri(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;

    if (native_node != NULL
        && native_node->type != XML_ENTITY_DECL
        && native_node->type != XML_NOTATION_NODE
        && native_node->ns != NULL
        && native_node->ns->href != NULL) {
        result.pointer = native_node->ns->href;
        result.length = xmlStrlen(native_node->ns->href);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_node_prefix(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;

    if (native_node != NULL
        && native_node->type != XML_ENTITY_DECL
        && native_node->type != XML_NOTATION_NODE
        && native_node->ns != NULL
        && native_node->ns->prefix != NULL) {
        result.pointer = native_node->ns->prefix;
        result.length = xmlStrlen(native_node->ns->prefix);
    }
    return result;
}

int32_t elephc_dom_native_node_set_prefix(
    void *node,
    const uint8_t *prefix,
    size_t prefix_length
)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlNodePtr namespace_host = NULL;
    xmlNsPtr namespace = NULL;
    xmlNsPtr current;
    char *prefix_string;
    const xmlChar *effective_prefix;
    const xmlChar *namespace_uri;
    int32_t result = 0;

    if (native_node == NULL
        || (prefix_length != 0 && prefix == NULL)
        || prefix_length == SIZE_MAX) {
        return -1;
    }
    if (native_node->type == XML_ELEMENT_NODE) {
        namespace_host = native_node;
    } else if (native_node->type == XML_ATTRIBUTE_NODE) {
        namespace_host = native_node->parent;
        if (namespace_host == NULL && native_node->doc != NULL) {
            namespace_host = xmlDocGetRootElement(native_node->doc);
        }
    } else {
        return 0;
    }

    prefix_string = malloc(prefix_length + 1);
    if (prefix_string == NULL) {
        return -1;
    }
    if (prefix_length != 0) {
        memcpy(prefix_string, prefix, prefix_length);
    }
    prefix_string[prefix_length] = '\0';
    effective_prefix = prefix_string[0] == '\0'
        ? NULL
        : (const xmlChar *) prefix_string;

    if (namespace_host == NULL
        || native_node->ns == NULL
        || xmlStrEqual(native_node->ns->prefix, effective_prefix)) {
        goto done;
    }
    namespace_uri = native_node->ns->href;
    if (namespace_uri == NULL
        || (prefix_length == 3
            && memcmp(prefix, "xml", 3) == 0
            && !xmlStrEqual(namespace_uri, elephc_dom_xml_namespace))
        || (native_node->type == XML_ATTRIBUTE_NODE
            && prefix_length == 5
            && memcmp(prefix, "xmlns", 5) == 0
            && !xmlStrEqual(namespace_uri, elephc_dom_xmlns_namespace))
        || (native_node->type == XML_ATTRIBUTE_NODE
            && xmlStrEqual(
                native_node->name,
                (const xmlChar *) "xmlns"
            ))) {
        result = 14;
        goto done;
    }

    current = namespace_host->nsDef;
    while (current != NULL) {
        if (xmlStrEqual(effective_prefix, current->prefix)
            && xmlStrEqual(namespace_uri, current->href)) {
            namespace = current;
            break;
        }
        current = current->next;
    }
    if (namespace == NULL) {
        namespace = xmlNewNs(
            namespace_host,
            namespace_uri,
            effective_prefix
        );
        if (namespace == NULL) {
            result = 1401;
            goto done;
        }
    }
    xmlSetNs(native_node, namespace);

done:
    free(prefix_string);
    return result;
}

elephc_dom_native_buffer elephc_dom_native_node_local_name(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;

    if (native_node != NULL
        && (native_node->type == XML_ELEMENT_NODE
            || native_node->type == XML_ATTRIBUTE_NODE)
        && native_node->name != NULL) {
        result.pointer = (uint8_t *) native_node->name;
        result.length = xmlStrlen(native_node->name);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_node_base_uri(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlDocPtr document;
    xmlChar *uri;

    if (native_node == NULL) {
        return result;
    }
    document = native_node->type == XML_DOCUMENT_NODE
        || native_node->type == XML_HTML_DOCUMENT_NODE
        ? (xmlDocPtr) native_node
        : native_node->doc;
    uri = xmlNodeGetBase(document, native_node);
    if (uri != NULL) {
        result.pointer = uri;
        result.length = xmlStrlen(uri);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_node_path(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlChar *path;

    if (node == NULL) {
        return result;
    }
    path = xmlGetNodePath((xmlNodePtr) node);
    if (path != NULL) {
        result.pointer = path;
        result.length = xmlStrlen(path);
    }
    return result;
}

int64_t elephc_dom_native_node_line(void *node)
{
    return node == NULL ? 0 : xmlGetLineNo((xmlNodePtr) node);
}

int32_t elephc_dom_native_node_has_attributes(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node != NULL && native_node->properties != NULL;
}

int32_t elephc_dom_native_attribute_is_id(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    return native_node != NULL
        && native_node->type == XML_ATTRIBUTE_NODE
        && ((xmlAttrPtr) native_node)->atype == XML_ATTRIBUTE_ID;
}

int32_t elephc_dom_native_attribute_set_is_id(void *node, int32_t is_id)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlAttrPtr attribute;

    if (native_node == NULL
        || native_node->type != XML_ATTRIBUTE_NODE
        || (is_id != 0 && is_id != 1)) {
        return 0;
    }
    attribute = (xmlAttrPtr) native_node;
    if (is_id != 0) {
        attribute->atype = XML_ATTRIBUTE_ID;
    } else if (attribute->atype == XML_ATTRIBUTE_ID) {
        xmlRemoveID(attribute->doc, attribute);
        attribute->atype = 0;
    }
    return 1;
}

elephc_dom_native_buffer elephc_dom_native_document_type_name(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlDtdPtr doctype = (xmlDtdPtr) node;

    if (doctype != NULL && doctype->name != NULL) {
        result.pointer = (uint8_t *) doctype->name;
        result.length = xmlStrlen(doctype->name);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_document_type_public_id(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlDtdPtr doctype = (xmlDtdPtr) node;

    if (doctype != NULL && doctype->ExternalID != NULL) {
        result.pointer = (uint8_t *) doctype->ExternalID;
        result.length = xmlStrlen(doctype->ExternalID);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_document_type_system_id(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlDtdPtr doctype = (xmlDtdPtr) node;

    if (doctype != NULL && doctype->SystemID != NULL) {
        result.pointer = (uint8_t *) doctype->SystemID;
        result.length = xmlStrlen(doctype->SystemID);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_document_type_internal_subset(
    void *node
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlDtdPtr doctype = (xmlDtdPtr) node;
    xmlBufferPtr buffer;
    xmlNodePtr child;

    if (doctype == NULL || doctype->children == NULL) {
        return result;
    }
    buffer = xmlBufferCreate();
    if (buffer == NULL) {
        return result;
    }
    for (child = doctype->children; child != NULL; child = child->next) {
        int length = xmlNodeDump(buffer, NULL, child, 0, 0);
        const xmlChar *content = xmlBufferContent(buffer);
        size_t content_length = xmlBufferLength(buffer);

        if (length < 0
            || (content_length != 0
                && content[content_length - 1] != '\n'
                && xmlBufferAdd(buffer, (const xmlChar *) "\n", 1) != 0)) {
            xmlBufferFree(buffer);
            return result;
        }
    }
    result.length = xmlBufferLength(buffer);
    result.pointer = xmlMalloc(result.length);
    if (result.pointer != NULL) {
        memcpy(result.pointer, xmlBufferContent(buffer), result.length);
    }
    xmlBufferFree(buffer);
    return result;
}

typedef enum {
    ELEPHC_DOM_DTD_TABLE_ENTITIES = 0,
    ELEPHC_DOM_DTD_TABLE_NOTATIONS = 1,
} elephc_dom_dtd_table_kind;

static xmlHashTablePtr elephc_dom_dtd_lookup_table(void *node, int32_t kind) {
    xmlDtdPtr doctype = (xmlDtdPtr) node;
    if (doctype == NULL || doctype->type != XML_DTD_NODE) {
        return NULL;
    }
    if (kind == ELEPHC_DOM_DTD_TABLE_ENTITIES) {
        return (xmlHashTablePtr) doctype->entities;
    }
    if (kind == ELEPHC_DOM_DTD_TABLE_NOTATIONS) {
        return (xmlHashTablePtr) doctype->notations;
    }
    return NULL;
}

/* A non-null zero-length value lets the Rust bridge distinguish PHP's empty
 * string from NULL without allocating temporary storage. */
static const uint8_t elephc_dom_empty_buffer[] = "";

typedef struct {
    void *target;
    size_t index;
    size_t target_index;
} elephc_dom_dtd_collect_state;

static void elephc_dom_dtd_collect_scanner(void *payload, void *data,
        const xmlChar *name) {
    (void) name;
    elephc_dom_dtd_collect_state *state = data;
    if (state->index < state->target_index) {
        state->index++;
        return;
    }
    if (state->index == state->target_index && state->target == NULL) {
        state->target = payload;
    }
}

size_t elephc_dom_native_document_type_dtd_table_size(void *node, int32_t kind) {
    xmlHashTablePtr table = elephc_dom_dtd_lookup_table(node, kind);
    return table == NULL ? 0 : (size_t) xmlHashSize(table);
}

void *elephc_dom_native_document_type_dtd_table_at(void *node, int32_t kind,
        size_t index) {
    xmlHashTablePtr table = elephc_dom_dtd_lookup_table(node, kind);
    elephc_dom_dtd_collect_state state;
    if (table == NULL || index >= (size_t) xmlHashSize(table)) {
        return NULL;
    }
    state.target = NULL;
    state.index = 0;
    state.target_index = index;
    xmlHashScan(table, elephc_dom_dtd_collect_scanner, &state);
    return state.target;
}

void *elephc_dom_native_document_type_dtd_table_lookup(void *node, int32_t kind,
        const uint8_t *name, size_t name_length) {
    xmlHashTablePtr table = elephc_dom_dtd_lookup_table(node, kind);
    xmlChar *key;
    void *payload;
    if (table == NULL || name == NULL || name_length == 0
            || name_length > INT_MAX) {
        return NULL;
    }
    key = elephc_dom_copy_c_string(name, name_length);
    if (key == NULL) {
        return NULL;
    }
    payload = xmlHashLookup(table, (const xmlChar *) key);
    free(key);
    return payload;
}

/*
 * Mirrors php-src `create_notation` from `ext/dom/dom_iterators.c`. The
 * `xmlNotation` payloads stored in a doctype's notation hash table are not
 * `xmlNode` values, so the bridge must synthesize an independently owned
 * fake `xmlEntity` whose `type` is `XML_NOTATION_NODE` and whose name,
 * ExternalID, and SystemID fields are duplicated. Lifetime is owned by the
 * caller; the fake node must be released by the dedicated notation release
 * helper once the bridge handle goes out of scope. `xmlFreeNode()` does not
 * own this synthetic `xmlEntity` layout.
 */
static xmlNodePtr elephc_dom_create_notation(const xmlChar *name,
        const xmlChar *external_id, const xmlChar *system_id) {
    xmlEntityPtr ret = (xmlEntityPtr) xmlMalloc(sizeof(xmlEntity));
    if (ret == NULL) {
        return NULL;
    }
    memset(ret, 0, sizeof(xmlEntity));
    ret->type = XML_NOTATION_NODE;
    ret->name = name != NULL ? xmlStrdup(name) : NULL;
    ret->ExternalID = external_id != NULL ? xmlStrdup(external_id) : NULL;
    ret->SystemID = system_id != NULL ? xmlStrdup(system_id) : NULL;
    return (xmlNodePtr) ret;
}

/*
 * Synthesizes one fresh notation wrapper node from the libxml2 notation
 * payload returned by the index-based DTD table scan. Returns NULL when
 * the payload pointer does not actually point at an `xmlNotation` record.
 */
void *elephc_dom_native_notation_synthesize(void *payload) {
    xmlNotationPtr notation = (xmlNotationPtr) payload;
    if (notation == NULL || notation->name == NULL) {
        return NULL;
    }
    return elephc_dom_create_notation(notation->name, notation->PublicID,
            notation->SystemID);
}

/* Frees the standalone fake notation created above. xmlFreeNode does not own
 * XML_NOTATION_NODE's xmlEntity field layout, so release its copied strings and
 * the allocation directly, matching php-src's create_notation lifetime. */
void elephc_dom_native_notation_node_free(void *node) {
    xmlEntityPtr notation = (xmlEntityPtr) node;
    if (notation == NULL || notation->type != XML_NOTATION_NODE) {
        return;
    }
    xmlFree((xmlChar *) notation->name);
    xmlFree(notation->ExternalID);
    xmlFree(notation->SystemID);
    xmlFree(notation);
}

elephc_dom_native_buffer elephc_dom_native_entity_external_id(void *node) {
    elephc_dom_native_buffer result = {NULL, 0};
    xmlEntityPtr entity = (xmlEntityPtr) node;
    if (entity != NULL
            && entity->etype == XML_EXTERNAL_GENERAL_UNPARSED_ENTITY
            && entity->ExternalID != NULL) {
        result.pointer = (uint8_t *) entity->ExternalID;
        result.length = xmlStrlen(entity->ExternalID);
    }
    return result;
}

elephc_dom_native_buffer elephc_dom_native_entity_system_id(void *node) {
    elephc_dom_native_buffer result = {NULL, 0};
    xmlEntityPtr entity = (xmlEntityPtr) node;
    if (entity != NULL
            && entity->etype == XML_EXTERNAL_GENERAL_UNPARSED_ENTITY
            && entity->SystemID != NULL) {
        result.pointer = (uint8_t *) entity->SystemID;
        result.length = xmlStrlen(entity->SystemID);
    }
    return result;
}


elephc_dom_native_buffer elephc_dom_native_entity_notation_name(void *node) {
    elephc_dom_native_buffer result = {NULL, 0};
    xmlEntityPtr entity = (xmlEntityPtr) node;
    if (entity == NULL
            || entity->etype != XML_EXTERNAL_GENERAL_UNPARSED_ENTITY) {
        return result;
    }
    /* php-src reads `entity->content` directly because `xmlNodeGetContent`
     * can produce the entity-replacement-text form, which differs from the
     * exact byte sequence PHP exposes as `notationName`. An absent content
     * field on an external unparsed entity is PHP's empty string, not NULL. */
    if (entity->content == NULL) {
        result.pointer = (uint8_t *) elephc_dom_empty_buffer;
    } else {
        result.pointer = (uint8_t *) entity->content;
        result.length = xmlStrlen(entity->content);
    }
    return result;
}

/* Returns one notation public identifier, preserving PHP's empty-string
 * representation when the declaration has no public identifier. */
elephc_dom_native_buffer elephc_dom_native_notation_public_id(void *node) {
    elephc_dom_native_buffer result = {NULL, 0};
    xmlEntityPtr notation = (xmlEntityPtr) node;
    if (notation == NULL || notation->type != XML_NOTATION_NODE) {
        return result;
    }
    if (notation->ExternalID == NULL) {
        result.pointer = (uint8_t *) elephc_dom_empty_buffer;
    } else {
        result.pointer = (uint8_t *) notation->ExternalID;
        result.length = xmlStrlen(notation->ExternalID);
    }
    return result;
}

/* Returns one notation system identifier, preserving PHP's empty-string
 * representation when the declaration has no system identifier. */
elephc_dom_native_buffer elephc_dom_native_notation_system_id(void *node) {
    elephc_dom_native_buffer result = {NULL, 0};
    xmlEntityPtr notation = (xmlEntityPtr) node;
    if (notation == NULL || notation->type != XML_NOTATION_NODE) {
        return result;
    }
    if (notation->SystemID == NULL) {
        result.pointer = (uint8_t *) elephc_dom_empty_buffer;
    } else {
        result.pointer = (uint8_t *) notation->SystemID;
        result.length = xmlStrlen(notation->SystemID);
    }
    return result;
}

int32_t elephc_dom_native_node_set_content(
    void *node,
    const uint8_t *content,
    size_t content_length)
{
    if (node == NULL
        || content_length > INT_MAX
        || (content == NULL && content_length != 0)) {
        return 0;
    }
    xmlNodeSetContentLen(
        (xmlNodePtr) node,
        (const xmlChar *) (content_length == 0
            ? (const uint8_t *) ""
            : content),
        (int) content_length
    );
    return 1;
}

static const xmlChar *elephc_dom_character_data_content(
    xmlNodePtr node
)
{
    return node->content == NULL
        ? (const xmlChar *) ""
        : node->content;
}

static int32_t elephc_dom_character_data_unsigned(
    int64_t input,
    int32_t modern,
    uint32_t *output
)
{
    if (input < INT_MIN
        || input > INT_MAX
        || (input < 0 && modern == 0)) {
        return 0;
    }
    *output = (uint32_t) input;
    return 1;
}

static int32_t elephc_dom_character_data_set_segments(
    xmlNodePtr node,
    int32_t prefix_length,
    const uint8_t *middle,
    size_t middle_length,
    int32_t suffix_offset,
    int32_t total_length
)
{
    const xmlChar *content = elephc_dom_character_data_content(node);
    xmlChar *prefix = NULL;
    xmlChar *suffix = NULL;
    xmlBufferPtr buffer;
    int32_t success = 0;

    if (middle_length > INT_MAX) {
        return 0;
    }
    if (prefix_length != 0) {
        prefix = xmlUTF8Strndup(content, prefix_length);
        if (prefix == NULL) {
            return 0;
        }
    }
    if (suffix_offset < total_length) {
        suffix = xmlUTF8Strsub(
            content,
            suffix_offset,
            total_length - suffix_offset
        );
        if (suffix == NULL) {
            xmlFree(prefix);
            return 0;
        }
    }
    buffer = xmlBufferCreate();
    if (buffer == NULL) {
        xmlFree(prefix);
        xmlFree(suffix);
        return 0;
    }
    if ((prefix == NULL
            || xmlBufferAdd(
                buffer,
                prefix,
                xmlStrlen(prefix)
            ) == 0)
        && (middle_length == 0
            || xmlBufferAdd(
                buffer,
                middle,
                (int) middle_length
            ) == 0)
        && (suffix == NULL
            || xmlBufferAdd(
                buffer,
                suffix,
                xmlStrlen(suffix)
            ) == 0)
        && xmlBufferLength(buffer) <= INT_MAX) {
        xmlNodeSetContentLen(
            node,
            xmlBufferContent(buffer),
            (int) xmlBufferLength(buffer)
        );
        success = 1;
    }
    xmlBufferFree(buffer);
    xmlFree(prefix);
    xmlFree(suffix);
    return success;
}

int64_t elephc_dom_native_character_data_length(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;

    if (native_node == NULL) {
        return -1;
    }
    return xmlUTF8Strlen(
        elephc_dom_character_data_content(native_node)
    );
}

elephc_dom_native_buffer_result
elephc_dom_native_character_data_substring(
    void *node,
    int64_t offset_input,
    int64_t count_input,
    int32_t modern
)
{
    elephc_dom_native_buffer_result result = {NULL, 0, 0, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;
    const xmlChar *content;
    int32_t length;
    uint32_t offset;
    uint32_t count;

    if (native_node == NULL) {
        result.error_code = -1;
        return result;
    }
    content = elephc_dom_character_data_content(native_node);
    length = xmlUTF8Strlen(content);
    if (length < 0) {
        result.error_code = -1;
        return result;
    }
    if (!elephc_dom_character_data_unsigned(
            offset_input,
            modern,
            &offset
        )
        || !elephc_dom_character_data_unsigned(
            count_input,
            modern,
            &count
        )
        || offset > (uint32_t) length) {
        result.error_code = 1;
        return result;
    }
    if (count > (uint32_t) length - offset) {
        count = (uint32_t) length - offset;
    }
    result.pointer = xmlUTF8Strsub(
        content,
        (int) offset,
        (int) count
    );
    if (result.pointer == NULL) {
        result.pointer = xmlStrdup((const xmlChar *) "");
    }
    if (result.pointer == NULL) {
        result.error_code = -1;
        return result;
    }
    result.length = xmlStrlen(result.pointer);
    return result;
}

int32_t elephc_dom_native_character_data_append(
    void *node,
    const uint8_t *data,
    size_t data_length
)
{
    if (node == NULL
        || data_length > INT_MAX
        || (data == NULL && data_length != 0)) {
        return -1;
    }
    return xmlTextConcat(
        (xmlNodePtr) node,
        data == NULL ? (const xmlChar *) "" : data,
        (int) data_length
    ) == 0
        ? 0
        : -1;
}

int32_t elephc_dom_native_character_data_insert(
    void *node,
    int64_t offset_input,
    const uint8_t *data,
    size_t data_length,
    int32_t modern
)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    int32_t length;
    uint32_t offset;

    if (native_node == NULL
        || (data == NULL && data_length != 0)) {
        return -1;
    }
    length = xmlUTF8Strlen(
        elephc_dom_character_data_content(native_node)
    );
    if (length < 0) {
        return -1;
    }
    if (!elephc_dom_character_data_unsigned(
            offset_input,
            modern,
            &offset
        )
        || offset > (uint32_t) length) {
        return 1;
    }
    return elephc_dom_character_data_set_segments(
        native_node,
        (int32_t) offset,
        data,
        data_length,
        (int32_t) offset,
        length
    )
        ? 0
        : -1;
}

int32_t elephc_dom_native_character_data_delete(
    void *node,
    int64_t offset,
    int64_t count_input,
    int32_t modern
)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    int32_t length;
    uint32_t count;

    if (native_node == NULL) {
        return -1;
    }
    length = xmlUTF8Strlen(
        elephc_dom_character_data_content(native_node)
    );
    if (length < 0) {
        return -1;
    }
    if (offset < 0
        || offset > INT_MAX
        || offset > length
        || !elephc_dom_character_data_unsigned(
            count_input,
            modern,
            &count
        )) {
        return 1;
    }
    if (count > (uint32_t) (length - offset)) {
        count = (uint32_t) (length - offset);
    }
    return elephc_dom_character_data_set_segments(
        native_node,
        (int32_t) offset,
        NULL,
        0,
        (int32_t) offset + (int32_t) count,
        length
    )
        ? 0
        : -1;
}

int32_t elephc_dom_native_character_data_replace(
    void *node,
    int64_t offset,
    int64_t count_input,
    const uint8_t *data,
    size_t data_length,
    int32_t modern
)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    int32_t length;
    uint32_t count;

    if (native_node == NULL
        || (data == NULL && data_length != 0)) {
        return -1;
    }
    length = xmlUTF8Strlen(
        elephc_dom_character_data_content(native_node)
    );
    if (length < 0) {
        return -1;
    }
    if (offset < 0
        || offset > INT_MAX
        || offset > length
        || !elephc_dom_character_data_unsigned(
            count_input,
            modern,
            &count
        )) {
        return 1;
    }
    if (count > (uint32_t) (length - offset)) {
        count = (uint32_t) (length - offset);
    }
    return elephc_dom_character_data_set_segments(
        native_node,
        (int32_t) offset,
        data,
        data_length,
        (int32_t) offset + (int32_t) count,
        length
    )
        ? 0
        : -1;
}

elephc_dom_native_buffer elephc_dom_native_text_whole_text(void *node)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr current = (xmlNodePtr) node;
    xmlBufferPtr buffer;

    if (current == NULL) {
        return result;
    }
    while (current->prev != NULL
        && (current->prev->type == XML_TEXT_NODE
            || current->prev->type == XML_CDATA_SECTION_NODE)) {
        current = current->prev;
    }
    buffer = xmlBufferCreate();
    if (buffer == NULL) {
        return result;
    }
    while (current != NULL
        && (current->type == XML_TEXT_NODE
            || current->type == XML_CDATA_SECTION_NODE)) {
        if (current->content != NULL
            && xmlBufferAdd(
                buffer,
                current->content,
                xmlStrlen(current->content)
            ) != 0) {
            xmlBufferFree(buffer);
            return result;
        }
        current = current->next;
    }
    result.length = xmlBufferLength(buffer);
    result.pointer = malloc(result.length == 0 ? 1 : result.length);
    if (result.pointer != NULL && result.length != 0) {
        memcpy(result.pointer, xmlBufferContent(buffer), result.length);
    }
    xmlBufferFree(buffer);
    return result;
}

elephc_dom_native_pointer_result elephc_dom_native_text_split(
    void *node,
    int64_t offset_input
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;
    const xmlChar *content;
    xmlChar *first = NULL;
    xmlChar *second = NULL;
    xmlNodePtr split = NULL;
    int32_t length;
    int32_t offset;

    if (native_node == NULL
        || offset_input < 0
        || offset_input > INT_MAX) {
        result.error_code = -1;
        return result;
    }
    content = elephc_dom_character_data_content(native_node);
    length = xmlUTF8Strlen(content);
    offset = (int32_t) offset_input;
    if (length < 0) {
        result.error_code = -1;
        return result;
    }
    if (offset > length) {
        result.error_code = 1;
        return result;
    }
    first = xmlUTF8Strndup(content, offset);
    second = xmlUTF8Strsub(content, offset, length - offset);
    if (first == NULL || second == NULL) {
        result.error_code = 11;
        goto done;
    }
    xmlNodeSetContent(native_node, first);
    split = xmlNewDocText(native_node->doc, second);
    if (split == NULL) {
        result.error_code = 11;
        goto done;
    }
    if (native_node->parent != NULL) {
        split->type = XML_ELEMENT_NODE;
        xmlAddNextSibling(native_node, split);
        split->type = XML_TEXT_NODE;
    }
    result.pointer = split;

done:
    xmlFree(first);
    xmlFree(second);
    return result;
}

int32_t elephc_dom_native_text_is_blank(void *node)
{
    return node != NULL && xmlIsBlankNode((xmlNodePtr) node);
}

static int32_t elephc_dom_node_is_equal(
    const xmlNode *left,
    const xmlNode *right,
    int32_t modern
);

static int32_t elephc_dom_node_content_equal(
    const xmlNode *left,
    const xmlNode *right
)
{
    xmlChar *left_content = xmlNodeGetContent(left);
    xmlChar *right_content = xmlNodeGetContent(right);
    int32_t equal = xmlStrEqual(left_content, right_content);

    xmlFree(left_content);
    xmlFree(right_content);
    return equal;
}

static int32_t elephc_dom_node_namespace_uri_equal(
    const xmlNode *left,
    const xmlNode *right
)
{
    const xmlChar *left_uri = left->ns == NULL ? NULL : left->ns->href;
    const xmlChar *right_uri = right->ns == NULL ? NULL : right->ns->href;
    return xmlStrEqual(left_uri, right_uri);
}

static int32_t elephc_dom_node_namespace_prefix_equal(
    const xmlNode *left,
    const xmlNode *right
)
{
    const xmlChar *left_prefix =
        left->ns == NULL ? NULL : left->ns->prefix;
    const xmlChar *right_prefix =
        right->ns == NULL ? NULL : right->ns->prefix;
    return xmlStrEqual(left_prefix, right_prefix);
}

static int32_t elephc_dom_attribute_is_equal(
    const xmlAttr *left,
    const xmlAttr *right
)
{
    return left != NULL
        && right != NULL
        && xmlStrEqual(left->name, right->name)
        && elephc_dom_node_namespace_uri_equal(
            (const xmlNode *) left,
            (const xmlNode *) right
        )
        && elephc_dom_node_content_equal(
            (const xmlNode *) left,
            (const xmlNode *) right
        );
}

static size_t elephc_dom_node_list_length(const xmlNode *node)
{
    size_t length = 0;
    while (node != NULL) {
        length++;
        node = node->next;
    }
    return length;
}

static int32_t elephc_dom_node_list_equal_ordered(
    const xmlNode *left,
    const xmlNode *right,
    int32_t modern
)
{
    if (elephc_dom_node_list_length(left)
        != elephc_dom_node_list_length(right)) {
        return 0;
    }
    while (left != NULL) {
        if (!elephc_dom_node_is_equal(left, right, modern)) {
            return 0;
        }
        left = left->next;
        right = right->next;
    }
    return 1;
}

static int32_t elephc_dom_node_list_equal_unordered(
    const xmlNode *left,
    const xmlNode *right,
    int32_t modern
)
{
    const xmlNode *candidate;
    if (elephc_dom_node_list_length(left)
        != elephc_dom_node_list_length(right)) {
        return 0;
    }
    while (left != NULL) {
        int32_t found = 0;
        for (candidate = right; candidate != NULL; candidate = candidate->next) {
            if (elephc_dom_node_is_equal(left, candidate, modern)) {
                found = 1;
                break;
            }
        }
        if (!found) {
            return 0;
        }
        left = left->next;
    }
    return 1;
}

static size_t elephc_dom_namespace_list_length(const xmlNs *namespace)
{
    size_t length = 0;
    while (namespace != NULL) {
        length++;
        namespace = namespace->next;
    }
    return length;
}

static int32_t elephc_dom_namespace_list_equal_unordered(
    const xmlNs *left,
    const xmlNs *right
)
{
    const xmlNs *candidate;
    if (elephc_dom_namespace_list_length(left)
        != elephc_dom_namespace_list_length(right)) {
        return 0;
    }
    while (left != NULL) {
        int32_t found = 0;
        for (candidate = right; candidate != NULL; candidate = candidate->next) {
            if (xmlStrEqual(left->prefix, candidate->prefix)
                && xmlStrEqual(left->href, candidate->href)) {
                found = 1;
                break;
            }
        }
        if (!found) {
            return 0;
        }
        left = left->next;
    }
    return 1;
}

static int32_t elephc_dom_node_is_equal(
    const xmlNode *left,
    const xmlNode *right,
    int32_t modern
)
{
    if (left == NULL || right == NULL || left->type != right->type) {
        return left == right;
    }
    switch (left->type) {
        case XML_ELEMENT_NODE:
            return xmlStrEqual(left->name, right->name)
                && elephc_dom_node_namespace_prefix_equal(left, right)
                && elephc_dom_node_namespace_uri_equal(left, right)
                && elephc_dom_node_list_equal_unordered(
                    (const xmlNode *) left->properties,
                    (const xmlNode *) right->properties,
                    modern
                )
                && (modern
                    || elephc_dom_namespace_list_equal_unordered(
                        left->nsDef,
                        right->nsDef
                    ))
                && elephc_dom_node_list_equal_ordered(
                    left->children,
                    right->children,
                    modern
                );
        case XML_DTD_NODE: {
            const xmlDtd *left_dtd = (const xmlDtd *) left;
            const xmlDtd *right_dtd = (const xmlDtd *) right;
            return xmlStrEqual(left_dtd->name, right_dtd->name)
                && xmlStrEqual(left_dtd->ExternalID, right_dtd->ExternalID)
                && xmlStrEqual(left_dtd->SystemID, right_dtd->SystemID);
        }
        case XML_PI_NODE:
            return xmlStrEqual(left->name, right->name)
                && xmlStrEqual(left->content, right->content);
        case XML_TEXT_NODE:
        case XML_COMMENT_NODE:
        case XML_CDATA_SECTION_NODE:
            return xmlStrEqual(left->content, right->content);
        case XML_ATTRIBUTE_NODE:
            return elephc_dom_attribute_is_equal(
                (const xmlAttr *) left,
                (const xmlAttr *) right
            );
        case XML_ENTITY_REF_NODE:
            return xmlStrEqual(left->name, right->name);
        case XML_ENTITY_DECL:
        case XML_NOTATION_NODE:
        case XML_ENTITY_NODE: {
            const xmlEntity *left_entity = (const xmlEntity *) left;
            const xmlEntity *right_entity = (const xmlEntity *) right;
            return left_entity->etype == right_entity->etype
                && xmlStrEqual(left_entity->name, right_entity->name)
                && xmlStrEqual(left_entity->ExternalID, right_entity->ExternalID)
                && xmlStrEqual(left_entity->SystemID, right_entity->SystemID)
                && elephc_dom_node_content_equal(left, right);
        }
        case XML_DOCUMENT_FRAG_NODE:
        case XML_HTML_DOCUMENT_NODE:
        case XML_DOCUMENT_NODE:
            return elephc_dom_node_list_equal_ordered(
                left->children,
                right->children,
                modern
            );
        default:
            return 0;
    }
}

int32_t elephc_dom_native_node_is_equal(
    void *node,
    void *other,
    int32_t modern
)
{
    return elephc_dom_node_is_equal(
        (const xmlNode *) node,
        (const xmlNode *) other,
        modern
    );
}

#define ELEPHC_DOM_POSITION_DISCONNECTED 0x01
#define ELEPHC_DOM_POSITION_PRECEDING 0x02
#define ELEPHC_DOM_POSITION_FOLLOWING 0x04
#define ELEPHC_DOM_POSITION_CONTAINS 0x08
#define ELEPHC_DOM_POSITION_CONTAINED_BY 0x10
#define ELEPHC_DOM_POSITION_IMPLEMENTATION_SPECIFIC 0x20

int64_t elephc_dom_native_node_compare_document_position(
    void *node,
    void *other_node
)
{
    xmlNodePtr this_node = (xmlNodePtr) node;
    xmlNodePtr other = (xmlNodePtr) other_node;
    xmlNodePtr node1;
    xmlNodePtr node2;
    xmlNodePtr attr1 = NULL;
    xmlNodePtr attr2 = NULL;
    xmlNodePtr node1_root;
    xmlNodePtr node2_root;
    size_t node1_depth = 0;
    size_t node2_depth = 0;
    int32_t node2_is_ancestor_of_node1 = 0;
    int32_t node1_is_ancestor_of_node2 = 0;

    if (this_node == NULL || other == NULL) {
        return -1;
    }
    if (this_node == other) {
        return 0;
    }
    node1 = other;
    node2 = this_node;
    if (node1->type == XML_ATTRIBUTE_NODE) {
        attr1 = node1;
        node1 = attr1->parent;
    }
    if (node2->type == XML_ATTRIBUTE_NODE) {
        const xmlAttr *attribute;
        attr2 = node2;
        node2 = attr2->parent;
        if (attr1 != NULL && node1 != NULL && node2 == node1) {
            for (attribute = node2->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (elephc_dom_attribute_is_equal(
                        attribute,
                        (const xmlAttr *) attr1
                    )) {
                    return ELEPHC_DOM_POSITION_IMPLEMENTATION_SPECIFIC
                        | ELEPHC_DOM_POSITION_PRECEDING;
                }
                if (elephc_dom_attribute_is_equal(
                        attribute,
                        (const xmlAttr *) attr2
                    )) {
                    return ELEPHC_DOM_POSITION_IMPLEMENTATION_SPECIFIC
                        | ELEPHC_DOM_POSITION_FOLLOWING;
                }
            }
        }
    }
    if (node1 == NULL || node2 == NULL) {
        goto disconnected;
    }
    node1_root = node1;
    while (node1_root->parent != NULL) {
        node1_root = node1_root->parent;
        if (node1_root == node2) {
            node2_is_ancestor_of_node1 = 1;
        }
        node1_depth++;
    }
    node2_root = node2;
    while (node2_root->parent != NULL) {
        node2_root = node2_root->parent;
        if (node2_root == node1) {
            node1_is_ancestor_of_node2 = 1;
        }
        node2_depth++;
    }
    if (node1_root != node2_root) {
        goto disconnected;
    }
    if ((node1_is_ancestor_of_node2 && attr1 == NULL)
        || (node1 == node2 && attr2 != NULL)) {
        return ELEPHC_DOM_POSITION_CONTAINS
            | ELEPHC_DOM_POSITION_PRECEDING;
    }
    if ((node2_is_ancestor_of_node1 && attr2 == NULL)
        || (node1 == node2 && attr1 != NULL)) {
        return ELEPHC_DOM_POSITION_CONTAINED_BY
            | ELEPHC_DOM_POSITION_FOLLOWING;
    }
    if (node1_is_ancestor_of_node2) {
        return ELEPHC_DOM_POSITION_PRECEDING;
    }
    if (node2_is_ancestor_of_node1) {
        return ELEPHC_DOM_POSITION_FOLLOWING;
    }
    while (node1_depth > node2_depth) {
        node1 = node1->parent;
        node1_depth--;
    }
    while (node2_depth > node1_depth) {
        node2 = node2->parent;
        node2_depth--;
    }
    while (node1->parent != node2->parent) {
        node1 = node1->parent;
        node2 = node2->parent;
    }
    do {
        node1 = node1->next;
        if (node1 == node2) {
            return ELEPHC_DOM_POSITION_PRECEDING;
        }
    } while (node1 != NULL);
    return ELEPHC_DOM_POSITION_FOLLOWING;

disconnected:
    return ELEPHC_DOM_POSITION_DISCONNECTED
        | ELEPHC_DOM_POSITION_IMPLEMENTATION_SPECIFIC
        | (node1 < node2
            ? ELEPHC_DOM_POSITION_PRECEDING
            : ELEPHC_DOM_POSITION_FOLLOWING);
}

elephc_dom_native_buffer elephc_dom_native_node_lookup_namespace_uri(
    void *node,
    const uint8_t *prefix,
    size_t prefix_length,
    int32_t default_namespace)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlDocPtr document;
    xmlNsPtr ns;
    char *prefix_string = NULL;

    if (native_node == NULL) {
        return result;
    }
    if (default_namespace == 0) {
        prefix_string = elephc_dom_copy_c_string(prefix, prefix_length);
        if (prefix_string == NULL) {
            return result;
        }
    }
    document = native_node->type == XML_DOCUMENT_NODE
        || native_node->type == XML_HTML_DOCUMENT_NODE
        ? (xmlDocPtr) native_node
        : native_node->doc;
    if (document != NULL
        && document->_private
            == (void *) &elephc_dom_modern_xml_marker) {
        const xmlChar *requested_prefix = default_namespace != 0
            ? NULL
            : (const xmlChar *) prefix_string;

        if (requested_prefix != NULL
            && xmlStrEqual(
                requested_prefix,
                (const xmlChar *) "xml"
            )) {
            result.pointer = (uint8_t *) elephc_dom_xml_namespace;
            result.length = sizeof(elephc_dom_xml_namespace) - 1;
            free(prefix_string);
            return result;
        }
        if (requested_prefix != NULL
            && xmlStrEqual(
                requested_prefix,
                (const xmlChar *) "xmlns"
            )) {
            result.pointer = (uint8_t *) elephc_dom_xmlns_namespace;
            result.length = sizeof(elephc_dom_xmlns_namespace) - 1;
            free(prefix_string);
            return result;
        }
        ns = elephc_dom_modern_lookup_namespace(
            document,
            native_node,
            requested_prefix
        );
    } else {
        ns = xmlSearchNs(
            document,
            native_node,
            default_namespace != 0
                ? NULL
                : (const xmlChar *) prefix_string
        );
    }
    free(prefix_string);
    if (ns != NULL && ns->href != NULL) {
        result.pointer = ns->href;
        result.length = xmlStrlen(ns->href);
    }
    return result;
}

static const xmlChar *elephc_dom_modern_lookup_prefix(
    xmlDocPtr document,
    xmlNodePtr node,
    const xmlChar *namespace_uri
)
{
    xmlNodePtr origin = node;
    xmlNodePtr current = elephc_dom_namespace_lookup_element(node);

    while (current != NULL) {
        xmlAttrPtr attribute;
        xmlNsPtr declaration;
        const xmlChar *candidate = NULL;

        if (current->ns != NULL
            && current->ns->prefix != NULL
            && xmlStrEqual(current->ns->href, namespace_uri)) {
            candidate = current->ns->prefix;
        }
        for (attribute = current->properties;
            candidate == NULL && attribute != NULL;
            attribute = attribute->next) {
            if (elephc_dom_is_namespace_attribute(attribute)) {
                const xmlChar *declared_prefix =
                    elephc_dom_namespace_attribute_prefix(attribute);

                if (declared_prefix != NULL
                    && xmlStrEqual(
                        elephc_dom_namespace_attribute_uri(attribute),
                        namespace_uri
                    )) {
                    candidate = declared_prefix;
                }
            } else if (attribute->ns != NULL
                && attribute->ns->prefix != NULL
                && xmlStrEqual(
                    attribute->ns->href,
                    namespace_uri
                )) {
                candidate = attribute->ns->prefix;
            }
        }
        for (declaration = current->nsDef;
            candidate == NULL && declaration != NULL;
            declaration = declaration->next) {
            if (declaration->prefix != NULL
                && xmlStrEqual(declaration->href, namespace_uri)) {
                candidate = declaration->prefix;
            }
        }
        if (candidate != NULL) {
            xmlNsPtr in_scope = elephc_dom_modern_lookup_namespace(
                document,
                origin,
                candidate
            );
            if (in_scope != NULL
                && xmlStrEqual(in_scope->href, namespace_uri)) {
                return candidate;
            }
        }
        current = current->parent;
        while (current != NULL
            && current->type != XML_ELEMENT_NODE) {
            current = current->parent;
        }
    }
    return NULL;
}

elephc_dom_native_buffer elephc_dom_native_node_lookup_prefix(
    void *node,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlDocPtr document;
    xmlNsPtr ns;
    char *namespace_string;

    if (native_node == NULL) {
        return result;
    }
    namespace_string =
        elephc_dom_copy_c_string(namespace_uri, namespace_uri_length);
    if (namespace_string == NULL) {
        return result;
    }
    document = native_node->type == XML_DOCUMENT_NODE
        || native_node->type == XML_HTML_DOCUMENT_NODE
        ? (xmlDocPtr) native_node
        : native_node->doc;
    if (document != NULL
        && document->_private
            == (void *) &elephc_dom_modern_xml_marker) {
        const xmlChar *prefix = NULL;

        if (!xmlStrEqual(
                (const xmlChar *) namespace_string,
                elephc_dom_xml_namespace
            )
            && !xmlStrEqual(
                (const xmlChar *) namespace_string,
                elephc_dom_xmlns_namespace
            )) {
            prefix = elephc_dom_modern_lookup_prefix(
                document,
                native_node,
                (const xmlChar *) namespace_string
            );
        }
        if (prefix != NULL) {
            result.pointer = (uint8_t *) prefix;
            result.length = xmlStrlen(prefix);
        }
    } else {
        ns = xmlSearchNsByHref(
            document,
            native_node,
            (const xmlChar *) namespace_string
        );
        if (ns != NULL && ns->prefix != NULL) {
            result.pointer = ns->prefix;
            result.length = xmlStrlen(ns->prefix);
        }
    }
    free(namespace_string);
    return result;
}

static int32_t elephc_dom_initialize_cloned_template_fragments(
    xmlNodePtr source,
    xmlNodePtr clone,
    xmlDocPtr target_document,
    int32_t deep
)
{
    while (source != NULL && clone != NULL) {
        if (source->type == XML_ELEMENT_NODE
            && clone->type == XML_ELEMENT_NODE) {
            xmlNodePtr source_fragment =
                elephc_dom_template_fragment(source);

            clone->_private = NULL;
            if (source_fragment != NULL) {
                xmlNodePtr clone_fragment =
                    xmlNewDocFragment(target_document);

                if (clone_fragment == NULL) {
                    return 0;
                }
                clone_fragment->parent = clone;
                clone->_private = clone_fragment;
            }
            if (deep != 0
                && !elephc_dom_initialize_cloned_template_fragments(
                    source->children,
                    clone->children,
                    target_document,
                    1
                )) {
                return 0;
            }
        }
        source = source->next;
        clone = clone->next;
    }
    return source == NULL && clone == NULL;
}

static int32_t elephc_dom_reset_adopted_template_fragments(
    xmlNodePtr node,
    xmlDocPtr target_document
)
{
    while (node != NULL) {
        xmlNodePtr fragment =
            elephc_dom_template_fragment(node);
        xmlNodePtr replacement = NULL;

        if (!elephc_dom_reset_adopted_template_fragments(
                node->children,
                target_document
            )) {
            return 0;
        }
        if (fragment != NULL) {
            replacement = xmlNewDocFragment(target_document);
            if (replacement == NULL) {
                return 0;
            }
            replacement->parent = node;
            elephc_dom_free_template_fragments(fragment->children);
            node->_private = NULL;
            xmlFreeNode(fragment);
            node->_private = replacement;
        }
        node = node->next;
    }
    return 1;
}

void *elephc_dom_native_document_clone(
    void *document,
    int32_t deep,
    int32_t family
)
{
    xmlDocPtr source = (xmlDocPtr) document;
    xmlDocPtr clone;

    if (source == NULL
        || (source->type != XML_DOCUMENT_NODE
            && source->type != XML_HTML_DOCUMENT_NODE)) {
        return NULL;
    }
    clone = xmlCopyDoc(source, deep != 0);
    if (clone == NULL) {
        return NULL;
    }
    clone->_private = NULL;
    if (family == 1) {
        clone->_private = (void *) &elephc_dom_modern_xml_marker;
    } else if (family == 2) {
        elephc_dom_native_html_copy_document_mode(source, clone);
    }
    if (deep != 0
        && !elephc_dom_initialize_cloned_template_fragments(
            source->children,
            clone->children,
            clone,
            1
        )) {
        elephc_dom_native_document_free(clone);
        return NULL;
    }
    return clone;
}

static xmlNodePtr elephc_dom_clone_modern_node(
    xmlNodePtr source,
    xmlDocPtr document,
    int32_t deep
)
{
    xmlNodePtr clone;
    xmlNodePtr child;

    clone = xmlDocCopyNode(source, document, 2);
    if (clone == NULL || deep == 0) {
        return clone;
    }
    if (source->type != XML_ELEMENT_NODE
        && source->type != XML_DOCUMENT_FRAG_NODE) {
        return clone;
    }
    clone->children = NULL;
    clone->last = NULL;
    for (child = source->children; child != NULL; child = child->next) {
        xmlNodePtr child_clone;

        if (child->type == XML_DTD_NODE) {
            continue;
        }
        if (child->type == XML_ELEMENT_NODE
            || child->type == XML_DOCUMENT_FRAG_NODE) {
            child_clone = elephc_dom_clone_modern_node(
                child,
                document,
                1
            );
        } else if (child->type == XML_ENTITY_REF_NODE) {
            child_clone = xmlDocCopyNode(child, document, 0);
            elephc_dom_sync_entity_reference(child_clone);
        } else {
            child_clone = xmlDocCopyNode(child, document, 1);
        }
        if (child_clone == NULL) {
            xmlFreeNode(clone);
            return NULL;
        }
        child_clone->parent = clone;
        child_clone->prev = clone->last;
        child_clone->next = NULL;
        if (clone->last == NULL) {
            clone->children = child_clone;
        } else {
            clone->last->next = child_clone;
        }
        clone->last = child_clone;
    }
    return clone;
}

void *elephc_dom_native_node_clone(
    void *node,
    int32_t deep,
    int32_t modern
)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlNodePtr clone;

    if (native_node == NULL || native_node->doc == NULL) {
        return NULL;
    }
    clone = modern != 0
        ? elephc_dom_clone_modern_node(
            native_node,
            native_node->doc,
            deep
        )
        : xmlDocCopyNode(
            native_node,
            native_node->doc,
            deep != 0 ? 1 : 2
        );
    if (clone != NULL && modern != 0) {
        elephc_dom_update_entity_reference_links(clone, 0);
    }
    if (clone != NULL
        && (xmlReconciliateNs(native_node->doc, clone) < 0
            || !elephc_dom_initialize_cloned_template_fragments(
                native_node,
                clone,
                native_node->doc,
                deep
            ))) {
        elephc_dom_free_template_fragments(clone);
        xmlFreeNode(clone);
        return NULL;
    }
    if (clone != NULL && modern != 0) {
        elephc_dom_update_entity_reference_links(clone, 1);
    }
    return clone;
}

elephc_dom_native_pointer_result elephc_dom_native_document_import_node(
    void *document,
    void *node,
    int32_t deep,
    int32_t modern
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlDocPtr target_document = (xmlDocPtr) document;
    xmlNodePtr source = (xmlNodePtr) node;

    if (target_document == NULL || source == NULL) {
        result.error_code = -1;
        return result;
    }
    if (source->type == XML_DOCUMENT_NODE
        || source->type == XML_HTML_DOCUMENT_NODE) {
        result.error_code = 9;
        return result;
    }
    if (source->doc == target_document) {
        result.pointer = source;
        return result;
    }
    result.pointer = modern != 0
        ? elephc_dom_clone_modern_node(
            source,
            target_document,
            deep
        )
        : xmlDocCopyNode(
            source,
            target_document,
            deep != 0 ? 1 : 2
        );
    if (result.pointer == NULL) {
        result.error_code = modern != 0 ? 11 : -1;
        return result;
    }
    if (modern != 0) {
        elephc_dom_update_entity_reference_links(result.pointer, 0);
    }
    if (xmlReconciliateNs(target_document, result.pointer) < 0) {
        elephc_dom_free_template_fragments(result.pointer);
        xmlFreeNode(result.pointer);
        result.pointer = NULL;
        result.error_code = modern != 0 ? 11 : -1;
    } else if (!elephc_dom_initialize_cloned_template_fragments(
            source,
            result.pointer,
            target_document,
            deep
        )) {
        elephc_dom_free_template_fragments(result.pointer);
        xmlFreeNode(result.pointer);
        result.pointer = NULL;
        result.error_code = modern != 0 ? 11 : -1;
    }
    if (result.pointer != NULL && modern != 0) {
        elephc_dom_update_entity_reference_links(result.pointer, 1);
    }
    return result;
}

elephc_dom_native_pointer_result elephc_dom_native_document_adopt_node(
    void *document,
    void *node,
    int32_t modern
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlDocPtr target_document = (xmlDocPtr) document;
    xmlNodePtr adopted = (xmlNodePtr) node;
    xmlDocPtr source_document;

    if (target_document == NULL || adopted == NULL) {
        result.error_code = -1;
        return result;
    }
    if (adopted->type == XML_DOCUMENT_NODE
        || adopted->type == XML_HTML_DOCUMENT_NODE
        || adopted->type == XML_DOCUMENT_TYPE_NODE
        || adopted->type == XML_DTD_NODE
        || adopted->type == XML_ENTITY_NODE
        || adopted->type == XML_NOTATION_NODE) {
        result.error_code = 9;
        return result;
    }
    source_document = adopted->doc;
    if (source_document == target_document) {
        xmlUnlinkNode(adopted);
        result.pointer = adopted;
        return result;
    }
    elephc_dom_update_entity_reference_links(adopted, 0);
    if (modern != 0) {
        xmlUnlinkNode(adopted);
        if (!elephc_dom_reset_adopted_template_fragments(
                adopted,
                target_document
            )) {
            elephc_dom_update_entity_reference_links(adopted, 1);
            result.error_code = 11;
            return result;
        }
        xmlSetTreeDoc(adopted, target_document);
        if (xmlReconciliateNs(target_document, adopted) < 0) {
            elephc_dom_update_entity_reference_links(adopted, 1);
            result.error_code = 11;
            return result;
        }
    } else if (xmlDOMWrapAdoptNode(
            NULL,
            source_document,
            adopted,
            target_document,
            NULL,
            0
        ) != 0) {
        elephc_dom_update_entity_reference_links(adopted, 1);
        result.error_code = -1;
        return result;
    }
    elephc_dom_update_entity_reference_links(adopted, 1);
    result.pointer = adopted;
    return result;
}

void *elephc_dom_native_document_get_element_by_id(
    void *document,
    const uint8_t *id,
    size_t id_length
)
{
    xmlDocPtr native_document = (xmlDocPtr) document;
    xmlNodePtr base;
    xmlNodePtr node;
    char *id_string;

    if (native_document == NULL
        || (id == NULL && id_length != 0)
        || (id_length != 0 && memchr(id, '\0', id_length) != NULL)) {
        return NULL;
    }
    id_string = malloc(id_length + 1);
    if (id_string == NULL) {
        return NULL;
    }
    if (id_length != 0) {
        memcpy(id_string, id, id_length);
    }
    id_string[id_length] = '\0';
    base = (xmlNodePtr) native_document;
    node = base->children;
    while (node != NULL) {
        if (node->type == XML_ELEMENT_NODE) {
            const xmlAttr *attribute;
            for (attribute = node->properties;
                attribute != NULL;
                attribute = attribute->next) {
                if (attribute->atype == XML_ATTRIBUTE_ID) {
                    xmlChar *value = xmlNodeListGetString(
                        native_document,
                        attribute->children,
                        1
                    );
                    int32_t matches = xmlStrEqual(
                        value,
                        (const xmlChar *) id_string
                    );
                    xmlFree(value);
                    if (matches) {
                        free(id_string);
                        return node;
                    }
                }
            }
        }
        node = elephc_dom_next_descendant(node, base);
    }
    free(id_string);
    return NULL;
}

size_t elephc_dom_native_node_child_count(void *node)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlNodePtr child;
    size_t count = 0;

    if (native_node != NULL && native_node->type == XML_ENTITY_REF_NODE) {
        elephc_dom_sync_entity_reference(native_node);
    }
    child = native_node == NULL ? NULL : native_node->children;

    while (child != NULL) {
        count++;
        child = child->next;
    }
    return count;
}

void *elephc_dom_native_node_child_at(void *node, size_t index)
{
    xmlNodePtr native_node = (xmlNodePtr) node;
    xmlNodePtr child;

    if (native_node != NULL && native_node->type == XML_ENTITY_REF_NODE) {
        elephc_dom_sync_entity_reference(native_node);
    }
    child = native_node == NULL ? NULL : native_node->children;

    while (child != NULL && index != 0) {
        child = child->next;
        index--;
    }
    return child;
}

static xmlNodePtr elephc_dom_next_descendant(
    xmlNodePtr node,
    xmlNodePtr root
)
{
    if (node->type != XML_ENTITY_REF_NODE && node->children != NULL) {
        return node->children;
    }
    while (node != root && node->next == NULL) {
        node = node->parent;
    }
    return node == root ? NULL : node->next;
}

void *elephc_dom_native_descendant_element_first(void *root)
{
    xmlNodePtr native_root = (xmlNodePtr) root;
    xmlNodePtr current =
        native_root == NULL ? NULL : native_root->children;

    while (current != NULL && current->type != XML_ELEMENT_NODE) {
        current = elephc_dom_next_descendant(current, native_root);
    }
    return current;
}

void *elephc_dom_native_descendant_element_next(
    void *root,
    void *current
)
{
    xmlNodePtr native_root = (xmlNodePtr) root;
    xmlNodePtr candidate = current == NULL || native_root == NULL
        ? NULL
        : elephc_dom_next_descendant(
            (xmlNodePtr) current,
            native_root
        );

    while (candidate != NULL
        && candidate->type != XML_ELEMENT_NODE) {
        candidate = elephc_dom_next_descendant(candidate, native_root);
    }
    return candidate;
}

typedef struct {
    const xmlChar *prefix;
    const xmlChar *namespace_uri;
} elephc_dom_namespace_info_candidate;

static int32_t elephc_dom_namespace_info_prefix_equal(
    const xmlChar *left,
    const xmlChar *right
)
{
    return (left == NULL && right == NULL)
        || (left != NULL
            && right != NULL
            && xmlStrEqual(left, right));
}

static int32_t elephc_dom_namespace_info_candidate_append(
    elephc_dom_namespace_info_candidate **candidates,
    size_t *count,
    size_t *capacity,
    const xmlChar *prefix,
    const xmlChar *namespace_uri
)
{
    elephc_dom_namespace_info_candidate *resized;

    for (size_t index = 0; index < *count; index++) {
        if (elephc_dom_namespace_info_prefix_equal(
                (*candidates)[index].prefix,
                prefix
            )) {
            return 1;
        }
    }
    if (*count == *capacity) {
        size_t next_capacity = *capacity == 0 ? 8 : *capacity * 2;

        if (next_capacity < *capacity
            || next_capacity > SIZE_MAX / sizeof(**candidates)) {
            return 0;
        }
        resized = realloc(
            *candidates,
            next_capacity * sizeof(**candidates)
        );
        if (resized == NULL) {
            return 0;
        }
        *candidates = resized;
        *capacity = next_capacity;
    }
    (*candidates)[*count].prefix = prefix;
    (*candidates)[*count].namespace_uri = namespace_uri;
    (*count)++;
    return 1;
}

static int32_t elephc_dom_namespace_info_result_append(
    elephc_dom_native_namespace_info_result *result,
    size_t *capacity,
    xmlNodePtr element,
    const elephc_dom_namespace_info_candidate *candidate
)
{
    elephc_dom_native_namespace_info *resized;

    if (result->count == *capacity) {
        size_t next_capacity = *capacity == 0 ? 8 : *capacity * 2;

        if (next_capacity < *capacity
            || next_capacity > SIZE_MAX / sizeof(*result->items)) {
            return 0;
        }
        resized = realloc(
            result->items,
            next_capacity * sizeof(*result->items)
        );
        if (resized == NULL) {
            return 0;
        }
        result->items = resized;
        *capacity = next_capacity;
    }
    result->items[result->count].element = element;
    result->items[result->count].prefix =
        (const uint8_t *) candidate->prefix;
    result->items[result->count].prefix_length =
        candidate->prefix == NULL ? 0 : xmlStrlen(candidate->prefix);
    result->items[result->count].namespace_uri =
        (const uint8_t *) candidate->namespace_uri;
    result->items[result->count].namespace_uri_length =
        candidate->namespace_uri == NULL
            ? 0
            : xmlStrlen(candidate->namespace_uri);
    result->count++;
    return 1;
}

static int32_t elephc_dom_collect_in_scope_namespace_info(
    elephc_dom_native_namespace_info_result *result,
    size_t *result_capacity,
    xmlNodePtr element
)
{
    elephc_dom_namespace_info_candidate *candidates = NULL;
    size_t candidate_count = 0;
    size_t candidate_capacity = 0;
    xmlNodePtr current;

    for (current = element; current != NULL; current = current->parent) {
        xmlAttrPtr attribute;

        if (current->type != XML_ELEMENT_NODE) {
            continue;
        }
        attribute = current->properties;
        if (attribute != NULL) {
            while (attribute->next != NULL) {
                attribute = attribute->next;
            }
        }
        while (attribute != NULL) {
            if (elephc_dom_is_namespace_attribute(attribute)
                && attribute->children != NULL
                && attribute->children->content != NULL) {
                const xmlChar *prefix =
                    attribute->ns->prefix == NULL
                        ? NULL
                        : attribute->name;

                if (!elephc_dom_namespace_info_candidate_append(
                        &candidates,
                        &candidate_count,
                        &candidate_capacity,
                        prefix,
                        attribute->children->content
                    )) {
                    free(candidates);
                    return 0;
                }
            }
            attribute = attribute->prev;
        }
    }

    while (candidate_count != 0) {
        const elephc_dom_namespace_info_candidate *candidate =
            &candidates[--candidate_count];

        if (candidate->prefix == NULL
            && (candidate->namespace_uri == NULL
                || candidate->namespace_uri[0] == '\0')) {
            continue;
        }
        if (!elephc_dom_namespace_info_result_append(
                result,
                result_capacity,
                element,
                candidate
            )) {
            free(candidates);
            return 0;
        }
    }
    free(candidates);
    return 1;
}

elephc_dom_native_namespace_info_result
elephc_dom_native_element_namespace_info(
    void *element,
    int32_t include_descendants
)
{
    elephc_dom_native_namespace_info_result result = {NULL, 0, 0, 0};
    xmlNodePtr root = (xmlNodePtr) element;
    xmlNodePtr current;
    size_t capacity = 0;

    if (root == NULL || root->type != XML_ELEMENT_NODE) {
        return result;
    }
    if (!elephc_dom_collect_in_scope_namespace_info(
            &result,
            &capacity,
            root
        )) {
        result.allocation_failed = 1;
        return result;
    }
    if (include_descendants == 0) {
        return result;
    }
    current = root->children;
    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE
            && !elephc_dom_collect_in_scope_namespace_info(
                &result,
                &capacity,
                current
            )) {
            result.allocation_failed = 1;
            return result;
        }
        current = elephc_dom_next_descendant(current, root);
    }
    return result;
}

void elephc_dom_native_namespace_info_result_free(
    elephc_dom_native_namespace_info *items
)
{
    free(items);
}

static int32_t elephc_dom_element_matches_name(
    xmlNodePtr element,
    const xmlChar *name,
    int32_t match_local_name
)
{
    xmlChar *qualified;
    int32_t matches;

    if (xmlStrEqual(name, (const xmlChar *) "*")) {
        return 1;
    }
    if (match_local_name != 0) {
        return xmlStrEqual(element->name, name);
    }
    if (element->ns == NULL || element->ns->prefix == NULL) {
        return xmlStrEqual(element->name, name);
    }
    qualified = xmlBuildQName(
        element->name,
        element->ns->prefix,
        NULL,
        0
    );
    if (qualified == NULL) {
        return 0;
    }
    matches = xmlStrEqual(qualified, name);
    xmlFree(qualified);
    return matches;
}

static int32_t elephc_dom_element_matches_namespace(
    xmlNodePtr element,
    const xmlChar *namespace_uri,
    const xmlChar *local_name
)
{
    int32_t namespace_matches =
        xmlStrEqual(namespace_uri, (const xmlChar *) "*")
        || ((namespace_uri == NULL || namespace_uri[0] == '\0')
            && (element->ns == NULL
                || element->ns->href == NULL
                || element->ns->href[0] == '\0'))
        || (element->ns != NULL
            && xmlStrEqual(element->ns->href, namespace_uri));
    return namespace_matches
        && (xmlStrEqual(local_name, (const xmlChar *) "*")
            || xmlStrEqual(element->name, local_name));
}

static void *elephc_dom_descendant_element_at(
    void *root,
    size_t index,
    const xmlChar *name,
    const xmlChar *namespace_uri,
    const xmlChar *local_name,
    int32_t match_local_name
)
{
    xmlNodePtr native_root = (xmlNodePtr) root;
    xmlNodePtr current =
        native_root == NULL ? NULL : native_root->children;

    while (current != NULL) {
        int32_t matches = current->type == XML_ELEMENT_NODE
            && (name != NULL
                ? elephc_dom_element_matches_name(
                    current,
                    name,
                    match_local_name
                )
                : elephc_dom_element_matches_namespace(
                    current,
                    namespace_uri,
                    local_name
                ));
        if (matches != 0) {
            if (index == 0) {
                return current;
            }
            index--;
        }
        current = elephc_dom_next_descendant(current, native_root);
    }
    return NULL;
}

static size_t elephc_dom_descendant_element_count(
    void *root,
    const xmlChar *name,
    const xmlChar *namespace_uri,
    const xmlChar *local_name,
    int32_t match_local_name
)
{
    xmlNodePtr native_root = (xmlNodePtr) root;
    xmlNodePtr current =
        native_root == NULL ? NULL : native_root->children;
    size_t count = 0;

    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE
            && (name != NULL
                ? elephc_dom_element_matches_name(
                    current,
                    name,
                    match_local_name
                )
                : elephc_dom_element_matches_namespace(
                    current,
                    namespace_uri,
                    local_name
                ))) {
            count++;
        }
        current = elephc_dom_next_descendant(current, native_root);
    }
    return count;
}

void *elephc_dom_native_descendant_element_at_name(
    void *root,
    size_t index,
    const uint8_t *name,
    size_t name_length,
    int32_t match_local_name
)
{
    char *name_string = elephc_dom_copy_c_string(name, name_length);
    void *element;

    if (name_string == NULL) {
        return NULL;
    }
    element = elephc_dom_descendant_element_at(
        root,
        index,
        (const xmlChar *) name_string,
        NULL,
        NULL,
        match_local_name
    );
    free(name_string);
    return element;
}

void *elephc_dom_native_descendant_element_at_ns(
    void *root,
    size_t index,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *local_name,
    size_t local_name_length
)
{
    char *namespace_string = NULL;
    char *name_string;
    void *element;

    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            return NULL;
        }
    }
    name_string = elephc_dom_copy_c_string(local_name, local_name_length);
    if (name_string == NULL) {
        free(namespace_string);
        return NULL;
    }
    element = elephc_dom_descendant_element_at(
        root,
        index,
        NULL,
        (const xmlChar *) namespace_string,
        (const xmlChar *) name_string,
        0
    );
    free(namespace_string);
    free(name_string);
    return element;
}

size_t elephc_dom_native_descendant_element_count_name(
    void *root,
    const uint8_t *name,
    size_t name_length,
    int32_t match_local_name
)
{
    char *name_string = elephc_dom_copy_c_string(name, name_length);
    size_t count;

    if (name_string == NULL) {
        return 0;
    }
    count = elephc_dom_descendant_element_count(
        root,
        (const xmlChar *) name_string,
        NULL,
        NULL,
        match_local_name
    );
    free(name_string);
    return count;
}

size_t elephc_dom_native_descendant_element_count_ns(
    void *root,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *local_name,
    size_t local_name_length
)
{
    char *namespace_string = NULL;
    char *name_string;
    size_t count;

    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            return 0;
        }
    }
    name_string = elephc_dom_copy_c_string(local_name, local_name_length);
    if (name_string == NULL) {
        free(namespace_string);
        return 0;
    }
    count = elephc_dom_descendant_element_count(
        root,
        NULL,
        (const xmlChar *) namespace_string,
        (const xmlChar *) name_string,
        0
    );
    free(namespace_string);
    free(name_string);
    return count;
}

elephc_dom_native_buffer elephc_dom_native_element_get_attribute(
    void *element,
    const uint8_t *name,
    size_t name_length)
{
    elephc_dom_native_buffer result = {NULL, 0};
    char *name_string;
    xmlAttrPtr attribute;

    if (element == NULL) {
        return result;
    }
    name_string = elephc_dom_copy_c_string(name, name_length);
    if (name_string == NULL) {
        return result;
    }
    attribute = elephc_dom_attribute_by_qualified_name(
        (xmlNodePtr) element,
        (const xmlChar *) name_string
    );
    free(name_string);
    if (attribute != NULL) {
        result.pointer = xmlNodeListGetString(
            attribute->doc,
            attribute->children,
            1
        );
        if (result.pointer != NULL) {
            result.length = xmlStrlen(result.pointer);
        }
    }
    return result;
}

void *elephc_dom_native_element_get_attribute_node(
    void *element,
    const uint8_t *name,
    size_t name_length)
{
    char *name_string;
    xmlAttrPtr attribute;

    if (element == NULL) {
        return NULL;
    }
    name_string = elephc_dom_copy_c_string(name, name_length);
    if (name_string == NULL) {
        return NULL;
    }
    attribute = elephc_dom_attribute_by_qualified_name(
        (xmlNodePtr) element,
        (const xmlChar *) name_string
    );
    free(name_string);
    return attribute;
}

elephc_dom_native_buffer elephc_dom_native_element_get_attribute_ns(
    void *element,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *local_name,
    size_t local_name_length
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    char *namespace_string = NULL;
    char *name_string;
    xmlChar *value;

    if (element == NULL) {
        return result;
    }
    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            return result;
        }
    }
    name_string = elephc_dom_copy_c_string(local_name, local_name_length);
    if (name_string == NULL) {
        free(namespace_string);
        return result;
    }
    value = xmlGetNsProp(
        (xmlNodePtr) element,
        (const xmlChar *) name_string,
        namespace_string == NULL || namespace_string[0] == '\0'
            ? NULL
            : (const xmlChar *) namespace_string
    );
    free(namespace_string);
    free(name_string);
    if (value != NULL) {
        result.pointer = value;
        result.length = xmlStrlen(value);
    }
    return result;
}

void *elephc_dom_native_element_get_attribute_node_ns(
    void *element,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *local_name,
    size_t local_name_length
)
{
    char *namespace_string = NULL;
    char *name_string;
    xmlAttrPtr attribute;

    if (element == NULL) {
        return NULL;
    }
    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            return NULL;
        }
    }
    name_string = elephc_dom_copy_c_string(local_name, local_name_length);
    if (name_string == NULL) {
        free(namespace_string);
        return NULL;
    }
    attribute = xmlHasNsProp(
        (xmlNodePtr) element,
        (const xmlChar *) name_string,
        namespace_string == NULL || namespace_string[0] == '\0'
            ? NULL
            : (const xmlChar *) namespace_string
    );
    free(namespace_string);
    free(name_string);
    return attribute;
}

void *elephc_dom_native_element_set_attribute(
    void *element,
    const uint8_t *name,
    size_t name_length,
    const uint8_t *value,
    size_t value_length)
{
    char *name_string;
    char *value_string;
    xmlAttrPtr attribute;

    if (element == NULL) {
        return NULL;
    }
    name_string = elephc_dom_copy_c_string(name, name_length);
    value_string = elephc_dom_copy_c_string(value, value_length);
    if (name_string == NULL
        || value_string == NULL
        || xmlValidateName((const xmlChar *) name_string, 0) != 0) {
        free(name_string);
        free(value_string);
        return NULL;
    }
    attribute = xmlSetProp(
        (xmlNodePtr) element,
        (const xmlChar *) name_string,
        (const xmlChar *) value_string
    );
    free(name_string);
    free(value_string);
    return attribute;
}

elephc_dom_native_pointer_result elephc_dom_native_element_set_attribute_ns(
    void *element,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *qualified_name,
    size_t qualified_name_length,
    const uint8_t *value,
    size_t value_length,
    int32_t modern
)
{
    elephc_dom_native_pointer_result result = {NULL, 0, 0};
    xmlNodePtr node = (xmlNodePtr) element;
    char *namespace_string = NULL;
    char *name_string = NULL;
    char *value_string = NULL;
    xmlChar *local_name = NULL;
    xmlChar *prefix = NULL;
    xmlNsPtr namespace = NULL;

    if (node == NULL || node->doc == NULL) {
        return result;
    }
    if (namespace_uri != NULL || namespace_uri_length != 0) {
        namespace_string = elephc_dom_copy_c_string(
            namespace_uri,
            namespace_uri_length
        );
        if (namespace_string == NULL) {
            result.error_code = 11;
            goto done;
        }
    }
    name_string = elephc_dom_copy_c_string(
        qualified_name,
        qualified_name_length
    );
    value_string = elephc_dom_copy_c_string(value, value_length);
    if (name_string == NULL || value_string == NULL) {
        result.error_code = 11;
        goto done;
    }
    result.error_code = elephc_dom_validate_and_split_qname(
        namespace_string,
        name_string,
        modern,
        &local_name,
        &prefix
    );
    if (result.error_code != 0) {
        goto done;
    }
    namespace = elephc_dom_document_namespace(
        node->doc,
        modern != 0
                && namespace_string != NULL
                && xmlStrEqual(
                    (const xmlChar *) namespace_string,
                    elephc_dom_xmlns_namespace
                )
            ? NULL
            : node,
        namespace_string,
        prefix
    );
    if (namespace_string != NULL
        && namespace_string[0] != '\0'
        && namespace == NULL) {
        result.error_code = 11;
        goto done;
    }
    result.pointer = xmlSetNsProp(
        node,
        namespace,
        local_name,
        (const xmlChar *) value_string
    );
    if (result.pointer == NULL) {
        result.error_code = 11;
    }

done:
    xmlFree(local_name);
    xmlFree(prefix);
    free(namespace_string);
    free(name_string);
    free(value_string);
    return result;
}

static void elephc_dom_detach_attribute(
    xmlNodePtr element,
    xmlAttrPtr attribute
)
{
    xmlNsPtr removed_namespace =
        elephc_dom_namespace_attribute_mapping(attribute);

    if (attribute->prev == NULL) {
        element->properties = attribute->next;
    } else {
        attribute->prev->next = attribute->next;
    }
    if (attribute->next != NULL) {
        attribute->next->prev = attribute->prev;
    }
    attribute->parent = NULL;
    attribute->prev = NULL;
    attribute->next = NULL;
    if (removed_namespace != NULL) {
        elephc_dom_redefine_removed_namespace(
            element,
            removed_namespace
        );
    }
}

static void elephc_dom_attach_attribute(
    xmlNodePtr element,
    xmlAttrPtr attribute
)
{
    xmlAttrPtr last = element->properties;

    attribute->parent = element;
    attribute->prev = NULL;
    attribute->next = NULL;
    if (last == NULL) {
        element->properties = attribute;
        return;
    }
    while (last->next != NULL) {
        last = last->next;
    }
    last->next = attribute;
    attribute->prev = last;
}

int32_t elephc_dom_native_attribute_adopt(
    void *attribute,
    void *document
)
{
    xmlAttrPtr native_attribute = (xmlAttrPtr) attribute;
    xmlDocPtr target_document = (xmlDocPtr) document;
    xmlNsPtr namespace = NULL;
    char *href = NULL;
    char *prefix = NULL;

    if (native_attribute == NULL
        || native_attribute->type != XML_ATTRIBUTE_NODE
        || target_document == NULL) {
        return 0;
    }
    if (native_attribute->doc == target_document) {
        return 1;
    }
    if (native_attribute->ns != NULL
        && native_attribute->ns->href != NULL) {
        href = (char *) xmlStrdup(native_attribute->ns->href);
        if (native_attribute->ns->prefix != NULL) {
            prefix = (char *) xmlStrdup(native_attribute->ns->prefix);
        }
        if (href == NULL
            || (native_attribute->ns->prefix != NULL && prefix == NULL)) {
            xmlFree(href);
            xmlFree(prefix);
            return 0;
        }
    }
    xmlSetTreeDoc((xmlNodePtr) native_attribute, target_document);
    if (href != NULL) {
        namespace = elephc_dom_document_namespace(
            target_document,
            NULL,
            href,
            (const xmlChar *) prefix
        );
        xmlFree(href);
        xmlFree(prefix);
        if (namespace == NULL) {
            return 0;
        }
    }
    native_attribute->ns = namespace;
    return 1;
}

void *elephc_dom_native_element_set_attribute_node(
    void *element,
    void *attribute,
    int32_t use_namespace
)
{
    xmlNodePtr native_element = (xmlNodePtr) element;
    xmlAttrPtr native_attribute = (xmlAttrPtr) attribute;
    xmlAttrPtr previous;

    if (native_element == NULL
        || native_attribute == NULL
        || native_attribute->type != XML_ATTRIBUTE_NODE) {
        return NULL;
    }
    previous = use_namespace != 0
        && native_attribute->ns != NULL
        && native_attribute->ns->href != NULL
        ? xmlHasNsProp(
            native_element,
            native_attribute->name,
            native_attribute->ns->href
        )
        : xmlHasProp(native_element, native_attribute->name);
    if (previous == native_attribute) {
        return NULL;
    }
    if (previous != NULL) {
        elephc_dom_detach_attribute(native_element, previous);
    }
    if (native_attribute->parent != NULL) {
        elephc_dom_detach_attribute(
            native_attribute->parent,
            native_attribute
        );
    }
    elephc_dom_attach_attribute(native_element, native_attribute);
    if (native_element->doc != NULL) {
        xmlReconciliateNs(native_element->doc, native_element);
    }
    return previous;
}

int32_t elephc_dom_native_element_remove_attribute_node(
    void *element,
    void *attribute
)
{
    xmlNodePtr native_element = (xmlNodePtr) element;
    xmlAttrPtr native_attribute = (xmlAttrPtr) attribute;

    if (native_element == NULL
        || native_attribute == NULL
        || native_attribute->type != XML_ATTRIBUTE_NODE
        || native_attribute->parent != native_element) {
        return 0;
    }
    elephc_dom_detach_attribute(native_element, native_attribute);
    return 1;
}

void *elephc_dom_native_element_remove_attribute(
    void *element,
    const uint8_t *name,
    size_t name_length)
{
    char *name_string;
    xmlAttrPtr attribute;

    if (element == NULL) {
        return NULL;
    }
    name_string = elephc_dom_copy_c_string(name, name_length);
    if (name_string == NULL) {
        return NULL;
    }
    attribute = elephc_dom_attribute_by_qualified_name(
        (xmlNodePtr) element,
        (const xmlChar *) name_string
    );
    if (attribute == NULL) {
        free(name_string);
        return NULL;
    }
    elephc_dom_detach_attribute((xmlNodePtr) element, attribute);
    free(name_string);
    return attribute;
}

static xmlNsPtr elephc_dom_legacy_namespace_declaration(
    xmlNodePtr element,
    const xmlChar *local_name
)
{
    xmlNsPtr namespace;

    if (element == NULL) {
        return NULL;
    }
    for (namespace = element->nsDef; namespace != NULL;
        namespace = namespace->next) {
        if ((local_name == NULL || local_name[0] == '\0')
                ? namespace->prefix == NULL && namespace->href != NULL
                : namespace->prefix != NULL
                    && xmlStrEqual(local_name, namespace->prefix)) {
            return namespace;
        }
    }
    return NULL;
}

static void elephc_dom_clear_eliminated_namespace(
    xmlNodePtr element,
    xmlNsPtr eliminated
)
{
    xmlNodePtr current = element;

    while (current != NULL) {
        if (current->type == XML_ELEMENT_NODE) {
            xmlAttrPtr attribute;

            if (current->ns == eliminated) {
                current->ns = NULL;
            }
            for (attribute = current->properties; attribute != NULL;
                attribute = attribute->next) {
                if (attribute->ns == eliminated) {
                    attribute->ns = NULL;
                }
            }
        }
        current = elephc_dom_next_descendant(current, element);
    }
}

static void elephc_dom_preserve_eliminated_namespace(
    xmlDocPtr document,
    xmlNsPtr namespace
)
{
    if (document == NULL) {
        return;
    }
    if (document->oldNs == NULL) {
        document->oldNs = xmlMalloc(sizeof(*document->oldNs));
        if (document->oldNs == NULL) {
            return;
        }
        memset(document->oldNs, 0, sizeof(*document->oldNs));
        document->oldNs->type = XML_LOCAL_NAMESPACE;
        document->oldNs->href = xmlStrdup(elephc_dom_xml_namespace);
        document->oldNs->prefix = xmlStrdup((const xmlChar *) "xml");
    } else {
        namespace->next = document->oldNs->next;
    }
    document->oldNs->next = namespace;
}

static void elephc_dom_eliminate_legacy_namespace(
    xmlNodePtr element,
    xmlNsPtr namespace
)
{
    xmlNsPtr *link = &element->nsDef;

    if (namespace->href != NULL) {
        xmlFree((void *) namespace->href);
        namespace->href = NULL;
    }
    if (namespace->prefix != NULL) {
        xmlFree((void *) namespace->prefix);
        namespace->prefix = NULL;
    }
    while (*link != NULL && *link != namespace) {
        link = &(*link)->next;
    }
    if (*link == namespace) {
        *link = namespace->next;
    }
    namespace->next = NULL;
    elephc_dom_preserve_eliminated_namespace(
        element->doc,
        namespace
    );
    elephc_dom_clear_eliminated_namespace(element, namespace);
}

void *elephc_dom_native_element_remove_attribute_ns(
    void *element,
    const uint8_t *namespace_uri,
    size_t namespace_uri_length,
    const uint8_t *local_name,
    size_t local_name_length,
    int32_t legacy
)
{
    xmlNodePtr node = (xmlNodePtr) element;
    char *namespace_string = NULL;
    char *name_string = NULL;
    xmlNsPtr declaration = NULL;
    xmlAttrPtr attribute = elephc_dom_native_element_get_attribute_node_ns(
        element,
        namespace_uri,
        namespace_uri_length,
        local_name,
        local_name_length
    );

    if (node == NULL) {
        return NULL;
    }
    if (legacy != 0) {
        if (namespace_uri != NULL || namespace_uri_length != 0) {
            namespace_string = elephc_dom_copy_c_string(
                namespace_uri,
                namespace_uri_length
            );
            if (namespace_string == NULL) {
                return NULL;
            }
        }
        name_string = elephc_dom_copy_c_string(
            local_name,
            local_name_length
        );
        if (name_string == NULL) {
            free(namespace_string);
            return NULL;
        }
        declaration = elephc_dom_legacy_namespace_declaration(
            node,
            (const xmlChar *) name_string
        );
        if (declaration != NULL) {
            if (!xmlStrEqual(
                    (const xmlChar *) namespace_string,
                    declaration->href
                )) {
                free(namespace_string);
                free(name_string);
                return NULL;
            }
            elephc_dom_eliminate_legacy_namespace(node, declaration);
        }
        free(namespace_string);
        free(name_string);
    }
    if (attribute != NULL) {
        elephc_dom_detach_attribute(node, attribute);
    }
    return attribute;
}

size_t elephc_dom_native_element_attribute_count(
    void *element,
    int32_t include_namespace_declarations
)
{
    xmlNodePtr node = (xmlNodePtr) element;
    xmlAttrPtr attribute;
    xmlNsPtr namespace;
    size_t count = 0;

    if (node == NULL) {
        return 0;
    }
    if (include_namespace_declarations != 0) {
        for (namespace = node->nsDef; namespace != NULL;
            namespace = namespace->next) {
            count++;
        }
    }
    for (attribute = node->properties; attribute != NULL;
        attribute = attribute->next) {
        count++;
    }
    return count;
}

void *elephc_dom_native_element_attribute_at(
    void *element,
    size_t index
)
{
    xmlAttrPtr attribute =
        element == NULL ? NULL : ((xmlNodePtr) element)->properties;

    while (attribute != NULL && index != 0) {
        attribute = attribute->next;
        index--;
    }
    return attribute;
}

elephc_dom_native_buffer elephc_dom_native_element_attribute_name_at(
    void *element,
    size_t index,
    int32_t include_namespace_declarations
)
{
    elephc_dom_native_buffer result = {NULL, 0};
    xmlNodePtr node = (xmlNodePtr) element;
    xmlAttrPtr attribute;
    xmlNsPtr namespace;

    if (node == NULL) {
        return result;
    }
    if (include_namespace_declarations != 0) {
        for (namespace = node->nsDef; namespace != NULL;
            namespace = namespace->next) {
            if (index-- == 0) {
                result.pointer = namespace->prefix == NULL
                    ? xmlStrdup((const xmlChar *) "xmlns")
                    : xmlBuildQName(
                        namespace->prefix,
                        (const xmlChar *) "xmlns",
                        NULL,
                        0
                    );
                if (result.pointer != NULL) {
                    result.length = xmlStrlen(result.pointer);
                }
                return result;
            }
        }
    }
    for (attribute = node->properties; attribute != NULL;
        attribute = attribute->next) {
        if (index-- == 0) {
            return elephc_dom_native_node_name(attribute);
        }
    }
    return result;
}

void *elephc_dom_native_element_first_child(void *element)
{
    xmlNodePtr child =
        element == NULL ? NULL : ((xmlNodePtr) element)->children;

    while (child != NULL && child->type != XML_ELEMENT_NODE) {
        child = child->next;
    }
    return child;
}

void *elephc_dom_native_element_last_child(void *element)
{
    xmlNodePtr child =
        element == NULL ? NULL : ((xmlNodePtr) element)->last;

    while (child != NULL && child->type != XML_ELEMENT_NODE) {
        child = child->prev;
    }
    return child;
}

void *elephc_dom_native_element_previous_sibling(void *element)
{
    xmlNodePtr sibling =
        element == NULL ? NULL : ((xmlNodePtr) element)->prev;

    while (sibling != NULL && sibling->type != XML_ELEMENT_NODE) {
        sibling = sibling->prev;
    }
    return sibling;
}

void *elephc_dom_native_element_next_sibling(void *element)
{
    xmlNodePtr sibling =
        element == NULL ? NULL : ((xmlNodePtr) element)->next;

    while (sibling != NULL && sibling->type != XML_ELEMENT_NODE) {
        sibling = sibling->next;
    }
    return sibling;
}

int64_t elephc_dom_native_element_child_count(void *element)
{
    xmlNodePtr child =
        element == NULL ? NULL : ((xmlNodePtr) element)->children;
    int64_t count = 0;

    while (child != NULL) {
        if (child->type == XML_ELEMENT_NODE) {
            count++;
        }
        child = child->next;
    }
    return count;
}

void elephc_dom_native_node_free(void *node)
{
    if (node == NULL) {
        return;
    }
    switch (((xmlNodePtr) node)->type) {
        case XML_ATTRIBUTE_NODE:
            xmlFreeProp((xmlAttrPtr) node);
            break;
        case XML_DTD_NODE:
        case XML_DOCUMENT_TYPE_NODE:
            xmlFreeDtd((xmlDtdPtr) node);
            break;
        default:
            elephc_dom_free_template_fragments((xmlNodePtr) node);
            xmlFreeNode((xmlNodePtr) node);
            break;
    }
}
