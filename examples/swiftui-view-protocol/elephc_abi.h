// The C view of elephc's cdylib string ABI.
//
// Swift refuses a Swift-declared struct in a `@convention(c)` function type --
// it is not C-representable, so the compiler cannot know it rides the platform's
// aggregate-return registers. Declaring it here and importing the header makes
// `ElephcStr` a genuine C type, after which the function pointers type-check and
// the calls follow the same ABI a C host would use.

#ifndef ELEPHC_ABI_H
#define ELEPHC_ABI_H

#include <stddef.h>
#include <stdint.h>

/// What a string-returning `#[Export]` hands back: a pointer and a length.
///
/// The buffer is owned by the caller and must be released with `elephc_free`.
/// It is a PHP byte string -- not NUL-terminated, and free to contain interior
/// zero bytes -- so `len` is authoritative and `strlen` is wrong.
typedef struct {
    const char *ptr;
    size_t len;
} ElephcStr;

// --- lifecycle -------------------------------------------------------------

/// Prepares heap and globals. Returns 0 on success.
int32_t elephc_init(void);

/// Releases a buffer an export returned. Null-safe.
void elephc_free(void *pointer);

// --- the exports view.php declares with #[Export] ---------------------------

/// Returns the current view tree as serialized JSON.
ElephcStr render_view(void);

/// Applies an action and returns the next view tree.
ElephcStr dispatch(const char *action, size_t action_length);

#endif /* ELEPHC_ABI_H */
