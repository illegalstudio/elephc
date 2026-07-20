---
title: "ob_flush()"
description: "Flushes (sends) the contents of the active output buffer."
sidebar:
  order: 192
---

## ob_flush()

```php
function ob_flush(): bool
```

Flushes (sends) the contents of the active output buffer.

**Parameters**: none.

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_flush.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_flush.md).

