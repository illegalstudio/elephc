---
title: "Regex"
description: "PCRE2-backed regular expressions, preg_* functions, SPL regex iterators, and the managed pcre2 package."
sidebar:
  order: 7
---

elephc implements PHP regular expressions with PCRE2 through an Elephc-owned
opaque shim over PCRE2's POSIX-compatible wrapper. Regex support is pay-for-use:
programs without regex do not link PCRE2, while programs using `preg_*`,
`mb_ereg_match()`, `RegexIterator`, or `RecursiveRegexIterator` require the
managed `pcre2` package during the final native link.

## Install the managed package

Building the Elephc compiler itself does not require PCRE2. A project that links
a regex-enabled program declares and installs it once:

```bash
cd path/to/project
elephc native add pcre2
```

The repository's minimal `examples/hello-preg/` project shows this exact
first-run flow with a committed manifest and lock.

This creates or updates `elephc.toml`, writes a deterministic `elephc.lock`, and
builds the exact catalogued PCRE2 10.47 source into the target/toolchain cache.
Commit both project files. The final linker uses verified exact archive paths;
there is no production fallback to Homebrew, apt, Alpine packages, custom
`--link-path` values, or raw `--link pcre2-*` flags.

Other contributors and CI install from the committed state with:

```bash
elephc native install --locked
```

Use `--offline` when the verified source and artifact are already cached. See
[Native dependencies](../compiling/native-dependencies.md) for command,
toolchain, cache, and integrity details.

## Compiling a regex program

```php
<?php
$subject = "order-42";

if (preg_match('/order-(\d+)/', $subject, $matches)) {
    echo $matches[1];
}
```

Compile and run it like any other project source:

```bash
elephc path/to/program.php
./path/to/program
```

Compilation never downloads or builds PCRE2. Missing project state reports the
discovered/search path and a copy-paste `recovery:` line containing
`elephc native add pcre2` or `elephc native install --locked --target <target>`.
Front-end-only `--check`, `--emit-ir`, and `--emit-asm` paths do not perform the
final link and therefore do not require an installed artifact.

## Dynamic eval

Regex calls visible in ordinary source are auto-detected. Source passed
dynamically to `eval()` is opaque, so Magician does not enable PCRE2 merely
because dynamic eval is present. Opt in explicitly when evaluated code may call
regex functions:

```bash
elephc native add pcre2
elephc --with-regex path/to/program.php
```

Without `--with-regex` or another statically visible regex use, the program
still compiles, but regex functions do not exist inside dynamic eval and calls
to them fail at runtime. Declaring or installing PCRE2 alone does not link it.
See [`examples/eval_regex/`](https://github.com/illegalstudio/elephc/tree/main/examples/eval_regex)
for a complete project.

## Supported functions

| Function | Signature | Description |
|---|---|---|
| `preg_match()` | `preg_match($pattern, $subject, &$matches = null): int` | Test regex match (1 or 0); optional `$matches` receives the full match and capture groups |
| `preg_match_all()` | `preg_match_all($pattern, $subject, &$matches = [], $flags = 0): int` | Count all non-overlapping matches; optional `$matches` receives numbered groups |
| `preg_replace()` | `preg_replace($pattern, $replacement, $subject): string` | Replace all regex matches; `$0`..`$99` and `\0`..`\99` replacement backreferences expand captured groups |
| `preg_replace_callback()` | `preg_replace_callback($pattern, $callback, $subject): string` | Replace all regex matches with the callback return value; callback receives `array<string>` matches |
| `preg_split()` | `preg_split($pattern, $subject, $limit = -1, $flags = 0): array` | Split string by regex; supports no-empty, delimiter-capture, offset-capture, and positive limits |
| `mb_ereg_match()` | `mb_ereg_match($pattern, $subject, $options = null): bool` | Test whether the pattern matches at the **start** of the subject (anchored, like PHP's mbregex). The pattern is a bare mbregex pattern with no delimiters. Runs on the same PCRE2-backed runtime; UTF-8/ASCII patterns are supported, and the `i` option enables case-insensitive matching (other recognized mbregex options are accepted without additional effect) |

## Pattern syntax

PCRE syntax is passed to PCRE2, so lookahead, lookbehind, lazy quantifiers,
shorthand classes, and Unicode property escapes are available. PHP-style
slash-delimited patterns are supported, and elephc maps these trailing modifiers
to PCRE2 wrapper flags:

| Modifier | Meaning |
|---|---|
| `i` | Case-insensitive matching |
| `m` | Multiline anchor behavior |
| `s` | Dotall; `.` can match newlines |
| `u` | UTF-8 and Unicode-property matching |
| `U` | Ungreedy matching |

Other trailing modifiers are currently not mapped to PCRE2 flags.

## Captures and replacements

`preg_match()` supports the optional `$matches` output parameter. `$matches[0]`
is the full match, and `$matches[1]` onward contain compiled numbered capture
groups. Unmatched interior captures are empty strings; trailing unmatched groups
are omitted.

`preg_match_all()` supports the same by-reference `$matches` parameter and the
PHP flags Termwind uses for width and trim styling:

| Constant | Behavior |
|---|---|
| `PREG_PATTERN_ORDER` (default) | `$matches[group][match]` |
| `PREG_SET_ORDER` | `$matches[match][group]` |
| `PREG_OFFSET_CAPTURE` | Each cell is `[value, offset]` |
| `PREG_UNMATCHED_AS_NULL` | Unmatched groups are `null` (offset `-1` with offset capture) |

A successful compile with zero matches still materializes one empty row per
compiled numbered group under `PREG_PATTERN_ORDER`. Named capture keys are not
populated.

`preg_replace()` expands `$0`..`$99` and `\0`..`\99` to captured groups.
Unmatched optional groups and missing groups expand to an empty string.

`preg_replace_callback()` passes the same `$matches` array shape to the
callback. Descriptor-backed closure captures and first-class-callable receivers
are preserved when callbacks are stored in variables or passed through
`callable` parameters. Runtime string callback variables can target user
functions and public static methods, and callable-array variables such as
`[$object, $method]` and `[$class, $method]` can target public methods when the
runtime strings select them.

## Split flags

`preg_split()` supports:

| Constant | Behavior |
|---|---|
| `PREG_SPLIT_NO_EMPTY` | Drop empty split elements |
| `PREG_SPLIT_DELIM_CAPTURE` | Include delimiter capture groups in the result |
| `PREG_SPLIT_OFFSET_CAPTURE` | Return value/offset pairs |

Positive limits are supported.

## SPL regex iterators

`RegexIterator` and `RecursiveRegexIterator` use the same managed PCRE2-backed
runtime as the `preg_*` functions. Their supported modes and flags are
documented in [SPL](spl.md). Using either class adds the logical `pcre2`
requirement only when a final native link is performed.

## Current limitations

- Regex runtime detection is conservative around dynamic `instanceof`: programs
  that can dynamically reference emitted SPL regex classes may link PCRE2 even
  when they do not call `preg_*` directly.
- Only the pattern modifiers listed above are mapped to PCRE2 flags today.
- Pattern and subject strings cross the current shim as NUL-terminated strings,
  preserving the existing embedded-NUL limitation.
