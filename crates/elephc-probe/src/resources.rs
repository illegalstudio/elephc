//! Purpose:
//! Service-wide CPU and memory for `--apm`: what the whole process tree of a
//! `--web` service — its master, workers and isolated handlers — has used,
//! measured from the master when the collector asks for a snapshot.
//!
//! Called from:
//! - `apm_snapshot_text`, on the probe endpoint thread, which lives in the
//!   master (it is started before the prefork, and threads do not survive one).
//!
//! Key details:
//! - CPU is cumulative and stays so when a worker dies: each live process
//!   counts its own time plus the time of the children it has reaped, and a
//!   reaped worker's time moves into its parent's reaped-children time. A
//!   child reaped while the tree is being walked is caught by reading every
//!   parent's reaped time again afterwards, and the walk is redone.
//! - Memory counts each page once over the tree: a page shared by n processes
//!   counts 1/n in each — Linux's proportional set size, and on macOS the same
//!   sum made from each memory region's private and shared resident pages.
//!   Workers forked from one master share their copy-on-write pages, and
//!   summing resident sizes (or macOS's physical footprint, which charges a
//!   shared page to every process that maps it) counts those once per worker.
//! - Memory is read once per process, after the CPU walk settled: it is the
//!   costly read (it walks the process's mappings); the CPU re-check is not.
//! - Runs off the request path, once per poll; it may allocate.

use std::sync::atomic::{AtomicU64, Ordering};

use crate::apm::Resources;

/// The highest CPU total reported so far. A total is cumulative: should a
/// measurement still come out lower — a child gone between reads despite the
/// re-check — the last figure stands rather than a drop the collector would
/// clamp, then charge again in full at the next poll.
static REPORTED_CPU_NS: AtomicU64 = AtomicU64::new(0);

/// How long a memory figure is reused before the tree's memory is read again.
///
/// Reading it walks every process's mappings — about 5 µs per region on
/// macOS, so near 10 ms for a large worker — while the collector asks every
/// second. Memory moves slowly next to CPU; reading it every few polls keeps
/// the endpoint's own CPU, which is counted in the service's, small.
const MEMORY_REFRESH: std::time::Duration = std::time::Duration::from_secs(5);

/// The last memory figure and when it was read.
static LAST_MEMORY: std::sync::Mutex<Option<(std::time::Instant, u64)>> = std::sync::Mutex::new(None);

/// Measures this process and all its live descendants.
pub fn service_usage() -> Resources {
    let root = unsafe { libc::getpid() };
    let mut last = LAST_MEMORY.lock().unwrap_or_else(|e| e.into_inner());
    let fresh = last.filter(|(read_at, _)| read_at.elapsed() < MEMORY_REFRESH).map(|(_, bytes)| bytes);
    let mut usage = measure_tree(root, &platform::System, fresh.is_none());
    match fresh {
        Some(bytes) => usage.memory_bytes = bytes,
        None => *last = Some((std::time::Instant::now(), usage.memory_bytes)),
    }
    drop(last);
    usage.cpu_ns = REPORTED_CPU_NS.fetch_max(usage.cpu_ns, Ordering::Relaxed).max(usage.cpu_ns);
    // What the CPU is a share of: the cores this process may be scheduled on,
    // which follows its affinity and, on Linux, its cgroup quota — a container
    // limited to two cores reads 2, not the host's count.
    usage.cores = std::thread::available_parallelism().map_or(0, |n| n.get() as u32);
    usage
}

/// One process's CPU, split in its own time and the time of the children it
/// has reaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cpu {
    own_ns: u64,
    reaped_ns: u64,
}

/// Where the figures of the process tree come from: the system, or a test's
/// scripted tree.
trait Reader {
    /// The process's CPU, or `None` when it is gone or not ours to read.
    fn cpu(&self, pid: libc::pid_t) -> Option<Cpu>;
    /// The process's share of memory in bytes; 0 when it cannot be read.
    fn memory(&self, pid: libc::pid_t) -> u64;
    /// Its live children.
    fn children(&self, pid: libc::pid_t) -> Vec<libc::pid_t>;
}

