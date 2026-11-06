---
title: "fwrite()"
description: "Binary-safe file write."
---

## fwrite()

```php
function fwrite(resource $stream, string $data): mixed
```

Binary-safe file write.

**Parameters**:
- `$stream` (`resource`)
- `$data` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fwrite` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fwrite.md).
