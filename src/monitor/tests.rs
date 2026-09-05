//! Purpose:
//! The unit tests for `monitor`: parsing, accounting, rendering, exporting, the
//! budget assertions, and the stitching.
//!
//! Called from:
//! - `cargo test -p elephc --lib`, through `mod tests` in `monitor/mod.rs`.
//!
//! Key details:
//! - Fixtures are canned captures with known numbers, so assertions are literal
//!   rather than approximate.
//! - Anything reaching a rendered page is asserted to arrive as text: the names
//!   come from a profiled program and must not be able to write the page.

    /// A bare name that IS a local file becomes absolute, so the OS runs it
    /// rather than searching `PATH` and reporting it missing.
    #[test]
    fn a_local_program_named_bare_is_resolved_before_it_is_spawned() {
        let dir = std::env::temp_dir().join(format!("elephc_spawnable_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let name = "shop";
        std::fs::write(dir.join(name), b"#!/bin/sh\nexit 0\n").expect("fixture");

        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(&dir).expect("enter scratch");
        let resolved = super::spawnable_path(name);
        std::env::set_current_dir(previous).expect("restore cwd");

        assert!(
            resolved.is_absolute(),
            "a bare local name must not be left for a PATH search: {}",
            resolved.display()
        );
        assert!(resolved.ends_with(name));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A name that matches no local file is left alone, so `monitor some-tool`
    /// can still mean a program on `PATH`.
    #[test]
    fn a_name_that_is_not_a_local_file_is_left_for_the_path() {
        let resolved = super::spawnable_path("elephc-no-such-program-anywhere");
        assert_eq!(
            resolved,
            std::path::PathBuf::from("elephc-no-such-program-anywhere"),
            "a PATH lookup was turned into a local path that does not exist"
        );
    }

    /// An absolute path is already unambiguous and must survive untouched.
    #[test]
    fn an_absolute_target_is_untouched() {
        let resolved = super::spawnable_path("/usr/bin/true");
        assert_eq!(resolved, std::path::PathBuf::from("/usr/bin/true"));
    }

    /// Empty exact captures distinguish an unavailable control channel, an
    /// interrupted window, and an acknowledged run that published no frames.
    #[test]
    fn empty_exact_capture_reasons_name_the_observed_outcome() {
        let binary = std::path::Path::new("shop");
        let clean = std::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .status()
            .expect("clean fixture");
        let failed = std::process::Command::new("sh")
            .args(["-c", "exit 17"])
            .status()
            .expect("failed fixture");

        let unavailable = super::no_profile_reason(&clean, binary, false);
        assert!(unavailable.contains("control channel"), "{unavailable}");
        assert!(unavailable.contains("unavailable"), "{unavailable}");

        let empty = super::no_profile_reason(&clean, binary, true);
        assert!(empty.contains("acknowledged monitoring"), "{empty}");
        assert!(empty.contains("selective instrumentation"), "{empty}");
        assert!(empty.contains("did not close or publish"), "{empty}");

        let ended = super::no_profile_reason(&failed, binary, true);
        assert!(ended.contains("status 17"), "{ended}");
        assert!(ended.contains("before the active capture window"), "{ended}");
        let ended_without_ack = super::no_profile_reason(&failed, binary, false);
        assert!(
            ended_without_ack.contains("status 17"),
            "process termination is not misdiagnosed as a channel failure: {ended_without_ack}"
        );
    }

    /// The parent distinguishes a socketpair it merely created from a child
    /// that reached and acknowledged the runtime activation point.
    #[test]
    fn the_control_ack_proves_the_capture_was_activated() {
        let channel = super::open_control_channel().expect("control socketpair");
        assert!(
            !super::control_channel_activated(&channel),
            "creation alone must not count as activation"
        );
        let sent = unsafe {
            libc::send(
                channel.child,
                super::CONTROL_ACK.as_ptr() as *const libc::c_void,
                super::CONTROL_ACK.len(),
                0,
            )
        };
        assert_eq!(sent, super::CONTROL_ACK.len() as isize);
        assert!(super::control_channel_activated(&channel));
        // Asked twice on purpose. The probe peeks, so the second question gets
        // the same answer as the first; when it consumed, the ACK it reported
        // was the ACK it had just destroyed, and nobody downstream could see it.
        assert!(
            super::control_channel_activated(&channel),
            "asking must not spend the answer"
        );
    }

    /// The activation probe must not eat what it was not looking for.
    ///
    /// It shares one stream with `request_snapshot`. While it consumed, a window
    /// that opened with a reply already queued — a child that answered the
    /// previous request after that read had given up — had that reply taken by
    /// the question, and the next length parsed came out of the middle of a
    /// message. That reads as `Gone`, which ends the live view and reaps a
    /// program that was replying perfectly well.
    ///
    /// The reply here is deliberately LONGER than the twenty bytes the probe
    /// asks for. The consuming version used `MSG_WAITALL`, which `MSG_DONTWAIT`
    /// does not override on macOS: given less than twenty bytes it waits for the
    /// rest, so a shorter reply makes this test HANG instead of fail, and a test
    /// that hangs reports nothing. At this length the bug returns immediately,
    /// twenty bytes poorer, and the read-back below says so.
    #[test]
    fn asking_whether_the_child_activated_does_not_consume_its_answer() {
        let channel = super::open_control_channel().expect("control socketpair");

        let payload = b"main 5;__rt_hash_set 3;fn_spin 1";
        let header = (payload.len() as u32).to_le_bytes();
        for chunk in [&header[..], &payload[..]] {
            let sent = unsafe {
                libc::send(channel.child, chunk.as_ptr() as *const libc::c_void, chunk.len(), 0)
            };
            assert_eq!(sent, chunk.len() as isize);
        }

        assert!(
            !super::control_channel_activated(&channel),
            "a snapshot reply is not an activation ACK"
        );

        let mut back = vec![0u8; header.len() + payload.len()];
        let read = unsafe {
            libc::recv(
                channel.parent,
                back.as_mut_ptr() as *mut libc::c_void,
                back.len(),
                libc::MSG_DONTWAIT,
            )
        };
        assert_eq!(read, back.len() as isize, "the reply was consumed by the question");
        assert_eq!(&back[..4], &header[..], "its length word must be intact");
        assert_eq!(&back[4..], &payload[..], "and so must its body");
    }

    /// A snapshot read must survive an activation ACK that arrives late.
    ///
    /// `await_activation` has a deadline. A child that boots slower than it sends
    /// its ACK into the snapshot read instead, and the reply is length-prefixed:
    /// the ACK's first four bytes are `ELEP`, which decodes as a 1.3 GB length,
    /// which the size bound refuses. That answer used to be `Gone`, and `Gone`
    /// ends a live view on a program that is running perfectly well.
    ///
    /// Staged the way the race actually happens — nothing waiting for the ACK
    /// first, both messages already in the buffer when the read begins — because
    /// the point is that the reader copes on its own, not that some earlier step
    /// drained it.
    #[test]
    fn a_late_activation_ack_does_not_end_the_snapshot_read() {
        let channel = super::open_control_channel().expect("control socketpair");

        // The child answers the request, but its ACK is still queued in front.
        let ack = unsafe {
            libc::send(
                channel.child,
                super::CONTROL_ACK.as_ptr() as *const libc::c_void,
                super::CONTROL_ACK.len(),
                0,
            )
        };
        assert_eq!(ack, super::CONTROL_ACK.len() as isize);

        let payload = b"main 5";
        let header = (payload.len() as u32).to_le_bytes();
        for chunk in [&header[..], &payload[..]] {
            let sent = unsafe {
                libc::send(channel.child, chunk.as_ptr() as *const libc::c_void, chunk.len(), 0)
            };
            assert_eq!(sent, chunk.len() as isize);
        }

        match super::request_snapshot(&channel) {
            super::Snapshot::Answered(text) => assert_eq!(text, "main 5"),
            super::Snapshot::Late { .. } => {
                panic!("a queued ACK must not read as a slow target")
            }
            super::Snapshot::Gone => {
                panic!("a queued ACK must not read as a dead channel — this is the bug")
            }
        }
    }

    use super::*;

    /// The refusal of an unequipped target must not depend on the platform.
    ///
    /// It did, once: `require_monitoring` was called after a
    /// `cfg!(target_os = "macos")` block, so the same binary that was refused on
    /// a laptop was run and quietly under-reported on a Linux server — an
    /// environment-dependent behaviour in the one command whose whole purpose is
    /// not to have any.
    ///
    /// Nothing that runs can catch this. A Linux-only ordering bug is invisible
    /// to every test that executes the binary on macOS, and the two branches
    /// cannot both be taken in one process. What is left is the order of the
    /// source itself, so that is what this reads — the file as an interface,
    /// because here it is the only witness.
    ///
    /// The dispatch has no runtime platform branch left: `--attach` was the last
    /// one, and it is no longer refused off macOS. So the ordering rule is
    /// vacuous TODAY and the test says so rather than pretending to check it —
    /// but it stays armed, because the way this broke the first time was
    /// somebody adding a branch above the gate, and that is precisely the edit
    /// this still catches.
    #[test]
    fn the_capability_gate_runs_before_any_platform_branch() {
        // `mod.rs`, because that is where `run` lives now: a guard that reads a
        // file as text makes that filename part of the interface, and splitting
        // `monitor.rs` into modules broke exactly this one — loudly, which is
        // the good case for a guard of this shape.
        let source = include_str!("mod.rs");
        let body = source
            .split_once("pub(crate) fn run(cmd: MonitorCommand) -> i32 {")
            .expect("the dispatch function must exist")
            .1;
        let gate = body
            .find("require_monitoring(")
            .expect("the dispatch must refuse an unequipped target");
        // A compile-time `#[cfg]` picks which code EXISTS and cannot put one
        // platform's behaviour behind another's branch; only a runtime `cfg!`
        // inside this one body can, which is why that is what is looked for.
        if let Some(platform) = body.find("cfg!(target_os") {
            assert!(
                gate < platform,
                "the capability check is inside or after a platform branch, so it \
                 would be enforced on one platform and skipped on another"
            );
        }
    }

    /// The exact capture's Speedscope/pprof export must account for the run
    /// exactly once — no more, no less.
    ///
    /// The graph shape that breaks a naive converter is here on purpose: `leaf`
    /// has TWO callers, so anything that emits it once per caller at full cost
    /// reports more time than the program had, and every percentage a viewer
    /// draws is then wrong while the file still opens and looks plausible.
    /// `spin` calls itself, which sends a converter with no cycle guard into a
    /// stack overflow rather than a wrong answer.
    ///
    /// The reference is the sum of every function's SELF time, which is what
    /// partitions a run — not the largest inclusive, which is only the deepest
    /// single call.
    #[test]
    fn exact_export_accounts_for_the_run_exactly_once() {
        use crate::call_graph::{CallGraph, GraphEdge, GraphNode};

        /// A graph node with only the two time dimensions set; every other
        /// metric is zero, so an assertion about time cannot be satisfied by
        /// something else drifting.
        fn node(name: &str, inclusive: u64, exclusive: u64) -> GraphNode {
            GraphNode {
                name: name.to_string(),
                inclusive,
                exclusive,
                call_count: None,
                alloc_inclusive: 0,
                alloc_exclusive: 0,
                io_inclusive: 0,
                io_exclusive: 0,
                retained_inclusive: 0,
                retained_exclusive: 0,
                wait_inclusive: 0,
                wait_exclusive: 0,
                network_inclusive: 0,
                network_exclusive: 0,
                network_wait_inclusive: 0,
                network_wait_exclusive: 0,
                causes: Vec::new(),
            }
        }

        // root(1000) ─┬─ left(400) ──┐
        //             └─ right(300) ─┴─ leaf(500 total, reached from both)
        // spin(200) calls itself.
        let graph = CallGraph {
            nodes: vec![
                node("root", 1000, 300),
                node("left", 400, 100),
                node("right", 300, 100),
                node("leaf", 500, 500),
                node("spin", 200, 200),
            ],
            edges: vec![
                GraphEdge { from: 0, to: 1, weight: 400, count: Some(1) },
                GraphEdge { from: 0, to: 2, weight: 300, count: Some(1) },
                GraphEdge { from: 1, to: 3, weight: 300, count: Some(1) },
                GraphEdge { from: 2, to: 3, weight: 200, count: Some(1) },
                GraphEdge { from: 4, to: 4, weight: 100, count: Some(3) },
            ],
            total: 1000,
            queries: Vec::new(),
            lines: None,
            trace: None,
        };

        let stacks = exact_stacks(&graph);
        let exported: u64 = stacks.iter().map(|(_, weight)| *weight).sum();
        let run: u64 = graph.nodes.iter().map(|n| n.exclusive).sum();
        assert_eq!(
            exported, run,
            "the export must sum to the run's self time, not a multiple of it"
        );

        // And `leaf` must appear under both callers, split rather than doubled.
        let leaf_total: u64 = stacks
            .iter()
            .filter(|(path, _)| path.last().map(|(n, _)| n == "leaf").unwrap_or(false))
            .map(|(_, weight)| *weight)
            .sum();
        assert_eq!(leaf_total, 500, "leaf's own time is 500 however many callers reach it");
        let leaf_paths = stacks
            .iter()
            .filter(|(path, _)| path.last().map(|(n, _)| n == "leaf").unwrap_or(false))
            .count();
        assert_eq!(leaf_paths, 2, "leaf should appear once per caller path");
    }

    /// A Speedscope export must be recognizable as the wrong kind of baseline.
    ///
    /// This is the detector behind the only error message that can rescue the
    /// mistake the docs themselves used to recommend — `--out` a Speedscope
    /// file, then hand it to `--baseline`. Without it the user is told the file
    /// is unreadable and left to guess which of two JSON files they wanted.
    #[test]
    fn a_speedscope_export_is_not_mistaken_for_an_exact_capture() {
        let dir = std::env::temp_dir().join(format!("elephc_baseline_{}", process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");

        let speedscope = dir.join("s.json");
        std::fs::write(
            &speedscope,
            r#"{"$schema":"https://www.speedscope.app/file-format-schema.json",
                "profiles":[],"shared":{"frames":[]}}"#,
        )
        .expect("write speedscope");
        assert!(looks_like_speedscope(&speedscope.display().to_string()));

        // An exact capture is JSON too, and must NOT be mistaken for one.
        let exact = dir.join("e.json");
        std::fs::write(&exact, r#"{"nodes":[],"edges":[],"total":0}"#).expect("write exact");
        assert!(!looks_like_speedscope(&exact.display().to_string()));

        // Neither is anything else — a missing file, or JSON of another shape.
        assert!(!looks_like_speedscope(&dir.join("absent.json").display().to_string()));
        let other = dir.join("o.json");
        std::fs::write(&other, r#"{"profiles":[]}"#).expect("write other");
        assert!(
            !looks_like_speedscope(&other.display().to_string()),
            "a bare `profiles` key is not enough to claim a file is Speedscope"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Bounding the export must not cost its arithmetic.
    ///
    /// These two pull against each other, and fixing one broke the other: the
    /// brake folds a subtree's time into its caller, and the leftover pass emits
    /// the self time of every node it never saw — so the first version counted
    /// folded subtrees twice. It measured +0.0053% on a real capture, small
    /// enough to pass for rounding, which is why the count and the total are
    /// asserted in the same test on the same graph.
    ///
    /// The shape is a chain of diamonds — layered dispatch, not adversarial
    /// input — where every level doubles the number of distinct paths. Before
    /// the brake, 52 nodes with a realistic nanosecond budget produced 319,930
    /// stacks; the bound was `log2(root nanoseconds)`, an accident of the units.
    #[test]
    fn bounding_the_export_does_not_cost_its_accounting() {
        use crate::call_graph::{CallGraph, GraphEdge, GraphNode};

        /// Same fixture shape as the block above, in this module's compact form.
        fn node(name: &str, inclusive: u64, exclusive: u64) -> GraphNode {
            GraphNode {
                name: name.to_string(), inclusive, exclusive, call_count: None,
                alloc_inclusive: 0, alloc_exclusive: 0, io_inclusive: 0, io_exclusive: 0,
                retained_inclusive: 0, retained_exclusive: 0,
                wait_inclusive: 0, wait_exclusive: 0,
                network_inclusive: 0,
                network_exclusive: 0,
                network_wait_inclusive: 0,
                network_wait_exclusive: 0,
                causes: Vec::new(),
            }
        }

        // n diamonds: j_k calls a_k and b_k, both of which call j_{k+1}.
        // Times are consistent — each frame's inclusive covers its own work plus
        // what it hands on — because an impossible graph proves nothing.
        // Each self time is DERIVED as inclusive minus what the frame hands on,
        // so the selfs sum to the run by construction. Hand-picked numbers gave
        // an impossible graph twice, and an impossible graph proves nothing.
        let n = 24usize;
        let run_ns = 10_000_000_000u64; // 10 s, so the floor is a real threshold
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut w = run_ns;
        for k in 0..n {
            let (j, a, b) = (3 * k, 3 * k + 1, 3 * k + 2);
            // j hands everything to its two arms; each arm keeps half of its own.
            nodes.push(node(&format!("j{k}"), w, 0));
            nodes.push(node(&format!("a{k}"), w / 2, w / 4));
            nodes.push(node(&format!("b{k}"), w / 2, w / 4));
            edges.push(GraphEdge { from: j, to: a, weight: w / 2, count: Some(1) });
            edges.push(GraphEdge { from: j, to: b, weight: w / 2, count: Some(1) });
            let next = 3 * (k + 1); // the next junction, or the leaf below
            edges.push(GraphEdge { from: a, to: next, weight: w / 4, count: Some(1) });
            edges.push(GraphEdge { from: b, to: next, weight: w / 4, count: Some(1) });
            w /= 2;
        }
        nodes.push(node("leaf", w, w)); // index 3n, where the last arms point
        let graph = CallGraph {
            nodes, edges, total: run_ns, queries: Vec::new(), lines: None, trace: None,
        };

        let run: u64 = graph.nodes.iter().map(|n| n.exclusive).sum();
        // Repeated halving loses a nanosecond here and there; what matters is
        // that the fixture is a partition, not that it lands on a round number.
        assert!(
            run_ns - run < 100,
            "the fixture itself must partition the run: {run} vs {run_ns}"
        );
        let stacks = exact_stacks(&graph);
        let exported: u64 = stacks.iter().map(|(_, w)| *w).sum();

        assert!(
            stacks.len() < 10 * graph.nodes.len(),
            "{} nodes produced {} stacks — the count is growing with the paths, \
             not with the graph",
            graph.nodes.len(),
            stacks.len()
        );
        // The export distributes the roots' inclusive time, which on a real
        // capture IS the sum of self times; here the two differ by the handful of
        // nanoseconds this fixture's own halving loses. What is being asserted is
        // that bounding the walk moved time into a caller rather than duplicating
        // or dropping it — a defect of that kind moved it by 0.0053% when it was
        // real, which is far outside this margin.
        let drift = exported.abs_diff(run);
        assert!(
            drift < 100,
            "bounding the walk must move time into a caller, never duplicate or \
             drop it: exported {exported} vs run {run} (drift {drift})"
        );
        assert!(
            stacks.iter().all(|(path, _)| path.len() <= EXACT_STACK_MAX_DEPTH),
            "no stack may exceed the depth bound"
        );
    }

    #[test]
    /// A key is 32 bytes of hex or nothing: a short, long, or non-hex value is
    /// refused rather than silently truncated into a wrong credential.
    fn probe_key_hex_parses_and_rejects_malformed() {
        let key = parse_hex_key(
            "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff",
        )
        .expect("valid 64-hex key");
        assert_eq!(key[0], 0x00);
        assert_eq!(key[31], 0xff);
        assert!(parse_hex_key("short").is_none());
        assert!(parse_hex_key(&"zz".repeat(32)).is_none());
    }

    #[test]
    /// The endpoint's folded wire text parses into the same display the local
    /// sampler produces, so one renderer serves both.
    fn folded_endpoint_text_becomes_display_stacks() {
        let text = "elephc-probe: <native>;{main};grind 480\n\
                    elephc-probe: <native>;{main};grind;<native> 20\n\
                    elephc-probe-samples: 500\n";
        let display = folded_text_to_display(text);
        assert_eq!(display.len(), 2);
        // First stack: native root, main, grind (all PHP but the native root).
        assert_eq!(display[0].1, 480);
        assert_eq!(display[0].0[0], ("<native>".to_string(), Kind::Native));
        assert_eq!(display[0].0[1], ("{main}".to_string(), Kind::Php));
        assert_eq!(display[0].0[2], ("grind".to_string(), Kind::Php));
        // The table attributes grind's 500 samples (both stacks name it).
        let table = why_table(&display, 1);
        assert!(table.contains("grind"), "{table}");
        assert!(table.contains("100.0%"), "{table}");
        // The `elephc-probe-samples:` trailer is not a stack line.
        assert!(!display.iter().any(|(stack, _)| stack
            .iter()
            .any(|(name, _)| name.contains("samples"))));
    }

    /// A trimmed `sample` report with the exact indentation and decoration
    /// characters the tool emits, including a function split across sibling
    /// nodes by call offset.
    const REPORT: &str = "Analysis of sampling bench (pid 1) every 1 millisecond
Load Address:    0x100000000
----

Call graph:
    100 Thread_1   DispatchQueue_1: com.apple.main-thread  (serial)
      100 start  (in dyld) + 6992  [0x1]
        60 main  (in bench) + 5164  [0x100000010]
        + 40 fn_hot_u_leaf  (in bench) + 308  [0x100000020]
        + ! 30 _rt_mixed_from_value  (in bench) + 196  [0x100000030]
        + ! : 20 _rt_heap_alloc  (in bench) + 628  [0x100000040]
        + 15 fn_hot_u_leaf  (in bench) + 12,4  [0x100000050,0x100000054]
        30 main  (in bench) + 99  [0x100000060]
        + 30 method_Engine_step  (in bench) + 1  [0x100000070]
        10 main  (in bench) + 3  [0x100000080]

Total number in stack (recursive counted multiple, when >=5):
";

    #[test]
    /// dyld frames are dropped and depths rebased on the first program frame,
    /// so the tree starts at the program rather than at the loader.
    fn parses_depths_and_addresses_from_decorated_prefixes() {
        let rows = parse_call_graph(REPORT);
        // dyld rows dropped, depths rebased on the first program frame.
        assert_eq!(rows[0].symbol, "main");
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[0].address, Some(0x100000010));
        assert_eq!(rows[1].symbol, "fn_hot_u_leaf");
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].symbol, "_rt_mixed_from_value");
        assert_eq!(rows[2].depth, 2);
        assert_eq!(rows[3].symbol, "_rt_heap_alloc");
        assert_eq!(rows[3].depth, 3);
        // The offset-split sibling stays at the same depth as its twin, and a
        // multi-bucket node keeps its first address.
        assert_eq!(rows[4].symbol, "fn_hot_u_leaf");
        assert_eq!(rows[4].depth, 1);
        assert_eq!(rows[4].address, Some(0x100000050));
    }

    #[test]
    /// Self weights sum to each parent's count — the partition property the
    /// whole display rests on.
    fn self_weights_partition_every_parent_count() {
        let rows = parse_call_graph(REPORT);
        let samples = build_samples(&rows);
        let total: u64 = samples.iter().map(|(_, weight)| weight).sum();
        // 100 thread samples minus nothing: every count is accounted for once.
        assert_eq!(total, 100);
        // main's first node keeps 60 - 40 - 15 = 5 self samples.
        assert!(samples.iter().any(|(stack, weight)| *weight == 5
            && stack.len() == 1
            && stack[0].symbol == "main"));
        // The helper chain bottoms out in heap_alloc with 20 self samples.
        assert!(samples.iter().any(|(stack, weight)| *weight == 20
            && stack.last().map(|f| f.symbol.as_str()) == Some("_rt_heap_alloc")));
    }

    /// Builds a one-slice log chunk for `service` with the given trace identity.
    fn slice_log(fn_name: &str, ns: u64, trace: &str, span: &str, parent: &str) -> String {
        format!(
            "elephc-instr-trace: trace={trace} span={span} parent={parent}\n\
             elephc-instr: {fn_name} calls=1 incl_ns={ns} excl_ns={ns} incl_allocs=0 \
             excl_allocs=0 incl_io=0 excl_io=0 incl_ret=0 excl_ret=0 incl_wait=0 excl_wait=0 \
             incl_network=0 excl_network=0 incl_network_wait=0 excl_network_wait=0\n"
        )
    }

    /// Builds a slice directly, so a summary can be exercised without a log round trip.
    fn slice_of(service: &str, ns: u64, start_us: Option<u64>, span: &str) -> Slice {
        let mut graph = parse_instrument_dump(&slice_log("handle", ns, "t1", span, "-"));
        if let Some(trace) = graph.trace.as_mut() {
            trace.start_us = start_us;
        }
        Slice {
            service: service.to_string(),
            graph,
        }
    }

    /// The database summary must survive a route containing spaces, and must say
    /// that its numbers are of a different kind from the sampled table above it.
    #[test]
    fn the_io_summary_reads_counters_off_the_end_of_the_line() {
        let text = "elephc-probe: a;b 3\n\
                    elephc-probe-samples: 17\n\
                    elephc-probe-io: <untagged> ops=551 wait_ns=3449131\n\
                    elephc-probe-io: GET /a b/c ops=2 wait_ns=1000\n\
                    elephc-probe-network: GET /a b/c ops=3 wait_ns=2000\n";
        let out = probe_io_summary(text);
        assert!(out.contains("Database"), "{out}");
        // A route can contain spaces, so the counters are read from the RIGHT;
        // splitting from the left would truncate the route and lose the row.
        assert!(out.contains("GET /a b/c"), "{out}");
        assert!(out.contains("551 ops"), "{out}");
        // Busiest first, so the row worth reading is the one at the top.
        let untagged = out.find("<untagged>").unwrap();
        let other = out.find("GET /a b/c").unwrap();
        assert!(untagged < other, "rows must sort by operation count:\n{out}");
        // The distinction the whole feature rests on.
        assert!(out.contains("Exact, not sampled"), "{out}");
        assert!(out.contains("Network - 3 outgoing operation(s)"), "{out}");

        // Nothing to say when the capture carries no events.
        assert!(probe_io_summary("elephc-probe: a 1\n").is_empty());
        // A malformed line is skipped rather than producing a bogus row.
        assert!(probe_io_summary("elephc-probe-io: x ops=nope wait_ns=1\n").is_empty());
    }

    /// The allocation summary distinguishes exact counter deltas from sampled
    /// attribution and from the unobserved edges of the capture window.
    #[test]
    fn the_allocation_summary_states_its_sampled_coverage_limits() {
        let text = "elephc-probe-alloc: a;b;load_price 900\n\
                    elephc-probe-alloc: a;record_audit 100\n";
        let out = probe_alloc_summary(text);
        assert!(out.contains("Allocations - 1000"), "{out}");
        assert!(out.contains("1000 observed between samples"), "{out}");
        assert!(out.contains("attribution is sampled"), "{out}");
        assert!(out.contains("after the final sample are outside"), "{out}");
        // Busiest first, named by the innermost PHP frame rather than the raw stack.
        let hot = out.find("load_price").unwrap();
        let cold = out.find("record_audit").unwrap();
        assert!(hot < cold, "{out}");
        assert!(out.contains("90.0%"), "{out}");
        // `<native>` leaves are skipped when naming a row, or every row would read
        // the same and the summary would say nothing.
        let native = probe_alloc_summary("elephc-probe-alloc: a;connect;<native> 5\n");
        assert!(native.contains("connect"), "{native}");
        assert!(probe_alloc_summary("elephc-probe: a 1\n").is_empty());
    }

    /// The client outlasts the server it is waiting on, in `--exact`.
    ///
    /// The two deadlines were written independently and disagreed: the client
    /// gave up at 10s while the server held the connection for `EXACT_WAIT`, so
    /// the documented no-traffic answer could never be received, and a slice from
    /// a request completing after the tenth second was lost with it. Asserting
    /// the relation rather than the number is the point — a later change to
    /// either constant has to keep them ordered.
    #[test]
    fn the_exact_client_waits_longer_than_the_server_it_asks() {
        let server = elephc_probe::endpoint::EXACT_WAIT;
        assert!(
            read_timeout(true) > server,
            "exact client timeout {:?} must outlast the server's {:?}",
            read_timeout(true),
            server
        );
        // The sampled answer is rendered immediately, so it must NOT inherit the
        // long deadline: a dead peer would hold the command for half a minute.
        assert!(
            read_timeout(false) < server,
            "the sampled timeout should stay short"
        );
    }

    /// A budget that is not a number is refused, not accepted as one.
    ///
    /// `f64::from_str` accepts "nan" and "inf", and either silently disables the
    /// check it belongs to: every comparison against NaN is false, so an
    /// assertion reports a failure it never measured, and `inf` with `<=` passes
    /// whatever the program did. `--fail-on-regression nan` is the dangerous
    /// one — a threshold a pipeline passes through a variable, arriving empty or
    /// misspelled, leaves a gate that can never trip and a run that exits 0
    /// through any regression.
    #[test]
    fn a_budget_that_is_not_a_number_is_refused() {
        for bad in ["nan", "NaN", "inf", "-inf", "infinity"] {
            assert!(
                parse_assert(&format!("calls:build<={bad}")).is_none(),
                "{bad} must not be accepted as a budget"
            );
            let argv = vec![
                "app.php".to_string(),
                "--fail-on-regression".to_string(),
                bad.to_string(),
            ];
            assert!(
                parse_monitor_args(&argv).is_err(),
                "--fail-on-regression {bad} must be refused, not left un-trippable"
            );
        }
        // Real thresholds keep working, including the boundary ones.
        assert!(parse_assert("calls:build<=10").is_some());
        assert!(parse_assert("time_pct:build<=0").is_some());
        let argv = vec![
            "app.php".to_string(),
            "--fail-on-regression".to_string(),
            "5.5".to_string(),
        ];
        assert!(parse_monitor_args(&argv).is_ok());
    }

    /// A gate a service target cannot run is refused, not silently skipped.
    ///
    /// `run_probe_host` reads `--exact`, `--out` and `--pprof` and nothing else,
    /// so `--assert` or `--baseline` against a service was parsed, stored, and
    /// never evaluated: the command exited 0 and the pipeline believed it had a
    /// gate. The refusal that already existed for `--exact` plus the exporters
    /// was written for this exact failure and covered only those flags.
    ///
    /// The second half matters as much as the first: the flags a service DOES
    /// honour must keep working, or the fix trades a silent pass for a refusal
    /// of legitimate use.
    #[test]
    fn a_budget_a_service_cannot_evaluate_is_refused() {
        // Through the real parser rather than a hand-built struct, so the test
        // breaks if a flag stops reaching the field it is checked on.
        let flags = |extra: &[&str]| {
            let mut argv = vec!["127.0.0.1:9411".to_string()];
            argv.extend(extra.iter().map(|a| a.to_string()));
            let cmd = parse_monitor_args(&argv).expect("these argv must parse");
            unhonoured_service_flags(&cmd)
        };

        assert_eq!(flags(&[]), None, "a bare read of a service is fine");

        assert_eq!(
            flags(&["--assert", "calls:build<=10"]).as_deref(),
            Some("--assert"),
            "a budget must be named as unhonoured"
        );

        assert_eq!(
            flags(&["--baseline", "base.json", "--fail-on-regression", "5"]).as_deref(),
            Some("--baseline, --fail-on-regression"),
            "every unhonoured flag is listed, so the message can name them all"
        );

        // The same budget by its other name. The first version of this refusal
        // listed `--assert` and not `--assert-file`, so the very defect it was
        // written to close stayed open through the second spelling.
        assert_eq!(
            flags(&["--assert-file", "budgets.elephc"]).as_deref(),
            Some("--assert-file"),
            "a budget file is a budget"
        );

        // Modes a service cannot honour: it answers once, through its endpoint.
        // `--serve` only parses alongside `--live` and `--html`, so this also
        // shows the html export riding through un-refused next to two modes that
        // are not.
        assert_eq!(flags(&["--live"]).as_deref(), Some("--live"));
        assert_eq!(
            flags(&["--live", "--html", "g.html", "--serve", "127.0.0.1:8080"]).as_deref(),
            Some("--live, --serve"),
        );

        // The decider is only half of it: an audit pointed out that deleting the
        // refusal from `run` leaves everything above green, because nothing here
        // reaches the call site. The source is the guarantee — the routing branch
        // must consult this before it hands the target to `run_probe_host`, and
        // must do so with a non-zero exit, since the whole defect was exiting 0.
        let routing = include_str!("mod.rs")
            .split_once("if remote_target(&cmd.target).is_some() || is_socket_path(&cmd.target) {")
            .expect("the service routing branch must exist")
            .1;
        // Split on the CALL, not the name: the comment above the refusal mentions
        // `run_probe_host`, and splitting on the name cut the branch before the
        // code it was meant to inspect.
        let branch = routing
            .split_once("return run_probe_host(")
            .expect("the branch must reach the service reader")
            .0;
        assert!(
            branch.contains("unhonoured_service_flags(&cmd)"),
            "the refusal must run BEFORE the service is read, or the gate is decorative"
        );
        assert!(
            branch.contains("return 2"),
            "and it must exit non-zero: exiting 0 with no gate run is the defect itself"
        );

        // What the service path really does honour must stay accepted, or the
        // fix trades a silent pass for a refusal of legitimate use.
        assert_eq!(
            flags(&["--exact"]),
            None,
            "--exact is read by run_probe_host and must not be refused here"
        );
        // Which exports a service honours is a fact about `run_probe_host`, and
        // reading it is the only way to state it. Held by hand, this list said
        // the sampled path rendered no graphs while it was calling
        // `write_graph_exports`, and `--html` against an address became exit 2
        // and no file. Asserting the call site here means re-adding that refusal
        // fails a test rather than shipping.
        let host = include_str!("remote.rs")
            .split_once("pub(crate) fn run_probe_host(")
            .expect("the service reader must exist")
            .1;
        let host_body = host.split_once("\n}\n").expect("a function body").0;
        for (writer, flag) in [
            ("write_speedscope(", "--out"),
            ("encode_folded_profile(", "--pprof"),
            ("write_graph_exports(", "--dot / --html"),
        ] {
            assert!(
                host_body.contains(writer),
                "{flag} is accepted below because the service path calls {writer}"
            );
        }
        assert_eq!(
            flags(&["--dot", "g.dot", "--html", "g.html"]),
            None,
            "the graph exporters are rendered from the sampled answer"
        );
        assert_eq!(
            flags(&["--out", "out.json", "--pprof", "p.pb"]),
            None,
            "the exporters the sampled path writes must not be refused here"
        );
    }

    /// A long non-ASCII function name shortens instead of killing the command.
    ///
    /// The two table renderers tested `name.len()` — bytes — and then sliced
    /// `&name[..n]`. When byte `n` fell inside a multi-byte character, `str`
    /// slicing panicked. The names come from the profiled program, so no hostile
    /// peer was required: a closure declared in a file with an accented name
    /// reaches it. The cases below place the accent exactly on the old cut point
    /// for each of the two widths, which is what the byte version could not
    /// survive.
    #[test]
    fn a_long_non_ascii_name_is_shortened_not_a_panic() {
        // The regression itself: 26 characters but 27 BYTES, with the accent
        // straddling byte 25. The old code compared bytes against the width, so
        // it decided this needed shortening and sliced mid-character. It fits by
        // the measure that matters — columns — so it comes back whole.
        let fits_by_chars_not_bytes = format!("{}éx", "a".repeat(24));
        assert_eq!(fits_by_chars_not_bytes.chars().count(), 26);
        assert_eq!(fits_by_chars_not_bytes.len(), 27, "27 bytes is what fooled it");
        assert_eq!(
            ellipsize(&fits_by_chars_not_bytes, 26),
            fits_by_chars_not_bytes,
            "a name that fits the column must survive intact"
        );

        // Same shape at the other renderer's width.
        let fits_at_24 = format!("{}éx", "b".repeat(22));
        assert_eq!(ellipsize(&fits_at_24, 24), fits_at_24);

        // Genuinely too long: shortened, and the result still fits the column.
        let too_long = format!("{}é_traiter_les_données", "c".repeat(30));
        let short = ellipsize(&too_long, 26);
        assert!(short.ends_with('…'), "it must say it was shortened: {short:?}");
        assert_eq!(short.chars().count(), 26, "shortened to the column width");

        // Multi-byte throughout, and an emoji outside the BMP — the cut lands on
        // a character boundary in both, which is the whole point.
        assert_eq!(ellipsize(&"é".repeat(40), 5).chars().count(), 5);
        assert_eq!(ellipsize(&"🦀".repeat(40), 5).chars().count(), 5);

        // Short names are untouched, accents included.
        assert_eq!(ellipsize("traiter_données", 26), "traiter_données");
        assert_eq!(ellipsize("", 26), "");
    }

    /// A path must never be read as a host, or a local socket silently becomes a
    /// network connection attempt — and the reverse would make `monitor host:port`
    /// try to open a file that does not exist.
    #[test]
    fn a_socket_path_is_never_mistaken_for_a_url() {
        let at = |spec: &str| {
            let t = remote_target(spec).expect("should parse");
            (t.host, t.port, t.tls)
        };
        assert_eq!(at("127.0.0.1:9000"), ("127.0.0.1".into(), 9000, false));
        assert_eq!(at("http://api.internal:8080"), ("api.internal".into(), 8080, false));
        // A path after the authority is not part of the address.
        assert_eq!(at("http://api.internal:8080/x"), ("api.internal".into(), 8080, false));
        // A scheme implies its port, and https implies TLS.
        assert_eq!(at("https://foo.example"), ("foo.example".into(), 443, true));
        assert_eq!(at("https://foo.example:8443"), ("foo.example".into(), 8443, true));
        assert_eq!(at("http://foo.example"), ("foo.example".into(), 80, false));

        // Filesystem paths, including one whose directory contains a colon.
        assert!(remote_target("/tmp/probe.sock").is_none());
        assert!(remote_target("./probe.sock").is_none());
        assert!(remote_target("/tmp/a:b/probe.sock").is_none());
        // A bare name is a binary, not a host: no port, no connection.
        assert!(remote_target("shop").is_none());
        assert!(remote_target("shop.php").is_none());
        // Malformed authorities are refused rather than half-parsed.
        assert!(remote_target(":9000").is_none(), "a port with no host");
        assert!(remote_target("host:").is_none(), "a host with no port");
        assert!(remote_target("host:notaport").is_none());
        assert!(remote_target("host:99999").is_none(), "a port past u16");
    }

    /// A crafted route must not be able to forge series in the metrics file.
    ///
    /// The exposition format ends a label value at an unescaped quote and a sample
    /// at a newline, so a path carrying either would write series a scraper trusts.
    /// Backslash is escaped first — escaping it last would re-escape the escapes we
    /// just added and corrupt every value containing one.
    #[test]
    fn prometheus_labels_cannot_be_escaped_out_of() {
        assert_eq!(escape_label(r#"GET /a"b"#), r#"GET /a\"b"#);
        assert_eq!(escape_label("GET /a\nelephc_requests_total 999"),
                   "GET /a\\nelephc_requests_total 999");
        assert_eq!(escape_label(r"back\slash"), r"back\\slash");
        // Ordinary routes are untouched, so escaping costs nothing in the common case.
        assert_eq!(escape_label("GET /orders/42"), "GET /orders/42");
    }

    /// The file must be the format Prometheus actually parses: typed, and with the
    /// quantile label inside the same brace group as the others.
    #[test]
    fn prometheus_output_is_a_valid_summary() {
        let mut slices: Vec<Slice> = (0..25)
            .map(|i| slice_of("api", 1_000_000 * (i + 1), None, &format!("s{i}")))
            .collect();
        slices[0].graph.nodes[0].network_inclusive = 25;
        slices[0].graph.nodes[0].network_exclusive = 25;
        slices[0].graph.nodes[0].network_wait_inclusive = 25_000_000;
        slices[0].graph.nodes[0].network_wait_exclusive = 25_000_000;
        let text = prometheus_text(&slices);
        assert!(text.contains("# TYPE elephc_request_duration_seconds summary"));
        assert!(
            text.contains(r#"elephc_request_duration_seconds{service="api",quantile="0.99"}"#),
            "quantile must join the existing label set, not open a second one:\n{text}"
        );
        // Durations are seconds, as every Prometheus convention requires.
        assert!(text.contains(" 0.025000\n"), "p99 of 25ms should be 0.025 s:\n{text}");
        assert!(
            text.contains("elephc_network_operations_per_request{service=\"api\"} 1.000"),
            "{text}"
        );
        assert!(
            text.contains("elephc_network_wait_seconds_per_request{service=\"api\"} 0.001000"),
            "{text}"
        );
        // Every sample line must carry a value, or the scrape fails wholesale.
        for line in text.lines().filter(|l| !l.starts_with('#') && !l.is_empty()) {
            let value = line.rsplit(' ').next().unwrap_or("");
            assert!(
                value.parse::<f64>().is_ok(),
                "unparseable sample value in {line:?}"
            );
        }
    }

    /// The decoder must give the operator the real path back, and must not turn a
    /// malformed escape into a guess.
    #[test]
    fn route_fields_round_trip_through_the_percent_encoding() {
        assert_eq!(decode_field("GET%20/orders/42"), "GET /orders/42");
        assert_eq!(decode_field("100%25"), "100%");
        assert_eq!(decode_field("a%09b"), "a\tb");
        assert_eq!(decode_field("/plain/path"), "/plain/path");
        // A truncated or non-hex escape is left as written rather than invented.
        assert_eq!(decode_field("half%2"), "half%2");
        assert_eq!(decode_field("bad%zz"), "bad%zz");
    }

    /// Nearest rank, so every percentile is a duration some request actually took.
    ///
    /// Interpolating would invent a number nobody experienced — the wrong trade when
    /// the inputs are exact. The edges matter more than the middle: p0 and p100 are
    /// where an off-by-one silently reports the wrong request as the slow one.
    #[test]
    fn percentiles_are_nearest_rank_and_never_interpolate() {
        let sorted: Vec<u64> = (1..=10).map(|n| n * 10).collect(); // 10..100
        assert_eq!(percentile(&sorted, 50.0), 50);
        assert_eq!(percentile(&sorted, 90.0), 90);
        assert_eq!(percentile(&sorted, 95.0), 100);
        assert_eq!(percentile(&sorted, 99.0), 100);
        // Never below the first element, never past the last, never a value that
        // is not in the input.
        assert_eq!(percentile(&sorted, 0.0), 10);
        assert_eq!(percentile(&sorted, 100.0), 100);
        assert_eq!(percentile(&[], 95.0), 0, "an empty set has no percentile");
        assert_eq!(percentile(&[42], 99.0), 42, "one sample is every percentile");
    }

    /// A rate needs a window, and the summary must refuse to invent one.
    #[test]
    fn the_rate_is_omitted_when_there_is_no_window_to_divide_by() {
        // Undated slices: no window at all.
        let undated = [slice_of("api", 1_000_000, None, "s1"), slice_of("api", 2_000_000, None, "s2")];
        assert!(service_stats(&undated)[0].rps.is_none(), "no timestamps, no rate");

        // Dated but instantaneous: dividing by zero would print `inf`.
        let same_instant = [
            slice_of("api", 0, Some(1_000_000), "s1"),
            slice_of("api", 0, Some(1_000_000), "s2"),
        ];
        assert!(service_stats(&same_instant)[0].rps.is_none(), "zero window, no rate");

        // A real window: two requests over 1s of wall clock.
        let spread = [
            slice_of("api", 0, Some(1_000_000), "s1"),
            slice_of("api", 0, Some(2_000_000), "s2"),
        ];
        let rps = service_stats(&spread)[0].rps.expect("a window exists");
        assert!((rps - 2.0).abs() < 0.01, "expected 2 req/s, got {rps}");
    }

    /// Below 20 requests the upper percentiles ARE the slowest requests, and saying
    /// "p99" without saying so dresses a maximum up as a distribution.
    #[test]
    fn a_thin_sample_says_its_percentiles_are_not_a_distribution() {
        let thin: Vec<Slice> = (0..5)
            .map(|i| slice_of("api", 1_000_000 * (i + 1), None, &format!("s{i}")))
            .collect();
        let note = "not a distribution";
        assert!(service_summary(&thin).contains(note), "{}", service_summary(&thin));

        let thick: Vec<Slice> = (0..25)
            .map(|i| slice_of("api", 1_000_000 * (i + 1), None, &format!("s{i}")))
            .collect();
        assert!(!service_summary(&thick).contains(note));
        // And one service being thin must flag the report even if another is not.
        let mut mixed: Vec<Slice> = (0..25)
            .map(|i| slice_of("api", 1_000_000 * (i + 1), None, &format!("s{i}")))
            .collect();
        mixed.extend((0..3).map(|i| slice_of("edge", 5_000_000, None, &format!("e{i}"))));
        assert!(service_summary(&mixed).contains(note), "a thin service must flag the report");
    }

    #[test]
    /// A service log is mostly not profiles; slices are cut on the trace line
    /// and everything else is passed over rather than misparsed.
    fn slices_split_on_the_trace_line_and_ignore_other_log_output() {
        let log = format!(
            "starting up\n{}{}",
            slice_log("a", 10, "t1", "s1", "-"),
            slice_log("a", 20, "t1", "s2", "-")
        );
        let slices = split_slices(&log);
        assert_eq!(slices.len(), 2, "one slice per trace line");
        assert!(slices[0].contains("incl_ns=10"), "{:?}", slices[0]);
        assert!(slices[1].contains("incl_ns=20"), "{:?}", slices[1]);
        // Text before the first trace line belongs to no slice.
        assert!(!slices[0].contains("starting up"));
        // A log with no slices yields none rather than one bogus entry.
        assert!(split_slices("just a log line\n").is_empty());
    }

    #[test]
    /// Spans nest by parent id, and one whose parent never arrived stays a root
    /// instead of being dropped — a partial trace is still worth reading.
    fn stitch_nests_spans_by_parent_and_keeps_orphans_as_roots() {
        let mk = |service: &str, chunk: String| Slice {
            service: service.to_string(),
            graph: parse_instrument_dump(&chunk),
        };
        let mut gateway = mk("gateway", slice_log("handle", 1_000, "tr", "aaaa", "-"));
        gateway.graph.nodes[0].network_inclusive = 2;
        gateway.graph.nodes[0].network_exclusive = 2;
        gateway.graph.nodes[0].network_wait_inclusive = 300;
        gateway.graph.nodes[0].network_wait_exclusive = 300;
        let slices = vec![
            gateway,
            mk("inventory", slice_log("stock", 400, "tr", "bbbb", "aaaa")),
            // Parent never appears in the logs (its service was not collected):
            // it must still render, as a root, not vanish.
            mk("billing", slice_log("charge", 700, "tr", "cccc", "zzzz")),
        ];
        let out = stitch_report(&slices);
        assert!(out.contains("trace tr"), "{out}");
        assert!(out.contains("● gateway"), "root is un-indented: {out}");
        assert!(out.contains("2 network"), "{out}");
        assert!(out.contains("300 ns network-wait"), "{out}");
        assert!(out.contains("  └─ inventory"), "child is nested: {out}");
        assert!(out.contains("● billing"), "orphan survives as a root: {out}");
        assert!(out.contains("1 trace(s) over 3 slice(s)"), "{out}");
    }

    #[test]
    /// Two addresses on one line accumulate; an address the dSYM could not place
    /// leaves both the counts and the denominator, so shares stay honest.
    fn line_attribution_sums_weights_and_drops_unresolvable_addresses() {
        // Two addresses on the same line accumulate; one address the dSYM could
        // not place is excluded from the counts AND from the denominator.
        let leaves = vec![
            (0x1000u64, 40u64),
            (0x1008u64, 20u64),
            (0x1000u64, 10u64),
            (0xdeadu64, 99u64), // unresolvable
        ];
        let mut resolved = HashMap::new();
        resolved.insert(0x1000u64, 7u32);
        resolved.insert(0x1008u64, 12u32);
        let (hits, total) = attribute_lines(&leaves, &resolved);
        assert_eq!(hits.get(&7).copied(), Some(50), "both hits on line 7 sum");
        assert_eq!(hits.get(&12).copied(), Some(20));
        assert_eq!(total, 70, "the unplaceable sample is not counted anywhere");
        assert_eq!(hits.len(), 2);
    }

    #[test]
    /// Query shapes are parsed and ordered by count, so the N+1 leads.
    fn parses_instrument_query_lines_hottest_first() {
        let dump = "\
elephc-instr: get_user calls=200 incl_ns=100 excl_ns=10 incl_allocs=0 excl_allocs=0 incl_io=200 excl_io=0
elephc-instr-query: 200 SELECT name FROM users WHERE id = ?
elephc-instr-query: 1 CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)
elephc-instr-query: 200 INSERT INTO users (name) VALUES (?)
";
        let graph = parse_instrument_dump(dump);
        assert_eq!(graph.queries.len(), 3);
        // The count and the full remainder of the line (with spaces) are kept.
        assert!(graph
            .queries
            .contains(&("SELECT name FROM users WHERE id = ?".to_string(), 200)));
        assert!(graph.queries.contains(&(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)".to_string(),
            1
        )));
        assert!(graph
            .queries
            .contains(&("INSERT INTO users (name) VALUES (?)".to_string(), 200)));
    }

    /// Exact instrumentation dumps retain inclusive and exclusive network metrics.
    #[test]
    fn parses_instrument_network_metrics() {
        let dump = "elephc-instr: fetch calls=2 incl_ns=500 excl_ns=400 incl_allocs=0 \
                    excl_allocs=0 incl_io=0 excl_io=0 incl_ret=0 excl_ret=0 incl_wait=0 \
                    excl_wait=0 incl_network=5 excl_network=2 incl_network_wait=700 \
                    excl_network_wait=300\n";
        let graph = parse_instrument_dump(dump);
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.nodes[0].network_inclusive, 5);
        assert_eq!(graph.nodes[0].network_exclusive, 2);
        assert_eq!(graph.nodes[0].network_wait_inclusive, 700);
        assert_eq!(graph.nodes[0].network_wait_exclusive, 300);
        let table = instrument_table(&graph);
        assert!(table.contains("network 2"), "{table}");
        assert!(table.contains("network-wait 300 ns"), "{table}");
        let html = crate::call_graph::render_html_exact(&graph, "network profile", &[]);
        assert!(html.contains("\"networkInclN\":5"), "{html}");
        assert!(html.contains("\"networkExclN\":2"), "{html}");
        assert!(html.contains("Net wait"), "{html}");
    }

    /// Logs written by the first monitoring implementation still load as direct metrics.
    #[test]
    fn parses_legacy_direct_network_metrics() {
        let dump = "elephc-instr: fetch calls=1 incl_ns=500 excl_ns=400 incl_allocs=0 \
                    excl_allocs=0 incl_io=0 excl_io=0 incl_ret=0 excl_ret=0 incl_wait=0 \
                    excl_wait=0 network_ops=2 network_wait=300\n";
        let graph = parse_instrument_dump(dump);
        assert_eq!(graph.nodes[0].network_inclusive, 2);
        assert_eq!(graph.nodes[0].network_exclusive, 2);
        assert_eq!(graph.nodes[0].network_wait_inclusive, 300);
        assert_eq!(graph.nodes[0].network_wait_exclusive, 300);
    }

    /// Saved graphs from before the network split restore direct values as inclusive too.
    #[test]
    fn loads_legacy_saved_network_metrics() {
        let path = std::env::temp_dir().join(format!(
            "elephc-monitor-legacy-network-{}.json",
            std::process::id()
        ));
        let legacy = r#"{
            "nodes": [{
                "name": "fetch",
                "inclusive": 1,
                "exclusive": 1,
                "call_count": 1,
                "alloc_inclusive": 0,
                "alloc_exclusive": 0,
                "network_ops": 2,
                "network_wait": 300,
                "causes": []
            }],
            "edges": [],
            "total": 1
        }"#;
        std::fs::write(&path, legacy).expect("write legacy graph fixture");
        let graph = load_exact_graph(path.to_str().expect("UTF-8 fixture path"))
            .expect("load legacy graph fixture");
        let _ = std::fs::remove_file(path);

        assert_eq!(graph.nodes[0].network_inclusive, 2);
        assert_eq!(graph.nodes[0].network_exclusive, 2);
        assert_eq!(graph.nodes[0].network_wait_inclusive, 300);
        assert_eq!(graph.nodes[0].network_wait_exclusive, 300);
    }

    #[test]
    /// Emitted symbol names map back to what the programmer wrote.
    fn demangles_php_symbols() {
        assert_eq!(demangle("main"), "{main}");
        assert_eq!(demangle("fn_hot_u_leaf"), "hot_leaf");
        assert_eq!(demangle("method_Engine_step"), "Engine::step");
        // `_u_` inside the class name survives the class/method split.
        assert_eq!(demangle("method_My_u_Class_run"), "My_Class::run");
        assert_eq!(demangle("_rt_heap_alloc"), "_rt_heap_alloc");
    }

    #[test]
    /// Runtime helpers are named as costs; PHP functions are left alone, since
    /// a user function is not a 'cause' of anything.
    fn causes_translate_helpers_and_ignore_php() {
        assert_eq!(cause_for("_rt_mixed_from_value"), Some("Mixed cell boxing"));
        assert_eq!(cause_for("_rt_heap_alloc"), Some("heap allocation"));
        assert_eq!(cause_for("_rt_mixed_cast_int"), Some("Mixed cell unboxing"));
        assert_eq!(cause_for("_rt_something_new"), Some("runtime helper"));
        assert_eq!(cause_for("fn_hot_u_leaf"), None);
        assert_eq!(cause_for("main"), None);
    }

    #[test]
    /// A helper's cost is charged to the PHP function that caused it, not to the
    /// helper — which is what makes the breakdown actionable.
    fn why_table_attributes_causes_to_the_php_owner() {
        let rows = parse_call_graph(REPORT);
        let samples = build_samples(&rows);
        let table = why_table(&render_stacks(&samples), 1);
        assert!(table.contains("samples: 100"));
        assert!(table.contains("{main}"));
        assert!(table.contains("100.0%"), "{table}");
        assert!(table.contains("hot_leaf"));
        assert!(table.contains(" 55.0%"), "{table}");
        assert!(table.contains("Engine::step"));
        // heap_alloc's 20 self samples surface as a cause under hot_leaf,
        // rendered with a proportion bar.
        assert!(table.contains("heap allocation"));
        assert!(table.contains(" 20.0%"), "{table}");
        assert!(table.contains("Mixed cell boxing"));
        assert!(table.contains(" 10.0%"), "{table}");
        assert!(table.contains('█') && table.contains('░'), "{table}");
    }

    /// The terminal waterfall must separate hops that ran one after the other from
    /// hops that overlapped — the whole point of placing them.
    #[test]
    fn placed_bars_start_where_the_span_started() {
        // Flush left, quarter width.
        assert_eq!(placed_bar(0.0, 25.0, 12), "███░░░░░░░░░");
        // Same duration, but starting halfway through the trace.
        assert_eq!(placed_bar(50.0, 25.0, 12), "······███░░░");
        // Two concurrent hops share an offset, so they line up.
        assert_eq!(placed_bar(25.0, 25.0, 12), placed_bar(25.0, 25.0, 12));
        // A hop too short to round to a cell still gets one: invisible is worse
        // than imprecise, because a missing bar reads as "did not run".
        assert_eq!(placed_bar(0.0, 0.0, 12), "█░░░░░░░░░░░");
        // A span running to the very end does not overflow its width.
        assert_eq!(placed_bar(75.0, 25.0, 12).chars().count(), 12);
        assert_eq!(placed_bar(100.0, 100.0, 12).chars().count(), 12);
    }

    #[test]
    /// The bar is a linear read of the percentage, including both extremes.
    fn bars_scale_with_the_percentage() {
        assert_eq!(bar(0.0, 10), "░░░░░░░░░░");
        assert_eq!(bar(50.0, 10), "█████░░░░░");
        assert_eq!(bar(100.0, 10), "██████████");
        // Out-of-range input clamps instead of panicking on repeat counts.
        assert_eq!(bar(140.0, 4), "████");
    }

    #[test]
    /// The note explaining merged processes appears when processes were merged,
    /// and not otherwise — an unconditional caveat teaches readers to skip it.
    fn multi_process_note_appears_only_when_merging() {
        let rows = parse_call_graph(REPORT);
        let samples = build_samples(&rows);
        let display = render_stacks(&samples);
        assert!(!why_table(&display, 1).contains("processes"));
        assert!(why_table(&display, 4).contains("samples: 100 · 4 processes"));
    }

    #[test]
    /// A live frame carries the delta against the previous window, which is what
    /// makes the top-style display readable while it moves.
    fn live_frame_tracks_trends_between_windows() {
        let rows = parse_call_graph(REPORT);
        let samples = build_samples(&rows);
        let display = render_stacks(&samples);
        let mut cumulative: BTreeMap<Vec<(String, Kind)>, u64> = BTreeMap::new();
        for (stack, weight) in &display {
            *cumulative.entry(stack.clone()).or_default() += weight;
        }
        let mut previous = HashMap::new();
        // First window: no prior data, so no arrow yet.
        let first = live_frame(
            &display,
            &cumulative,
            &mut previous,
            2,
            3,
            std::time::Duration::from_secs(63),
        );
        assert!(first.contains("live · 2 processes · window 3s · total 1m03s"));
        assert!(first.contains("CUMUL"));
        assert!(!first.contains('▲') && !first.contains('▼'), "{first}");
        // Second window identical to the first: flat trends.
        let second = live_frame(
            &display,
            &cumulative,
            &mut previous,
            2,
            3,
            std::time::Duration::from_secs(66),
        );
        assert!(second.contains('─'), "{second}");
    }

    #[test]
    /// Declaration ranges are found for both functions and methods, so a sample
    /// landing anywhere inside one is attributed to it.
    fn decl_ranges_cover_functions_and_methods() {
        let source = "<?php
function hot_leaf(int $n): int {
    return $n;
}

class Engine {
    public function step(): int {
        return 1;
    }
}

function call_hot(int $n): int {
    return hot_leaf($n);
}

echo call_hot(1);
";
        let ranges = php_decl_ranges(source);
        assert_eq!(
            ranges,
            vec![
                DeclRange { name: "hot_leaf".into(), start: 2, end: 4 },
                DeclRange { name: "Engine::step".into(), start: 7, end: 9 },
                DeclRange { name: "call_hot".into(), start: 12, end: 14 },
            ]
        );
    }

    #[test]
    /// A sample landing on a line owned by another function grows a virtual
    /// frame for it, which is how inlined callees stay visible.
    fn inlined_frame_is_injected_from_a_foreign_line() {
        // main sampled on a line owned by call_hot's range grows a virtual frame.
        let mut samples = vec![(
            vec![Frame {
                symbol: "main".to_string(),
                address: Some(0x100000010),
                inlined: false,
            }],
            60u64,
        )];
        let ranges = vec![DeclRange {
            name: "call_hot".to_string(),
            start: 32,
            end: 34,
        }];
        let mut lines = HashMap::new();
        lines.insert(0x100000010u64, 33u32);
        // Inline the core of inject_inlined_frames without atos: rewrite manually.
        for (stack, _) in samples.iter_mut() {
            let mut rewritten = Vec::new();
            for frame in stack.iter() {
                rewritten.push(frame.clone());
                if let Some(line) = frame.address.and_then(|a| lines.get(&a)) {
                    if let Some(owner) = ranges
                        .iter()
                        .find(|range| range.start <= *line && *line <= range.end)
                    {
                        if owner.name != demangle(&frame.symbol) {
                            rewritten.push(Frame {
                                symbol: format!("inlined:{}", owner.name),
                                address: None,
                                inlined: true,
                            });
                        }
                    }
                }
            }
            *stack = rewritten;
        }
        let display = render_stacks(&samples);
        assert_eq!(display[0].0[1].0, "call_hot (inlined)");
        assert_eq!(display[0].0[1].1, Kind::PhpInlined);
        let table = why_table(&display, 1);
        assert!(table.contains("call_hot (inlined)"), "{table}");
        assert!(table.contains("100.0%"), "{table}");
    }

    /// A small exact call graph parsed from a canned instrumentation dump,
    /// shared by the tests that render or export one.
    fn instr_graph() -> crate::call_graph::CallGraph {
        use crate::call_graph::{CallGraph, GraphEdge, GraphNode};
        let node = |name: &str, incl, excl, calls, ai, ae| GraphNode {
            name: name.into(),
            inclusive: incl,
            exclusive: excl,
            call_count: Some(calls),
            alloc_inclusive: ai,
            alloc_exclusive: ae,
            io_inclusive: 0,
            io_exclusive: 0,
            retained_inclusive: 0,
            retained_exclusive: 0,
            wait_inclusive: 0,
            wait_exclusive: 0,
            network_inclusive: 0,
            network_exclusive: 0,
            network_wait_inclusive: 0,
            network_wait_exclusive: 0,
            causes: vec![],
        };
        CallGraph {
            nodes: vec![
                node("run", 1_000_000, 1000, 1, 500, 5),
                node("leaf", 990_000, 990_000, 1200, 495, 495),
            ],
            edges: vec![GraphEdge { from: 0, to: 1, weight: 990_000, count: Some(1200) }],
            total: 1_000_000,
            queries: Vec::new(),
            lines: None,
            trace: None,
        }
    }

    #[test]
    /// Budget assertions parse in the documented form and evaluate against a
    /// capture, including the case that fails.
    fn parses_and_evaluates_assertions() {
        assert_eq!(
            parse_assert("calls:leaf<=1000"),
            Some(("calls".into(), "leaf".into(), "<=".into(), 1000.0))
        );
        assert_eq!(parse_assert("bogus"), None);
        let mut graph = instr_graph();
        graph.nodes[1].network_inclusive = 3;
        graph.nodes[1].network_exclusive = 3;
        graph.nodes[1].network_wait_inclusive = 2_000_000;
        graph.nodes[1].network_wait_exclusive = 2_000_000;
        let run = |specs: &[&str]| {
            let owned: Vec<(String, Option<String>)> =
                specs.iter().map(|s| ((*s).to_string(), None)).collect();
            assert_report(&evaluate_asserts(&graph, &owned))
        };
        // leaf has 1200 calls: <=1000 FAILS, <=2000 PASSES.
        let (report, ok) = run(&["calls:leaf<=1000"]);
        assert!(!ok, "{report}");
        assert!(report.contains("[FAIL] calls:leaf<=1000 (actual 1200)"), "{report}");
        assert!(report.contains("0 passed, 1 failed"), "{report}");
        let (report, ok) = run(&["calls:leaf<=2000", "time_pct:run>=90"]);
        assert!(ok, "{report}");
        assert!(report.contains("2 passed, 0 failed"), "{report}");
        let (report, ok) = run(&["network:leaf<=3", "network_wait_ms:*<=2"]);
        assert!(ok, "{report}");
        // A function the run never reached is reported as unevaluated, not as a
        // failure of the code: the budget names something that is not there.
        let (report, ok) = run(&["calls:ghost<1"]);
        assert!(!ok, "{report}");
        assert!(report.contains("[SKIP]") && report.contains("'ghost' never ran"), "{report}");
        // A typo in the metric names the ones that exist.
        let (report, ok) = run(&["bogons:leaf<1"]);
        assert!(!ok, "{report}");
        assert!(report.contains("unknown metric 'bogons'") && report.contains("retained"), "{report}");
    }

    /// Verifies `*` asserts on the whole run: self values sum across functions,
    /// while a time metric comes from the root rather than summing inclusives.
    #[test]
    fn run_total_assertions_use_the_star_target() {
        let graph = instr_graph();
        let run = |specs: &[&str]| {
            let owned: Vec<(String, Option<String>)> =
                specs.iter().map(|s| ((*s).to_string(), None)).collect();
            assert_report(&evaluate_asserts(&graph, &owned))
        };
        // run=1 call + leaf=1200 calls.
        let (report, ok) = run(&["calls:*<=1201"]);
        assert!(ok, "{report}");
        let (report, ok) = run(&["calls:*<=1200"]);
        assert!(!ok, "{report}");
        assert!(report.contains("(actual 1201)"), "{report}");
        // The whole run is 100% of itself, and its wall clock is the root's.
        let (report, ok) = run(&["time_pct:*<=100", "incl_ms:*<=1"]);
        assert!(ok, "{report}");
    }

    /// Verifies `.elephc` is found by walking up from the SOURCE, so one budget
    /// at a project root covers files nested under it, and the directory the
    /// profiler happens to run from cannot change which budget applies.
    #[test]
    fn project_file_is_found_by_walking_up_from_the_source() {
        let root = std::env::temp_dir().join(format!(
            "elephc_projectfile_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let nested = root.join("src/deep");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&nested).expect("create nested dirs");
        let source = nested.join("thing.php");
        std::fs::write(&source, "<?php").expect("write source");

        // No file anywhere above: nothing is claimed.
        assert!(find_project_file(source.to_str().expect("utf-8 path")).is_none());

        // One at the project root is found from two levels down.
        let at_root = root.join(PROJECT_FILE_NAME);
        std::fs::write(&at_root, "calls:f<=1\n").expect("write project file");
        let found = find_project_file(source.to_str().expect("utf-8 path")).expect("found");
        assert_eq!(found.canonicalize().ok(), at_root.canonicalize().ok());

        // A nearer one wins over the root: the closest budget describes the code.
        let nearer = nested.join(PROJECT_FILE_NAME);
        std::fs::write(&nearer, "calls:g<=2\n").expect("write nearer file");
        let found = find_project_file(source.to_str().expect("utf-8 path")).expect("found");
        assert_eq!(found.canonicalize().ok(), nearer.canonicalize().ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Verifies the budget file: one assertion per line, `#` comments dropped,
    /// and a trailing `#` kept as that assertion's label.
    #[test]
    fn assert_file_parses_specs_comments_and_labels() {
        let parsed = parse_assert_file(
            "# performance budget\n\ncalls:PDO::prepare <= 10   # once per request\n\
             queries:* <= 50\n   \n# trailing comment\n",
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "calls:PDO::prepare <= 10");
        assert_eq!(parsed[0].1.as_deref(), Some("once per request"));
        assert_eq!(parsed[1].0, "queries:* <= 50");
        assert_eq!(parsed[1].1, None);
    }

    #[test]
    /// The recommendation names the function that actually dominates, rather
    /// than the first row or the deepest one.
    fn recommends_the_hotspot() {
        let out = instrument_recommendations(&instr_graph());
        assert!(out.contains("leaf is the hotspot"), "{out}");
    }

    /// A `Late` that took the ACK on its way past says so.
    ///
    /// The ACK is sent ONCE. If a read consumes it and then gives up on the
    /// reply behind it, the caller's `activated` flag is still false over a
    /// message that no longer exists — so every later window opens by spending
    /// the whole activation deadline waiting for it again. The read is the only
    /// thing that knows, so the answer has to carry it.
    ///
    /// Staged against a real socketpair with a real deadline: only the ACK is
    /// sent, and nothing follows it.
    #[test]
    fn a_late_answer_reports_the_ack_it_consumed_on_the_way() {
        let channel = super::open_polled_control_channel().expect("a socketpair");
        let sent = unsafe {
            libc::send(
                channel.child,
                super::CONTROL_ACK.as_ptr() as *const libc::c_void,
                super::CONTROL_ACK.len(),
                0,
            )
        };
        assert_eq!(sent, super::CONTROL_ACK.len() as isize);

        match super::request_snapshot(&channel) {
            super::Snapshot::Late { activation_seen } => assert!(
                activation_seen,
                "the ACK was consumed by this read and nobody will send another"
            ),
            super::Snapshot::Answered(text) => {
                panic!("nothing was sent after the ACK, so there was nothing to answer: {text:?}")
            }
            super::Snapshot::Gone => panic!("a lone ACK is not a dead channel"),
        }
    }

    /// A window nobody answered in time does not end the view.
    ///
    /// The live loop ending is what REAPS the target: the program only outlives
    /// the view while the view is still running. So a slow answer treated as a
    /// dead one does not merely lose a window — it stops the healthy program the
    /// operator was in the middle of profiling.
    ///
    /// Driven against a real socketpair with a real receive deadline and nobody
    /// on the other end, because the distinction being tested is what the KERNEL
    /// reports, not what this code believes it will.
    #[test]
    fn a_window_that_times_out_is_not_a_dead_child() {
        let channel = super::open_polled_control_channel().expect("a socketpair");
        // Nothing answers, so the deadline is what ends the read. The marker the
        // parent wrote is on the child's side and is not read back here.
        let started = std::time::Instant::now();
        let outcome = super::request_snapshot(&channel);
        assert!(
            matches!(outcome, super::Snapshot::Late { activation_seen: false }),
            "a target that has not answered YET must not be reported as gone, and nothing \
             here sent an ACK for the read to have taken"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "the read waited far past its own deadline"
        );

        // And a closed peer is the other answer, from the same call.
        unsafe { libc::close(channel.child) };
        let mut closed = channel;
        closed.forget_child();
        assert!(
            matches!(super::request_snapshot(&closed), super::Snapshot::Gone),
            "a channel whose peer is closed must end the view rather than stall it"
        );
    }

    /// What gets stopped when a capture ends, and what does not.
    ///
    /// The rule decides the fate of a program this process launched, so getting
    /// it wrong either orphans work or destroys it. A program that ended on its
    /// own is never touched; a live view or a failed capture stops one that is
    /// still up, because both run until monitoring is over and waiting on a
    /// long-running target would hang forever.
    ///
    /// The case this rule deliberately does NOT see is a view whose channel
    /// broke: that is decided before it, because whose fault it is changes the
    /// answer.
    #[test]
    fn only_a_capture_that_owns_the_target_stops_it() {
        use super::Disposition::{Collect, LeaveAlone, Stop};
        // Already exited: nothing to stop, whatever the capture did.
        assert!(matches!(super::disposition(false, 0, true, false), Collect));
        assert!(matches!(super::disposition(false, 1, true, false), Collect));
        assert!(matches!(super::disposition(false, 1, false, false), Collect));
        // A one-shot capture that worked lets a live program finish by itself.
        assert!(matches!(super::disposition(true, 0, false, false), Collect));
        // A live view owns its target; a failed capture stops it too.
        assert!(matches!(super::disposition(true, 0, true, false), Stop));
        assert!(matches!(super::disposition(true, 1, false, false), Stop));
        // …but not when the view ended because OUR channel broke. This is the
        // one input that reverses the answer, on exactly the case that would
        // otherwise be stopped: a live view over a program that is still up.
        assert!(matches!(super::disposition(true, 0, true, true), LeaveAlone));
        assert!(matches!(super::disposition(true, 1, true, true), LeaveAlone));
        // A program that already exited is still collected, so a lost channel
        // never leaves a zombie behind.
        assert!(matches!(super::disposition(false, 0, true, true), Collect));
    }

    /// A reply that arrives in pieces is waited for, not cut off mid-sentence.
    ///
    /// The deadline resets on every piece, so a child that is merely slow keeps
    /// the channel. Only a stall with NOTHING arriving mid-message ends it — and
    /// ending it reaps the target, which is why the difference between "slow"
    /// and "stopped" is worth spending three deadlines on.
    ///
    /// Driven over a real socketpair with a real writer, because what is under
    /// test is how the kernel delivers a stream, not how this code imagines it.
    #[test]
    fn a_reply_arriving_in_pieces_is_waited_for() {
        let mut channel = super::open_polled_control_channel().expect("a socketpair");
        // One second, so the pause below genuinely PASSES the deadline. With the
        // production five, a test short enough to keep would never reach the
        // path it exists to cover.
        super::set_receive_deadline(channel.parent, 1);
        let child = channel.child;
        let body = b"elephc-probe: {main};hot 12\n";
        let header = (body.len() as u32).to_le_bytes();

        // A writer that sends the length, pauses, then the body a byte at a
        // time — the shape a loaded child produces.
        let writer = std::thread::spawn(move || {
            // The request byte the caller sends first, taken so it does not sit
            // in front of anything.
            let mut request = [0u8; 1];
            unsafe {
                libc::recv(child, request.as_mut_ptr() as *mut libc::c_void, 1, 0);
                libc::send(child, header.as_ptr() as *const libc::c_void, 4, 0);
            }
            // Longer than the deadline: the reader stalls mid-message, with the
            // length already consumed. Retrying is only safe because `filled`
            // says how much of THIS reply is still owed.
            std::thread::sleep(std::time::Duration::from_millis(1_800));
            for byte in body {
                unsafe {
                    libc::send(child, byte as *const u8 as *const libc::c_void, 1, 0);
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        });

        match super::request_snapshot(&channel) {
            super::Snapshot::Answered(text) => assert_eq!(text.as_bytes(), body),
            super::Snapshot::Late { .. } => {
                panic!("a reply that did arrive was reported as absent")
            }
            super::Snapshot::Gone => {
                panic!("a child sending its reply in pieces was reported as gone, which reaps it")
            }
        }
        writer.join().expect("the writer finished");
        channel.forget_child();
        unsafe { libc::close(child) };
    }

    /// The profiler's own lines are removed from a program's stderr; the
    /// program's are not.
    ///
    /// Both mechanisms write to the same stderr, so a monitor forwarding a
    /// program's diagnostics has to tell them apart — and a prefix alone is not
    /// enough. A program that writes `elephc-instrumentation disabled` had that
    /// line deleted by the tool watching it, silently, which is the failure this
    /// rule exists to prevent: the author's own message, gone, with nothing to
    /// say it ever existed.
    #[test]
    fn profiler_lines_are_told_from_the_programs_own() {
        for profiler in [
            "elephc-instr: hot calls=1 incl_ns=5",
            "elephc-instr-edge: a -> b count=1 ns=2",
            "elephc-instr-query: 3 SELECT ?",
            "elephc-instr-query-dropped: 2",
            "elephc-instr-trace: wrote /tmp/t.json",
            "elephc-probe: {main};hot 12",
            "elephc-probe-io: 4",
            "elephc-probe-samples: 934",
            "elephc-probe-alloc: {main};hot 594",
        ] {
            assert!(
                super::is_profiler_line(profiler),
                "profiler output reached the operator as if the program wrote it: {profiler}"
            );
        }
        for own in [
            "elephc-instrumentation disabled by config",
            "elephc-probes are not enabled here",
            "elephc-instr is what we call it",
            // A name the profiler does not use, which a prefix-and-colon rule
            // would still have taken.
            "elephc-instr-custom: our own channel",
            "elephc-probe-of-ours: hello",
            "warning: something the program wanted to say",
            "",
            "   ",
        ] {
            assert!(
                !super::is_profiler_line(own),
                "the program's own diagnostic was deleted by the tool watching it: {own}"
            );
        }
    }
