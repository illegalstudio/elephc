---
title: "opendir()"
description: "Lowers `opendir(path)` and boxes the directory stream as `resource|false`."
sidebar:
  order: 168
---

## opendir()

```php
function opendir(string $directory): mixed
```

Lowers `opendir(path)` and boxes the directory stream as `resource|false`.

**Parameters**:
- `$directory` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `opendir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/opendir.md).

