---
title: "class_attribute_args()"
description: "Returns the constructor arguments of a named attribute applied to a class."
sidebar:
  order: 67
---

## class_attribute_args()

```php
function class_attribute_args(string $class_name, string $attribute_name): array
```

Returns the constructor arguments of a named attribute applied to a class.

**Parameters**:
- `$class_name` (`string`)
- `$attribute_name` (`string`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/class_attribute_args.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/class_attribute_args.rs)).
- **Strict PHP mode**: hidden — this builtin is an elephc extension with no PHP equivalent, so programs compiled with [`--strict-php`](../../../compiling/cli-reference.md#strict-php-mode) treat the name as nonexistent, in compiled code and inside eval'd code.

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `class_attribute_args` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/class_attribute_args.md).

