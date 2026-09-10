---
title: "mb_eregi()"
description: "Searches without case sensitivity and optionally writes multibyte captures by reference."
sidebar:
  order: 820
---

## mb_eregi()

```php
function mb_eregi(string $pattern, string $string, mixed $matches = null): bool
```

Searches without case sensitivity and optionally writes multibyte captures by reference.

**Parameters**:
- `$pattern` (`string`)
- `$string` (`string`)
- `$matches` (`mixed`), passed by reference, default `null`, optional

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_eregi.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_eregi.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_eregi` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_eregi.md).
