// The C view of the probe library's ABI.
//
// Swift rejects a Swift-declared struct in a `@convention(c)` signature, and a
// statically linked export is reached as an ordinary C symbol, so both the
// return type and the entry points are declared here and imported through the
// bridging header.

#ifndef ELEPHC_PROBE_ABI_H
#define ELEPHC_PROBE_ABI_H

#include <stddef.h>
#include <stdint.h>

/// What a string-returning `#[Export]` hands back: a pointer and a length.
/// The buffer is owned by the caller and released with `elephc_free`. It is a
/// PHP byte string, so `len` is authoritative and `strlen` is wrong.
typedef struct {
    const char *ptr;
    size_t len;
} ElephcStr;

/// Prepares heap and globals. Returns 0 on success.
int32_t elephc_init(void);

/// Releases a buffer an export returned. Null-safe.
void elephc_free(void *pointer);

/// Runs every check against `writable_dir` and returns the report.
ElephcStr probe(const char *writable_dir, size_t length);

#endif /* ELEPHC_PROBE_ABI_H */
