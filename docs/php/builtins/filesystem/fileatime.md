---
title: "fileatime()"
description: "Gets last access time of file."
---

## fileatime()

```php
function fileatime(string $filename): mixed
```

Gets last access time of file.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `fileatime` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fileatime.md).
