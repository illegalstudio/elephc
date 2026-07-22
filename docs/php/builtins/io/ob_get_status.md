---
title: "ob_get_status()"
description: "Gets status of output buffers."
sidebar:
  order: 200
---

## ob_get_status()

```php
function ob_get_status(bool $full_status = false): array
```

Gets status of output buffers.

**Parameters**:
- `$full_status` (`bool`), default `false`, optional

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/core/ob_get_status.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/core/ob_get_status.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `ob_get_status` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_status.md).
