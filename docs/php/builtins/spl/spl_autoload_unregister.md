---
title: "spl_autoload_unregister()"
description: "Lowers autoload registration stubs by preserving arg effects and returning true."
sidebar:
  order: 307
---

## spl_autoload_unregister()

```php
function spl_autoload_unregister(callable $callback): bool
```

Lowers autoload registration stubs by preserving arg effects and returning true.

**Parameters**:
- `$callback` (`callable`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_unregister` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_unregister.md).

