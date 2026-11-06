---
title: "fopen()"
description: "Opens file or URL."
---

## fopen()

```php
function fopen(string $filename, string $mode, bool $use_include_path = false, mixed $context = null): mixed
```

Opens file or URL.

**Parameters**:
- `$filename` (`string`)
- `$mode` (`string`)
- `$use_include_path` (`bool`), default `false`, optional
- `$context` (`mixed`), default `null`, optional

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fopen` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fopen.md).
