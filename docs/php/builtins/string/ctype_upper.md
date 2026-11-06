---
title: "ctype_upper()"
description: "Checks if all characters in the string are uppercase letters."
sidebar:
  order: 912
---

## ctype_upper()

```php
function ctype_upper(string $text): bool
```

Checks if all characters in the string are uppercase letters.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_upper.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_upper.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ctype_upper` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ctype_upper.md).
