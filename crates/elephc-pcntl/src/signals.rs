//! Purpose:
//! Owns PCNTL signal registration, process-local queueing, masks, and synchronous waits.
//!
//! Called from:
//! - Process forking to isolate the child queue before signal delivery resumes.
//! - AOT and Magician PCNTL adapters through the bridge's stable C ABI.
//!
//! Key details:
//! - Async handlers route fixed-size records to backend-specific nonblocking self-pipes.
//! - Saturated pipes spill into preallocated lock-free queues that retain each siginfo record.
//! - Dispatch blocks signals so callers drain one stable snapshot and restore the prior mask.
//! - The child side of `fork()` replaces inherited descriptors before unmasking signals.

use crate::{current_errno, errno_location, record_errno, LAST_ERROR};
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::OnceLock;

/// Selects the generated AOT runtime's signal-handler queue.
pub const PCNTL_SIGNAL_OWNER_AOT: libc::c_int = 1;
/// Selects Magician's eval signal-handler queue.
pub const PCNTL_SIGNAL_OWNER_EVAL: libc::c_int = 2;

const SIGNAL_OWNER_COUNT: usize = 2;
const SIGNAL_OVERFLOW_CAPACITY: usize = 4096;

static SIGNAL_PIPES: OnceLock<SignalPipes> = OnceLock::new();
static OVERFLOW_QUEUES: [OverflowQueue; SIGNAL_OWNER_COUNT] =
    [const { OverflowQueue::new() }; SIGNAL_OWNER_COUNT];

/// Backend-specific descriptor pairs used to keep AOT and eval drains independent.
struct SignalPipes {
    queues: [SignalPipe; SIGNAL_OWNER_COUNT],
}

/// Mutable descriptor pair whose allocation itself remains process-global.
///
/// The atomics are required because `fork()` must replace the child copy without replacing the
/// `OnceLock`; after the fork the child owns private memory and therefore private descriptor words.
struct SignalPipe {
    read_descriptor: AtomicI32,
    write_descriptor: AtomicI32,
    error: AtomicI32,
}

/// One cell in the bounded multi-producer, single-consumer overflow queue.
struct OverflowSlot {
    sequence: AtomicU64,
    value: UnsafeCell<MaybeUninit<ElephcPcntlSigInfo>>,
}

// The sequence protocol grants a producer or the consumer exclusive access before the cell is
// read or written, so sharing the interior cell across signal handlers is synchronized.
unsafe impl Sync for OverflowSlot {}

impl OverflowSlot {
    /// Creates one uninitialized queue cell with its initial sequence number.
    const fn new(sequence: u64) -> Self {
        Self {
            sequence: AtomicU64::new(sequence),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }
}

/// Lock-free per-delivery fallback used when one backend's self-pipe is full.
struct OverflowQueue {
    enqueue_position: AtomicU64,
    dequeue_position: AtomicU64,
    slots: [OverflowSlot; SIGNAL_OVERFLOW_CAPACITY],
}

impl OverflowQueue {
    /// Creates one empty preallocated overflow queue.
    const fn new() -> Self {
        let mut slots = [const { OverflowSlot::new(0) }; SIGNAL_OVERFLOW_CAPACITY];
        let mut index = 0;
        while index < SIGNAL_OVERFLOW_CAPACITY {
            slots[index] = OverflowSlot::new(index as u64);
            index += 1;
        }
        Self {
            enqueue_position: AtomicU64::new(0),
            dequeue_position: AtomicU64::new(0),
            slots,
        }
    }

