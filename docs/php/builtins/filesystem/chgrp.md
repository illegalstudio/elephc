---
title: "chgrp()"
description: "Lowers `chgrp(path, group)` for integer GIDs and string group names."
sidebar:
  order: 83
---

## chgrp()

```php
function chgrp(string $filename, int $group): bool
```

Lowers `chgrp(path, group)` for integer GIDs and string group names.

**Parameters**:
- `$filename` (`string`)
- `$group` (`int`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `chgrp` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/chgrp.md).

