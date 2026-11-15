//! Purpose:
//! Writes a bounded postmortem document when a synchronous native signal
//! (SIGSEGV/SIGBUS/SIGILL/SIGFPE/SIGABRT) ends a `--debug` process configured
//! with `ELEPHC_DEBUG_POSTMORTEM`.
//!
//! Called from:
//! - `crate::elephc_debug_boot` after the embedded metadata and debugger state
//!   are ready.
//! - The process signal handlers installed by `install`.
//!
//! Key details:
//! - The signal handler performs only async-signal-safe work: it reads immutable
//!   prebuilt tables, walks the thread-local activation stack with `try_borrow`,
//!   formats JSON in a static buffer without allocating, and uses `open`/`write`/
//!   `close` followed by a default-handler re-raise.
//! - If the activation stack is currently borrowed, the document keeps an empty
//!   stack instead of risking a deadlock. Call events are not included: their
//!   ring requires a lock that the crashing thread may already hold.
//! - The boot thread installs an alternate signal stack, so a stack-overflow
//!   fault can still run the handler; other threads fall back to their normal
//!   stack. The normal uncaught-exception postmortem keeps its existing path and
//!   takes precedence when it already wrote the file.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;

use super::{display_function_name, Debugger};

/// Fixed path storage: the signal handler cannot allocate a `CString`.
const PATH_CAPACITY: usize = 4096;
/// Fixed document storage. The stack is capped so this is never exceeded in
/// practice; extra frames are omitted rather than growing the buffer.
const BUFFER_CAPACITY: usize = 64 * 1024;
/// Maximum PHP frames written to one crash document.
const MAX_FRAMES: usize = 256;

/// NUL-terminated postmortem path copied at boot.
static mut CRASH_PATH: [u8; PATH_CAPACITY] = [0; PATH_CAPACITY];
/// Path length excluding the trailing NUL; zero disables the handler.
static CRASH_PATH_LEN: AtomicUsize = AtomicUsize::new(0);
/// Signal-handler scratch buffer, reused by the one crash that ends the process.
static mut CRASH_BUFFER: [u8; BUFFER_CAPACITY] = [0; BUFFER_CAPACITY];
/// Alternate stack for the boot thread, so a stack-overflow fault can still run
/// the handler. Other threads fall back to their normal stack (or skip the doc).
static mut ALT_STACK: AlignedStack = AlignedStack([0; ALT_STACK_CAPACITY]);
/// Alternate stack size: large enough for `open`/`write` and the fixed buffer
/// bookkeeping, small enough to live in BSS.
const ALT_STACK_CAPACITY: usize = 64 * 1024;
/// Prevents a nested signal from overwriting the first crash document.
static CRASH_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Ensures handlers are installed once per process.
static HANDLERS_INSTALLED: AtomicBool = AtomicBool::new(false);

/// 16-byte-aligned signal stack storage, as the ABI expects for `ss_sp`.
#[repr(align(16))]
struct AlignedStack([u8; ALT_STACK_CAPACITY]);

/// Immutable lookup tables copied from the debugger metadata at boot.
struct CrashTables {
    /// Deduplicated names and file paths referenced by the tables below.
    strings: Vec<Box<str>>,
    /// Function id → name/file/start-line, with a default entry for unknown ids.
    functions: Vec<CrashFunction>,
    /// Source site id → file/line. Branch and path counters stay `None`.
    sites: Vec<Option<CrashSite>>,
}

/// One function as the signal handler needs it.
#[derive(Clone, Copy)]
struct CrashFunction {
    /// Index into `CrashTables::strings`.
    name: usize,
    /// Index into `CrashTables::strings`.
    file: usize,
    /// Function's first source line, when metadata has one.
    line: Option<u64>,
}

/// One source site as the signal handler needs it.
#[derive(Clone, Copy)]
struct CrashSite {
    /// Index into `CrashTables::strings`.
    file: usize,
    line: u64,
}

/// Lookup tables installed once the embedded metadata has been parsed.
static TABLES: OnceLock<CrashTables> = OnceLock::new();

/// Copies the configured postmortem path and enables the signal handlers.
pub(crate) fn install(path: &str, debugger: &Debugger) {
    arm_path(path);
    let _ = TABLES.set(CrashTables::build(debugger));
    install_handlers();
}

/// Copies `path` into the static NUL-terminated buffer used by the handler.
fn arm_path(path: &str) {
    let bytes = path.as_bytes();
    if bytes.len() >= PATH_CAPACITY || bytes.contains(&0) {
        CRASH_PATH_LEN.store(0, Ordering::Release);
        return;
    }
    unsafe {
        let destination = std::ptr::addr_of_mut!(CRASH_PATH) as *mut u8;
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
        *destination.add(bytes.len()) = 0;
    }
    CRASH_PATH_LEN.store(bytes.len(), Ordering::Release);
}