    /// Publishes one complete siginfo record without allocating, returning false when full.
    fn push(&self, info: &ElephcPcntlSigInfo) -> bool {
        let mut position = self.enqueue_position.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[position as usize % SIGNAL_OVERFLOW_CAPACITY];
            let sequence = slot.sequence.load(Ordering::Acquire);
            let difference = sequence.wrapping_sub(position) as i64;
            if difference == 0 {
                match self.enqueue_position.compare_exchange_weak(
                    position,
                    position.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        unsafe { (*slot.value.get()).write(*info) };
                        slot.sequence
                            .store(position.wrapping_add(1), Ordering::Release);
                        return true;
                    }
                    Err(observed) => position = observed,
                }
            } else if difference < 0 {
                return false;
            } else {
                position = self.enqueue_position.load(Ordering::Relaxed);
            }
        }
    }

    /// Claims and returns the oldest complete overflow record.
    fn pop(&self) -> Option<ElephcPcntlSigInfo> {
        let mut position = self.dequeue_position.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[position as usize % SIGNAL_OVERFLOW_CAPACITY];
            let sequence = slot.sequence.load(Ordering::Acquire);
            let difference = sequence.wrapping_sub(position.wrapping_add(1)) as i64;
            if difference == 0 {
                match self.dequeue_position.compare_exchange_weak(
                    position,
                    position.wrapping_add(1),
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        let info = unsafe { (*slot.value.get()).assume_init_read() };
                        slot.sequence.store(
                            position.wrapping_add(SIGNAL_OVERFLOW_CAPACITY as u64),
                            Ordering::Release,
                        );
                        return Some(info);
                    }
                    Err(observed) => position = observed,
                }
            } else if difference < 0 {
                return None;
            } else {
                position = self.dequeue_position.load(Ordering::Relaxed);
            }
        }
    }

    /// Discards inherited pending deliveries when a forked process receives private queues.
    fn clear(&self) {
        self.enqueue_position.store(0, Ordering::Relaxed);
        self.dequeue_position.store(0, Ordering::Relaxed);
        for (index, slot) in self.slots.iter().enumerate() {
            slot.sequence.store(index as u64, Ordering::Relaxed);
        }
    }
}

/// Opaque storage for a target-native signal mask saved across one dispatch snapshot.
///
/// Linux uses a 128-byte `sigset_t`; Darwin's representation is smaller. Keeping the largest
/// supported layout behind a fixed C ABI lets generated code reserve the same block on every
/// target without interpreting its contents.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct ElephcPcntlSignalMask {
    storage: [u8; 128],
}

impl Default for ElephcPcntlSignalMask {
    /// Returns zeroed opaque storage suitable for `elephc_pcntl_dispatch_begin()`.
    fn default() -> Self {
        Self { storage: [0; 128] }
    }
}

/// Stable, target-neutral signal-information record shared with generated AOT code.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ElephcPcntlSigInfo {
    pub signo: i64,
    pub error: i64,
    pub code: i64,
    pub status: i64,
    pub pid: i64,
    pub uid: i64,
    pub utime: i64,
    pub stime: i64,
    pub address: i64,
    pub band: i64,
    pub fd: i64,
    pub present: u64,
}

/// Presence bit for `ElephcPcntlSigInfo::signo`.
pub const SIGINFO_SIGNO: u64 = 1 << 0;
/// Presence bit for `ElephcPcntlSigInfo::error`.
pub const SIGINFO_ERRNO: u64 = 1 << 1;
/// Presence bit for `ElephcPcntlSigInfo::code`.
pub const SIGINFO_CODE: u64 = 1 << 2;
/// Presence bit for `ElephcPcntlSigInfo::status`.
pub const SIGINFO_STATUS: u64 = 1 << 3;
/// Presence bit for `ElephcPcntlSigInfo::pid`.
pub const SIGINFO_PID: u64 = 1 << 4;
/// Presence bit for `ElephcPcntlSigInfo::uid`.
pub const SIGINFO_UID: u64 = 1 << 5;
/// Presence bit for Linux `ElephcPcntlSigInfo::utime`.
pub const SIGINFO_UTIME: u64 = 1 << 6;
/// Presence bit for Linux `ElephcPcntlSigInfo::stime`.
pub const SIGINFO_STIME: u64 = 1 << 7;
/// Presence bit for `ElephcPcntlSigInfo::address`.
pub const SIGINFO_ADDRESS: u64 = 1 << 8;
/// Presence bit for Linux `ElephcPcntlSigInfo::band`.
pub const SIGINFO_BAND: u64 = 1 << 9;
/// Presence bit for Linux `ElephcPcntlSigInfo::fd`.
pub const SIGINFO_FD: u64 = 1 << 10;

/// Returns one past the largest signal number accepted by the current target.
#[cfg(target_os = "linux")]
fn signal_limit() -> libc::c_int {
    libc::SIGRTMAX() + 1
}

