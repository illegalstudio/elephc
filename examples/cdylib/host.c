// C harness for the Elephc cdylib demo. It includes the generated header,
// loads libauth.{so,dylib}, and exercises scalar plus owned-string exports.
//
// Linux: cc -o examples/cdylib/host examples/cdylib/host.c -ldl
// macOS: cc -o examples/cdylib/host examples/cdylib/host.c

#include "libauth.h"

#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint32_t (*abi_version_fn)(void);
typedef int32_t (*init_fn)(void);
typedef void (*shutdown_fn)(void);
typedef int32_t (*last_status_fn)(void);
typedef const char *(*last_error_fn)(void);
typedef void (*free_fn)(void *);
typedef int32_t (*validate_token_fn)(const char *, size_t);
typedef int64_t (*add_i64_fn)(int64_t, int64_t);
typedef int64_t (*scalar_failure_fn)(int64_t);
typedef int32_t (*roundtrip_fn)(const char *, size_t, char **, size_t *);

static void *must_sym(void *lib, const char *name) {
    void *symbol = dlsym(lib, name);
    if (!symbol) {
        fprintf(stderr, "missing symbol '%s': %s\n", name, dlerror());
        exit(2);
    }
    return symbol;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <path-to-libauth.{so,dylib}>\n", argv[0]);
        return 1;
    }
    void *lib = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!lib) {
        fprintf(stderr, "dlopen failed: %s\n", dlerror());
        return 2;
    }

    abi_version_fn abi_version = (abi_version_fn)must_sym(lib, "elephc_abi_version");
    init_fn init = (init_fn)must_sym(lib, "elephc_init");
    shutdown_fn shutdown = (shutdown_fn)must_sym(lib, "elephc_shutdown");
    last_status_fn last_status = (last_status_fn)must_sym(lib, "elephc_last_status");
    last_error_fn last_error = (last_error_fn)must_sym(lib, "elephc_last_error");
    free_fn release = (free_fn)must_sym(lib, "elephc_free");
    validate_token_fn validate = (validate_token_fn)must_sym(lib, "validate_token");
    add_i64_fn add = (add_i64_fn)must_sym(lib, "add_i64");
    scalar_failure_fn scalar_failure =
        (scalar_failure_fn)must_sym(lib, "scalar_failure");
    roundtrip_fn roundtrip_export = (roundtrip_fn)must_sym(lib, "roundtrip");

    if (abi_version() != ELEPHC_ABI_VERSION || init() != ELEPHC_STATUS_OK) {
        fprintf(stderr, "Elephc ABI initialization failed\n");
        return 3;
    }
    if (scalar_failure(7) != 0 || last_status() != ELEPHC_STATUS_PHP_EXCEPTION ||
        last_error() == NULL) {
        return 4;
    }
    if (add(40, 2) != 42 || last_status() != ELEPHC_STATUS_OK ||
        last_error() != NULL || validate("longenoughtoken", 15) != 0 ||
        validate("abc", 3) != 1) {
        return 5;
    }

    const char binary[] = {'A', 0, 'B'};
    char *output = NULL;
    size_t output_len = 0;
    if (roundtrip_export(binary, sizeof(binary), &output, &output_len) !=
            ELEPHC_STATUS_OK ||
        output_len != sizeof(binary) || memcmp(output, binary, sizeof(binary)) != 0) {
        return 6;
    }
    release(output);

    output = NULL;
    output_len = 0;
    static const char requested_failure[] = "__elephc_fail__";
    if (roundtrip_export(requested_failure, sizeof(requested_failure) - 1, &output, &output_len) !=
            ELEPHC_STATUS_PHP_EXCEPTION ||
        output != NULL || output_len != 0 || last_error() == NULL) {
        return 7;
    }
    if (roundtrip_export("alive", 5, &output, &output_len) != ELEPHC_STATUS_OK ||
        output_len != 5 || memcmp(output, "alive", 5) != 0 || last_error() != NULL) {
        return 8;
    }
    release(output);
    release(NULL);

    shutdown();
    dlclose(lib);
    puts("elephc cdylib demo OK: recoverable scalar ABI + binary string roundtrip");
    return 0;
}
