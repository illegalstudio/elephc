---
title: "chmod()"
description: "Changes file mode."
---

## chmod()

```php
function chmod(string $filename, int $permissions): bool
```

Changes file mode.

**Parameters**:
- `$filename` (`string`)
- `$permissions` (`int`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `chmod` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chmod.md).
