// C harness for the elephc cdylib demo. Loads libauth.{so,dylib} via dlopen,
// resolves the four lifecycle entry points plus the two exported PHP
// functions, and asserts they behave per the v1 contract.
//
// Build (Linux):
//   cc -o host examples/cdylib/host.c -ldl
// Build (macOS):
//   cc -o host examples/cdylib/host.c
// Run:
//   ./host examples/cdylib/libauth.so       # or libauth.dylib on macOS

#include <assert.h>
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef int32_t (*elephc_init_fn)(void);
typedef void (*elephc_shutdown_fn)(void);
typedef const char *(*elephc_last_error_fn)(void);

typedef int32_t (*validate_token_fn)(const char *ptr, size_t len);
typedef int64_t (*add_i64_fn)(int64_t a, int64_t b);

static void *must_sym(void *lib, const char *name) {
    void *sym = dlsym(lib, name);
    if (!sym) {
        fprintf(stderr, "missing symbol '%s': %s\n", name, dlerror());
        exit(2);
    }
    return sym;
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

    elephc_init_fn init = (elephc_init_fn)must_sym(lib, "elephc_init");
    elephc_shutdown_fn shutdown = (elephc_shutdown_fn)must_sym(lib, "elephc_shutdown");
    elephc_last_error_fn last_error = (elephc_last_error_fn)must_sym(lib, "elephc_last_error");
    validate_token_fn validate_token = (validate_token_fn)must_sym(lib, "validate_token");
    add_i64_fn add_i64 = (add_i64_fn)must_sym(lib, "add_i64");

    if (init() != 0) {
        fprintf(stderr, "elephc_init failed: %s\n", last_error() ? last_error() : "(no message)");
        return 3;
    }

    // Scalar args: add_i64 round-trips through C ABI registers directly.
    int64_t sum = add_i64(40, 2);
    if (sum != 42) {
        fprintf(stderr, "add_i64(40, 2) = %lld, expected 42\n", (long long)sum);
        return 4;
    }

    // String-in marshaling: ptr + len pair lands in two consecutive int regs.
    const char *ok = "longenoughtoken";
    int32_t rc_ok = validate_token(ok, strlen(ok));
    if (rc_ok != 0) {
        fprintf(stderr, "validate_token(\"%s\") = %d, expected 0\n", ok, rc_ok);
        return 5;
    }
    const char *short_tok = "abc";
    int32_t rc_short = validate_token(short_tok, strlen(short_tok));
    if (rc_short != 1) {
        fprintf(stderr, "validate_token(\"%s\") = %d, expected 1\n", short_tok, rc_short);
        return 6;
    }

    shutdown();
    dlclose(lib);
    printf("elephc cdylib demo OK: add_i64(40,2)=%lld, validate_token long=%d short=%d\n",
           (long long)sum, rc_ok, rc_short);
    return 0;
}
