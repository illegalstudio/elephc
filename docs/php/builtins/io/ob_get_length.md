---
title: "ob_get_length()"
description: "Returns the length of the output buffer."
sidebar:
  order: 198
---

## ob_get_length()

```php
function ob_get_length(): mixed
```

Returns the length of the output buffer.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_get_length.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_length.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_get_length` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_length.md).