/// Walks the tree under `root` and adds it up.
///
/// A child reaped between reading its parent and reading it would be counted
/// by neither, and counted again in full at the next poll. Every parent's
/// reaped time is read again after the walk: if one moved, a child was reaped
/// meanwhile, and the walk is redone. A few tries settle it; a tree that keeps
/// recycling past them takes the last walk, and `service_usage` keeps the
/// total from going back.
///
/// Memory is read once, over the processes of the walk that stood: only the
/// CPU is re-read to settle the walk (review: `smaps_rollup` walks a worker's
/// page tables, and reading it twice per poll doubled the cost). Without
/// `with_memory` it is not read at all.
fn measure_tree(root: libc::pid_t, reader: &dyn Reader, with_memory: bool) -> Resources {
    let mut usage = Resources::default();
    for attempt in 0..4 {
        let walked = walk(root, reader);
        usage = Resources::default();
        for (_, cpu) in &walked {
            usage.cpu_ns = usage.cpu_ns.saturating_add(cpu.own_ns).saturating_add(cpu.reaped_ns);
            usage.processes += 1;
        }
        let settled = walked.iter().all(|(pid, cpu)| reader.cpu(*pid).is_none_or(|now| now.reaped_ns == cpu.reaped_ns));
        if settled || attempt == 3 {
            if with_memory {
                usage.memory_bytes = walked.iter().fold(0u64, |sum, (pid, _)| sum.saturating_add(reader.memory(*pid)));
            }
            break;
        }
    }
    usage
}

