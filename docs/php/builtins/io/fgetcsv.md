---
title: "fgetcsv()"
description: "Gets line from file pointer and parse for CSV fields."
---

## fgetcsv()

```php
function fgetcsv(resource $stream, ?int $length = null, string $separator = ','): mixed
```

Gets line from file pointer and parse for CSV fields.

**Parameters**:
- `$stream` (`resource`)
- `$length` (`?int`), default `null`, optional
- `$separator` (`string`), default `','`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fgetcsv` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fgetcsv.md).