impl CrashTables {
    /// Builds the immutable tables from the debugger's parsed metadata.
    fn build(debugger: &Debugger) -> Self {
        let mut strings: Vec<Box<str>> = vec![Box::from("<unknown>"), Box::from("")];
        let mut string_ids: HashMap<String, usize> = HashMap::new();
        string_ids.insert("<unknown>".to_string(), 0);
        string_ids.insert(String::new(), 1);
        let mut source_ids: HashMap<u64, usize> = HashMap::new();

        let function_count = debugger
            .functions
            .keys()
            .max()
            .map(|id| *id as usize + 1)
            .unwrap_or(1);
        let default_function = CrashFunction {
            name: 0,
            file: 1,
            line: None,
        };
        let mut functions = vec![default_function; function_count.max(1)];
        for (id, function) in &debugger.functions {
            let Some(slot) = functions.get_mut(*id as usize) else {
                continue;
            };
            *slot = CrashFunction {
                name: intern_string(
                    &mut strings,
                    &mut string_ids,
                    display_function_name(&function.name),
                ),
                file: function
                    .source_id
                    .map(|source_id| intern_source(
                        &mut strings,
                        &mut string_ids,
                        &mut source_ids,
                        debugger,
                        source_id,
                    ))
                    .unwrap_or(1),
                line: function.line_start,
            };
        }

        let site_count = debugger
            .sites
            .keys()
            .max()
            .map(|id| *id as usize + 1)
            .unwrap_or(0);
        let mut sites = vec![None; site_count];
        for (id, site) in &debugger.sites {
            let Some(slot) = sites.get_mut(*id as usize) else {
                continue;
            };
            *slot = Some(CrashSite {
                file: intern_source(
                    &mut strings,
                    &mut string_ids,
                    &mut source_ids,
                    debugger,
                    site.source_id,
                ),
                line: site.line,
            });
        }
        CrashTables {
            strings,
            functions,
            sites,
        }
    }

    /// Returns the string at `index`, or the empty string.
    fn string(&self, index: usize) -> &str {
        self.strings.get(index).map(Box::as_ref).unwrap_or("")
    }
}

/// Interns a name/path in the crash table's string pool.
fn intern_string(
    strings: &mut Vec<Box<str>>,
    string_ids: &mut HashMap<String, usize>,
    value: &str,
) -> usize {
    if let Some(index) = string_ids.get(value) {
        return *index;
    }
    let index = strings.len();
    strings.push(Box::from(value));
    string_ids.insert(value.to_string(), index);
    index
}

/// Interns one metadata source path, reusing aliases of the same source id.
fn intern_source(
    strings: &mut Vec<Box<str>>,
    string_ids: &mut HashMap<String, usize>,
    source_ids: &mut HashMap<u64, usize>,
    debugger: &Debugger,
    source_id: u64,
) -> usize {
    if let Some(index) = source_ids.get(&source_id) {
        return *index;
    }
    let index = debugger
        .sources
        .get(&source_id)
        .map(|path| intern_string(strings, string_ids, path))
        .unwrap_or(1);
    source_ids.insert(source_id, index);
    index
}

/// Installs handlers for the synchronous signals a native crash can raise.
fn install_handlers() {
    if HANDLERS_INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = crash_signal_handler as *const () as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_NODEFER | libc::SA_ONSTACK;
        libc::sigemptyset(&mut action.sa_mask);
        let alternate = libc::stack_t {
            ss_sp: std::ptr::addr_of_mut!(ALT_STACK.0) as *mut libc::c_void,
            ss_flags: 0,
            ss_size: ALT_STACK_CAPACITY,
        };
        libc::sigaltstack(&alternate, std::ptr::null_mut());
        for signal in [
            libc::SIGSEGV,
            libc::SIGBUS,
            libc::SIGILL,
            libc::SIGFPE,
            libc::SIGABRT,
        ] {
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
}

/// Returns the faulting address from a signal's `siginfo_t`.
#[cfg(target_os = "linux")]
fn fault_address(info: *mut libc::siginfo_t) -> usize {
    if info.is_null() {
        return 0;
    }
    unsafe { (*info).si_addr() as usize }
}

/// Returns the faulting address from a signal's `siginfo_t`.
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn fault_address(info: *mut libc::siginfo_t) -> usize {
    if info.is_null() {
        return 0;
    }
    unsafe { (*info).si_addr as usize }
}

/// Writes the crash document and re-raises `signal` with its default action.
extern "C" fn crash_signal_handler(
    signal: libc::c_int,
    info: *mut libc::siginfo_t,
    _context: *mut libc::c_void,
) {
    let address = fault_address(info);
    if !CRASH_ACTIVE.swap(true, Ordering::AcqRel) {
        write_crash_document(signal, address);
    }
    unsafe {
        // A signal is blocked while its own handler runs. Restore the default
        // action and unblock it before re-raising so the process still reports
        // the original signal to its parent instead of a normal exit code.
        libc::signal(signal, libc::SIG_DFL);
        let mut unblock: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut unblock);
        libc::sigaddset(&mut unblock, signal);
        libc::sigprocmask(libc::SIG_UNBLOCK, &unblock, std::ptr::null_mut());
        libc::raise(signal);
        libc::kill(libc::getpid(), signal);
    }
    unsafe { libc::_exit(128 + signal) }
}

