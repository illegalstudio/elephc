---
title: "tempnam()"
description: "Lowers `tempnam(directory, prefix)` through the target-aware runtime helper."
sidebar:
  order: 131
---

## tempnam()

```php
function tempnam(string $directory, string $prefix): string
```

Lowers `tempnam(directory, prefix)` through the target-aware runtime helper.

**Parameters**:
- `$directory` (`string`)
- `$prefix` (`string`)

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `tempnam` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/tempnam.md).

