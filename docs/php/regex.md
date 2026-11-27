---
title: "Regex"
description: "PCRE2-backed regular expressions, preg_* functions, SPL regex iterators, and the managed pcre2 package."
sidebar:
  order: 7
---

elephc implements PHP regular expressions with PCRE2 through an Elephc-owned
opaque shim over PCRE2's POSIX-compatible wrapper. Programs using `preg_*`,
`RegexIterator`, or `RecursiveRegexIterator` require the managed `pcre2` package
during the final native link. Mbstring and opaque eval also use PCRE2 for output
MIME-pattern selection only when a custom expression selects that provider; the
shared default matcher needs no native package. Mbstring regex functions,
including `mb_ereg_match`, use
the separate managed Oniguruma provider. Programs without these features do not
link PCRE2 merely because the package is declared.

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
elephc build path/to/program.php
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
elephc build --with-regex path/to/program.php
```

Without `--with-regex` or another statically visible regex use, the program
still compiles, but regex functions do not exist inside dynamic eval and calls
to them fail at runtime. Declaring or installing PCRE2 alone does not link it.
See [`examples/eval_regex/`](https://github.com/illegalstudio/elephc/tree/main/examples/eval_regex)
for a complete project.

## Supported functions

| Function | Signature | Description |
|---|---|---|
| `preg_match()` | `preg_match($pattern, $subject, &$matches = null, $flags = 0, $offset = 0): int\|false` | Test regex match (`1` or `0`). Returns `false` for an invalid pattern or the removed `/e` modifier, and then leaves a supplied `$matches` variable unchanged. A completed search writes the full match and capture groups |
| `preg_match_all()` | `preg_match_all($pattern, $subject, &$matches = [], $flags = 0, $offset = 0): int\|false` | PHP returns the match count, or `false` for an invalid pattern or the removed `/e` modifier. elephc's type checker currently types the result as `mixed`. Optional `$matches` receives numbered groups |
| `preg_replace()` | `preg_replace($pattern, $replacement, $subject): string\|array\|null` | PHP returns the replaced value, or `null` on failure. elephc's type checker currently types the result as `mixed`. `$0`..`$99` and `\0`..`\99` replacement backreferences expand captured groups |
| `preg_replace_callback()` | `preg_replace_callback($pattern, $callback, $subject): mixed` | Replace all regex matches with the callback return value; callback receives `array<string>` matches. The static return is `mixed`, so a `string` function may return the call. An invalid pattern, including an unknown modifier or a missing delimiter, returns `null` |
| `preg_split()` | `preg_split($pattern, $subject, $limit = -1, $flags = 0): array\|false` | PHP returns the pieces, or `false` for an invalid pattern or the removed `/e` modifier. The checker types this result as `array\|false`. Supports no-empty, delimiter-capture, offset-capture, and limits. As in PHP, only returned pieces count toward `$limit`, `-1` and `0` mean no limit, and any other limit below 2 returns the subject as one piece |
| `preg_grep()` | `preg_grep($pattern, $array, $flags = 0): array\|false` | Return the entries whose string value matches, keeping their keys; `PREG_GREP_INVERT` keeps the others. An invalid pattern returns `false` |
| `preg_last_error()` | `preg_last_error(): int` | The `PREG_*_ERROR` code of the last `preg_*` call |
| `preg_last_error_msg()` | `preg_last_error_msg(): string` | PHP's message for that code |
| `mb_ereg_match()` | `mb_ereg_match($pattern, $string, $options = null): bool` | Test a bare multibyte regex at the start of the string through Oniguruma, using the current `mb_regex_encoding()` and `mb_regex_set_options()` settings. An explicit options argument overrides the current options for this call |

Capture output supports ordinary locals and native references whose payload is already boxed `mixed` or has the exact capture-array layout. Native references with a frozen scalar or string payload are rejected during compilation instead of receiving an incompatible boxed pointer. Use an untyped or `mixed` reference for a destination that can change type. Captures written through an eligible native `mixed` property alias remain visible to eval references sharing that property.

## Pattern syntax

PCRE syntax is passed to PCRE2, so lookahead, lookbehind, lazy quantifiers,
shorthand classes, and Unicode property escapes are available. Delimiters follow
PHP: leading whitespace is skipped, the delimiter is any byte other than a
letter, a digit, a backslash or NUL, `(`, `[`, `{` and `<` close with their
bracket pair (nested pairs are counted), and the first closing delimiter no
backslash escapes ends the pattern. An empty pattern, an invalid or missing
delimiter, and an unknown modifier make the call fail like PHP (`false`, or
`null` for the replace functions) without a warning. elephc maps these trailing
modifiers to PCRE2 compile options:

| Modifier | Meaning |
|---|---|
| `i` | Case-insensitive matching |
| `m` | Multiline anchor behavior |
| `s` | Dotall; `.` can match newlines |
| `u` | UTF-8 and Unicode-property matching |
| `U` | Ungreedy matching |
| `x` | Extended mode: unescaped whitespace and `#` comments in the pattern are ignored; whitespace inside a character class still matches |
| `A` | Anchored: a match must start at the search offset |
| `D` | Dollar end-only: `$` does not match before a final newline |
| `n` | No auto-capture: plain `(...)` groups do not capture |
| `J` | Duplicate subpattern names are allowed |
| `r` | Caseless restrict: under `i`, ASCII and non-ASCII letters do not match each other |

`S` and `X` are accepted with no effect, as in PHP 8. Space, newline, and carriage return in the modifier suffix are ignored. Every `preg_*` function rejects the removed `/e` modifier before compilation.

The global functions (`preg_match_all()`, `preg_replace()`,
`preg_replace_callback()`, `preg_split()`) step through the subject like PHP:
after an empty match the next search starts at the same offset but may not
match empty there, an empty match at the end of the subject is found, and `^`,
`\b` and lookbehind always see the whole subject.

## Captures and replacements

`preg_match()` supports the optional `$matches` output parameter. `$matches[0]`
is the full match, and `$matches[1]` onward contain compiled numbered capture
groups. Unmatched interior captures are empty strings; trailing unmatched groups
are omitted. A search that does not match replaces `$matches` with an empty
array. An invalid pattern, or a pattern that uses the removed `/e` modifier,
returns `false` and does not replace `$matches`.

`preg_match()` also accepts `$flags` and `$offset`. `PREG_OFFSET_CAPTURE` makes
each entry a `[value, byte_offset]` pair, and `PREG_UNMATCHED_AS_NULL` reports
unmatched groups as `null` and keeps trailing ones. An order flag
(`PREG_PATTERN_ORDER`, `PREG_SET_ORDER`, or any other bit in the low byte) throws
PHP's `ValueError`; other unknown bits are ignored, as in PHP. A call that passes
`$flags` or `$offset` stores `$matches` as `array<mixed>` instead of
`array<string>`.

`$offset` (for both `preg_match()` and `preg_match_all()`) starts the search at
that byte offset while anchors, `\G`, and lookbehind still see the whole
subject. A negative offset counts back from the end and stops at the start. An
offset past the end returns `false`, sets `$matches` to `[]` and records
`PREG_INTERNAL_ERROR`, like PHP; an offset inside a multibyte character under `/u`
returns `false` with `PREG_BAD_UTF8_OFFSET_ERROR`.

`preg_match_all()` supports the same by-reference `$matches` parameter and the
PHP flags Termwind uses for width and trim styling. PHP's return type is
`int|false`. elephc's type checker currently types the call as `mixed`, so a
function declared to return `int` can still return `preg_match_all(...)`.

| Constant | Behavior |
|---|---|
| `PREG_PATTERN_ORDER` (default) | `$matches[group][match]` |
| `PREG_SET_ORDER` | `$matches[match][group]` |
| `PREG_OFFSET_CAPTURE` | Each cell is `[value, offset]` |
| `PREG_UNMATCHED_AS_NULL` | Unmatched groups are `null` (offset `-1` with offset capture) |

Like PHP, each `PREG_SET_ORDER` row ends at its highest matched group unless
`PREG_UNMATCHED_AS_NULL` is set.

A successful compile with zero matches still materializes one empty row per
compiled numbered group under `PREG_PATTERN_ORDER`. Named capture keys are not
populated.

Global matching includes empty matches at the end of the subject and on an
empty subject. After an empty match, PCRE2 retries nonempty alternatives at that
position before advancing with the pattern's UTF-8 and newline rules. Offsets
remain byte offsets in the original subject, including with the `/u` modifier.

In compiled calls, `$matches` must be a local variable or a local reference
(including a by-reference parameter). Array elements, properties, global-bound
variables and static locals are currently rejected with a diagnostic rather
than silently discarding captures. Omitting `$matches` performs count-only
matching, also when named arguments supply `$flags`.

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

## Errors

Every `preg_*` call resets the state `preg_last_error()` reports. An invalid
pattern (or the removed `/e` modifier) sets `PREG_INTERNAL_ERROR`. A match
error makes the call fail like PHP: `preg_match()`, `preg_match_all()` and
`preg_split()` return `false`, `preg_replace()` and `preg_replace_callback()`
return `null`, and `preg_grep()` returns the entries collected so far (from PHP 8.6,
`--php-version 8.6`, it returns `false` instead, like php-src GH-11936). A subject
PCRE2 rejects as malformed UTF-8 under `/u` sets `PREG_BAD_UTF8_ERROR`; an
exhausted PCRE2 match limit sets `PREG_BACKTRACK_LIMIT_ERROR`, an exhausted depth
limit (`(*LIMIT_DEPTH=n)`) `PREG_RECURSION_LIMIT_ERROR`, and an exhausted heap
limit or memory `PREG_INTERNAL_ERROR`, like PHP without JIT. The managed PCRE2
has no JIT, so `PREG_JIT_STACKLIMIT_ERROR` never occurs. As in
PHP, an error recorded by a `preg_*` call inside a `preg_replace_callback()`
callback stays visible to `preg_last_error()` without failing the outer call.
`preg_grep()` compiles its pattern once and resets the state once per call, not
before each entry, so an error a `preg_*` call records inside an entry's
`__toString()` is still reported after the call (and from PHP 8.6 makes it return
`false`), unless a later entry's nested `preg_*` call succeeds and clears it.

eval() and compiled code share one error state, as PHP has one: an error recorded
inside eval() is what compiled code's `preg_last_error()` reports next, and the
other way round. Under `--web`, the state starts clean for every request.

`PCRE_VERSION`, `PCRE_VERSION_MAJOR` and `PCRE_VERSION_MINOR` describe the
managed PCRE2 package (10.47), not the PHP build's library.

## Current limitations

- Regex runtime detection is conservative around dynamic `instanceof`: programs
  that can dynamically reference emitted SPL regex classes may link PCRE2 even
  when they do not call `preg_*` directly.
- A malformed pattern or an unknown modifier fails silently: PHP's warnings
  ("Unknown modifier", "No ending delimiter", ...) are not emitted.
- PCRE2's own default match limit (10,000,000) applies instead of PHP's
  `pcre.backtrack_limit` (1,000,000).
- `preg_match()` passes its pattern to the managed shim with an explicit byte
  length, so an embedded NUL byte in the pattern is compiled like PHP does.
  `preg_match_all()`, `preg_replace()`, `preg_replace_callback()`, `preg_split()`
  and the SPL regex iterators still pass NUL-terminated patterns, so those calls
  truncate a pattern at an embedded NUL. Subjects are searched through
  `REG_STARTEND`, so embedded NUL subject bytes are preserved across the family.
  `FILTER_VALIDATE_REGEXP` uses the `preg_match()` path and keeps PHP's
  byte-exact pattern behavior.

<!-- elephc:generated:symbols:begin -->

## Functions {#functions}

Generated from the shared symbol catalog by `scripts/docs/gen_module_sections.py`; do not edit this section by hand. Each function links to its reference page.

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`preg_grep()`](./builtins/regex/preg_grep.md) | `(string $pattern, array $array, int $flags = 0): array|false` | `array|false` | ✓ | ✓ |
| [`preg_last_error()`](./builtins/regex/preg_last_error.md) | `(): int` | `int` | ✓ | ✓ |
| [`preg_last_error_msg()`](./builtins/regex/preg_last_error_msg.md) | `(): string` | `string` | ✓ | ✓ |
| [`preg_match()`](./builtins/regex/preg_match.md) | `(string $pattern, string $subject, array $matches = null, int $flags = 0, int $offset = 0): int|false` | `int|false` | ✓ | ✓ |
| [`preg_match_all()`](./builtins/regex/preg_match_all.md) | `(string $pattern, string $subject, mixed $matches = null, int $flags = 0, int $offset = 0): int|false` | `int|false` | ✓ | ✓ |
| [`preg_replace()`](./builtins/regex/preg_replace.md) | `(string $pattern, string $replacement, string $subject): mixed` | `mixed` | ✓ | ✓ |
| [`preg_replace_callback()`](./builtins/regex/preg_replace_callback.md) | `(string $pattern, callable $callback, string $subject): mixed` | `mixed` | ✓ | ✓ |
| [`preg_split()`](./builtins/regex/preg_split.md) | `(string $pattern, string $subject, int $limit = -1, int $flags = 0): array|false` | `array|false` | ✓ | ✓ |

Constants: `PCRE_VERSION`, `PCRE_VERSION_MAJOR`, `PCRE_VERSION_MINOR`, `PREG_BACKTRACK_LIMIT_ERROR`, `PREG_BAD_UTF8_ERROR`, `PREG_BAD_UTF8_OFFSET_ERROR`, `PREG_GREP_INVERT`, `PREG_INTERNAL_ERROR`, `PREG_JIT_STACKLIMIT_ERROR`, `PREG_NO_ERROR`, `PREG_OFFSET_CAPTURE`, `PREG_PATTERN_ORDER`, `PREG_RECURSION_LIMIT_ERROR`, `PREG_SET_ORDER`, `PREG_SPLIT_DELIM_CAPTURE`, `PREG_SPLIT_NO_EMPTY`, `PREG_SPLIT_OFFSET_CAPTURE`, `PREG_UNMATCHED_AS_NULL`.

<!-- elephc:generated:symbols:end -->