/// Every live process of the tree under `root`, read once, parents first.
fn walk(root: libc::pid_t, reader: &dyn Reader) -> Vec<(libc::pid_t, Cpu)> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(pid) = pending.pop() {
        // A runaway tree must not turn one poll into a scan of the machine.
        if out.len() >= 4_096 {
            break;
        }
        // Read before its children are listed: a child it reaps from now on
        // moves its reaped time, which the re-check sees.
        if let Some(cpu) = reader.cpu(pid) {
            out.push((pid, cpu));
            pending.extend(reader.children(pid));
        }
    }
    out
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{proportional_bytes, Cpu, Reader, Region};

    /// The running system.
    pub(super) struct System;

    impl Reader for System {
        /// `pid`'s CPU from `proc_pid_rusage`.
        fn cpu(&self, pid: libc::pid_t) -> Option<Cpu> {
            let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
            let status = unsafe {
                libc::proc_pid_rusage(
                    pid,
                    libc::RUSAGE_INFO_V2,
                    (&mut info as *mut libc::rusage_info_v2).cast::<libc::rusage_info_t>(),
                )
            };
            if status != 0 {
                return None;
            }
            // The times are mach absolute ticks, not nanoseconds: one tick is
            // 1 ns on Intel and 125/3 ns on Apple silicon.
            Some(Cpu {
                own_ns: ticks_to_ns(info.ri_user_time.saturating_add(info.ri_system_time)),
                reaped_ns: ticks_to_ns(info.ri_child_user_time.saturating_add(info.ri_child_system_time)),
            })
        }

        /// `pid`'s proportional memory, from its memory regions.
        ///
        /// Not `ri_phys_footprint`: it charges a page a forked worker still
        /// shares with its master to both, so a master and three workers over
        /// 200 MB they share read 800 MB. Each region tells its private and
        /// shared resident pages and how many mappings share its memory, which
        /// is what dividing a shared page among its sharers needs.
        fn memory(&self, pid: libc::pid_t) -> u64 {
            let page = match unsafe { libc::sysconf(libc::_SC_PAGESIZE) } {
                size if size > 0 => size as u64,
                _ => 16_384,
            };
            let mut regions = Vec::new();
            let mut address = 0u64;
            // A runaway map must not turn one poll into a long scan.
            while regions.len() < 65_536 {
                let mut info: RegionInfo = unsafe { std::mem::zeroed() };
                let size = std::mem::size_of::<RegionInfo>() as libc::c_int;
                let read = unsafe {
                    libc::proc_pidinfo(pid, PROC_PIDREGIONINFO, address, (&mut info as *mut RegionInfo).cast(), size)
                };
                if read != size || info.pri_size == 0 {
                    break;
                }
                regions.push(Region {
                    private_pages: u64::from(info.pri_private_pages_resident),
                    shared_pages: u64::from(info.pri_shared_pages_resident),
                    sharers: u64::from(info.pri_ref_count),
                });
                address = info.pri_address.saturating_add(info.pri_size);
            }
            proportional_bytes(&regions, page)
        }

        /// The live children of `pid`.
        fn children(&self, pid: libc::pid_t) -> Vec<libc::pid_t> {
            let mut buffer = vec![0 as libc::pid_t; 256];
            loop {
                let bytes = (buffer.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
                let count = unsafe { libc::proc_listchildpids(pid, buffer.as_mut_ptr().cast(), bytes) };
                if count <= 0 {
                    return Vec::new();
                }
                let count = count as usize;
                // A full buffer may have cut the list short: ask again with room.
                if count >= buffer.len() && buffer.len() < 65_536 {
                    buffer.resize(buffer.len() * 4, 0);
                    continue;
                }
                buffer.truncate(count.min(buffer.len()));
                buffer.retain(|child| *child > 0);
                return buffer;
            }
        }
    }

    /// `proc_pidinfo` flavor answering one memory region: the one holding the
    /// given address, or the next one after it.
    const PROC_PIDREGIONINFO: libc::c_int = 7;

    /// `struct proc_regioninfo` from `<sys/proc_info.h>`, which libc does not
    /// bind. Only the resident page counts, the sharing count and the extent are
    /// read; the size is checked against the C one below.
    #[repr(C)]
    struct RegionInfo {
        pri_protection: u32,
        pri_max_protection: u32,
        pri_inheritance: u32,
        pri_flags: u32,
        pri_offset: u64,
        pri_behavior: u32,
        pri_user_wired_count: u32,
        pri_user_tag: u32,
        pri_pages_resident: u32,
        pri_pages_shared_now_private: u32,
        pri_pages_swapped_out: u32,
        pri_pages_dirtied: u32,
        pri_ref_count: u32,
        pri_shadow_depth: u32,
        pri_share_mode: u32,
        pri_private_pages_resident: u32,
        pri_shared_pages_resident: u32,
        pri_obj_id: u32,
        pri_depth: u32,
        pri_address: u64,
        pri_size: u64,
    }

    // `sizeof(struct proc_regioninfo)` in the macOS SDK; the kernel answers
    // only a buffer of exactly that size.
    const _: () = assert!(std::mem::size_of::<RegionInfo>() == 96);

    /// `mach_timebase_info_data_t`: nanoseconds per tick as a fraction. Declared
    /// here because libc deprecates its copy in favour of another crate.
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }

    extern "C" {
        /// Fills the tick-to-nanosecond ratio; 0 on success.
        fn mach_timebase_info(info: *mut Timebase) -> libc::c_int;
    }

    /// Converts mach absolute ticks to nanoseconds.
    fn ticks_to_ns(ticks: u64) -> u64 {
        static TIMEBASE: std::sync::OnceLock<(u32, u32)> = std::sync::OnceLock::new();
        let (numer, denom) = *TIMEBASE.get_or_init(|| {
            let mut info = Timebase { numer: 0, denom: 0 };
            if unsafe { mach_timebase_info(&mut info) } != 0 || info.denom == 0 {
                (1, 1)
            } else {
                (info.numer, info.denom)
            }
        });
        (u128::from(ticks) * u128::from(numer) / u128::from(denom)) as u64
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{parse_smaps_pss, parse_stat, parse_statm, Cpu, Reader};

    /// The running system.
    pub(super) struct System;

    impl Reader for System {
        /// `pid`'s CPU from `/proc/<pid>/stat`.
        fn cpu(&self, pid: libc::pid_t) -> Option<Cpu> {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let fields = parse_stat(&stat)?;
            let ticks_per_second = match unsafe { libc::sysconf(libc::_SC_CLK_TCK) } {
                ticks if ticks > 0 => ticks as u64,
                _ => 100,
            };
            let to_ns = |ticks: u64| ticks.saturating_mul(1_000_000_000) / ticks_per_second;
            Some(Cpu { own_ns: to_ns(fields.own_ticks), reaped_ns: to_ns(fields.reaped_ticks) })
        }

        /// `pid`'s proportional memory from `/proc/<pid>/smaps_rollup` —
        /// `statm` on a kernel without it (before 4.14), whose figure counts
        /// shared anonymous pages in every process.
        fn memory(&self, pid: libc::pid_t) -> u64 {
            std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))
                .ok()
                .and_then(|rollup| parse_smaps_pss(&rollup))
                .or_else(|| {
                    let page = match unsafe { libc::sysconf(libc::_SC_PAGESIZE) } {
                        size if size > 0 => size as u64,
                        _ => 4_096,
                    };
                    std::fs::read_to_string(format!("/proc/{pid}/statm"))
                        .ok()
                        .and_then(|statm| parse_statm(&statm))
                        .map(|pages| pages.saturating_mul(page))
                })
                .unwrap_or(0)
        }

        /// The live children of `pid`: from `/proc/<pid>/task/*/children` when
        /// the kernel provides it, else by scanning `/proc` for their parent.
        fn children(&self, pid: libc::pid_t) -> Vec<libc::pid_t> {
            let mut out = Vec::new();
            let mut listed = false;
            if let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) {
                for task in tasks.flatten() {
                    if let Ok(text) = std::fs::read_to_string(task.path().join("children")) {
                        listed = true;
                        out.extend(text.split_whitespace().filter_map(|child| child.parse::<libc::pid_t>().ok()));
                    }
                }
            }
            if listed {
                return out;
            }
            let Ok(entries) = std::fs::read_dir("/proc") else { return out };
            for entry in entries.flatten() {
                let Some(child) = entry.file_name().to_str().and_then(|name| name.parse::<libc::pid_t>().ok()) else {
                    continue;
                };
                let Ok(stat) = std::fs::read_to_string(format!("/proc/{child}/stat")) else { continue };
                if parse_stat(&stat).is_some_and(|fields| fields.ppid == pid) {
                    out.push(child);
                }
            }
            out
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    use super::{Cpu, Reader};

    /// A system this module cannot read: no process, no children.
    pub(super) struct System;

    impl Reader for System {
        /// No per-process accounting on this platform.
        fn cpu(&self, _pid: libc::pid_t) -> Option<Cpu> {
            None
        }

        /// No per-process accounting on this platform.
        fn memory(&self, _pid: libc::pid_t) -> u64 {
            0
        }

        /// No process tree to walk on this platform.
        fn children(&self, _pid: libc::pid_t) -> Vec<libc::pid_t> {
            Vec::new()
        }
    }
}