/// Formats and writes one crash document with signal-handler-safe operations.
fn write_crash_document(signal: libc::c_int, address: usize) {
    if CRASH_PATH_LEN.load(Ordering::Acquire) == 0 {
        return;
    }
    if super::POSTMORTEM_FLUSHED.swap(true, Ordering::AcqRel) {
        return;
    }
    unsafe {
        let path = std::ptr::addr_of!(CRASH_PATH) as *const libc::c_char;
        let fd = libc::open(
            path,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o600,
        );
        if fd < 0 {
            return;
        }
        let pointer = std::ptr::addr_of_mut!(CRASH_BUFFER) as *mut u8;
        let mut buffer = CrashBuffer::new(pointer, BUFFER_CAPACITY);
        render_document(&mut buffer, signal, address, true);
        if buffer.overflow {
            buffer.reset();
            render_document(&mut buffer, signal, address, false);
        }
        write_all(fd, buffer.pointer, buffer.len);
        libc::close(fd);
    }
}

/// Builds the compact JSON document in `buffer`.
fn render_document(
    buffer: &mut CrashBuffer,
    signal: libc::c_int,
    address: usize,
    include_stack: bool,
) {
    buffer.push_str(
        "{\"format\":\"elephc-postmortem\",\"version\":1,\"reason\":\"native_crash\",\"signal\":",
    );
    buffer.push_i64(signal as i64);
    buffer.push_str(",\"address\":\"");
    buffer.push_hex(address);
    buffer.push_str("\",\"stack\":[");
    let Some(tables) = TABLES.get().filter(|_| include_stack) else {
        buffer.push_str("],\"calls\":[],\"captured\":0,\"dropped\":0}");
        return;
    };
    let mut frames = 0usize;
    super::ACTIVATIONS.with(|stack| {
        let Ok(stack) = stack.try_borrow() else {
            return;
        };
        for frame in stack.iter().rev() {
            if frames >= MAX_FRAMES {
                break;
            }
            let function = tables
                .functions
                .get(frame.function_id as usize)
                .copied()
                .unwrap_or(CrashFunction {
                    name: 0,
                    file: 1,
                    line: None,
                });
            let site = frame
                .last_site
                .and_then(|id| tables.sites.get(id as usize).and_then(|site| *site));
            let (file, line) = site
                .map(|site| (site.file, site.line))
                .unwrap_or((function.file, function.line.unwrap_or(0)));
            if frames > 0 {
                buffer.push(&[b',']);
            }
            frames += 1;
            buffer.push_str("{\"function\":");
            buffer.push_json_string(tables.string(function.name).as_bytes());
            buffer.push_str(",\"file\":");
            buffer.push_json_string(tables.string(file).as_bytes());
            buffer.push_str(",\"line\":");
            buffer.push_u64(line);
            buffer.push(&[b'}']);
        }
    });
    buffer.push_str("],\"calls\":[],\"captured\":0,\"dropped\":0}");
}

/// Writes every byte to `fd`, tolerating short writes and EINTR.
unsafe fn write_all(fd: libc::c_int, pointer: *const u8, length: usize) {
    let mut offset = 0usize;
    while offset < length {
        let written = unsafe {
            libc::write(
                fd,
                pointer.add(offset) as *const libc::c_void,
                length - offset,
            )
        };
        if written > 0 {
            offset += written as usize;
        } else if written < 0 && unsafe { crash_errno() } == libc::EINTR {
            continue;
        } else {
            break;
        }
    }
}

/// Reads the current thread's errno without constructing a `std::io::Error`.
#[cfg(target_os = "linux")]
unsafe fn crash_errno() -> libc::c_int {
    unsafe { *libc::__errno_location() }
}

/// Reads the current thread's errno without constructing a `std::io::Error`.
#[cfg(any(target_os = "macos", target_os = "ios"))]
unsafe fn crash_errno() -> libc::c_int {
    unsafe { *libc::__error() }
}

/// Allocation-free JSON writer over the static signal-handler buffer.
struct CrashBuffer {
    /// Start of the writable buffer.
    pointer: *mut u8,
    /// Bytes written so far.
    len: usize,
    /// Total writable capacity.
    capacity: usize,
    /// Whether an append was dropped for lack of capacity.
    overflow: bool,
}

