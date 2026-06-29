---
title: "get_resource_id()"
description: "Lowers `get_resource_id(resource)` by unboxing the native handle and making it one-based."
sidebar:
  order: 396
---

## get_resource_id()

```php
function get_resource_id(resource $resource): int
```

Lowers `get_resource_id(resource)` by unboxing the native handle and making it one-based.

**Parameters**:
- `$resource` (`resource`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `get_resource_id` is implemented in the compiler, see [the internals page](../../../internals/builtins/type/get_resource_id.md).

