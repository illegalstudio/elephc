---
title: "mb_http_input()"
description: "Returns recorded HTTP input encoding identification or the configured input encoding list."
---

## mb_http_input()

```php
function mb_http_input(?string $type = null): array|string|false
```

Returns recorded HTTP input encoding identification or the configured input encoding list.

**Parameters**:
- `$type` (`?string`), default `null`, optional

**Returns**: `array|string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_http_input` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_http_input.md).
