/*
 * Purpose: fixed-width C ABI between Elephc's Intl bridge and the managed ICU build.
 * Called from: libelephc_intl (Rust staticlib) and the managed ICU recipe.
 * Key details:
 * - Handles are opaque int64 identifiers owned by this shim; Rust never dereferences them.
 * - Args and results use a compact binary type/value stream owned by this shim.
 * - No process-global returned text buffer: callers supply output storage and receive the
 *   exact required length, matching the other managed native shims.
 * - ICU's version and data-version strings are queried through sizing calls.
 */
#ifndef ELEPHC_ICU_INTL_SHIM_H
#define ELEPHC_ICU_INTL_SHIM_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opens one ICU-backed Intl object. `spec` is a binary value stream. Returns 0 on failure
 * and stores the ICU error code in *error. */
int64_t elephc_icu_intl_open(const char *kind, const uint8_t *spec, uint64_t spec_len,
                             int32_t *error);

/* Releases one handle. Unknown, zero, and already-released handles are ignored. */
void elephc_icu_intl_close(int64_t handle);

/* Clones one handle into a new independent handle. Returns 0 on failure. */
int64_t elephc_icu_intl_clone(int64_t handle, int32_t *error);

/* Runs an instance method. `args` is a binary value stream. The encoded result value is
 * written to `out`; `*needed` always receives the exact byte count, and a zero-capacity
 * call is a successful sizing query. Returns the ICU error code (0 on success). */
int32_t elephc_icu_intl_call(int64_t handle, const char *method,
                             const uint8_t *args, uint64_t args_len,
                             uint8_t *out, uint64_t out_cap, uint64_t *needed,
                             int32_t *error);

/* Runs a class-level operation such as Locale::getDefault or Normalizer::normalize.
 * `kind` names the Intl family and `method` names the operation. */
int32_t elephc_icu_intl_static(const char *kind, const char *method,
                               const uint8_t *args, uint64_t args_len,
                               uint8_t *out, uint64_t out_cap, uint64_t *needed,
                               int32_t *error);

/* Returns the last ICU error code recorded for one handle (0 when the handle is gone). */
int32_t elephc_icu_intl_error_code(int64_t handle);

/* Copies the last error message for one handle, without a trailing NUL. */
int32_t elephc_icu_intl_error_message(int64_t handle, char *buffer, uint64_t capacity,
                                      uint64_t *needed);

/* Returns the number of live handles, for leak regressions. */
int64_t elephc_icu_intl_live_count(void);

/* Copies the ICU symbolic error name for one status code (e.g. "U_ILLEGAL_ARGUMENT_ERROR"). */
int32_t elephc_icu_intl_error_name(int32_t code, char *buffer, uint64_t capacity,
                                   uint64_t *needed);

/* Copies the ICU version string (e.g. "78.3"). */
int32_t elephc_icu_intl_version(char *buffer, uint64_t capacity, uint64_t *needed);

/* Copies the ICU data version string (e.g. "78.3"). */
int32_t elephc_icu_intl_data_version(char *buffer, uint64_t capacity, uint64_t *needed);

#ifdef __cplusplus
}
#endif

#endif
