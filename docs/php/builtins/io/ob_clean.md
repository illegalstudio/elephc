---
title: "ob_clean()"
description: "Cleans (erases) the contents of the active output buffer."
sidebar:
  order: 191
---

## ob_clean()

```php
function ob_clean(): bool
```

Cleans (erases) the contents of the active output buffer.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_clean.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_clean` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_clean.md).
