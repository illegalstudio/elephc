---
title: "putenv()"
description: "Lowers `putenv(assignment)` by copying the environment string into persistent heap storage."
sidebar:
  order: 119
---

## putenv()

```php
function putenv(string $assignment): bool
```

Lowers `putenv(assignment)` by copying the environment string into persistent heap storage.

**Parameters**:
- `$assignment` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `putenv` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/putenv.md).

