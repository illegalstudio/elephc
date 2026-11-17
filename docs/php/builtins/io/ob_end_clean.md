---
title: "ob_end_clean()"
description: "Cleans (erases) the contents of the active output buffer and turns it off."
---

## ob_end_clean()

```php
function ob_end_clean(): bool
```

Cleans (erases) the contents of the active output buffer and turns it off.

**Parameters**: none.

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `ob_end_clean` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/ob_end_clean.md).
