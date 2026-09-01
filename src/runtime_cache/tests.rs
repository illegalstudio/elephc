//! Purpose:
//! Regression tests for runtime-cache identity, integrity, security, pruning, and leases.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Environment-mutating probes respawn themselves serially with isolated cache roots.
//! - Cross-process fixtures pin the hardlink lease against concurrent pruning.

use super::*;

    /// Verifies the cache key changes with runtime-emitter build identity even
    /// when compiler version, target, heap, features, and PIC mode are identical.
    #[test]
    fn runtime_cache_key_covers_runtime_emitter_build_identity() {
        let target = Target::detect_host();
        let features = RuntimeFeatures::none();
        let first = runtime_cache_key_with_build_identity(
            8 * 1024 * 1024,
            target,
            features,
            false,
            b"emitter-build-a",
        );
        let second = runtime_cache_key_with_build_identity(
            8 * 1024 * 1024,
            target,
            features,
            false,
            b"emitter-build-b",
        );
        assert_ne!(first, second, "different runtime emitters must never share a cache entry");
    }

    /// Verifies every optional runtime-emission switch has an independent cache identity.
    #[test]
    fn runtime_cache_key_covers_every_runtime_feature_and_pic_mode() {
        let target = Target::detect_host();
        let baseline = runtime_cache_key_with_build_identity(
            8 * 1024 * 1024,
            target,
            RuntimeFeatures::none(),
            false,
            b"same-emitter",
        );
        let variants = [
            RuntimeFeatures { regex: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { timelib: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { mb_strlen: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { phar_archive: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { descriptor_invoker: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { eval_bridge: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { eval_scope: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { web: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { pdo_udf: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { fiber: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { generator: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { popen_resource: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { directory_resource: true, ..RuntimeFeatures::none() },
            RuntimeFeatures { float_precision: 13, ..RuntimeFeatures::none() },
            RuntimeFeatures { php_profile: 4, ..RuntimeFeatures::none() },
        ];

        let mut keys = std::collections::HashSet::from([baseline]);
        for features in variants {
            let key = runtime_cache_key_with_build_identity(
                8 * 1024 * 1024,
                target,
                features,
                false,
                b"same-emitter",
            );
            assert_ne!(key, baseline, "runtime feature was omitted from the cache identity");
            assert!(keys.insert(key), "runtime features produced colliding cache identities");
        }
        let pic = runtime_cache_key_with_build_identity(
            8 * 1024 * 1024,
            target,
            RuntimeFeatures::none(),
            true,
            b"same-emitter",
        );
        assert!(keys.insert(pic), "PIC mode collided with a runtime-feature identity");

        let static_library_boundary = identity::runtime_cache_key_with_build_identity_and_boundary(
            8 * 1024 * 1024,
            target,
            RuntimeFeatures::none(),
            false,
            true,
            b"same-emitter",
        );
        assert!(
            keys.insert(static_library_boundary),
            "non-PIC library-boundary mode collided with another runtime identity"
        );
    }

    /// Verifies Cargo reruns the identity builder when a runtime source file is
    /// added or removed, not only when an already-enumerated file is edited.
    #[test]
    fn runtime_build_identity_tracks_source_tree_membership() {
        let build_script = include_str!("../../build.rs");
        assert!(
            build_script.contains("cargo:rerun-if-changed=src"),
            "runtime build identity must be recomputed when src tree membership changes"
        );
    }

    /// Verifies the runtime cache directory is private to its owner so another
    /// local user cannot replace both the object and its integrity metadata.
    #[cfg(unix)]
    #[test]
    fn runtime_cache_directory_is_owner_only() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        const TEST_NAME: &str = "runtime_cache_directory_is_owner_only";
        if std::env::var("ELEPHC_RUNTIME_CACHE_MODE_PROBE").as_deref() != Ok(TEST_NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([TEST_NAME, "--nocapture", "--test-threads=1"])
                .env("ELEPHC_RUNTIME_CACHE_MODE_PROBE", TEST_NAME)
                .output()
                .expect("spawn isolated runtime-cache mode probe");
            assert!(
                output.status.success(),
                "isolated runtime-cache mode probe failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "elephc-runtime-cache-mode-{}-{unique}",
            std::process::id()
        ));
        let cache = root.join("elephc");
        fs::create_dir_all(&cache).expect("create permissive cache fixture");
        fs::set_permissions(&cache, fs::Permissions::from_mode(0o777))
            .expect("make fixture world-writable");
        std::env::set_var("XDG_CACHE_HOME", &root);

        prepare_runtime_object(
            8 * 1024 * 1024,
            Target::detect_host(),
            RuntimeFeatures::none(),
            false,
        )
        .expect("prepare runtime in hardened cache");
        let metadata = fs::metadata(&cache).expect("stat runtime cache directory");
        assert_eq!(
            metadata.mode() & 0o077,
            0,
            "runtime cache must reject group/other access"
        );
        assert_eq!(
            metadata.uid(),
            fs::metadata(&root)
                .expect("stat trusted cache root")
                .uid(),
            "runtime cache must be owned by the compiler user"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Verifies a cache hit whose object bytes were replaced is detected and
    /// rebuilt to the deterministic object originally produced by the assembler.
    #[test]
    fn tampered_runtime_object_cache_entry_is_rebuilt() {
        const TEST_NAME: &str = "tampered_runtime_object_cache_entry_is_rebuilt";
        if std::env::var("ELEPHC_RUNTIME_CACHE_PROBE").as_deref() != Ok(TEST_NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([TEST_NAME, "--nocapture", "--test-threads=1"])
                .env("ELEPHC_RUNTIME_CACHE_PROBE", TEST_NAME)
                .output()
                .expect("spawn isolated runtime-cache probe");
            assert!(
                output.status.success(),
                "isolated runtime-cache probe failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "elephc-runtime-cache-security-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create isolated cache root");
        std::env::set_var("XDG_CACHE_HOME", &root);

        let first = prepare_runtime_object(
            8 * 1024 * 1024,
            Target::detect_host(),
            RuntimeFeatures::none(),
            false,
        )
        .expect("build initial runtime object");
        let trusted = fs::read(&first.path).expect("read initial runtime object");
        fs::write(&first.path, b"attacker-controlled-object").expect("replace cache entry");

        let second = prepare_runtime_object(
            8 * 1024 * 1024,
            Target::detect_host(),
            RuntimeFeatures::none(),
            false,
        )
        .expect("repair tampered runtime object");
        let repaired = fs::read(&second.path).expect("read repaired runtime object");

        assert_eq!(repaired, trusted, "tampered cache bytes must never be reused");
        let _ = fs::remove_dir_all(&root);
    }

    /// Guards the warm-cache architecture: lookup and integrity metadata must
    /// be resolved before the expensive full runtime generator is invoked.
    #[test]
    fn warm_runtime_cache_lookup_precedes_runtime_assembly_generation() {
        let source = include_str!("../runtime_cache.rs");
        let lookup = source
            .find("cache_path.exists()")
            .expect("runtime cache lookup remains explicit");
        let generation = source
            .find("generate_runtime_with_features_mode")
            .expect("runtime assembly generator remains explicit");

        assert!(
            lookup < generation,
            "a warm cache hit must not regenerate the complete runtime assembly"
        );
    }

    /// Verifies cache housekeeping bounds superseded runtime objects while
    /// preserving the active entry, its integrity sidecar, and unrelated files.
    #[test]
    fn runtime_cache_pruning_bounds_superseded_build_identities() {
        const TEST_NAME: &str = "runtime_cache_pruning_bounds_superseded_build_identities";
        if std::env::var("ELEPHC_RUNTIME_CACHE_PRUNE_PROBE").as_deref() != Ok(TEST_NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([TEST_NAME, "--nocapture", "--test-threads=1"])
                .env("ELEPHC_RUNTIME_CACHE_PRUNE_PROBE", TEST_NAME)
                .output()
                .expect("spawn isolated runtime-cache pruning probe");
            assert!(
                output.status.success(),
                "isolated runtime-cache pruning probe failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "elephc-runtime-cache-prune-{}-{unique}",
            std::process::id()
        ));
        let cache = root.join("elephc");
        fs::create_dir_all(&cache).expect("create cache-pruning fixture");
        for identity in 0..12 {
            let object = cache.join(format!(
                "runtime-v0-test-rt{identity:016x}-heap1.o"
            ));
            fs::write(&object, format!("object-{identity}"))
                .expect("write cache object fixture");
            fs::write(
                cache.join(format!("{}.integrity", object.file_name().unwrap().to_string_lossy())),
                format!("integrity-{identity}"),
            )
            .expect("write cache integrity fixture");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let unrelated = cache.join("README.keep");
        fs::write(&unrelated, "unrelated").expect("write unrelated cache fixture");
        std::env::set_var("XDG_CACHE_HOME", &root);

        let prepared = prepare_runtime_object(
            8 * 1024 * 1024,
            Target::detect_host(),
            RuntimeFeatures::none(),
            false,
        )
        .expect("prepare active runtime while pruning old identities");

        let remaining: Vec<_> = fs::read_dir(&cache)
            .expect("read pruned cache fixture")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "o"))
            .collect();
        assert!(remaining.len() <= 8, "cache retained {remaining:?}");
        assert!(prepared.path.exists(), "the active runtime object was pruned");
        assert!(
            cache.join(format!(
                "{}.integrity",
                prepared.path.file_name().unwrap().to_string_lossy()
            ))
            .exists(),
            "the active runtime integrity sidecar was pruned"
        );
        assert!(unrelated.exists(), "cache pruning removed an unrelated file");
        let _ = fs::remove_dir_all(&root);
    }

    /// Verifies cache pruning removes assembler and integrity temporaries left
    /// behind by a dead compiler process without touching unrelated files.
    #[test]
    fn runtime_cache_pruning_removes_crash_abandoned_temporaries() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "elephc-runtime-cache-crash-litter-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create crash-litter fixture");

        let canonical = root.join("runtime-v0-test-rt0000000000000001-heap1.o");
        fs::write(&canonical, b"active").expect("write active cache object");
        let abandoned_asm = root.join(
            "runtime-v0-test-rt0000000000000002-heap1.4294967295_1.s",
        );
        let abandoned_object = root.join(
            "runtime-v0-test-rt0000000000000002-heap1.4294967295_1.o",
        );
        let abandoned_integrity = root.join(
            "runtime-v0-test-rt0000000000000002-heap1.integrity.4294967295.tmp",
        );
        fs::write(&abandoned_asm, b"assembly").expect("write abandoned assembly");
        fs::write(&abandoned_object, b"object").expect("write abandoned object");
        fs::write(&abandoned_integrity, b"checksum").expect("write abandoned integrity temp");
        let unrelated = root.join("runtime-not-owned.tmp");
        fs::write(&unrelated, b"keep").expect("write unrelated fixture");

        prune_runtime_cache_objects(&root, &canonical);

        assert!(!abandoned_asm.exists(), "abandoned assembly was retained");
        assert!(!abandoned_object.exists(), "abandoned object was retained");
        assert!(
            !abandoned_integrity.exists(),
            "abandoned integrity temporary was retained"
        );
        assert!(canonical.exists(), "active cache object was removed");
        assert!(unrelated.exists(), "unrelated cache file was removed");
        let _ = fs::remove_dir_all(&root);
    }

    /// Verifies pruning in one compiler process cannot delete a prepared object
    /// that another live compiler process has not consumed at link time yet.
    #[test]
    fn runtime_cache_pruning_preserves_cross_process_prepared_object_lease() {
        const TEST_NAME: &str =
            "runtime_cache_pruning_preserves_cross_process_prepared_object_lease";
        if std::env::var("ELEPHC_RUNTIME_CACHE_LEASE_PROBE").as_deref() != Ok(TEST_NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([TEST_NAME, "--nocapture", "--test-threads=1"])
                .env("ELEPHC_RUNTIME_CACHE_LEASE_PROBE", TEST_NAME)
                .output()
                .expect("spawn isolated runtime-cache lease probe");
            assert!(
                output.status.success(),
                "isolated runtime-cache lease probe failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        if std::env::var("ELEPHC_RUNTIME_CACHE_LEASE_ROLE").as_deref() == Ok("holder") {
            let ready = PathBuf::from(
                std::env::var_os("ELEPHC_RUNTIME_CACHE_LEASE_READY")
                    .expect("lease holder ready path"),
            );
            let release = PathBuf::from(
                std::env::var_os("ELEPHC_RUNTIME_CACHE_LEASE_RELEASE")
                    .expect("lease holder release path"),
            );
            let prepared = prepare_runtime_object(
                7 * 1024 * 1024,
                Target::detect_host(),
                RuntimeFeatures::none(),
                false,
            )
            .expect("prepare runtime object held across another compiler's pruning");
            fs::write(&ready, prepared.path.to_string_lossy().as_bytes())
                .expect("publish held runtime object path");
            for _ in 0..400 {
                if release.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            assert!(release.exists(), "lease holder timed out waiting for release");
            assert!(
                prepared.path.exists(),
                "another compiler pruned a runtime object before its holder linked it"
            );
            return;
        }

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "elephc-runtime-cache-lease-{}-{unique}",
            std::process::id()
        ));
        let cache = root.join("elephc");
        let ready = root.join("holder.ready");
        let release = root.join("holder.release");
        fs::create_dir_all(&cache).expect("create cache-lease fixture");
        std::env::set_var("XDG_CACHE_HOME", &root);

        let mut holder = std::process::Command::new(std::env::current_exe().unwrap())
            .args([TEST_NAME, "--nocapture", "--test-threads=1"])
            .env("ELEPHC_RUNTIME_CACHE_LEASE_PROBE", TEST_NAME)
            .env("ELEPHC_RUNTIME_CACHE_LEASE_ROLE", "holder")
            .env("ELEPHC_RUNTIME_CACHE_LEASE_READY", &ready)
            .env("ELEPHC_RUNTIME_CACHE_LEASE_RELEASE", &release)
            .env("XDG_CACHE_HOME", &root)
            .spawn()
            .expect("spawn runtime-cache lease holder");
        for _ in 0..400 {
            if ready.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert!(ready.exists(), "runtime-cache lease holder did not become ready");
        let held_path = PathBuf::from(
            fs::read_to_string(&ready).expect("read held runtime object path"),
        );

        for identity in 0..12 {
            let object = cache.join(format!(
                "runtime-v0-lease-fixture-rt{identity:016x}-heap1.o"
            ));
            fs::write(&object, format!("object-{identity}"))
                .expect("write competing cache object fixture");
            fs::write(
                cache.join(format!("{}.integrity", object.file_name().unwrap().to_string_lossy())),
                format!("integrity-{identity}"),
            )
            .expect("write competing cache integrity fixture");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let competing = prepare_runtime_object(
            9 * 1024 * 1024,
            Target::detect_host(),
            RuntimeFeatures::none(),
            false,
        )
        .expect("prepare competing runtime while pruning old identities");
        let held_survived = held_path.exists();
        fs::write(&release, b"release").expect("release runtime-cache lease holder");
        let holder_status = holder.wait().expect("wait for runtime-cache lease holder");

        assert!(holder_status.success(), "runtime-cache lease holder failed");
        assert!(
            held_survived,
            "pruning removed another live compiler's prepared runtime object"
        );
        assert!(competing.path.exists(), "competing active runtime object was pruned");
        let _ = fs::remove_dir_all(&root);
    }
