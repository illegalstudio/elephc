//! Purpose:
//! Declares libc routines used by eval builtins that mirror PHP process and network helpers.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env`
//! - `crate::interpreter::builtins::filesystem`
//!
//! Key details:
//! - These bindings are unsafe FFI declarations only; call sites own pointer validation and
//!   PHP-compatible fallback behavior.

unsafe extern "C" {
    /// Reverse-resolves one socket address through libc's `gethostbyaddr`.
    #[link_name = "gethostbyaddr"]
    pub(super) fn libc_gethostbyaddr(
        addr: *const libc::c_void,
        len: libc::socklen_t,
        type_: libc::c_int,
    ) -> *mut libc::hostent;

    /// Looks up one IP protocol entry by protocol name or alias.
    #[link_name = "getprotobyname"]
    pub(super) fn libc_getprotobyname(name: *const libc::c_char) -> *mut libc::protoent;

    /// Looks up one IP protocol entry by protocol number.
    #[link_name = "getprotobynumber"]
    pub(super) fn libc_getprotobynumber(proto: libc::c_int) -> *mut libc::protoent;

    /// Looks up one internet service entry by service name and protocol.
    #[link_name = "getservbyname"]
    pub(super) fn libc_getservbyname(
        name: *const libc::c_char,
        proto: *const libc::c_char,
    ) -> *mut libc::servent;

    /// Looks up one internet service entry by port and protocol.
    #[link_name = "getservbyport"]
    pub(super) fn libc_getservbyport(
        port: libc::c_int,
        proto: *const libc::c_char,
    ) -> *mut libc::servent;

    /// Sets the process file-creation mask and returns the previous mask.
    pub(super) fn umask(mask: u32) -> u32;
}
