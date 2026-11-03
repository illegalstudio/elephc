---
title: "ob_get_flush()"
description: "Flushes the output buffer, returns it as a string and turns off output buffering."
---

## ob_get_flush()

```php
function ob_get_flush(): mixed
```

Flushes the output buffer, returns it as a string and turns off output buffering.

**Parameters**: none.

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ob_get_flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_get_flush.md).
