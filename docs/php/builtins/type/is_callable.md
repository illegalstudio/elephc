---
title: "is_callable()"
description: "Lowers `is_callable(value)` through static lookup or runtime callable-shape helpers."
sidebar:
  order: 417
---

## is_callable()

```php
function is_callable(mixed $value, bool $syntax_only = false, string $callable_name = null): bool
```

Lowers `is_callable(value)` through static lookup or runtime callable-shape helpers.

**Parameters**:
- `$value` (`mixed`)
- `$syntax_only` (`bool`), default `false`, optional
- `$callable_name` (`string`), passed by reference, default `null`, optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `is_callable` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/is_callable.md).