/// Returns one past the largest signal number accepted by Darwin.
#[cfg(target_os = "macos")]
const fn signal_limit() -> libc::c_int {
    32
}

/// Builds a native signal set from the stable widened integer-array ABI.
///
/// # Safety
/// `signals` must be readable for `count` consecutive `i64` values when `count` is nonzero.
unsafe fn build_signal_set(
    signals: *const i64,
    count: usize,
    allow_empty: bool,
) -> Option<libc::sigset_t> {
    if count == 0 && !allow_empty {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return None;
    }
    if count != 0 && signals.is_null() {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return None;
    }
    let mut set = std::mem::zeroed::<libc::sigset_t>();
    if libc::sigemptyset(&mut set) != 0 {
        record_errno();
        return None;
    }
    for index in 0..count {
        let signal = std::ptr::read_unaligned(signals.add(index));
        if signal < 1 || signal >= i64::from(signal_limit()) {
            LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
            return None;
        }
        if libc::sigaddset(&mut set, signal as libc::c_int) != 0 {
            record_errno();
            return None;
        }
    }
    Some(set)
}

/// Validates PHP's widened signal-array ABI without changing process signal state.
///
/// Returns one when valid, minus one for a forbidden empty array, minus two for an out-of-range
/// signal, and zero for an invalid native pointer.
///
/// # Safety
/// `signals` must be readable for `count` consecutive `i64` values when `count` is nonzero.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_validate_signal_set(
    signals: *const i64,
    count: usize,
    allow_empty: libc::c_int,
) -> libc::c_int {
    if count == 0 {
        return if allow_empty != 0 { 1 } else { -1 };
    }
    if signals.is_null() {
        return 0;
    }
    for index in 0..count {
        let signal = std::ptr::read_unaligned(signals.add(index));
        if signal < 1 || signal >= i64::from(signal_limit()) {
            return -2;
        }
    }
    1
}

/// Copies target-native signal information into the stable PCNTL record.
unsafe fn copy_signal_siginfo(
    signal: libc::c_int,
    info: &libc::siginfo_t,
) -> ElephcPcntlSigInfo {
    let mut stable = ElephcPcntlSigInfo {
        signo: i64::from(info.si_signo),
        error: i64::from(info.si_errno),
        code: i64::from(info.si_code),
        present: SIGINFO_SIGNO | SIGINFO_ERRNO | SIGINFO_CODE,
        ..ElephcPcntlSigInfo::default()
    };
    if signal == libc::SIGCHLD {
        stable.status = i64::from(info.si_status());
        stable.pid = i64::from(info.si_pid());
        stable.uid = i64::from(info.si_uid());
        stable.present |= SIGINFO_STATUS | SIGINFO_PID | SIGINFO_UID;
        #[cfg(target_os = "linux")]
        {
            stable.utime = info.si_utime();
            stable.stime = info.si_stime();
            stable.present |= SIGINFO_UTIME | SIGINFO_STIME;
        }
    } else if signal == libc::SIGUSR1
        || signal == libc::SIGUSR2
        || is_realtime_signal(signal)
    {
        stable.pid = i64::from(info.si_pid());
        stable.uid = i64::from(info.si_uid());
        stable.present |= SIGINFO_PID | SIGINFO_UID;
    } else if matches!(signal, libc::SIGILL | libc::SIGFPE | libc::SIGSEGV | libc::SIGBUS) {
        stable.address = info.si_addr() as usize as i64;
        stable.present |= SIGINFO_ADDRESS;
    }
    #[cfg(target_os = "linux")]
    if signal == libc::SIGPOLL {
        #[repr(C)]
        struct PollSigInfo {
            signo: libc::c_int,
            error: libc::c_int,
            code: libc::c_int,
            alignment: libc::c_int,
            band: libc::c_long,
            fd: libc::c_int,
        }
        let poll = &*(info as *const libc::siginfo_t).cast::<PollSigInfo>();
        stable.band = poll.band;
        stable.fd = i64::from(poll.fd);
        stable.present |= SIGINFO_BAND | SIGINFO_FD;
    }
    stable
}

