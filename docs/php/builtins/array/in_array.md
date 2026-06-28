---
title: "in_array()"
description: "Lowers `in_array()` for indexed arrays with scalar or string payloads."
sidebar:
  order: 35
---

## in_array()

```php
function in_array(mixed $needle, array $haystack, bool $strict): mixed
```

Lowers `in_array()` for indexed arrays with scalar or string payloads.

**Parameters**:
- `$needle` (`mixed`)
- `$haystack` (`array`)
- `$strict` (`bool`), optional

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `in_array` is implemented in the compiler, see [the internals page](../../../internals/builtins/array/in_array.md).

