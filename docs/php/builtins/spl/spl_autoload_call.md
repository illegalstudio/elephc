---
title: "spl_autoload_call()"
description: "Try all registered __autoload() functions to load the requested class."
sidebar:
  order: 324
---

## spl_autoload_call()

```php
function spl_autoload_call(string $class): void
```

Try all registered __autoload() functions to load the requested class.

**Parameters**:
- `$class` (`string`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `spl_autoload_call` is implemented in the compiler, see [the internals page](../../../internals/builtins/spl/spl_autoload_call.md).

