---
title: "mb_detect_order()"
description: "Reads or updates the request's encoding detection order."
sidebar:
  order: 836
---

## mb_detect_order()

```php
function mb_detect_order(array|string|null $encoding = null): array|bool
```

Reads or updates the request's encoding detection order.

**Parameters**:
- `$encoding` (`array|string|null`), default `null`, optional

**Returns**: `array|bool`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/string/mb_detect_order.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/string/mb_detect_order.rs)).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_detect_order` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_detect_order.md).
