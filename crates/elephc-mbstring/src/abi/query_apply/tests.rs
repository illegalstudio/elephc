//! Purpose:
//! Verifies query executor ordering, wire rejection, and every published cursor owner.
//!
//! Called from:
//! - Focused mbstring bridge unit tests through the versioned C entry.
//!
//! Key details:
//! - Opaque integer handles model pins independently of native heap representation.
//! - Callback status and acquired ownership are independently configurable.
//! - Root retargeting during cursor cleanup must be visible before terminal root removal.

use super::*;
use std::collections::{BTreeMap, VecDeque};

/// Independent callback state with observable pin counts and mutation order.
struct Host {
    root: usize, child: usize, index: i64, available: u64, empty_child: bool,
    pins: BTreeMap<usize, usize>, events: Vec<String>, bad_releases: usize,
    statuses: BTreeMap<&'static str, VecDeque<i32>>, retarget: BTreeMap<usize, usize>,
}

impl Host {
    /// Starts with a live root, distinct child handles, and one available negative append index.
    fn new() -> Self {
        Self { root: 10, child: 20, index: -3, available: 1, empty_child: false,
            pins: BTreeMap::new(), events: Vec::new(), bad_releases: 0,
            statuses: BTreeMap::new(), retarget: BTreeMap::new() }
    }

    /// Supplies one independent status sequence for a protected storage operation.
    fn statuses(&mut self, name: &'static str, values: &[i32]) {
        self.statuses.insert(name, values.iter().copied().collect());
    }

    /// Consumes one configured status without changing ownership observations.
    fn status(&mut self, name: &'static str) -> i32 {
        self.statuses.get_mut(name).and_then(VecDeque::pop_front).unwrap_or(0)
    }

    /// Acquires one modeled cursor root without introducing any PHP value copies.
    fn pin(&mut self, value: usize) { if value != 0 { *self.pins.entry(value).or_default() += 1; } }

    /// Requires exact retirement of all published pins, including failed callback outputs.
    fn clean(&self) { assert!(self.pins.is_empty(), "{:?}", self.pins); assert_eq!(self.bad_releases, 0); }
}

/// Borrows the test context only for the duration of one protected callback.
unsafe fn host<'a>(context: *mut c_void) -> &'a mut Host { unsafe { &mut *context.cast::<Host>() } }

/// Publishes a pin on the writer's current root independently of the configured status.
unsafe extern "C" fn root(context: *mut c_void, _: *mut c_void, out: *mut *mut c_void) -> i32 {
    let host = unsafe { host(context) };
    host.events.push(format!("root:{}", host.root));
    host.pin(host.root);
    unsafe { out.write(host.root as *mut c_void); }
    host.status("root")
}

/// Reports an independently configured append decision without reserving an index.
unsafe extern "C" fn next(context: *mut c_void, cursor: *mut c_void, out: *mut MbQueryIndexV1) -> i32 {
    let host = unsafe { host(context) };
    host.events.push(format!("next:{}", cursor as usize));
    unsafe { out.write(MbQueryIndexV1 { available: host.available, index: host.index }); }
    host.status("next")
}

/// Publishes a distinct child pin, including when the callback reports fatal or pending.
unsafe extern "C" fn enter(context: *mut c_void, cursor: *mut c_void, key: *const MbHostValueV1, out: *mut *mut c_void) -> i32 {
    let host = unsafe { host(context) };
    host.events.push(format!("enter:{}:{}", cursor as usize, unsafe { key_text(&*key) }));
    let child = if host.empty_child { 0 } else { host.child };
    host.child += 1;
    host.pin(child);
    unsafe { out.write(child as *mut c_void); }
    host.status("enter")
}

/// Observes the exact borrowed key/value bytes after all preceding cursor transitions.
unsafe extern "C" fn store(context: *mut c_void, cursor: *mut c_void, key: *const MbHostValueV1, value: *const MbHostValueV1) -> i32 {
    let host = unsafe { host(context) };
    host.events.push(format!("store:{}:{}:{}", cursor as usize, unsafe { key_text(&*key) }, unsafe { key_text(&*value) }));
    host.status("store")
}

