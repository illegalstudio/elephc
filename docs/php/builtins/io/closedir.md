---
title: "closedir()"
description: "Lowers `closedir(dir_handle)` for libc, glob, and userspace-wrapper handles."
sidebar:
  order: 136
---

## closedir()

```php
function closedir(resource $dir_handle): void
```

Lowers `closedir(dir_handle)` for libc, glob, and userspace-wrapper handles.

**Parameters**:
- `$dir_handle` (`resource`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `closedir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/closedir.md).

