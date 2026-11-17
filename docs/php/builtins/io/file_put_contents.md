---
title: "file_put_contents()"
description: "Writes data to a file."
---

## file_put_contents()

```php
function file_put_contents(string $filename, string $data, int $flags = 0): int
```

Writes data to a file.

**Parameters**:
- `$filename` (`string`)
- `$data` (`string`)
- `$flags` (`int`), default `0`, optional

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `file_put_contents` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/file_put_contents.md).
