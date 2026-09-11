---
title: "mb_strtolower()"
description: "Converts every character to its Unicode lowercase mapping."
sidebar:
  order: 852
---

## mb_strtolower()

```php
function mb_strtolower(string $string, ?string $encoding = null): string
```

Converts every character to its Unicode lowercase mapping.

**Parameters**:
- `$string` (`string`)
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported - declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_strtolower.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_strtolower.rs)).

_No examples yet - check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_strtolower` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_strtolower.md).
