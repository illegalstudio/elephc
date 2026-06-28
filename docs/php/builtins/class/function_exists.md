---
title: "function_exists()"
description: "Lowers `function_exists(\"name\")` for compile-time string names."
sidebar:
  order: 58
---

## function_exists()

```php
function function_exists(string $function): bool
```

Lowers `function_exists("name")` for compile-time string names.

**Parameters**:
- `$function` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `function_exists` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/function_exists.md).

