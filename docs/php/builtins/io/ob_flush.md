---
title: "ob_flush()"
description: "Flushes (sends) the contents of the active output buffer."
---

## ob_flush()

```php
function ob_flush(): bool
```

Flushes (sends) the contents of the active output buffer.

**Parameters**: none.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ob_flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_flush.md).
