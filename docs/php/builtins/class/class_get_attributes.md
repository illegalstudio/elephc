---
title: "class_get_attributes()"
description: "Returns an array of ReflectionAttribute objects for all attributes of a class."
---

## class_get_attributes()

```php
function class_get_attributes(string $class_name): array
```

Returns an array of ReflectionAttribute objects for all attributes of a class.

**Parameters**:
- `$class_name` (`string`)

**Returns**: `array`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Hidden in strict PHP mode: this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `class_get_attributes` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_get_attributes.md).
