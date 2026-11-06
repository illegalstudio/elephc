---
title: "ctype_print()"
description: "Checks if all characters in the string are printable."
sidebar:
  order: 910
---

## ctype_print()

```php
function ctype_print(string $text): bool
```

Checks if all characters in the string are printable.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_print.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_print.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ctype_print` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ctype_print.md).
