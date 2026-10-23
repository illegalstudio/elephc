---
title: "get_debug_type()"
description: "Gets the type name of a variable in a way that is suitable for debugging."
sidebar:
  order: 957
---

## get_debug_type()

```php
function get_debug_type(mixed $value): string
```

Gets the type name of a variable in a way that is suitable for debugging.

**Parameters**:
- `$value` (`mixed`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported through a declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/types/get_debug_type.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/get_debug_type.rs)).

**Examples**:

```php
// Full example: examples/type-ops/main.php
echo get_debug_type(42);           // int
echo get_debug_type(new Square());   // Square
echo get_debug_type(new class {});   // class@anonymous
```

## Internals

For how `get_debug_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/get_debug_type.md).
