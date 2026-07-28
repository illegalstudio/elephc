---
title: "Process builtins"
description: "Builtins in the Process category."
sidebar:
  order: 113
---

## Process builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`die()`](./process/die.md) | `(int $status): void` | `void` | ✓ | ✓ |
| [`exec()`](./process/exec.md) | `(string $command): string` | `string` | ✓ | ✓ |
| [`exit()`](./process/exit.md) | `(int $status): void` | `void` | ✓ | ✓ |
| [`passthru()`](./process/passthru.md) | `(string $command): void` | `void` | ✓ | ✓ |
| [`pclose()`](./process/pclose.md) | `(resource $handle): int` | `int` | ✓ | ✓ |
| [`popen()`](./process/popen.md) | `(string $command, string $mode): mixed` | `mixed` | ✓ | ✓ |
| [`proc_close()`](./process/proc_close.md) | `(resource $process): int` | `int` | ✓ | ✓ |
| [`proc_get_status()`](./process/proc_get_status.md) | `(resource $process): array|false` | `array|false` | ✓ | ✓ |
| [`proc_open()`](./process/proc_open.md) | `(array|string $command, array $descriptor_spec, array &$pipes, ?string $cwd = null, ?array $env_vars = null, ?array $options = null): resource|false` | `resource|false` | ✓ | ✓ |
| [`proc_terminate()`](./process/proc_terminate.md) | `(resource $process, int $signal = 15): bool` | `bool` | ✓ | ✓ |
| [`readline()`](./process/readline.md) | `(string $prompt = null): mixed` | `mixed` | ✓ | ✓ |
| [`shell_exec()`](./process/shell_exec.md) | `(string $command): string` | `string` | ✓ | ✓ |
| [`sleep()`](./process/sleep.md) | `(int $seconds): int` | `int` | ✓ | ✓ |
| [`system()`](./process/system.md) | `(string $command): string` | `string` | ✓ | ✓ |
| [`usleep()`](./process/usleep.md) | `(int $microseconds): void` | `void` | ✓ | ✓ |
