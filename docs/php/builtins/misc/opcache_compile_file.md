---
title: "opcache_compile_file()"
description: "Compiles a script into the opcode cache without executing it."
sidebar:
  order: 641
---

## opcache_compile_file()

```php
function opcache_compile_file(mixed $filename): bool
```

Compiles a script into the opcode cache without executing it.

**Parameters**:
- `$filename` (`mixed`)

**Returns**: `bool`

## Availability

- **Compiled (AOT)**: supported through an injected elephc-PHP prelude.
- **`eval()` (magician interpreter)**: supported through native OPcache prelude declarations or dedicated interpreter handlers; see [OPcache](../../opcache.md).

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `opcache_compile_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/opcache_compile_file.md).
