//! Purpose:
//! Defines typed PCNTL runtime operations carried by EIR independently of PHP source names.
//!
//! Called from:
//! - PCNTL builtin semantic descriptors and the target-aware runtime-call backend.
//!
//! Key details:
//! - Arity, effects, ownership, and platform availability are centralized for the maintained PCNTL surface.

use crate::builtins::semantics::BuiltinResultOwnership;
use crate::ir::{Effects, RuntimeCallSignature};

/// Supported-target availability for one PCNTL operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PcntlTargetSupport {
    /// Available on macOS AArch64, Linux AArch64, and Linux x86_64.
    All,
    /// Available only on Linux AArch64 and Linux x86_64.
    Linux,
    /// Available only on macOS AArch64.
    MacOs,
}

/// Typed maintained PCNTL operation selected before target code generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PcntlRuntime {
    Alarm,
    AsyncSignals,
    Daemon,
    Exec,
    Fork,
    GetCpu,
    GetCpuAffinity,
    GetLastError,
    GetPriority,
    GetQosClass,
    SetCpuAffinity,
    SetNs,
    SetProcessGroup,
    SetPriority,
    SetQosClass,
    SetSession,
    Signal,
    SignalDispatch,
    SignalGetHandler,
    SignalMask,
    SignalTimedWait,
    SignalWaitInfo,
    StrError,
    Unshare,
    Wait,
    WaitId,
    WaitPid,
    WExitStatus,
    WIfContinued,
    WIfExited,
    WIfSignaled,
    WIfStopped,
    WStopSig,
    WTermSig,
}

impl PcntlRuntime {
    /// Returns the normalized operand-count contract enforced by EIR validation.
    pub const fn signature(self) -> RuntimeCallSignature {
        let (min_operands, max_operands) = match self {
            Self::Alarm
            | Self::SignalGetHandler
            | Self::StrError
            | Self::Unshare
            | Self::WExitStatus
            | Self::WIfContinued
            | Self::WIfExited
            | Self::WIfSignaled
            | Self::WIfStopped
            | Self::WStopSig
            | Self::WTermSig => (1, Some(1)),
            Self::AsyncSignals => (0, Some(1)),
            Self::Daemon => (0, Some(2)),
            Self::Exec => (1, Some(3)),
            Self::Fork
            | Self::GetCpu
            | Self::GetLastError
            | Self::GetQosClass
            | Self::SignalDispatch => (0, Some(0)),
            Self::GetCpuAffinity => (0, Some(1)),
            Self::GetPriority => (0, Some(2)),
            Self::SetCpuAffinity => (0, Some(2)),
            Self::SetNs => (0, Some(2)),
            Self::SetProcessGroup => (2, Some(2)),
            Self::SetPriority => (1, Some(3)),
            Self::SetQosClass => (0, Some(1)),
            Self::SetSession => (0, Some(0)),
            Self::Signal => (2, Some(3)),
            Self::SignalMask => (2, Some(3)),
            Self::SignalTimedWait => (1, Some(4)),
            Self::SignalWaitInfo => (1, Some(2)),
            Self::Wait => (1, Some(3)),
            Self::WaitId => (0, Some(5)),
            Self::WaitPid => (2, Some(4)),
        };
        RuntimeCallSignature::Polymorphic {
            min_operands,
            max_operands,
        }
    }