/// Reports whether a signal is in Linux's target-native realtime range.
#[cfg(target_os = "linux")]
fn is_realtime_signal(signal: libc::c_int) -> bool {
    signal >= libc::SIGRTMIN() && signal <= libc::SIGRTMAX()
}

/// Reports that Darwin has no realtime signal range exposed by PCNTL.
#[cfg(target_os = "macos")]
const fn is_realtime_signal(_signal: libc::c_int) -> bool {
    false
}

/// Creates one nonblocking close-on-exec signal pipe or returns the native errno.
fn create_signal_pipe() -> Result<(libc::c_int, libc::c_int), libc::c_int> {
    let mut descriptors = [-1; 2];
    if unsafe { libc::pipe(descriptors.as_mut_ptr()) } != 0 {
        return Err(current_errno());
    }
    for descriptor in descriptors {
        let status_flags = unsafe { libc::fcntl(descriptor, libc::F_GETFL) };
        let descriptor_flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
        if status_flags == -1
            || descriptor_flags == -1
            || unsafe { libc::fcntl(descriptor, libc::F_SETFL, status_flags | libc::O_NONBLOCK) }
                == -1
            || unsafe {
                libc::fcntl(descriptor, libc::F_SETFD, descriptor_flags | libc::FD_CLOEXEC)
            } == -1
        {
            let error = current_errno();
            unsafe {
                libc::close(descriptors[0]);
                libc::close(descriptors[1]);
            }
            return Err(error);
        }
    }
    Ok((descriptors[0], descriptors[1]))
}

/// Maps one stable backend owner identifier to its queue-array index.
const fn signal_owner_index(owner: libc::c_int) -> Option<usize> {
    match owner {
        PCNTL_SIGNAL_OWNER_AOT => Some(0),
        PCNTL_SIGNAL_OWNER_EVAL => Some(1),
        _ => None,
    }
}

/// Creates one queue descriptor pair, retaining its initialization error when unavailable.
fn initialize_signal_pipe() -> SignalPipe {
    match create_signal_pipe() {
        Ok((read_descriptor, write_descriptor)) => SignalPipe {
            read_descriptor: AtomicI32::new(read_descriptor),
            write_descriptor: AtomicI32::new(write_descriptor),
            error: AtomicI32::new(0),
        },
        Err(error) => SignalPipe {
            read_descriptor: AtomicI32::new(-1),
            write_descriptor: AtomicI32::new(-1),
            error: AtomicI32::new(error),
        },
    }
}

/// Creates or loads the process-local nonblocking self-pipe for one handler backend.
fn ensure_signal_pipe(owner: libc::c_int) -> Option<(libc::c_int, libc::c_int)> {
    let Some(index) = signal_owner_index(owner) else {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return None;
    };
    let queues = SIGNAL_PIPES.get_or_init(|| SignalPipes {
        queues: std::array::from_fn(|_| initialize_signal_pipe()),
    });
    let pipe = &queues.queues[index];
    let read_descriptor = pipe.read_descriptor.load(Ordering::Acquire);
    let write_descriptor = pipe.write_descriptor.load(Ordering::Acquire);
    if read_descriptor == -1 || write_descriptor == -1 {
        LAST_ERROR.store(pipe.error.load(Ordering::Relaxed), Ordering::Relaxed);
        None
    } else {
        Some((read_descriptor, write_descriptor))
    }
}

/// Reports whether this process has initialized its asynchronous signal queue.
pub(crate) fn signal_queue_initialized() -> bool {
    SIGNAL_PIPES.get().is_some()
}

