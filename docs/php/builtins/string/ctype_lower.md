---
title: "ctype_lower()"
description: "Checks if all characters in the string are lowercase letters."
sidebar:
  order: 909
---

## ctype_lower()

```php
function ctype_lower(string $text): bool
```

Checks if all characters in the string are lowercase letters.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_lower.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_lower.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ctype_lower` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ctype_lower.md).
