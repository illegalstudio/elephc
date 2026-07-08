---
title: "fnmatch()"
description: "Matches a filename against a pattern."
sidebar:
  order: 118
---

## fnmatch()

```php
function fnmatch(string $pattern, string $filename, int $flags = 0): bool
```

Matches a filename against a pattern.

**Parameters**:
- `$pattern` (`string`)
- `$filename` (`string`)
- `$flags` (`int`), default `0`, optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fnmatch` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/fnmatch.md).

