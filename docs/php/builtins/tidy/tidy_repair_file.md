---
title: "tidy_repair_file()"
description: "Repairs an HTML file and returns the repaired markup."
---

## tidy_repair_file()

```php
function tidy_repair_file(string $filename, array|string|null $config = null, ?string $encoding = null, bool $useIncludePath = false): string|false
```

Repairs an HTML file and returns the repaired markup.

**Parameters**:
- `$filename` (`string`)
- `$config` (`array|string|null`), default `null`, optional
- `$encoding` (`?string`), default `null`, optional
- `$useIncludePath` (`bool`), default `false`, optional

**Returns**: `string|false`

## Availability

| Compiled (AOT) | Magician (eval) |
|---|---|
| Available | Available |

_No examples yet. Check `examples/` and `showcases/` for usage patterns._

## Internals

For how `tidy_repair_file` is implemented in the compiler, see [the internals page](../../../internals/builtins/tidy/tidy_repair_file.md).