/// One memory region's resident pages, as macOS reports them.
#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    /// Pages only this mapping holds (copied on write, or never shared).
    private_pages: u64,
    /// Pages of memory other mappings also hold.
    shared_pages: u64,
    /// How many mappings hold the region's memory, this one included.
    sharers: u64,
}

/// A process's proportional memory over its regions, bytes: its private pages,
/// plus each shared page divided among the mappings that share it — the macOS
/// counterpart of Linux's proportional set size, so pages forked workers share
/// with their master add up to themselves once over the tree.
#[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
fn proportional_bytes(regions: &[Region], page: u64) -> u64 {
    let pages = regions.iter().fold(0u64, |sum, region| {
        sum.saturating_add(region.private_pages).saturating_add(region.shared_pages / region.sharers.max(1))
    });
    pages.saturating_mul(page)
}

/// What `parse_stat` reads of a `/proc/<pid>/stat` line.
#[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StatFields {
    ppid: i32,
    /// utime + stime, clock ticks.
    own_ticks: u64,
    /// cutime + cstime: the children it has reaped, clock ticks.
    reaped_ticks: u64,
}

/// Parses a `/proc/<pid>/stat` line: the parent pid, the process's own CPU
/// ticks and its reaped children's. The command name sits in parentheses and
/// may itself hold spaces and parentheses, so fields are counted from the LAST
/// `)`.
#[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
fn parse_stat(line: &str) -> Option<StatFields> {
    let rest = &line[line.rfind(')')? + 1..];
    // After the name: state(3) ppid(4) pgrp session tty tpgid flags minflt
    // cminflt majflt cmajflt utime(14) stime(15) cutime(16) cstime(17).
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let field = |number: usize| fields.get(number - 3).and_then(|value| value.parse::<i64>().ok());
    let ticks = |numbers: [usize; 2]| {
        numbers.iter().map(|number| field(*number).map(|ticks| ticks.max(0) as u64)).sum::<Option<u64>>()
    };
    Some(StatFields { ppid: field(4)? as i32, own_ticks: ticks([14, 15])?, reaped_ticks: ticks([16, 17])? })
}

