---
title: "ob_get_contents()"
description: "Returns the contents of the output buffer."
sidebar:
  order: 194
---

## ob_get_contents()

```php
function ob_get_contents(): mixed
```

Returns the contents of the output buffer.

**Parameters**: none.

**Returns**: `mixed`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_get_contents.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_contents.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_get_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_contents.md).

