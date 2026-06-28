---
title: "stream_resolve_include_path()"
description: "Lowers `stream_resolve_include_path(filename)` as realpath-backed `string|false`."
sidebar:
  order: 192
---

## stream_resolve_include_path()

```php
function stream_resolve_include_path(string $filename): mixed
```

Lowers `stream_resolve_include_path(filename)` as realpath-backed `string|false`.

**Parameters**:
- `$filename` (`string`)

**Returns**: `mixed`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `stream_resolve_include_path` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/stream_resolve_include_path.md).

