---
title: "link()"
description: "Creates a hard link."
---

## link()

```php
function link(string $target, string $link): bool
```

Creates a hard link.

**Parameters**:
- `$target` (`string`)
- `$link` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `link` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/link.md).
