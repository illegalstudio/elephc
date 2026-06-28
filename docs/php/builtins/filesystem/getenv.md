---
title: "getenv()"
description: "Lowers `getenv(name)` through the target-aware environment lookup helper."
sidebar:
  order: 103
---

## getenv()

```php
function getenv(string $name, bool $local_only): mixed
```

Lowers `getenv(name)` through the target-aware environment lookup helper.

**Parameters**:
- `$name` (`string`)
- `$local_only` (`bool`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `getenv` is implemented in the compiler, see [the internals page](../../../internals/builtins/filesystem/getenv.md).

