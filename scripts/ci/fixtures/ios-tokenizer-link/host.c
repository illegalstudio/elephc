#include "libmain.h"

int main(void) {
    if (elephc_abi_version() != ELEPHC_ABI_VERSION) return 1;
    if (elephc_init() != ELEPHC_STATUS_OK) return 2;
    (void)ios_tokenizer_link_smoke();
    elephc_shutdown();
    return 0;
}
