---
title: "fpassthru()"
description: "Lowers `fpassthru(stream)` through the remaining-bytes stream runtime helper."
sidebar:
  order: 149
---

## fpassthru()

```php
function fpassthru(resource $stream): int
```

Lowers `fpassthru(stream)` through the remaining-bytes stream runtime helper.

**Parameters**:
- `$stream` (`resource`)

**Returns**: `int`

_No examples yet — check `examples/` and `showcases/` for usage patterns._







## Internals

For how `fpassthru` is implemented in the compiler, see [the internals page](../../../internals/builtins/io/fpassthru.md).

