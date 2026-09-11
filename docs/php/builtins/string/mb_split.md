---
title: "mb_split()"
description: "Splits a string using a multibyte regex and an optional maximum number of fields."
sidebar:
  order: 838
---

## mb_split()

```php
function mb_split(string $pattern, string $string, int $limit = -1): array|false
```

Splits a string using a multibyte regex and an optional maximum number of fields.

**Parameters**:
- `$pattern` (`string`)
- `$string` (`string`)
- `$limit` (`int`), default `-1`, optional

**Returns**: `array|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_split.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_split.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_split.md).
