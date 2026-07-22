---
title: "ob_get_flush()"
description: "Flushes the output buffer, returns it as a string and turns off output buffering."
sidebar:
  order: 197
---

## ob_get_flush()

```php
function ob_get_flush(): mixed
```

Flushes the output buffer, returns it as a string and turns off output buffering.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_get_flush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_flush.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_get_flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_flush.md).
