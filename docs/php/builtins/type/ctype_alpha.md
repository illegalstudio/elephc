---
title: "ctype_alpha()"
description: "Checks if all characters in the string are alphabetic."
sidebar:
  order: 417
---

## ctype_alpha()

```php
function ctype_alpha(string $text): bool
```

Checks if all characters in the string are alphabetic.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_alpha.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_alpha.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ctype_alpha` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/ctype_alpha.md).

