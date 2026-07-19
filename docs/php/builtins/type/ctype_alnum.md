---
title: "ctype_alnum()"
description: "Checks if all characters in the string are alphanumeric."
sidebar:
  order: 429
---

## ctype_alnum()

```php
function ctype_alnum(string $text): bool
```

Checks if all characters in the string are alphanumeric.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_alnum.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_alnum.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ctype_alnum` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/ctype_alnum.md).

