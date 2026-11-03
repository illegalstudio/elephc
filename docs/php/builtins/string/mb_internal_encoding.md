---
title: "mb_internal_encoding()"
description: "Reads or changes the default encoding used by mbstring text operations."
---

## mb_internal_encoding()

```php
function mb_internal_encoding(?string $encoding = null): string|bool
```

Reads or changes the default encoding used by mbstring text operations.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_internal_encoding` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_internal_encoding.md).
