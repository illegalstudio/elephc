/*
 * Test-only libxml allocation-failure instrumentation. This translation unit
 * remains unlinked from production binaries because only Rust cfg(test) code
 * references its exported entry points.
 */

#include <stddef.h>
#include <stdint.h>

#include <libxml/parser.h>
#include <libxml/xmlmemory.h>

#if defined(__GNUC__) || defined(__clang__)
#define ELEPHC_DOM_TEST_EXPORT __attribute__((visibility("hidden")))
#else
#define ELEPHC_DOM_TEST_EXPORT
#endif

typedef struct {
    uint64_t context_id;
    xmlParserCtxtPtr parser;
    int32_t host_status;
} elephc_dom_resource_loader_context;

extern xmlParserErrors elephc_dom_resource_loader(
    void *opaque,
    const char *url,
    const char *public_id,
    xmlResourceType type,
    xmlParserInputFlags flags,
    xmlParserInput **out
);

static xmlFreeFunc elephc_dom_test_xml_free_original;
static xmlMallocFunc elephc_dom_test_xml_malloc_original;
static xmlReallocFunc elephc_dom_test_xml_realloc_original;
static xmlStrdupFunc elephc_dom_test_xml_strdup_original;
static int elephc_dom_test_xml_memory_hooks_installed = 0;
static _Thread_local size_t elephc_dom_test_xml_allocation_count = 0;
static _Thread_local size_t elephc_dom_test_xml_fail_allocation = 0;

static void *elephc_dom_test_xml_malloc(size_t size)
{
    if (elephc_dom_test_xml_fail_allocation != 0) {
        elephc_dom_test_xml_allocation_count++;
        if (elephc_dom_test_xml_allocation_count
            == elephc_dom_test_xml_fail_allocation) {
            return NULL;
        }
    }
    return elephc_dom_test_xml_malloc_original(size);
}

static void elephc_dom_test_xml_free(void *pointer)
{
    elephc_dom_test_xml_free_original(pointer);
}

static void *elephc_dom_test_xml_realloc(void *pointer, size_t size)
{
    return elephc_dom_test_xml_realloc_original(pointer, size);
}

static char *elephc_dom_test_xml_strdup(const char *value)
{
    return elephc_dom_test_xml_strdup_original(value);
}

ELEPHC_DOM_TEST_EXPORT int elephc_dom_native_test_install_resource_loader_allocator(void)
{
    if (elephc_dom_test_xml_memory_hooks_installed != 0) {
        return 1;
    }
    if (xmlMemGet(
            &elephc_dom_test_xml_free_original,
            &elephc_dom_test_xml_malloc_original,
            &elephc_dom_test_xml_realloc_original,
            &elephc_dom_test_xml_strdup_original
        ) != 0
        || elephc_dom_test_xml_free_original == NULL
        || elephc_dom_test_xml_malloc_original == NULL
        || elephc_dom_test_xml_realloc_original == NULL
        || elephc_dom_test_xml_strdup_original == NULL
        || xmlMemSetup(
            elephc_dom_test_xml_free,
            elephc_dom_test_xml_malloc,
            elephc_dom_test_xml_realloc,
            elephc_dom_test_xml_strdup
        ) != 0) {
        return 0;
    }
    elephc_dom_test_xml_memory_hooks_installed = 1;
    return 1;
}

ELEPHC_DOM_TEST_EXPORT int elephc_dom_native_test_resource_loader_input_from_io_failure(
    uint64_t host_context,
    size_t failing_allocation
)
{
    elephc_dom_resource_loader_context loader = {host_context, NULL, 0};
    xmlParserInput *input = NULL;
    xmlParserErrors status;

    if (elephc_dom_test_xml_memory_hooks_installed == 0
        || (failing_allocation != 1 && failing_allocation != 4)) {
        return 0;
    }
    elephc_dom_test_xml_allocation_count = 0;
    elephc_dom_test_xml_fail_allocation = failing_allocation;
    status = elephc_dom_resource_loader(
        &loader,
        "elephc-test-resource",
        NULL,
        (xmlResourceType) 0,
        0,
        &input
    );
    elephc_dom_test_xml_fail_allocation = 0;
    elephc_dom_test_xml_allocation_count = 0;
    return input == NULL && status == XML_ERR_NO_MEMORY && loader.host_status == 0
        ? 0x494F434C
        : 0;
}