/// Replaces the inherited signal pipe in the child so parent and child queues are independent.
///
/// The caller keeps signals blocked from before `fork()` until this routine returns, so no native
/// handler can observe the descriptor swap halfway through.
pub(crate) fn reset_signal_pipe_after_fork() {
    let Some(queues) = SIGNAL_PIPES.get() else {
        return;
    };
    for (owner_index, pipe) in queues.queues.iter().enumerate() {
        let old_read = pipe.read_descriptor.swap(-1, Ordering::AcqRel);
        let old_write = pipe.write_descriptor.swap(-1, Ordering::AcqRel);
        match create_signal_pipe() {
            Ok((read_descriptor, write_descriptor)) => {
                pipe.error.store(0, Ordering::Relaxed);
                pipe.write_descriptor.store(write_descriptor, Ordering::Release);
                pipe.read_descriptor.store(read_descriptor, Ordering::Release);
            }
            Err(error) => {
                pipe.error.store(error, Ordering::Relaxed);
                LAST_ERROR.store(error, Ordering::Relaxed);
            }
        }
        unsafe {
            if old_read >= 0 {
                libc::close(old_read);
            }
            if old_write >= 0 {
                libc::close(old_write);
            }
        }
        OVERFLOW_QUEUES[owner_index].clear();
    }
}

/// Queues one AOT-owned record through the generated runtime's signal pipe.
unsafe extern "C" fn queued_signal_handler_aot(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    queued_signal_handler(signal, info, context, 0);
}

/// Queues one eval-owned record through Magician's signal pipe.
unsafe extern "C" fn queued_signal_handler_eval(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    context: *mut libc::c_void,
) {
    queued_signal_handler(signal, info, context, 1);
}

/// Queues one fixed-size signal record through its backend's async-signal-safe self-pipe.
unsafe fn queued_signal_handler(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
    owner_index: usize,
) {
    let saved_errno = *errno_location();
    if let Some(queues) = SIGNAL_PIPES.get() {
        let pipe = &queues.queues[owner_index];
        let write_descriptor = pipe.write_descriptor.load(Ordering::Acquire);
        if write_descriptor < 0 {
            *errno_location() = saved_errno;
            return;
        }
        let stable = if info.is_null() {
            ElephcPcntlSigInfo {
                signo: i64::from(signal),
                present: SIGINFO_SIGNO,
                ..ElephcPcntlSigInfo::default()
            }
        } else {
            copy_signal_siginfo(signal, &*info)
        };
        let written = libc::write(
            write_descriptor,
            std::ptr::from_ref(&stable).cast(),
            std::mem::size_of::<ElephcPcntlSigInfo>(),
        );
        if written != std::mem::size_of::<ElephcPcntlSigInfo>() as isize {
            let _ = OVERFLOW_QUEUES[owner_index].push(&stable);
        }
    }
    *errno_location() = saved_errno;
}

/// Schedules `SIGALRM` after `seconds` and returns the prior alarm's remaining seconds.
#[no_mangle]
pub extern "C" fn elephc_pcntl_alarm(seconds: i64) -> i64 {
    i64::from(unsafe { libc::alarm(seconds as libc::c_uint) })
}

/// Returns one past the highest signal number accepted by the current target.
#[no_mangle]
pub extern "C" fn elephc_pcntl_signal_limit() -> libc::c_int {
    signal_limit()
}

/// Installs a default, ignored, or queued PCNTL signal disposition.
///
/// `disposition` uses the bridge-private values zero for `SIG_DFL`, one for `SIG_IGN`, and two
/// for the backend-specific queue consumed by `elephc_pcntl_signal_next()`. `owner` selects the
/// AOT or eval callable table. Returns one on success or zero after recording errno or `EINVAL`.
#[no_mangle]
pub extern "C" fn elephc_pcntl_signal(
    signal: libc::c_int,
    disposition: libc::c_int,
    restart_syscalls: libc::c_int,
    owner: libc::c_int,
) -> libc::c_int {
    if signal < 1
        || signal >= signal_limit()
        || !(0..=2).contains(&disposition)
        || signal_owner_index(owner).is_none()
    {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return 0;
    }
    if disposition == 2 && ensure_signal_pipe(owner).is_none() {
        return 0;
    }
    let mut action = unsafe { std::mem::zeroed::<libc::sigaction>() };
    action.sa_sigaction = match disposition {
        0 => libc::SIG_DFL,
        1 => libc::SIG_IGN,
        _ if owner == PCNTL_SIGNAL_OWNER_AOT => {
            queued_signal_handler_aot as *const () as libc::sighandler_t
        }
        _ => queued_signal_handler_eval as *const () as libc::sighandler_t,
    };
    if unsafe { libc::sigfillset(&mut action.sa_mask) } != 0 {
        record_errno();
        return 0;
    }
    action.sa_flags = if disposition == 2 { libc::SA_SIGINFO } else { 0 };
    if restart_syscalls != 0 {
        action.sa_flags |= libc::SA_RESTART;
    }
    if unsafe { libc::sigaction(signal, &action, std::ptr::null_mut()) } != 0 {
        record_errno();
        return 0;
    }
    1
}

