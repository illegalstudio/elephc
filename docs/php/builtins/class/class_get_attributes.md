---
title: "class_get_attributes()"
description: "Returns an array of ReflectionAttribute objects for all attributes of a class."
sidebar:
  order: 70
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

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/class_get_attributes.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_get_attributes.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `class_get_attributes` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_get_attributes.md).