/// Records which live root received the terminal deletion.
unsafe extern "C" fn remove(context: *mut c_void, cursor: *mut c_void, key: *const MbHostValueV1) -> i32 {
    let host = unsafe { host(context) };
    host.events.push(format!("remove:{}:{}", cursor as usize, unsafe { key_text(&*key) }));
    host.status("remove")
}

/// Retires an acquired pin and optionally changes the root while cleanup runs.
unsafe extern "C" fn release(context: *mut c_void, cursor: *mut c_void) -> i32 {
    let host = unsafe { host(context) };
    let cursor = cursor as usize;
    host.events.push(format!("release:{cursor}"));
    if let Some(count) = host.pins.get_mut(&cursor) {
        *count -= 1;
        if *count == 0 { host.pins.remove(&cursor); }
    } else { host.bad_releases += 1; }
    if let Some(root) = host.retarget.get(&cursor) { host.root = *root; }
    host.status("release")
}

/// Encodes normalized integer identity and binary bytes without relying on PHP name parsing.
unsafe fn key_text(key: &MbHostValueV1) -> String {
    if key.tag == HOST_INT { return format!("i{}", key.lo as i64); }
    let bytes = if key.hi == 0 { &[] } else { unsafe { std::slice::from_raw_parts(key.lo as *const u8, key.hi as usize) } };
    format!("s{:?}", bytes)
}

/// Supplies the complete independent storage callback inventory.
fn storage() -> MbQueryStorageV1 {
    MbQueryStorageV1 { abi_version: 1, struct_size: size_of::<MbQueryStorageV1>() as u32,
        root: Some(root), next: Some(next), enter: Some(enter), store: Some(store),
        remove: Some(remove), release: Some(release) }
}

/// Constructs a normalized integer instruction or an append that ignores its placeholder key.
fn step(operation: u64, index: Option<i64>) -> MbQueryStepV1 {
    MbQueryStepV1 { operation, append: u64::from(index.is_none()),
        key: MbHostValueV1 { tag: HOST_INT, lo: index.unwrap_or(0) as u64, hi: 0 } }
}

/// Calls the actual C entry with binary value bytes and inspects separately published nesting metadata.
fn apply(host: &mut Host, steps: &[MbQueryStepV1]) -> (i32, u64) {
    let mut out = MbQueryRegisteredV1 { nesting_exceeded: 99 };
    let value = b"v\0x";
    let status = unsafe { elephc_mbstring_query_apply_v1(host as *mut Host as *mut c_void,
        ptr::null_mut(), steps.as_ptr(), steps.len() as u64, value.as_ptr(), value.len() as u64,
        &mut out, &storage()) };
    host.clean();
    (status, out.nesting_exceeded)
}

/// Transfers each new child pin before cleanup, preserving normalized negative append identity.
#[test]
fn query_steps_transfer_cursors_and_borrow_binary_values() {
    let mut host = Host::new();
    host.statuses("enter", &[2, 0]);
    let mut final_step = step(QUERY_STORE, Some(0));
    final_step.key = MbHostValueV1 { tag: HOST_STRING, lo: b"k\0z".as_ptr() as u64, hi: 3 };
    assert_eq!(apply(&mut host, &[step(QUERY_ENTER, Some(5)), step(QUERY_ENTER, None), final_step]), (2, 0));
    assert_eq!(host.events, ["root:10", "enter:10:i5", "release:10", "next:20", "enter:20:i-3",
        "release:20", "store:21:s[107, 0, 122]:s[118, 0, 120]", "release:21"]);
}

/// Stops an exhausted append before the planned root removal and leaves earlier transitions complete.
#[test]
fn query_append_exhaustion_suppresses_later_removal() {
    let mut host = Host::new();
    host.index = i64::MAX;
    host.available = 0;
    assert_eq!(apply(&mut host, &[step(QUERY_ENTER, Some(5)), step(QUERY_ENTER, None), step(QUERY_REMOVE_ROOT, Some(5))]), (0, 0));
    assert_eq!(host.events, ["root:10", "enter:10:i5", "release:10", "next:20", "release:20"]);
}

