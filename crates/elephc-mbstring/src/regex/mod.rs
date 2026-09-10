//! Purpose:
//! Owns shared mbregex compilation, matching, capture extraction, and provider lifetimes.
//!
//! Called from:
//! - Mbregex request adapters and native-provider compatibility tests.
//!
//! Key details:
//! - The native provider handles Oniguruma only; PHP settings and byte validation stay in Rust.
//! - Compiled patterns are thread-confined and results own copied metadata without native borrows.

mod settings;
mod request;
mod subject;
pub use settings::{Limits, Options, RegexEncoding};
pub use request::{CallbackReplacement, Event, Match, RegisterKey, Registers, Replacement, ReplacementError, ReplaceResult, Session};
pub use subject::Subject;

use std::{ffi::c_void, marker::PhantomData, ops::Range, ptr::NonNull, rc::Rc, sync::{Mutex, OnceLock}};
use elephc_builtin_contract::mbstring_abi::regex::*;

static PROVIDER: OnceLock<MbRegexProviderV1> = OnceLock::new();
static REGISTER: Mutex<()> = Mutex::new(());

/// Reports whether the complete shared native provider was successfully initialized.
pub fn provider_available() -> bool { PROVIDER.get().is_some() }

/// Distinguishes absent/broken native integration from PHP regex diagnostics.
#[derive(Debug, PartialEq, Eq)]
pub enum RegexError { Unavailable, Provider, Pattern(Vec<u8>), Search(Vec<u8>) }

/// Installs a complete, process-lifetime native provider and initializes Oniguruma once.
///
/// # Safety
/// Every callback must obey the versioned contract, remain mapped for process lifetime,
/// and use matched allocation/free functions without entering PHP or unwinding.
pub unsafe fn install_provider(provider: MbRegexProviderV1) -> bool {
    if !provider.is_complete() { return false; }
    let Ok(_lock) = REGISTER.lock() else { return false; };
    if let Some(current) = PROVIDER.get() {
        macro_rules! same { ($field:ident) => { std::ptr::fn_addr_eq(current.$field.unwrap(), provider.$field.unwrap()) }; }
        return same!(initialize) && same!(compile) && same!(free_regex) && same!(search)
            && same!(free_region) && same!(region_count) && same!(region_get)
            && same!(name_count) && same!(name_get) && same!(name_lookup)
            && same!(numbered_backrefs) && same!(error) && same!(compiled_options);
    }
    if unsafe { provider.initialize.unwrap()() } != 0 { return false; }
    PROVIDER.set(provider).is_ok()
}

/// One native compiled pattern, independent of the current PHP argument-validation alias.
pub struct Regex {
    handle: NonNull<c_void>,
    provider: &'static MbRegexProviderV1,
    pattern_length: usize,
    _thread: PhantomData<Rc<()>>,
}

/// Copied capture metadata keeps unmatched and empty matches distinct for each PHP consumer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captures {
    pub groups: Vec<Option<Range<usize>>>,
    pub names: Vec<(Vec<u8>, usize)>,
}

/// Releases a native region on every success, error, and failed metadata validation path.
struct Region { handle: *mut c_void, provider: &'static MbRegexProviderV1 }

impl Drop for Region {
    /// Retires this operation's region without invalidating the separately owned compiled pattern.
    fn drop(&mut self) { unsafe { self.provider.free_region.unwrap()(self.handle); } }
}

impl Drop for Regex {
    /// Frees the compiled pattern and its borrowed name metadata with its original provider.
    fn drop(&mut self) { unsafe { self.provider.free_regex.unwrap()(self.handle.as_ptr()); } }
}

impl Regex {
    /// Validates pattern bytes using PHP's selected alias, then compiles with explicit options.
    pub fn compile(pattern: &[u8], encoding: RegexEncoding, options: Options) -> Result<Self, RegexError> {
        if !encoding.is_valid(pattern) {
            return Err(RegexError::Pattern(format!("Pattern is not valid under {} encoding", encoding.name()).into_bytes()));
        }
        let provider = PROVIDER.get().ok_or(RegexError::Unavailable)?;
        let request = MbRegexCompileV1 { pattern: pattern.as_ptr(), length: pattern.len() as u64,
            encoding: encoding.id, options: options.bits, syntax: options.syntax, reserved: 0 };
        let mut handle = std::ptr::null_mut();
        let mut error = [0u8; 256];
        let status = unsafe { provider.compile.unwrap()(&request, &mut handle, error.as_mut_ptr(), error.len() as u64) };
        if status != 0 {
            if !handle.is_null() { unsafe { provider.free_regex.unwrap()(handle); } return Err(RegexError::Provider); }
            if status == -100000 || status > 0 { return Err(RegexError::Provider); }
            let error = if error[0] == 0 { native_error(provider, status)? }
                else { error.split(|&byte| byte == 0).next().unwrap().to_vec() };
            let mut message = b"mbregex compile err: ".to_vec();
            message.extend(error);
            return Err(RegexError::Pattern(message));
        }
        Ok(Self { handle: NonNull::new(handle).ok_or(RegexError::Provider)?, provider,
            pattern_length: pattern.len(), _thread: PhantomData })
    }

