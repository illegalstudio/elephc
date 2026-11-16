---
title: "mb_preferred_mime_name()"
description: "Returns the preferred MIME encoding name, or false when none is registered."
---

## mb_preferred_mime_name()

```php
function mb_preferred_mime_name(string $encoding): string|false
```

Returns the preferred MIME encoding name, or false when none is registered.

**Parameters**:
- `$encoding` (`string`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_preferred_mime_name` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_preferred_mime_name.md).
