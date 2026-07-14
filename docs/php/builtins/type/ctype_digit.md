---
title: "ctype_digit()"
description: "Checks if all characters in the string are digits."
sidebar:
  order: 417
---

## ctype_digit()

```php
function ctype_digit(string $text): bool
```

Checks if all characters in the string are digits.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_digit.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_digit.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ctype_digit` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/ctype_digit.md).

