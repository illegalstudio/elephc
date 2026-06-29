---
title: "spl_autoload_call()"
description: "Lowers no-op autoload calls by preserving arg effects and returning PHP null if used."
sidebar:
  order: 305
---

## spl_autoload_call()

```php
function spl_autoload_call(string $class): void
```

Lowers no-op autoload calls by preserving arg effects and returning PHP null if used.

**Parameters**:
- `$class` (`string`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_call` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_call.md).

