---
title: "proc_open()"
description: "Execute a command and open file pointers for process I/O."
sidebar:
  order: 332
---

## proc_open()

```php
function proc_open(array|string $command, array $descriptor_spec, array &$pipes, ?string $cwd = null, ?array $env_vars = null, ?array $options = null): resource|false
```

Execute a command and open file pointers for process I/O.

**Parameters**:
- `$command` (`array|string`)
- `$descriptor_spec` (`array`)
- `$pipes` (`array`), passed by reference
- `$cwd` (`?string`), default `null`, optional
- `$env_vars` (`?array`), default `null`, optional
- `$options` (`?array`), default `null`, optional

**Returns**: `resource|false`

## Availability

- **Compiled (AOT)**: supported by the Elephc code generator.
- **`eval()` (magician interpreter)**: supported — declarative interpreter builtin ([`crates/elephc-magician/src/interpreter/builtins/filesystem/proc_open.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/proc_open.rs)).

**Examples**:

See [`examples/process-pipes`](../../../../examples/process-pipes/main.php) for a complete stdin/stdout/stderr exchange and process-status query.

**Notes**:
- Windows accepts string and array commands, UTF-8 working directories and environment maps, plus PHP's documented process options.
- The Windows descriptor runtime supports `pipe`, `socket`, `file`, `redirect`, stream-resource, and `null` entries while preserving sparse integer keys.
- Descriptors above 2 are rejected on Windows because `STARTUPINFOW` cannot expose them as matching numbered CRT descriptors.
- The Windows `blocking_pipes` option selects blocking `ReadFile` behavior; the default probes readable pipes and reports `EAGAIN` when no bytes are available.




## Internals

For how `proc_open` is implemented in the compiler, see [the internals page](../../../internals/builtins/process/proc_open.md).
