---
title: "stream_resolve_include_path()"
description: "Resolves filename against the include path."
---

## stream_resolve_include_path()

```php
function stream_resolve_include_path(string $filename): mixed
```

Resolves filename against the include path.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `stream_resolve_include_path` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_resolve_include_path.md).
