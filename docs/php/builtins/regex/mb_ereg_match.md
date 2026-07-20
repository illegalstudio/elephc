---
title: "mb_ereg_match()"
description: "Tests whether a regex pattern matches the beginning of a string (multibyte)."
sidebar:
  order: 332
---

## mb_ereg_match()

```php
function mb_ereg_match(string $pattern, string $subject, string $options = null): bool
```

Tests whether a regex pattern matches the beginning of a string (multibyte).

**Parameters**:
- `$pattern` (`string`)
- `$subject` (`string`)
- `$options` (`string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/regex/mb_ereg_match.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/regex/mb_ereg_match.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `mb_ereg_match` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/mb_ereg_match.md).

