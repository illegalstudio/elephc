---
title: "realpath()"
description: "Returns canonicalized absolute pathname."
---

## realpath()

```php
function realpath(string $path): mixed
```

Returns canonicalized absolute pathname.

**Parameters**:
- `$path` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `realpath` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/realpath.md).
