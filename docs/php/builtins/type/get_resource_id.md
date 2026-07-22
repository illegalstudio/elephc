---
title: "get_resource_id()"
description: "Returns an integer identifier for the given resource."
sidebar:
  order: 436
---

## get_resource_id()

```php
function get_resource_id(resource $resource): int
```

Returns an integer identifier for the given resource.

**Parameters**:
- `$resource` (`resource`)

**Returns**: `int`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/symbols/get_resource_id.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_resource_id.rs)).

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `get_resource_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/get_resource_id.md).
