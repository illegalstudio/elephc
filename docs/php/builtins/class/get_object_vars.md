---
title: "get_object_vars()"
description: "Returns the accessible non-static properties of an object."
sidebar:
  order: 91
---

## get_object_vars()

```php
function get_object_vars(mixed $object): array
```

Returns the accessible non-static properties of an object.

**Parameters**:
- `$object` (`mixed`)

**Returns**: `array`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_object_vars.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_object_vars.rs)).

**Examples**:

// Full example: examples/get-object-vars/main.php
$vars = get_object_vars($object);
echo $vars['name'];

## Internals

For how `get_object_vars` is implemented in the compiler, see [the internals page](../../../internals/builtins/class/get_object_vars.md).
