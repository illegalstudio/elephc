---
title: "xmlwriter_flush()"
description: "Flushes the buffer: the buffered string for a memory writer, the byte count written for a URI writer."
---

## xmlwriter_flush()

```php
function xmlwriter_flush(mixed $writer, bool $empty = true): mixed
```

Flushes the buffer: the buffered string for a memory writer, the byte count written for a URI writer.

**Parameters**:
- `$writer` (`mixed`)
- `$empty` (`bool`), default `true`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_flush` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_flush.md).
