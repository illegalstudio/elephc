---
title: "ctype_cntrl()"
description: "Checks if all characters in the string are control characters."
sidebar:
  order: 907
---

## ctype_cntrl()

```php
function ctype_cntrl(string $text): bool
```

Checks if all characters in the string are control characters.

**Parameters**:
- `$text` (`string`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/ctype_cntrl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/ctype_cntrl.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ctype_cntrl` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/ctype_cntrl.md).
