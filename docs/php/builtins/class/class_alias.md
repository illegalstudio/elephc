---
title: "class_alias()"
description: "Lowers the defensive `class_alias()` fallback that remains after AOT alias extraction."
sidebar:
  order: 49
---

## class_alias()

```php
function class_alias(string $class, string $alias, bool $autoload): bool
```

Lowers the defensive `class_alias()` fallback that remains after AOT alias extraction.

**Parameters**:
- `$class` (`string`)
- `$alias` (`string`)
- `$autoload` (`bool`), optional

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `class_alias` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_alias.md).

