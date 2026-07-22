---
title: "ob_get_clean()"
description: "Gets the current buffer contents and deletes the current output buffer."
sidebar:
  order: 195
---

## ob_get_clean()

```php
function ob_get_clean(): mixed
```

Gets the current buffer contents and deletes the current output buffer.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_get_clean.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_clean.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_get_clean` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_clean.md).
