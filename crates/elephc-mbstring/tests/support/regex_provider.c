/* Exercise real decoder lookahead at an inaccessible page boundary without requiring sanitizer builds. */
#include "elephc_oniguruma.h"
#include <fcntl.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

/* Return a numbered setup/provider failure; an unpadded native read fails at the protected page. */
int elephc_test_guarded_subjects(void) {
    const elephc_onig_provider_v1 *provider = elephc_oniguruma_v1_provider();
    long page_size = sysconf(_SC_PAGESIZE);
    if (page_size < 32) return 1;
    int descriptor = open("/dev/zero", O_RDWR);
    if (descriptor < 0) return 2;
    uint8_t *pages = mmap(NULL, (size_t)page_size * 2, PROT_READ | PROT_WRITE, MAP_PRIVATE, descriptor, 0);
    close(descriptor);
    if (pages == MAP_FAILED) return 3;
    if (mprotect(pages + page_size, (size_t)page_size, PROT_NONE)) {
        munmap(pages, (size_t)page_size * 2);
        return 4;
    }
    const uint8_t subjects[][12] = {
        {0xc3, 0xa9, 0xce, 0xb1, 0xf0, 0x9f, 0xa6, 0x80},
        {0, 0xe9, 3, 0xb1, 0xd8, 0x3e, 0xdd, 0x80},
        {0xe9, 0, 0xb1, 3, 0x3e, 0xd8, 0x80, 0xdd},
        {0, 0, 0, 0xe9, 0, 0, 3, 0xb1, 0, 1, 0xf9, 0x80},
        {0xe9, 0, 0, 0, 0xb1, 3, 0, 0, 0x80, 0xf9, 1, 0},
    };
    const char *patterns[] = {".", "$", "(.*)"};
    int failure = 0;
    for (unsigned encoding = 1; encoding <= 5 && !failure; encoding++) {
        size_t length = encoding <= 3 ? 8 : 12;
        uint8_t *subject = pages + page_size - length;
        memcpy(subject, subjects[encoding - 1], length);
        for (unsigned pattern_index = 0; pattern_index < 3 && !failure; pattern_index++) {
            const char *text = patterns[pattern_index];
            size_t width = encoding == 1 ? 1 : encoding <= 3 ? 2 : 4;
            uint8_t pattern[16] = {0}, error[256] = {0};
            for (size_t byte = 0; byte < strlen(text); byte++) {
                size_t index = byte * width + (encoding == 2 || encoding == 4 ? width - 1 : 0);
                pattern[index] = (uint8_t)text[byte];
            }
            void *regex = NULL;
            elephc_onig_compile_v1 compile = {pattern, strlen(text) * width, encoding, 12, 0, 0};
            if (provider->compile(&compile, &regex, error, sizeof(error)) || !regex) { failure = 5; break; }
            for (size_t offset = 0; offset <= length; offset++) {
                void *region = NULL;
                elephc_onig_search_v1 search = {subject, length, offset, 0, 3, 100000, 1000000};
                int64_t status = provider->search(regex, &search, &region);
                if (status < -1 || (status >= 0 && !region)) failure = 6;
                provider->free_region(region);
            }
            provider->free_regex(regex);
        }
    }
    munmap(pages, (size_t)page_size * 2);
    return failure;
}
