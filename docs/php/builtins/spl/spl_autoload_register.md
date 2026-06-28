---
title: "spl_autoload_register()"
description: "Lowers autoload registration stubs by preserving arg effects and returning true."
sidebar:
  order: 306
---

## spl_autoload_register()

```php
function spl_autoload_register(callable $callback, bool $throw, bool $prepend): bool
```

Lowers autoload registration stubs by preserving arg effects and returning true.

**Parameters**:
- `$callback` (`callable`), optional
- `$throw` (`bool`), optional
- `$prepend` (`bool`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_register` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_register.md).