/// Releases the nested cursor before resolving the root that cleanup itself may have retargeted.
#[test]
fn query_removal_resolves_live_root_after_cursor_cleanup() {
    for destination in [0, 50] {
        let mut host = Host::new();
        host.retarget.insert(20, destination);
        host.statuses("release", &[0, 2]);
        assert_eq!(apply(&mut host, &[step(QUERY_ENTER, Some(5)), step(QUERY_REMOVE_ROOT, Some(5))]), (2, 1));
        let mut expected = vec!["root:10".to_owned(), "enter:10:i5".to_owned(), "release:10".to_owned(),
            "release:20".to_owned(), format!("root:{destination}")];
        if destination != 0 { expected.extend(["remove:50:i5".to_owned(), "release:50".to_owned()]); }
        assert_eq!(host.events, expected);
    }
}

/// Consumes every failed callback's published cursor and lets an earlier pending exception win.
#[test]
fn query_failure_statuses_do_not_leak_published_cursors() {
    for callback in ["root", "enter", "store", "release"] {
        for status in [1, 19, 2] {
            let mut host = Host::new();
            host.statuses(callback, &[status]);
            let result = apply(&mut host, &[step(QUERY_ENTER, Some(5)), step(QUERY_STORE, Some(6))]);
            assert_eq!(result, (if status == 2 { 2 } else { 1 }, 0), "{callback}:{status}");
        }
    }
    let mut host = Host::new();
    host.statuses("enter", &[2]);
    host.statuses("store", &[1]);
    assert_eq!(apply(&mut host, &[step(QUERY_ENTER, Some(5)), step(QUERY_STORE, Some(6))]), (2, 0));
    let mut host = Host::new();
    host.empty_child = true;
    assert_eq!(apply(&mut host, &[step(QUERY_ENTER, Some(5))]), (1, 0));
}

/// Ignores empty plans and non-array roots without manufacturing nesting diagnostics.
#[test]
fn query_empty_or_non_array_roots_do_not_write() {
    let mut host = Host::new();
    assert_eq!(apply(&mut host, &[]), (0, 0));
    assert!(host.events.is_empty());
    host.root = 0;
    assert_eq!(apply(&mut host, &[step(QUERY_REMOVE_ROOT, Some(5))]), (0, 0));
    assert_eq!(host.events, ["root:0"]);
}

/// Rejects malformed instructions and append metadata while balancing an already acquired root.
#[test]
fn query_malformed_steps_fail_after_balanced_cleanup() {
    for malformed in [
        step(99, Some(1)), MbQueryStepV1 { append: 2, ..step(QUERY_ENTER, Some(1)) },
        step(QUERY_REMOVE_ROOT, None),
        MbQueryStepV1 { key: MbHostValueV1::null(), ..step(QUERY_STORE, Some(1)) },
        MbQueryStepV1 { key: MbHostValueV1 { tag: HOST_STRING, lo: 0, hi: 1 }, ..step(QUERY_STORE, Some(1)) },
    ] {
        let mut host = Host::new();
        assert_eq!(apply(&mut host, &[malformed]), (1, 0));
        assert_eq!(host.events, ["root:10", "release:10"]);
    }
    let mut host = Host::new();
    host.available = 2;
    assert_eq!(apply(&mut host, &[step(QUERY_STORE, None)]), (1, 0));
}

/// Rejects unsupported tables and impossible ranges before invoking any storage callback.
#[test]
fn query_invalid_transport_has_no_storage_effects() {
    let mut host = Host::new();
    let mut out = MbQueryRegisteredV1::default();
    for table in [MbQueryStorageV1 { abi_version: 2, ..storage() },
        MbQueryStorageV1 { struct_size: 8, ..storage() }, MbQueryStorageV1 { release: None, ..storage() }] {
        let status = unsafe { elephc_mbstring_query_apply_v1(&mut host as *mut Host as *mut c_void,
            ptr::null_mut(), ptr::null(), 0, ptr::null(), 0, &mut out, &table) };
        assert_eq!(status, 1);
    }
    for (steps, count, bytes, length) in [(ptr::null(), 1, ptr::null(), 0),
        (ptr::null(), 0, ptr::null(), 1), (ptr::dangling(), u64::MAX, ptr::null(), 0)] {
        let status = unsafe { elephc_mbstring_query_apply_v1(&mut host as *mut Host as *mut c_void,
            ptr::null_mut(), steps, count, bytes, length, &mut out, &storage()) };
        assert_eq!(status, 1);
    }
    assert_eq!(out.nesting_exceeded, 0);
    assert!(host.events.is_empty());
    host.clean();
}
