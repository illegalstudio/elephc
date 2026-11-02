---
title: "ctype: real LC_CTYPE via libc setlocale, ctype is* predicates, and PCRE2 tables"
author: Guillaume
date: 2026-11-02
branch: feat/ctype-locale
touches:
  - Cargo.lock
  - Cargo.toml
  - ROADMAP.md
  - crates/elephc-builtin-contract/src/catalog_data.rs
  - crates/elephc-builtin-contract/src/registry.rs
  - crates/elephc-builtin-contract/src/support.rs
  - crates/elephc-magician/src/interpreter/builtins/hooks/direct.rs
  - crates/elephc-magician/src/interpreter/builtins/hooks/values.rs
  - crates/elephc-magician/src/interpreter/builtins/string/ctype_alnum.rs
  - crates/elephc-magician/src/interpreter/builtins/string/ctype_alpha.rs
---

## Decided
- ctype byte classification now calls libc `isalnum`/`isalpha`/`iscntrl`/`isdigit`/`isgraph`/`islower`/`isprint`/`ispunct`/`isspace`/`isupper`/`isxdigit` through `__rt_ctype_byte`, so the active `LC_CTYPE` is authoritative. The C locale still answers the original ASCII matrix.
- `setlocale()` is a synthetic-AST prelude over two internal libc builtins: `__elephc_setlocale_query(category)` and `__elephc_setlocale_set(category, locale)`. The prelude tries variadic locale names in order and treats integer `0`/string `"0"` as the query form.
- `LC_ALL`, `LC_CTYPE`, and the other `LC_*` constants are emitted with Apple BSD libc numbers on macOS/iOS and glibc numbers on Linux.
- `preg_*` no longer forces `LC_CTYPE=C.UTF-8`. The PCRE2 shim builds `pcre2_maketables()` for the current locale, sets them on a compile context, and uses `pcre2_code_copy_with_tables()` so the code owns a stable copy before the temporary table block is freed. Both the extended and ordinary POSIX compile paths use it.
- The locale work was isolated in `.claude/worktrees/ctype-locale` to avoid the concurrent `ext/intl` edits in the main checkout.

## Tried and ruled out
- Keeping `emit_prepare_regex_locale()` setting `C.UTF-8`: it overwrote an explicit `setlocale(LC_CTYPE,"de_DE")`, made `setlocale(...,"0")` report `C.UTF-8`, and made `ctype_lower("\xe4")` true before any explicit locale change on macOS.
- Only changing the extended PCRE2 compile path: ordinary `preg_match` uses the POSIX `pcre2_regcomp` path, so the locale tables were never installed. Both paths now share the locale-aware compiler.
- Letting `pcre2_maketables()` storage live only until compile return: the code keeps a pointer to the tables. `pcre2_code_copy_with_tables()` plus `pcre2_maketables_free()` gives each code a stable copy without leaking.

## Watch out
- AArch64 `__rt_ctype_string` now calls `__rt_ctype_byte`, so it must save `x29`/`x30`; forgetting that returns into the middle of the scan loop (the first implementation did exactly that).
- `setlocale` returns a pointer to a static libc buffer; always `strlen` + `__rt_str_persist` it before the next locale change.
- Locale names are copied into a fixed 256-byte NUL-terminated stack buffer; names longer than 255 bytes are refused with `false` instead of truncated.
- The worktree test provider compiles the embedded PCRE2 shim from `include_str!`, so a shim change rebuilds the test binary but the production managed package still needs its normal source-hash rebuild.
