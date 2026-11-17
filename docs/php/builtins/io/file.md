---
title: "file()"
description: "Reads an entire file into an array."
---

## file()

```php
function file(string $filename, int $flags = 0): mixed
```

Reads an entire file into an array.

**Parameters**:
- `$filename` (`string`)
- `$flags` (`int`), default `0`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `file` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file.md).
