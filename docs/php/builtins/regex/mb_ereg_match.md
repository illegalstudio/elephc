---
title: "mb_ereg_match()"
description: "Tests a raw multibyte regex at the string start using the current regex encoding and options."
sidebar:
  order: 729
---

## mb_ereg_match()

```php
function mb_ereg_match(string $pattern, string $string, ?string $options = null): bool
```

Tests a raw multibyte regex at the string start using the current regex encoding and options.

**Parameters**:
- `$pattern` (`string`)
- `$string` (`string`)
- `$options` (`?string`), default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_match.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_ereg_match.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_ereg_match` is implemented in the compiler, see [the internals page](../../../internals/builtins/regex/mb_ereg_match.md).
