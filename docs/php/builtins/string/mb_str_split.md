---
title: "mb_str_split()"
description: "Splits a string into chunks measured in encoded characters."
sidebar:
  order: 840
---

## mb_str_split()

```php
function mb_str_split(string $string, int $length = 1, ?string $encoding = null): array
```

Splits a string into chunks measured in encoded characters.

**Parameters**:
- `$string` (`string`)
- `$length` (`int`), default `1`, optional
- `$encoding` (`?string`), default `null`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_str_split.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_str_split.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_str_split` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_str_split.md).
