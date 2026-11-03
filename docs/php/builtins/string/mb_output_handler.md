---
title: "mb_output_handler()"
description: "Converts an output-buffer phase using the request encodings, MIME selection, and substitution settings."
---

## mb_output_handler()

```php
function mb_output_handler(string $string, int $status): string
```

Converts an output-buffer phase using the request encodings, MIME selection, and substitution settings.

**Parameters**:
- `$string` (`string`)
- `$status` (`int`)

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mb_output_handler` is implemented in the compiler, see [the internals page](../../../internals/builtins/string/mb_output_handler.md).
