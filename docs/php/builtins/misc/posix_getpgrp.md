---
title: "posix_getpgrp()"
description: "Returns the process group id of the current process."
---

## posix_getpgrp()

```php
function posix_getpgrp(): int
```

Returns the process group id of the current process.

**Parameters**: none.

**Returns**: `int`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

- Compiled code can call it on the three executable/release hosts (macOS ARM64, Linux ARM64, and Linux x86_64); calls are refused at compile time for iOS library targets.

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `posix_getpgrp` is implemented in the compiler, see [the internals page](../../../internals/builtins/misc/posix_getpgrp.md).
