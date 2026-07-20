---
title: "ob_end_flush()"
description: "Flushes (sends) the contents of the active output buffer and turns it off."
sidebar:
  order: 191
---

## ob_end_flush()

```php
function ob_end_flush(): bool
```

Flushes (sends) the contents of the active output buffer and turns it off.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_end_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_end_flush.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_end_flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_end_flush.md).

