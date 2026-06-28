---
title: "readfile()"
description: "Lowers `readfile(path)` and boxes the runtime byte-count-or-false result."
sidebar:
  order: 120
---

## readfile()

```php
function readfile(string $filename, bool $use_include_path, mixed $context): mixed
```

Lowers `readfile(path)` and boxes the runtime byte-count-or-false result.

**Parameters**:
- `$filename` (`string`)
- `$use_include_path` (`bool`)
- `$context` (`mixed`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `readfile` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/readfile.md).

