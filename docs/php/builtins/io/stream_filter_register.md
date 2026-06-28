---
title: "stream_filter_register()"
description: "Lowers `stream_filter_register(filter_name, class)` into the user-filter registry helper."
sidebar:
  order: 182
---

## stream_filter_register()

```php
function stream_filter_register(string $filter_name, string $class): bool
```

Lowers `stream_filter_register(filter_name, class)` into the user-filter registry helper.

**Parameters**:
- `$filter_name` (`string`)
- `$class` (`string`)

**Returns**: `bool`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_filter_register` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_filter_register.md).

