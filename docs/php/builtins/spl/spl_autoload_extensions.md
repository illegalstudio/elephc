---
title: "spl_autoload_extensions()"
description: "Lowers `spl_autoload_extensions()` against the legacy mutable extension globals."
sidebar:
  order: 321
---

## spl_autoload_extensions()

```php
function spl_autoload_extensions(string $file_extensions): string
```

Lowers `spl_autoload_extensions()` against the legacy mutable extension globals.

**Parameters**:
- `$file_extensions` (`string`), optional

**Returns**: `string`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_extensions` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_extensions.md).

