---
title: "mb_http_output()"
description: "Reads or changes the encoding selected for HTTP output conversion."
---

## mb_http_output()

```php
function mb_http_output(?string $encoding = null): string|bool
```

Reads or changes the encoding selected for HTTP output conversion.

**Parameters**:
- `$encoding` (`?string`), default `null`, optional

**Returns**: `string|bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_http_output` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_http_output.md).