    /// Searches at a byte offset after the PHP operation's own validation, returning copied capture bounds.
    /// Callers validate using the live alias when required; progressive searches retain their initialized subject.
    pub fn search(&self, subject: &[u8], offset: usize, anchored: bool, limits: Limits) -> Result<Option<Captures>, RegexError> {
        self.search_subject(&Subject::new(subject), offset, anchored, limits)
    }

    /// Searches a retained padded subject without copying it again during progressive or replacement loops.
    pub fn search_subject(&self, subject: &Subject, offset: usize, anchored: bool, limits: Limits) -> Result<Option<Captures>, RegexError> {
        if offset > subject.len() { return Err(RegexError::Provider); }
        let request = MbRegexSearchV1 { subject: subject.native_ptr(), length: subject.len() as u64,
            offset: offset as u64, anchored: u32::from(anchored),
            flags: SUBJECT_PADDED | u32::from(limits.stack.is_some()) | (u32::from(limits.retry.is_some()) << 1),
            stack_limit: u64::from(limits.stack.unwrap_or(0)), retry_limit: u64::from(limits.retry.unwrap_or(0)) };
        let mut region = Region { handle: std::ptr::null_mut(), provider: self.provider };
        let status = unsafe { self.provider.search.unwrap()(self.handle.as_ptr(), &request, &mut region.handle) };
        if status < 0 {
            if !region.handle.is_null() || status == -100000 { return Err(RegexError::Provider); }
            if status == -1 { return Ok(None); }
            let code = i32::try_from(status).map_err(|_| RegexError::Provider)?;
            return Err(RegexError::Search(native_error(self.provider, code)?));
        }
        if region.handle.is_null() || status as u64 > subject.len() as u64 { return Err(RegexError::Provider); }
        self.captures(&region, subject.len()).map(Some)
    }

    /// Reports PHP's permission to expand numbered replacement backreferences in this pattern.
    pub fn numbered_backrefs(&self) -> Result<bool, RegexError> {
        match unsafe { self.provider.numbered_backrefs.unwrap()(self.handle.as_ptr()) } {
            0 => Ok(false), 1 => Ok(true), _ => Err(RegexError::Provider),
        }
    }

    /// Reads the actual native option mask, including defaults contributed by the chosen syntax.
    pub fn compiled_options(&self) -> Result<u32, RegexError> {
        let options = unsafe { self.provider.compiled_options.unwrap()(self.handle.as_ptr()) };
        if options == u32::MAX { Err(RegexError::Provider) } else { Ok(options) }
    }

    /// Copies bounded region and name metadata before the native region owner is released.
    fn captures(&self, region: &Region, length: usize) -> Result<Captures, RegexError> {
        let count = unsafe { self.provider.region_count.unwrap()(region.handle) };
        if count == 0 || count > self.pattern_length as u64 + 1 { return Err(RegexError::Provider); }
        let mut groups = Vec::with_capacity(count as usize);
        for index in 0..count {
            let (mut begin, mut end) = (0i64, 0i64);
            let status = unsafe { self.provider.region_get.unwrap()(region.handle, index, &mut begin, &mut end) };
            if status != 0 { return Err(RegexError::Provider); }
            if begin == -1 && end == -1 { groups.push(None); }
            else if begin >= 0 && begin <= end && end as u64 <= length as u64 { groups.push(Some(begin as usize..end as usize)); }
            else { return Err(RegexError::Provider); }
        }
        let name_count = unsafe { self.provider.name_count.unwrap()(self.handle.as_ptr()) };
        if name_count > count { return Err(RegexError::Provider); }
        let mut names = Vec::with_capacity(name_count as usize);
        for index in 0..name_count {
            let mut name = MbRegexNameV1 { bytes: std::ptr::null(), length: 0, group: -1 };
            let status = unsafe { self.provider.name_get.unwrap()(self.handle.as_ptr(), region.handle, index, &mut name) };
            if status != 0 || name.bytes.is_null() || name.length > self.pattern_length as u64
                || name.group < 0 || name.group as u64 >= count { return Err(RegexError::Provider); }
            let bytes = unsafe { std::slice::from_raw_parts(name.bytes, name.length as usize) }.to_vec();
            names.push((bytes, name.group as usize));
        }
        Ok(Captures { groups, names })
    }
}

/// Copies a native error while retaining a distinct failure for malformed provider output.
fn native_error(provider: &MbRegexProviderV1, code: i32) -> Result<Vec<u8>, RegexError> {
    let mut error = [0u8; 256];
    let length = unsafe { provider.error.unwrap()(code, error.as_mut_ptr(), error.len() as u64) };
    if length < 0 || length as usize >= error.len() || error[length as usize] != 0 { return Err(RegexError::Provider); }
    Ok(error[..length as usize].to_vec())
}
