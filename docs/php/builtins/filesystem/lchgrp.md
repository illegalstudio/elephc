---
title: "lchgrp()"
description: "Changes group ownership of a symlink."
---

## lchgrp()

```php
function lchgrp(string $filename, string $group): bool
```

Changes group ownership of a symlink.

**Parameters**:
- `$filename` (`string`)
- `$group` (`string`)

**Returns**: `bool`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `lchgrp` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lchgrp.md).
