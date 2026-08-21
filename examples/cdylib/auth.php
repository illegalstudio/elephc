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
    // Returns 0 on accept, 1 on reject through the stable scalar C signature.
    if (strlen($token) >= token_min_length()) {
        return 0;
    }
    return 1;
}

#[Export]
function add_i64(int $a, int $b): int {
    return $a + $b;
}

#[Export]
function scalar_failure(int $value): int {
    if ($value === 7) {
        throw new RuntimeException("requested scalar failure");
    }
    throw new RuntimeException("unexpected scalar failure input");
}

#[Export]
function roundtrip(string $input): string {
    if ($input === "__elephc_fail__") {
        throw new RuntimeException("requested roundtrip failure");
    }

    return $input;
}
