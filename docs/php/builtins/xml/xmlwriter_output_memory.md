---
title: "xmlwriter_output_memory()"
description: "Returns the buffered output of a memory writer."
---

## xmlwriter_output_memory()

```php
function xmlwriter_output_memory(mixed $writer, bool $flush = true): string
```

Returns the buffered output of a memory writer.

**Parameters**:
- `$writer` (`mixed`)
- `$flush` (`bool`), default `true`, optional

**Returns**: `string`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `xmlwriter_output_memory` is implemented in the compiler, see [the internals page](../../../internals/builtins/xml/xmlwriter_output_memory.md).
