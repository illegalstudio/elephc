---
title: "ob_get_clean()"
description: "Gets the current buffer contents and deletes the current output buffer."
---

## ob_get_clean()

```php
function ob_get_clean(): mixed
```

Gets the current buffer contents and deletes the current output buffer.

**Parameters**: none.

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ob_get_clean` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_clean.md).
