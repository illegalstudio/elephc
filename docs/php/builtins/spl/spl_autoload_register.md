---
title: "spl_autoload_register()"
description: "Register given function as __autoload() implementation."
sidebar:
  order: 327
---

## spl_autoload_register()

```php
function spl_autoload_register(callable $callback = null, bool $throw = true, bool $prepend = false): bool
```

Register given function as __autoload() implementation.

**Parameters**:
- `$callback` (`callable`), default `null`, optional
- `$throw` (`bool`), default `true`, optional
- `$prepend` (`bool`), default `false`, optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_register` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_register.md).