impl CrashBuffer {
    /// Creates an empty writer over caller-owned storage.
    fn new(pointer: *mut u8, capacity: usize) -> Self {
        Self {
            pointer,
            len: 0,
            capacity,
            overflow: false,
        }
    }

    /// Discards everything written so a smaller fallback document can replace it.
    fn reset(&mut self) {
        self.len = 0;
        self.overflow = false;
    }

    /// Appends raw bytes, silently dropping anything past capacity.
    fn push(&mut self, bytes: &[u8]) {
        if self.len + bytes.len() > self.capacity {
            self.overflow = true;
            return;
        }
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.pointer.add(self.len),
                bytes.len(),
            );
        }
        self.len += bytes.len();
    }

    /// Appends one byte.
    fn push_byte(&mut self, byte: u8) {
        self.push(&[byte]);
    }

    /// Appends a UTF-8 string.
    fn push_str(&mut self, value: &str) {
        self.push(value.as_bytes());
    }

    /// Appends an unsigned decimal integer without allocating.
    fn push_u64(&mut self, mut value: u64) {
        let mut digits = [0u8; 20];
        let mut index = digits.len();
        if value == 0 {
            self.push_byte(b'0');
            return;
        }
        while value > 0 {
            index -= 1;
            digits[index] = b'0' + (value % 10) as u8;
            value /= 10;
        }
        self.push(&digits[index..]);
    }

    /// Appends a signed decimal integer without allocating.
    fn push_i64(&mut self, value: i64) {
        if value < 0 {
            self.push_byte(b'-');
            self.push_u64(value.unsigned_abs());
        } else {
            self.push_u64(value as u64);
        }
    }

    /// Appends a lowercase hexadecimal address with a `0x` prefix.
    fn push_hex(&mut self, value: usize) {
        self.push_str("0x");
        if value == 0 {
            self.push_byte(b'0');
            return;
        }
        let mut digits = [0u8; 16];
        let mut index = digits.len();
        let mut value = value as u64;
        while value > 0 {
            index -= 1;
            let digit = (value & 0xf) as u8;
            digits[index] = if digit < 10 {
                b'0' + digit
            } else {
                b'a' + digit - 10
            };
            value >>= 4;
        }
        self.push(&digits[index..]);
    }

    /// Appends one JSON string with quotes and mandatory escapes.
    fn push_json_string(&mut self, bytes: &[u8]) {
        self.push_byte(b'"');
        for &byte in bytes {
            match byte {
                b'"' => self.push_str("\\\""),
                b'\\' => self.push_str("\\\\"),
                b'\n' => self.push_str("\\n"),
                b'\r' => self.push_str("\\r"),
                b'\t' => self.push_str("\\t"),
                0x00..=0x1f => {
                    let hex = b"0123456789abcdef";
                    self.push_str("\\u00");
                    self.push_byte(hex[(byte >> 4) as usize]);
                    self.push_byte(hex[(byte & 0xf) as usize]);
                }
                _ => self.push_byte(byte),
            }
        }
        self.push_byte(b'"');
    }
}

#[cfg(test)]
mod tests {
    use super::CrashBuffer;

    /// Verifies the allocation-free writer produces valid JSON escaping and
    /// decimal/hex spellings without crossing its capacity.
    #[test]
    fn crash_buffer_formats_json_and_digits() {
        let mut storage = [0u8; 128];
        let mut buffer = CrashBuffer::new(storage.as_mut_ptr(), storage.len());
        buffer.push_json_string(b"a\"b\\c\n\x01");
        buffer.push_str(",");
        buffer.push_i64(-42);
        buffer.push_str(",\"");
        buffer.push_hex(0x1f);
        buffer.push_str("\"");
        let text = std::str::from_utf8(&storage[..buffer.len]).expect("utf-8 JSON");
        assert_eq!(text, "\"a\\\"b\\\\c\\n\\u0001\",-42,\"0x1f\"");
        let parsed: serde_json::Value =
            serde_json::from_str(&format!("[{text}]")).expect("valid JSON");
        assert_eq!(parsed[0], "a\"b\\c\n\u{1}");
        assert_eq!(parsed[1], -42);
        assert_eq!(parsed[2], "0x1f");
    }

    /// Verifies capacity overflow drops the tail instead of writing past the
    /// static buffer.
    #[test]
    fn crash_buffer_never_overruns_capacity() {
        let mut storage = [0u8; 4];
        let mut buffer = CrashBuffer::new(storage.as_mut_ptr(), storage.len());
        buffer.push_str("abcdef");
        assert_eq!(buffer.len, 0);
        assert!(buffer.overflow);
        buffer.reset();
        buffer.push_str("abc");
        buffer.push_str("def");
        assert_eq!(&storage[..3], b"abc");
    }
}
