<?php
// Minimal end-to-end demo for `elephc --emit cdylib`.
//
// Build:
//   elephc --emit cdylib examples/cdylib/auth.php
//   # produces examples/cdylib/libauth.so (Linux) or libauth.dylib (macOS)
//
// Then build the C harness and run it (see host.c).

// A non-exported helper. Internal-only; the C-ABI surface never sees it.
function token_min_length(): int {
    return 8;
}

#[Export]
function validate_token(string $token): int {
    // Returns 0 on accept, 1 on reject. Matches the v1 error-channel convention
    // (`int32_t` status, outparams for richer data, `elephc_last_error()` for a
    // human-readable message — none of which we need for this trivial demo).
    if (strlen($token) >= token_min_length()) {
        return 0;
    }
    return 1;
}

#[Export]
function add_i64(int $a, int $b): int {
    return $a + $b;
}
