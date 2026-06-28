---
title: "rewinddir()"
description: "Lowers `rewinddir(dir_handle)` for libc, glob, and userspace-wrapper handles."
sidebar:
  order: 171
---

## rewinddir()

```php
function rewinddir(resource $dir_handle): void
```

Lowers `rewinddir(dir_handle)` for libc, glob, and userspace-wrapper handles.

**Parameters**:
- `$dir_handle` (`resource`)

**Returns**: `void`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `rewinddir` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/rewinddir.md).

