---
title: "ctype_xdigit()"
description: "Checks if all characters in the string are hexadecimal digits."
sidebar:
  order: 913
---

## ctype_xdigit()

```php
function ctype_xdigit(string $text): bool
```

Checks if all characters in the string are hexadecimal digits.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_xdigit.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_xdigit.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ctype_xdigit` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ctype_xdigit.md).
