---
title: "mime_content_type()"
description: "Detects a file or stream resource MIME content type."
---

## mime_content_type()

```php
function mime_content_type(string|resource $filename): string|false
```

Detects a file or stream resource MIME content type.

**Parameters**:
- `$filename` (`string|resource`)

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `mime_content_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/mime_content_type.md).
