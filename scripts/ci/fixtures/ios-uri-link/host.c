#include "libmain.h"

int main(void) {
    if (elephc_abi_version() != ELEPHC_ABI_VERSION) return 1;
    if (elephc_init() != ELEPHC_STATUS_OK) return 2;
    if (ios_uri_link_smoke() != 0) return 3;
    elephc_shutdown();
    return 0;
}