/// Pops one queued signal record without blocking.
///
/// Returns one when a complete record was written, zero when the queue is empty, or `-1` after
/// recording an unexpected read error.
///
/// # Safety
/// `info` must point to writable storage for one `ElephcPcntlSigInfo` value.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_signal_next(
    info: *mut ElephcPcntlSigInfo,
    owner: libc::c_int,
) -> libc::c_int {
    if info.is_null() {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return -1;
    }
    let Some(owner_index) = signal_owner_index(owner) else {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return -1;
    };
    let Some((read_descriptor, _)) = ensure_signal_pipe(owner) else {
        return -1;
    };
    loop {
        let read = libc::read(
            read_descriptor,
            info.cast(),
            std::mem::size_of::<ElephcPcntlSigInfo>(),
        );
        if read == std::mem::size_of::<ElephcPcntlSigInfo>() as isize {
            return 1;
        }
        if read == -1 && current_errno() == libc::EINTR {
            continue;
        }
        if read == -1 && current_errno() == libc::EAGAIN {
            if let Some(overflow) = OVERFLOW_QUEUES[owner_index].pop() {
                *info = overflow;
                return 1;
            }
            return 0;
        }
        if read >= 0 {
            LAST_ERROR.store(libc::EIO, Ordering::Relaxed);
        } else {
            record_errno();
        }
        return -1;
    }
}

/// Injects one overflow record directly for deterministic bridge regression tests.
#[cfg(test)]
pub(crate) fn queue_signal_overflow_for_test(
    owner: libc::c_int,
    info: &ElephcPcntlSigInfo,
) -> bool {
    let owner_index = signal_owner_index(owner).expect("test owner must be valid");
    OVERFLOW_QUEUES[owner_index].push(info)
}

/// Returns the fixed per-backend overflow capacity for deterministic saturation tests.
#[cfg(test)]
pub(crate) const fn signal_overflow_capacity_for_test() -> usize {
    SIGNAL_OVERFLOW_CAPACITY
}

/// Blocks signal delivery while one dispatcher drains the queue snapshot.
///
/// Returns one after saving the prior thread mask, or zero after recording errno.
///
/// # Safety
/// `previous_mask` must point to writable `ElephcPcntlSignalMask` storage that remains valid until
/// the matching `elephc_pcntl_dispatch_end()` call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_dispatch_begin(
    previous_mask: *mut ElephcPcntlSignalMask,
) -> libc::c_int {
    if previous_mask.is_null() {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return 0;
    }
    debug_assert!(
        std::mem::size_of::<libc::sigset_t>() <= std::mem::size_of::<ElephcPcntlSignalMask>()
    );
    let mut full_mask = std::mem::zeroed::<libc::sigset_t>();
    if libc::sigfillset(&mut full_mask) != 0
        || libc::sigprocmask(
            libc::SIG_SETMASK,
            &full_mask,
            previous_mask.cast::<libc::sigset_t>(),
        ) != 0
    {
        record_errno();
        return 0;
    }
    1
}

/// Restores the thread mask saved by `elephc_pcntl_dispatch_begin()`.
///
/// Returns one on success, or zero after recording errno.
///
/// # Safety
/// `previous_mask` must point to storage initialized by a successful matching begin call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_dispatch_end(
    previous_mask: *const ElephcPcntlSignalMask,
) -> libc::c_int {
    if previous_mask.is_null() {
        LAST_ERROR.store(libc::EFAULT, Ordering::Relaxed);
        return 0;
    }
    if libc::sigprocmask(
        libc::SIG_SETMASK,
        previous_mask.cast::<libc::sigset_t>(),
        std::ptr::null_mut(),
    ) != 0
    {
        record_errno();
        return 0;
    }
    1
}