/// Parses `/proc/<pid>/smaps_rollup` into the process's proportional set size,
/// bytes: each page it maps divided by the number of processes mapping it, so
/// the pages forked workers share add up to themselves once over the tree.
#[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
fn parse_smaps_pss(text: &str) -> Option<u64> {
    let line = text.lines().find(|line| line.starts_with("Pss:"))?;
    let mut fields = line.split_whitespace().skip(1);
    let value: u64 = fields.next()?.parse().ok()?;
    match fields.next() {
        Some("kB") | None => Some(value.saturating_mul(1024)),
        Some(_) => None,
    }
}

/// Parses `/proc/<pid>/statm` into resident pages minus file-backed shared
/// ones: the fallback when `smaps_rollup` is missing, which still counts
/// anonymous pages shared after a fork in every process that maps them.
#[cfg_attr(not(any(test, target_os = "linux")), allow(dead_code))]
fn parse_statm(line: &str) -> Option<u64> {
    let mut fields = line.split_whitespace().skip(1).map(|value| value.parse::<u64>().ok());
    let resident = fields.next()??;
    let shared = fields.next()??;
    Some(resident.saturating_sub(shared))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// The command name may hold spaces and parentheses; fields count from the
    /// last `)`, and a process's own and reaped CPU are told apart.
    #[test]
    fn stat_lines_parse_past_an_awkward_command_name() {
        let line = "4242 (php (worker) 1) S 4200 4200 4200 0 -1 4194560 100 0 0 0 250 50 7 3 20 0 1 0 12345 0 0";
        assert_eq!(parse_stat(line), Some(StatFields { ppid: 4200, own_ticks: 300, reaped_ticks: 10 }));
        assert_eq!(parse_stat("garbage"), None);
        assert_eq!(parse_stat("1 (init) S"), None, "a cut line is not read as zero");
    }

    /// Linux memory is the proportional set size of `smaps_rollup`, in bytes;
    /// `statm`'s resident-minus-shared is the fallback.
    #[test]
    fn memory_is_read_proportionally() {
        let rollup = "55d1c0000000-7ffd2a3f1000 ---p 00000000 00:00 0 [rollup]\nRss: 30000 kB\nPss: 12345 kB\nPss_Anon: 9000 kB\nShared_Clean: 400 kB\n";
        assert_eq!(parse_smaps_pss(rollup), Some(12_345 * 1024));
        assert_eq!(parse_smaps_pss("Rss: 1 kB\n"), None);
        assert_eq!(parse_statm("50000 12000 4000 300 0 9000 0\n"), Some(8_000));
        assert_eq!(parse_statm("50000"), None);
    }

    /// A shared page counts once over the processes sharing it: a master and
    /// three workers mapping the same 100 pages add up to 100 pages, plus what
    /// each holds privately; an unshared region counts in full.
    #[test]
    fn shared_pages_are_divided_among_their_sharers() {
        let shared = Region { private_pages: 0, shared_pages: 100, sharers: 4 };
        let own = Region { private_pages: 10, shared_pages: 0, sharers: 1 };
        let tree: u64 = (0..4).map(|_| proportional_bytes(&[shared, own], 4096)).sum();
        assert_eq!(tree, (100 + 40) * 4096);
        let orphan = Region { private_pages: 0, shared_pages: 7, sharers: 0 };
        assert_eq!(proportional_bytes(&[orphan], 1), 7, "a zero sharing count is not a division by zero");
    }

    /// Workers forked from a master over memory it filled do not each count
    /// that memory: their proportional share is a fraction of it. Measured on
    /// the running system — macOS's physical footprint, which this replaced,
    /// gave each child the whole 64 MB.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn forked_workers_do_not_count_the_memory_they_share() {
        const BYTES: usize = 64 << 20;
        let mut buffer = vec![0u8; BYTES];
        for (index, byte) in buffer.iter_mut().enumerate() {
            *byte = index as u8 | 1;
        }
        let mut children = Vec::new();
        for _ in 0..2 {
            // The child only reads the inherited buffer and sleeps: nothing
            // that needs a lock another test thread might have held at fork.
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0, "fork failed");
            if pid == 0 {
                let mut sum = 0u64;
                for page in buffer.chunks(4096) {
                    sum = sum.wrapping_add(u64::from(page[0]));
                }
                std::hint::black_box(sum);
                unsafe {
                    libc::sleep(3);
                    libc::_exit(0);
                }
            }
            children.push(pid);
        }
        std::thread::sleep(std::time::Duration::from_millis(1_000));
        let shares: Vec<u64> = children.iter().map(|pid| platform::System.memory(*pid)).collect();
        for pid in &children {
            unsafe {
                libc::kill(*pid, libc::SIGKILL);
                libc::waitpid(*pid, std::ptr::null_mut(), 0);
            }
        }
        std::hint::black_box(&buffer);
        for share in &shares {
            assert!(*share > 0, "a live child has memory: {shares:?}");
            assert!(*share < (BYTES as u64) * 2 / 3, "a child's share of the 64 MB it maps: {shares:?}");
        }
    }

    /// Memory is read once per process, after the walk settled; CPU is what
    /// the settle check re-reads (review: reading memory twice doubled the
    /// cost of every poll).
    #[test]
    fn memory_is_read_once_per_process() {
        let tree = HashMap::from([(1, (100, 0, 10, vec![2, 3])), (2, (5, 0, 20, vec![])), (3, (7, 0, 30, vec![]))]);
        let reader = Scripted { table: RefCell::new(tree), reap_after_reading: RefCell::new(None), memory_reads: RefCell::new(0) };
        let usage = measure_tree(1, &reader, true);
        assert_eq!(usage.memory_bytes, 60);
        assert_eq!(*reader.memory_reads.borrow(), 3, "one memory read per process");
        // Between refreshes the CPU is measured and the memory is not read.
        let between = measure_tree(1, &reader, false);
        assert_eq!((between.cpu_ns, between.processes), (112, 3));
        assert_eq!(*reader.memory_reads.borrow(), 3, "no memory read between refreshes");
    }

    /// A scripted process tree: pid → (own, reaped, memory, children), with an
    /// optional reap that happens right after a given process is first read.
    struct Scripted {
        table: RefCell<HashMap<libc::pid_t, (u64, u64, u64, Vec<libc::pid_t>)>>,
        reap_after_reading: RefCell<Option<(libc::pid_t, libc::pid_t)>>,
        memory_reads: RefCell<usize>,
    }

    impl Reader for Scripted {
        /// Reads a process's CPU; reading the trigger parent reaps its child.
        fn cpu(&self, pid: libc::pid_t) -> Option<Cpu> {
            let read = self.table.borrow().get(&pid).map(|(own, reaped, _, _)| Cpu { own_ns: *own, reaped_ns: *reaped });
            let trigger = *self.reap_after_reading.borrow();
            if let Some((parent, child)) = trigger.filter(|(parent, _)| *parent == pid) {
                self.reap_after_reading.replace(None);
                let mut table = self.table.borrow_mut();
                if let Some((own, reaped, ..)) = table.remove(&child) {
                    let entry = table.get_mut(&parent).expect("parent in the table");
                    entry.1 += own + reaped;
                    entry.3.retain(|c| *c != child);
                }
            }
            read
        }

        /// The process's scripted memory, counting the reads.
        fn memory(&self, pid: libc::pid_t) -> u64 {
            *self.memory_reads.borrow_mut() += 1;
            self.table.borrow().get(&pid).map_or(0, |entry| entry.2)
        }

        /// The children the table lists.
        fn children(&self, pid: libc::pid_t) -> Vec<libc::pid_t> {
            self.table.borrow().get(&pid).map(|entry| entry.3.clone()).unwrap_or_default()
        }
    }

    /// A worker reaped between reading the master and reading it is neither
    /// lost nor counted twice: the master's reaped time moved, the walk is
    /// redone, and the total is the same as if nothing had happened.
    #[test]
    fn a_worker_reaped_mid_walk_is_counted_once() {
        let tree = || {
            HashMap::from([
                (1, (100, 0, 10, vec![2, 3])),
                (2, (500, 0, 20, vec![])),
                (3, (70, 30, 30, vec![])),
            ])
        };
        let calm = Scripted { table: RefCell::new(tree()), reap_after_reading: RefCell::new(None), memory_reads: RefCell::new(0) };
        let expected = measure_tree(1, &calm, true);
        assert_eq!((expected.cpu_ns, expected.memory_bytes, expected.processes), (700, 60, 3));
        let recycling = Scripted {
            table: RefCell::new(tree()),
            reap_after_reading: RefCell::new(Some((1, 2))),
            memory_reads: RefCell::new(0),
        };
        let measured = measure_tree(1, &recycling, true);
        assert_eq!(measured.cpu_ns, 700, "the reaped worker's time is in the master's: {measured:?}");
        assert_eq!(measured.processes, 2, "and it is no longer a live process");
    }

    /// The tree is measured from this process: at least itself, with CPU spent
    /// and memory in use; a child it starts is counted while it lives.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn the_tree_counts_this_process_and_its_children() {
        let alone = service_usage();
        assert!(alone.processes >= 1, "{alone:?}");
        assert!(alone.cores >= 1, "the cores the CPU is a share of: {alone:?}");
        assert!(alone.memory_bytes > 0, "{alone:?}");
        // Other tests start and stop children of this process meanwhile, so the
        // count is not compared: the child this test started is looked for.
        let mut child = std::process::Command::new("sleep").arg("5").spawn().expect("spawn sleep");
        std::thread::sleep(std::time::Duration::from_millis(100));
        let pid = child.id() as libc::pid_t;
        let system = platform::System;
        let listed = system.children(unsafe { libc::getpid() }).contains(&pid);
        let measured = (system.cpu(pid), system.memory(pid));
        let _ = child.kill();
        let _ = child.wait();
        assert!(listed, "a live child is part of the tree");
        assert!(measured.0.is_some() && measured.1 > 0, "{measured:?}");
        // Burned by CPU time, not by wall time, which a loaded machine
        // stretches: 200 ms of wall clock could hold less than 100 ms of CPU
        // (review).
        let spin = std::time::Instant::now();
        let mut x = 0u64;
        let mut later = service_usage();
        while later.cpu_ns < alone.cpu_ns + 100_000_000 && spin.elapsed() < std::time::Duration::from_secs(20) {
            for _ in 0..1_000_000 {
                x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
            }
            later = service_usage();
        }
        assert!(later.cpu_ns >= alone.cpu_ns + 100_000_000, "the CPU spent shows: {alone:?} then {later:?}");
    }
}
