---
title: "get_resource_type()"
description: "Returns the type of a resource."
sidebar:
  order: 421
---

## get_resource_type()

```php
function get_resource_type(resource $resource): string
```

Returns the type of a resource.

**Parameters**:
- `$resource` (`resource`)

**Returns**: `string`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_resource_type.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_resource_type.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `get_resource_type` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/get_resource_type.md).

