---
title: "Regex"
description: "PCRE2-backed regular expressions, preg_* functions, SPL regex iterators, and native library requirements."
sidebar:
  order: 6
---

elephc implements PHP regular expressions with PCRE2 through PCRE2's
POSIX-compatible wrapper. Regex support is an optional runtime feature:
programs that do not use regex do not link PCRE2, while programs that use
`preg_*`, `RegexIterator`, or `RecursiveRegexIterator` request PCRE2 during the
final native link step.

## Build requirements

Building `elephc` itself does not require PCRE2:

```bash
cargo build
```

PCRE2 is required when compiling a PHP program whose generated binary can call
regex helpers. elephc adds the required native libraries automatically:

```text
-lpcre2-posix -lpcre2-8
```

Install the PCRE2 development package before compiling regex-using programs:

| Platform | Command |
|---|---|
| macOS with Homebrew | `brew install pcre2` |
| Debian / Ubuntu | `sudo apt install libpcre2-dev` |
| Alpine Linux | `apk add pcre2-dev` |
| Fedora | `sudo dnf install pcre2-devel` |
| Arch Linux | `sudo pacman -S pcre2` |

On macOS, elephc automatically searches `/opt/homebrew/lib` and
`/usr/local/lib` when optional native dependencies are linked. On Linux, the
system package normally installs PCRE2 into the default linker search path. If
PCRE2 is installed in a custom prefix, pass the library directory explicitly:

```bash
cargo run -- --link-path /opt/pcre2/lib path/to/program.php
```

The project Docker images used by `scripts/test-linux-x86_64.sh` and
`scripts/test-linux-arm64.sh` include `pcre2-dev`; rebuild the image after a
Dockerfile change with `--rebuild`.

## Compiling a Regex Program

Example PHP source:

```php
<?php
$subject = "order-42";

if (preg_match('/order-(\d+)/', $subject, $matches)) {
    echo $matches[1];
}
```

Compile and run it the same way as any other elephc program:

```bash
cargo run -- path/to/program.php
./path/to/program
```

No manual `--link pcre2-posix` or `--link pcre2-8` flag is needed when PCRE2 is
installed in a known linker path; elephc requests those libraries when regex is
present in the checked program.

Common linker failures mean the native PCRE2 libraries were not found:

| Error shape | Fix |
|---|---|
| `library not found for -lpcre2-posix` | Install PCRE2 with Homebrew or pass `--link-path` for a custom prefix |
| `cannot find -lpcre2-posix` | Install the platform PCRE2 development package |
| `undefined reference to pcre2_*` | Ensure both `pcre2-posix` and `pcre2-8` are available when doing manual link experiments |

## Supported Functions

| Function | Signature | Description |
|---|---|---|
| `preg_match()` | `preg_match($pattern, $subject, &$matches = null): int` | Test regex match (1 or 0); optional `$matches` receives the full match and capture groups |
| `preg_match_all()` | `preg_match_all($pattern, $subject): int` | Count all non-overlapping matches |
| `preg_replace()` | `preg_replace($pattern, $replacement, $subject): string` | Replace all regex matches; `$0`..`$99` and `\0`..`\99` replacement backreferences expand captured groups |
| `preg_replace_callback()` | `preg_replace_callback($pattern, $callback, $subject): string` | Replace all regex matches with the callback return value; callback receives `array<string>` matches |
| `preg_split()` | `preg_split($pattern, $subject, $limit = -1, $flags = 0): array` | Split string by regex; supports no-empty, delimiter-capture, offset-capture, and positive limits |

## Pattern Syntax

PCRE syntax is passed to PCRE2 directly, so lookahead, lookbehind, lazy
quantifiers, shorthand classes, and Unicode property escapes are available.
PHP-style slash-delimited patterns are supported, and elephc maps these trailing
modifiers to PCRE2 wrapper flags:

| Modifier | Meaning |
|---|---|
| `i` | Case-insensitive matching |
| `m` | Multiline anchor behavior |
| `s` | Dotall; `.` can match newlines |
| `u` | UTF-8 and Unicode-property matching |
| `U` | Ungreedy matching |

Other trailing modifiers are currently not mapped to PCRE2 flags.

## Captures and Replacements

`preg_match()` supports the optional `$matches` output parameter. `$matches[0]`
is the full match, and `$matches[1]` onward contain compiled numbered capture
groups. Unmatched interior captures are empty strings; trailing unmatched groups
are omitted.

`preg_replace()` expands `$0`..`$99` and `\0`..`\99` to captured groups.
Unmatched optional groups and missing groups expand to an empty string.

`preg_replace_callback()` passes the same `$matches` array shape to the
callback. Descriptor-backed closure captures and first-class-callable receivers
are preserved when callbacks are stored in variables or passed through
`callable` parameters. Runtime string callback variables can target user
functions and public static methods, and callable-array variables such as
`[$object, $method]` and `[$class, $method]` can target public methods when the
runtime strings select them.

## Split Flags

`preg_split()` supports:

| Constant | Behavior |
|---|---|
| `PREG_SPLIT_NO_EMPTY` | Drop empty split elements |
| `PREG_SPLIT_DELIM_CAPTURE` | Include delimiter capture groups in the result |
| `PREG_SPLIT_OFFSET_CAPTURE` | Return value/offset pairs |

Positive limits are supported.

## SPL Regex Iterators

`RegexIterator` and `RecursiveRegexIterator` use the same PCRE2-backed runtime
as the `preg_*` functions. Their supported modes and flags are documented in
[SPL](spl.md). Using either class makes the generated binary request PCRE2 at
link time.

## Current Limitations

- PCRE2 is currently an external native dependency, not a vendored static bundle.
  A future dependency workflow may provide a command such as `elephc lib add
  pcre2`, but that command does not exist yet.
- Regex runtime detection is conservative around dynamic `instanceof`: programs
  that can dynamically reference emitted SPL regex classes may link PCRE2 even
  when they do not call `preg_*` directly.
- Only the pattern modifiers listed above are mapped to PCRE2 flags today.