/// Changes the calling thread's signal mask and optionally returns its prior members.
///
/// A nonnegative return is the number of signals written to `old_signals`; `-1` records errno
/// or `EINVAL` and leaves the caller's PHP output untouched.
///
/// # Safety
/// `signals` must be readable for `count` values. When non-null, `old_signals` must be writable
/// for `old_capacity` values.
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigprocmask(
    how: libc::c_int,
    signals: *const i64,
    count: usize,
    old_signals: *mut i64,
    old_capacity: usize,
) -> i64 {
    if !matches!(how, libc::SIG_BLOCK | libc::SIG_UNBLOCK | libc::SIG_SETMASK) {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return -1;
    }
    let Some(set) = build_signal_set(signals, count, how == libc::SIG_SETMASK) else {
        return -1;
    };
    let mut old_set = std::mem::zeroed::<libc::sigset_t>();
    if libc::sigprocmask(how, &set, &mut old_set) != 0 {
        record_errno();
        return -1;
    }
    if old_signals.is_null() {
        return 0;
    }
    let mut old_count = 0usize;
    for signal in 1..signal_limit() {
        if libc::sigismember(&old_set, signal) != 1 {
            continue;
        }
        if old_count == old_capacity {
            LAST_ERROR.store(libc::EOVERFLOW, Ordering::Relaxed);
            return -1;
        }
        *old_signals.add(old_count) = i64::from(signal);
        old_count += 1;
    }
    old_count as i64
}

/// Waits synchronously for one Linux signal and writes its stable signal-information record.
///
/// Returns the delivered signal number or `-1` after recording errno.
///
/// # Safety
/// `signals` must be readable for `count` values and `info` must be null or writable for one
/// `ElephcPcntlSigInfo` value.
#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigwaitinfo(
    signals: *const i64,
    count: usize,
    info: *mut ElephcPcntlSigInfo,
) -> i64 {
    let Some(set) = build_signal_set(signals, count, false) else {
        return -1;
    };
    let mut native_info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    *errno_location() = 0;
    let mut signal = libc::sigwaitinfo(&set, native_info.as_mut_ptr());
    if signal == -1 {
        record_errno();
        return -1;
    }
    let native_info = native_info.assume_init();
    if signal == 0 && native_info.si_signo != 0 {
        signal = native_info.si_signo;
    }
    if !info.is_null() {
        *info = copy_signal_siginfo(signal, &native_info);
    }
    i64::from(signal)
}

/// Waits up to the supplied Linux timeout for one selected signal.
///
/// Returns the delivered signal number or `-1`. A timeout intentionally leaves the previous
/// PCNTL last-error value unchanged, matching PHP's `EAGAIN` behavior.
///
/// # Safety
/// `signals` must be readable for `count` values and `info` must be null or writable for one
/// `ElephcPcntlSigInfo` value.
#[cfg(target_os = "linux")]
#[no_mangle]
pub unsafe extern "C" fn elephc_pcntl_sigtimedwait(
    signals: *const i64,
    count: usize,
    info: *mut ElephcPcntlSigInfo,
    seconds: i64,
    nanoseconds: i64,
) -> i64 {
    let Some(set) = build_signal_set(signals, count, false) else {
        return -1;
    };
    if seconds < 0 || !(0..1_000_000_000).contains(&nanoseconds) || (seconds == 0 && nanoseconds == 0)
    {
        LAST_ERROR.store(libc::EINVAL, Ordering::Relaxed);
        return -1;
    }
    let timeout = libc::timespec {
        tv_sec: seconds as _,
        tv_nsec: nanoseconds as libc::c_long,
    };
    *errno_location() = 0;
    let mut native_info = std::mem::MaybeUninit::<libc::siginfo_t>::zeroed();
    let mut signal = libc::sigtimedwait(&set, native_info.as_mut_ptr(), &timeout);
    if signal == -1 {
        if *errno_location() != libc::EAGAIN {
            record_errno();
        }
        return -1;
    }
    let native_info = native_info.assume_init();
    if signal == 0 && native_info.si_signo != 0 {
        signal = native_info.si_signo;
    }
    if !info.is_null() {
        *info = copy_signal_siginfo(signal, &native_info);
    }
    i64::from(signal)
}