    /// Returns the stable backend-neutral EIR spelling for this operation.
    pub const fn as_eir(self) -> &'static str {
        match self {
            Self::Alarm => "pcntl.alarm",
            Self::AsyncSignals => "pcntl.async_signals",
            Self::Daemon => "pcntl.daemon",
            Self::Exec => "pcntl.exec",
            Self::Fork => "pcntl.fork",
            Self::GetCpu => "pcntl.getcpu",
            Self::GetCpuAffinity => "pcntl.getcpuaffinity",
            Self::GetLastError => "pcntl.get_last_error",
            Self::GetPriority => "pcntl.getpriority",
            Self::GetQosClass => "pcntl.getqos_class",
            Self::SetCpuAffinity => "pcntl.setcpuaffinity",
            Self::SetNs => "pcntl.setns",
            Self::SetProcessGroup => "posix.setpgid",
            Self::SetPriority => "pcntl.setpriority",
            Self::SetQosClass => "pcntl.setqos_class",
            Self::SetSession => "posix.setsid",
            Self::Signal => "pcntl.signal",
            Self::SignalDispatch => "pcntl.signal_dispatch",
            Self::SignalGetHandler => "pcntl.signal_get_handler",
            Self::SignalMask => "pcntl.sigprocmask",
            Self::SignalTimedWait => "pcntl.sigtimedwait",
            Self::SignalWaitInfo => "pcntl.sigwaitinfo",
            Self::StrError => "pcntl.strerror",
            Self::Unshare => "pcntl.unshare",
            Self::Wait => "pcntl.wait",
            Self::WaitId => "pcntl.waitid",
            Self::WaitPid => "pcntl.waitpid",
            Self::WExitStatus => "pcntl.wexitstatus",
            Self::WIfContinued => "pcntl.wifcontinued",
            Self::WIfExited => "pcntl.wifexited",
            Self::WIfSignaled => "pcntl.wifsignaled",
            Self::WIfStopped => "pcntl.wifstopped",
            Self::WStopSig => "pcntl.wstopsig",
            Self::WTermSig => "pcntl.wtermsig",
        }
    }

    /// Returns the supported target subset for this operation.
    pub const fn target_support(self) -> PcntlTargetSupport {
        match self {
            Self::GetCpu
            | Self::GetCpuAffinity
            | Self::SetCpuAffinity
            | Self::SetNs
            | Self::SignalTimedWait
            | Self::SignalWaitInfo
            | Self::Unshare => PcntlTargetSupport::Linux,
            Self::GetQosClass | Self::SetQosClass => PcntlTargetSupport::MacOs,
            _ => PcntlTargetSupport::All,
        }
    }

    /// Returns the conservative PHP-observable effects of this process operation.
    pub const fn effects(self) -> Effects {
        use Effects as E;
        match self {
            Self::WExitStatus
            | Self::WIfContinued
            | Self::WIfExited
            | Self::WIfSignaled
            | Self::WIfStopped
            | Self::WStopSig
            | Self::WTermSig => E::PURE,
            Self::StrError => E::from_bits_retain(E::ALLOC_HEAP.bits()),
            Self::GetCpu
            | Self::GetCpuAffinity
            | Self::GetLastError
            => E::from_bits_retain(
                E::READS_PROCESS.bits() | E::READS_HEAP.bits() | E::ALLOC_HEAP.bits(),
            ),
            Self::GetPriority | Self::SignalGetHandler => E::from_bits_retain(
                E::READS_PROCESS.bits()
                    | E::READS_HEAP.bits()
                    | E::ALLOC_HEAP.bits()
                    | E::MAY_THROW.bits(),
            ),
            Self::GetQosClass => E::from_bits_retain(
                E::READS_PROCESS.bits()
                    | E::READS_HEAP.bits()
                    | E::ALLOC_HEAP.bits()
                    | E::MAY_THROW.bits(),
            ),
            Self::Exec => E::from_bits_retain(
                E::READS_HEAP.bits()
                    | E::READS_PROCESS.bits()
                    | E::WRITES_PROCESS.bits()
                    | E::MAY_WARN.bits()
                    | E::MAY_THROW.bits(),
            ),
            Self::SignalDispatch => E::from_bits_retain(
                E::READS_HEAP.bits()
                    | E::WRITES_HEAP.bits()
                    | E::READS_PROCESS.bits()
                    | E::WRITES_PROCESS.bits()
                    | E::MAY_THROW.bits(),
            ),
            Self::SetQosClass => E::from_bits_retain(
                E::READS_PROCESS.bits()
                    | E::WRITES_PROCESS.bits()
                    | E::READS_HEAP.bits()
                    | E::MAY_THROW.bits(),
            ),
            Self::Wait | Self::WaitId | Self::WaitPid => E::from_bits_retain(
                E::READS_PROCESS.bits()
                    | E::WRITES_PROCESS.bits()
                    | E::WRITES_HEAP.bits()
                    | E::MAY_WARN.bits(),
            ),
            Self::SetCpuAffinity
            | Self::Daemon
            | Self::SetNs
            | Self::SetProcessGroup
            | Self::SetPriority
            | Self::Signal
            | Self::SignalMask
            | Self::SignalTimedWait
            | Self::SignalWaitInfo
            | Self::Unshare => E::from_bits_retain(
                E::READS_PROCESS.bits()
                    | E::WRITES_PROCESS.bits()
                    | E::READS_HEAP.bits()
                    | E::WRITES_HEAP.bits()
                    | E::MAY_WARN.bits()
                    | E::MAY_THROW.bits(),
            ),
            _ => E::from_bits_retain(
                E::READS_PROCESS.bits()
                    | E::WRITES_PROCESS.bits()
                    | E::READS_HEAP.bits()
                    | E::WRITES_HEAP.bits()
                    | E::MAY_WARN.bits(),
            ),
        }
    }

    /// Returns the ownership contract of the PHP value produced by this operation.
    pub const fn result_ownership(self) -> BuiltinResultOwnership {
        match self {
            Self::GetCpuAffinity
            | Self::GetPriority
            | Self::SignalGetHandler
            | Self::SignalTimedWait
            | Self::SignalWaitInfo
            | Self::StrError
            | Self::WExitStatus
            | Self::WStopSig
            | Self::WTermSig => BuiltinResultOwnership::Fresh,
            Self::GetQosClass => BuiltinResultOwnership::Fresh,
            _ => BuiltinResultOwnership::NonHeap,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies conditional PCNTL operations declare their exact supported target subsets.
    #[test]
    fn platform_specific_operations_are_explicit() {
        assert_eq!(PcntlRuntime::GetCpu.target_support(), PcntlTargetSupport::Linux);
        assert_eq!(
            PcntlRuntime::GetQosClass.target_support(),
            PcntlTargetSupport::MacOs
        );
        assert_eq!(PcntlRuntime::Fork.target_support(), PcntlTargetSupport::All);
    }

    /// Verifies output-argument wait operations retain their PHP-visible arity bounds.
    #[test]
    fn wait_signatures_preserve_optional_outputs() {
        assert_eq!(
            PcntlRuntime::WaitPid.signature(),
            RuntimeCallSignature::Polymorphic {
                min_operands: 2,
                max_operands: Some(4),
            }
        );
        assert_eq!(
            PcntlRuntime::WaitId.signature(),
            RuntimeCallSignature::Polymorphic {
                min_operands: 0,
                max_operands: Some(5),
            }
        );
    }

    /// Verifies every PCNTL EIR spelling is unique.
    #[test]
    fn eir_names_are_unique() {
        let operations = [
            PcntlRuntime::Alarm,
            PcntlRuntime::AsyncSignals,
            PcntlRuntime::Daemon,
            PcntlRuntime::Exec,
            PcntlRuntime::Fork,
            PcntlRuntime::GetCpu,
            PcntlRuntime::GetCpuAffinity,
            PcntlRuntime::GetLastError,
            PcntlRuntime::GetPriority,
            PcntlRuntime::GetQosClass,
            PcntlRuntime::SetCpuAffinity,
            PcntlRuntime::SetNs,
            PcntlRuntime::SetProcessGroup,
            PcntlRuntime::SetPriority,
            PcntlRuntime::SetQosClass,
            PcntlRuntime::SetSession,
            PcntlRuntime::Signal,
            PcntlRuntime::SignalDispatch,
            PcntlRuntime::SignalGetHandler,
            PcntlRuntime::SignalMask,
            PcntlRuntime::SignalTimedWait,
            PcntlRuntime::SignalWaitInfo,
            PcntlRuntime::StrError,
            PcntlRuntime::Unshare,
            PcntlRuntime::Wait,
            PcntlRuntime::WaitId,
            PcntlRuntime::WaitPid,
            PcntlRuntime::WExitStatus,
            PcntlRuntime::WIfContinued,
            PcntlRuntime::WIfExited,
            PcntlRuntime::WIfSignaled,
            PcntlRuntime::WIfStopped,
            PcntlRuntime::WStopSig,
            PcntlRuntime::WTermSig,
        ];
        let names = operations
            .iter()
            .map(|operation| operation.as_eir())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(names.len(), operations.len());
    }
}
