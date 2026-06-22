---
title: "spl_autoload()"
description: "Lowers no-op autoload calls by preserving arg effects and returning PHP null if used."
sidebar:
  order: 304
---

## spl_autoload()

```php
function spl_autoload(string $class, string $file_extensions): void
```

Lowers no-op autoload calls by preserving arg effects and returning PHP null if used.

**Parameters**:
- `$class` (`string`)
- `$file_extensions` (`string`), optional

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload.md).

