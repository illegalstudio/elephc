---
title: "readdir()"
description: "Lowers `readdir(dir_handle)` for libc, glob, and userspace-wrapper handles."
sidebar:
  order: 169
---

## readdir()

```php
function readdir(resource $dir_handle): mixed
```

Lowers `readdir(dir_handle)` for libc, glob, and userspace-wrapper handles.

**Parameters**:
- `$dir_handle` (`resource`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `readdir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/readdir.md).

