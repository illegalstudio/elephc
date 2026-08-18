//! Purpose:
//! Implements PCNTL builtins for Magician through the shared OS bridge.
//!
//! Called from:
//! - Declarative eval hooks and source-sensitive call dispatch.
//!
//! Key details:
//! - Direct calls preserve by-reference outputs; callable calls warn when storage is unavailable.
//! - Platform-specific names follow the same Linux/macOS visibility as the AOT backend.

use super::super::*;

mod arrays;
mod dispatch;
mod exec;
mod pcntl_alarm;
mod pcntl_async_signals;
mod pcntl_errno;
mod pcntl_exec;
mod pcntl_fork;
mod pcntl_get_last_error;
mod pcntl_getcpu;
mod pcntl_getcpuaffinity;
mod pcntl_getpriority;
mod pcntl_getqos_class;
mod pcntl_setcpuaffinity;
mod pcntl_setns;
mod pcntl_setpriority;
mod pcntl_setqos_class;
mod pcntl_signal;
mod pcntl_signal_dispatch;
mod pcntl_signal_get_handler;
mod pcntl_sigprocmask;
mod pcntl_sigtimedwait;
mod pcntl_sigwaitinfo;
mod pcntl_strerror;
mod pcntl_unshare;
mod pcntl_wait;
mod pcntl_waitid;
mod pcntl_waitpid;
mod pcntl_wexitstatus;
mod pcntl_wifcontinued;
mod pcntl_wifexited;
mod pcntl_wifsignaled;
mod pcntl_wifstopped;
mod pcntl_wstopsig;
mod pcntl_wtermsig;
mod qos;
mod scalars;
mod signals;
mod waits;

use arrays::*;
pub(in crate::interpreter) use dispatch::*;
use exec::*;
use qos::*;
use scalars::*;
use signals::eval_pcntl_signal_result;
pub(in crate::interpreter) use signals::eval_pcntl_maybe_dispatch;
use waits::*;
