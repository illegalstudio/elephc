---
title: "ob_get_level()"
description: "Returns the nesting level of the output buffering mechanism."
sidebar:
  order: 197
---

## ob_get_level()

```php
function ob_get_level(): int
```

Returns the nesting level of the output buffering mechanism.

**Parameters**: none.

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_get_level.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_level.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_get_level` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_level.md).

