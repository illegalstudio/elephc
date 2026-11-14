---
title: "tidy_get_error_buffer()"
description: "Returns the warnings and errors produced while parsing the document, or false when there are none."
---

## tidy_get_error_buffer()

```php
function tidy_get_error_buffer(mixed $tidy): string|false
```

Returns the warnings and errors produced while parsing the document, or false when there are none.

**Parameters**:
- `$tidy` (`mixed`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tidy_get_error_buffer` is implemented in the compiler, see [the internals page](../../../internals/builtins/tidy/tidy_get_error_buffer.md).
