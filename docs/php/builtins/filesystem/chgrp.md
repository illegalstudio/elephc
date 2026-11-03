---
title: "chgrp()"
description: "Changes file group."
---

## chgrp()

```php
function chgrp(string $filename, string $group): bool
```

Changes file group.

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

For how `chgrp` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chgrp.md).
