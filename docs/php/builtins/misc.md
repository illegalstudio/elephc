---
title: "Misc builtins"
description: "Builtins in the Misc category."
sidebar:
  order: 118
---

## Misc builtins

| Function | Signature | Returns | AOT | eval() |
|---|---|---|:-:|:-:|
| [`constant()`](./misc/constant.md) | `(string $name): mixed` | `mixed` | ✓ | ✓ |
| [`debug_backtrace()`](./misc/debug_backtrace.md) | `(int $options = 1, int $limit = 0): array` | `array` | ✓ | ✓ |
| [`debug_print_backtrace()`](./misc/debug_print_backtrace.md) | `(int $options = 0, int $limit = 0): void` | `void` | ✓ | ✓ |
| [`define()`](./misc/define.md) | `(string $constant_name, mixed $value): bool` | `bool` | ✓ | ✓ |
| [`defined()`](./misc/defined.md) | `(string $constant_name): bool` | `bool` | ✓ | ✓ |
| [`empty()`](./misc/empty.md) | `(mixed $value): bool` | `bool` | ✓ | ✓ |
| [`error_reporting()`](./misc/error_reporting.md) | `(mixed $error_level = null): int` | `int` | ✓ | ✓ |
| [`extension_loaded()`](./misc/extension_loaded.md) | `(string $extension): bool` | `bool` | ✓ | ✓ |
| [`func_get_arg()`](./misc/func_get_arg.md) | `(int $position): mixed` | `mixed` | ✓ | ✓ |
| [`func_get_args()`](./misc/func_get_args.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`func_num_args()`](./misc/func_num_args.md) | `(): int` | `int` | ✓ | ✓ |
| [`gc_collect_cycles()`](./misc/gc_collect_cycles.md) | `(): int` | `int` | ✓ | ✓ |
| [`gc_disable()`](./misc/gc_disable.md) | `(): void` | `void` | ✓ | ✓ |
| [`gc_enable()`](./misc/gc_enable.md) | `(): void` | `void` | ✓ | ✓ |
| [`gc_enabled()`](./misc/gc_enabled.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`gc_mem_caches()`](./misc/gc_mem_caches.md) | `(): int` | `int` | ✓ | ✓ |
| [`gc_status()`](./misc/gc_status.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`get_defined_constants()`](./misc/get_defined_constants.md) | `(bool $categorize = false): array` | `array` | ✓ | ✓ |
| [`get_defined_functions()`](./misc/get_defined_functions.md) | `(bool $exclude_disabled = true): array` | `array` | ✓ | ✓ |
| [`get_defined_vars()`](./misc/get_defined_vars.md) | `(): array` | `array` | ✓ | ✓ |
| [`get_extension_funcs()`](./misc/get_extension_funcs.md) | `(string $extension): mixed` | `mixed` | ✓ | ✓ |
| [`get_included_files()`](./misc/get_included_files.md) | `(): array` | `array` | ✓ | ✓ |
| [`get_loaded_extensions()`](./misc/get_loaded_extensions.md) | `(bool $zend_extensions = false): array` | `array` | ✓ | ✓ |
| [`get_mangled_object_vars()`](./misc/get_mangled_object_vars.md) | `(mixed $object): array` | `array` | ✓ | ✓ |
| [`get_required_files()`](./misc/get_required_files.md) | `(): array` | `array` | ✓ | ✓ |
| [`get_resources()`](./misc/get_resources.md) | `(mixed $type = null): array` | `array` | ✓ | ✓ |
| [`header()`](./misc/header.md) | `(string $header, bool $replace = true, int $response_code = 0): void` | `void` | ✓ | ✓ |
| [`http_response_code()`](./misc/http_response_code.md) | `(int $response_code = 0): int` | `int` | ✓ | ✓ |
| [`ini_restore()`](./misc/ini_restore.md) | `(string $option): void` | `void` | ✓ | — |
| [`isset()`](./misc/isset.md) | `(mixed $var, ...$vars): bool` | `bool` | ✓ | ✓ |
| [`opcache_compile_file()`](./misc/opcache_compile_file.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_get_configuration()`](./misc/opcache_get_configuration.md) | `(): array` | `array` | ✓ | — |
| [`opcache_get_status()`](./misc/opcache_get_status.md) | `(mixed $include_scripts = true): mixed` | `mixed` | ✓ | — |
| [`opcache_invalidate()`](./misc/opcache_invalidate.md) | `(mixed $filename, mixed $force = false): bool` | `bool` | ✓ | — |
| [`opcache_is_script_cached()`](./misc/opcache_is_script_cached.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_is_script_cached_in_file_cache()`](./misc/opcache_is_script_cached_in_file_cache.md) | `(mixed $filename): bool` | `bool` | ✓ | — |
| [`opcache_jit_blacklist()`](./misc/opcache_jit_blacklist.md) | `(mixed $closure): void` | `void` | ✓ | — |
| [`opcache_reset()`](./misc/opcache_reset.md) | `(): bool` | `bool` | ✓ | — |
| [`pcntl_alarm()`](./misc/pcntl_alarm.md) | `(int $seconds): int` | `int` | ✓ | ✓ |
| [`pcntl_async_signals()`](./misc/pcntl_async_signals.md) | `(bool $enable = null): bool` | `bool` | ✓ | ✓ |
| [`pcntl_daemon()`](./misc/pcntl_daemon.md) | `(bool $no_chdir = false, bool $no_close = false): bool` | `bool` | ✓ | ✓ |
| [`pcntl_errno()`](./misc/pcntl_errno.md) | `(): int` | `int` | ✓ | ✓ |
| [`pcntl_exec()`](./misc/pcntl_exec.md) | `(string $path, mixed $args = [], mixed $env_vars = []): bool` | `bool` | ✓ | ✓ |
| [`pcntl_fork()`](./misc/pcntl_fork.md) | `(): int` | `int` | ✓ | ✓ |
| [`pcntl_get_last_error()`](./misc/pcntl_get_last_error.md) | `(): int` | `int` | ✓ | ✓ |
| [`pcntl_getcpu()`](./misc/pcntl_getcpu.md) | `(): int` | `int` | ✓ | ✓ |
| [`pcntl_getcpuaffinity()`](./misc/pcntl_getcpuaffinity.md) | `(int $process_id = null): mixed` | `mixed` | ✓ | ✓ |
| [`pcntl_getpriority()`](./misc/pcntl_getpriority.md) | `(int $process_id = null, int $mode = 0): mixed` | `mixed` | ✓ | ✓ |
| [`pcntl_getqos_class()`](./misc/pcntl_getqos_class.md) | `(): mixed` | `mixed` | ✓ | ✓ |
| [`pcntl_setcpuaffinity()`](./misc/pcntl_setcpuaffinity.md) | `(int $process_id = null, mixed $cpu_ids = []): bool` | `bool` | ✓ | ✓ |
| [`pcntl_setns()`](./misc/pcntl_setns.md) | `(int $process_id = null, int $nstype = 1073741824): bool` | `bool` | ✓ | ✓ |
| [`pcntl_setpriority()`](./misc/pcntl_setpriority.md) | `(int $priority, int $process_id = null, int $mode = 0): bool` | `bool` | ✓ | ✓ |
| [`pcntl_setqos_class()`](./misc/pcntl_setqos_class.md) | `(mixed $qos_class): void` | `void` | ✓ | ✓ |
| [`pcntl_signal()`](./misc/pcntl_signal.md) | `(int $signal, mixed $handler, bool $restart_syscalls = true): bool` | `bool` | ✓ | ✓ |
| [`pcntl_signal_dispatch()`](./misc/pcntl_signal_dispatch.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`pcntl_signal_get_handler()`](./misc/pcntl_signal_get_handler.md) | `(int $signal): mixed` | `mixed` | ✓ | ✓ |
| [`pcntl_sigprocmask()`](./misc/pcntl_sigprocmask.md) | `(int $mode, mixed $signals, mixed $old_signals = []): bool` | `bool` | ✓ | ✓ |
| [`pcntl_sigtimedwait()`](./misc/pcntl_sigtimedwait.md) | `(mixed $signals, mixed $info = [], int $seconds = 0, int $nanoseconds = 0): mixed` | `mixed` | ✓ | ✓ |
| [`pcntl_sigwaitinfo()`](./misc/pcntl_sigwaitinfo.md) | `(mixed $signals, mixed $info = []): mixed` | `mixed` | ✓ | ✓ |
| [`pcntl_strerror()`](./misc/pcntl_strerror.md) | `(int $error_code): string` | `string` | ✓ | ✓ |
| [`pcntl_unshare()`](./misc/pcntl_unshare.md) | `(int $flags): bool` | `bool` | ✓ | ✓ |
| [`pcntl_wait()`](./misc/pcntl_wait.md) | `(mixed $status, int $flags = 0, mixed $resource_usage = []): int` | `int` | ✓ | ✓ |
| [`pcntl_waitid()`](./misc/pcntl_waitid.md) | `(int $idtype = 0, int $id = null, mixed $info = [], int $flags = 4, mixed $resource_usage = []): bool` | `bool` | ✓ | ✓ |
| [`pcntl_waitpid()`](./misc/pcntl_waitpid.md) | `(int $process_id, mixed $status, int $flags = 0, mixed $resource_usage = []): int` | `int` | ✓ | ✓ |
| [`pcntl_wexitstatus()`](./misc/pcntl_wexitstatus.md) | `(int $status): mixed` | `mixed` | ✓ | ✓ |
| [`pcntl_wifcontinued()`](./misc/pcntl_wifcontinued.md) | `(int $status): bool` | `bool` | ✓ | ✓ |
| [`pcntl_wifexited()`](./misc/pcntl_wifexited.md) | `(int $status): bool` | `bool` | ✓ | ✓ |
| [`pcntl_wifsignaled()`](./misc/pcntl_wifsignaled.md) | `(int $status): bool` | `bool` | ✓ | ✓ |
| [`pcntl_wifstopped()`](./misc/pcntl_wifstopped.md) | `(int $status): bool` | `bool` | ✓ | ✓ |
| [`pcntl_wstopsig()`](./misc/pcntl_wstopsig.md) | `(int $status): mixed` | `mixed` | ✓ | ✓ |
| [`pcntl_wtermsig()`](./misc/pcntl_wtermsig.md) | `(int $status): mixed` | `mixed` | ✓ | ✓ |
| [`php_sapi_name()`](./misc/php_sapi_name.md) | `(): string` | `string` | ✓ | — |
| [`php_uname()`](./misc/php_uname.md) | `(string $mode = 'a'): string` | `string` | ✓ | ✓ |
| [`phpversion()`](./misc/phpversion.md) | `(?string $extension = null): string|false` | `string|false` | ✓ | ✓ |
| [`posix_setpgid()`](./misc/posix_setpgid.md) | `(int $process_id, int $process_group_id): bool` | `bool` | ✓ | ✓ |
| [`posix_setsid()`](./misc/posix_setsid.md) | `(): int` | `int` | ✓ | ✓ |
| [`print_r()`](./misc/print_r.md) | `(mixed $value, bool $return = false): mixed` | `mixed` | ✓ | ✓ |
| [`restore_error_handler()`](./misc/restore_error_handler.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`restore_exception_handler()`](./misc/restore_exception_handler.md) | `(): bool` | `bool` | ✓ | ✓ |
| [`serialize()`](./misc/serialize.md) | `(mixed $value): string` | `string` | ✓ | — |
| [`set_error_handler()`](./misc/set_error_handler.md) | `(mixed $callback, int $error_levels = E_ALL): mixed` | `mixed` | ✓ | ✓ |
| [`set_exception_handler()`](./misc/set_exception_handler.md) | `(mixed $callback): mixed` | `mixed` | ✓ | ✓ |
| [`trigger_error()`](./misc/trigger_error.md) | `(string $message, int $error_level = E_USER_NOTICE): bool` | `bool` | ✓ | ✓ |
| [`unserialize()`](./misc/unserialize.md) | `(string $data, mixed $options = []): mixed` | `mixed` | ✓ | — |
| [`unset()`](./misc/unset.md) | `(mixed $var, ...$vars): void` | `void` | ✓ | ✓ |
| [`user_error()`](./misc/user_error.md) | `(string $message, int $error_level = 1024): bool` | `bool` | ✓ | ✓ |
| [`var_dump()`](./misc/var_dump.md) | `(mixed $value, ...$values): void` | `void` | ✓ | ✓ |
| [`zend_version()`](./misc/zend_version.md) | `(): string` | `string` | ✓ | ✓ |
