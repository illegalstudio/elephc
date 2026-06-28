---
title: "lchgrp()"
description: "Lowers `lchgrp(path, group)` for integer GIDs and string group names without following symlinks."
sidebar:
  order: 112
---

## lchgrp()

```php
function lchgrp(string $filename, int $group): bool
```

Lowers `lchgrp(path, group)` for integer GIDs and string group names without following symlinks.

**Parameters**:
- `$filename` (`string`)
- `$group` (`int`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `lchgrp` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/lchgrp.md).

