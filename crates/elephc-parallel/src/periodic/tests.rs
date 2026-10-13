//! Purpose:
//! Tests periodic cadence deterministically and native wake/identity ownership through the C ABI.
//!
//! Called from:
//! - `super` unit tests for the optional periodic bridge.
//!
//! Key details:
//! - Synthetic monotonic instants avoid wall-clock scheduling assumptions.
//! - Commands may originate on another OS thread but exchange scalar identities only.

use super::*;
use std::time::Duration;

/// Invalid, sub-resolution and out-of-range intervals never allocate a registration.
#[test]
fn periodic_interval_validation_is_bounded() {
    for value in [0.0, -1.0, f64::NAN, f64::INFINITY, 1e-12, f64::MAX] {
        assert!(cadence::interval(value).is_none(), "{value}");
    }
    assert_eq!(cadence::interval(0.001), Some(Duration::from_millis(1)));
}

/// The first invocation is delayed and a claim cannot overlap an already active invocation.
#[test]
fn periodic_first_deadline_and_non_overlap() {
    let now = Instant::now();
    let mut cadence = Cadence::new(Duration::from_millis(10), now).unwrap();
    assert_eq!(cadence.claim(now), 0);
    assert_eq!(cadence.claim(now + Duration::from_millis(10)), 1);
    assert_eq!(cadence.claim(now + Duration::from_secs(1)), 2);
}

/// Completion skips missed slots and preserves the original fixed cadence without a catch-up burst.
#[test]
fn periodic_skips_missed_deadlines() {
    let now = Instant::now();
    let mut cadence = Cadence::new(Duration::from_millis(10), now).unwrap();
    assert_eq!(cadence.claim(now + Duration::from_millis(10)), 1);
    assert!(cadence.complete(now + Duration::from_millis(37)));
    assert_eq!(cadence.remaining(now + Duration::from_millis(37)), 0.003);
    assert_eq!(cadence.claim(now + Duration::from_millis(39)), 0);
    assert_eq!(cadence.claim(now + Duration::from_millis(40)), 1);
}

/// An idle change rearms from the command, but repeating the same value does not delay admission.
#[test]
fn periodic_idle_change_and_same_interval() {
    let now = Instant::now();
    let mut cadence = Cadence::new(Duration::from_millis(10), now).unwrap();
    assert!(cadence.set_interval(Duration::from_millis(20), now + Duration::from_millis(5)));
    assert!(cadence.set_interval(Duration::from_millis(20), now + Duration::from_millis(8)));
    assert_eq!(cadence.claim(now + Duration::from_millis(24)), 0);
    assert_eq!(cadence.claim(now + Duration::from_millis(25)), 1);
}

/// A change during execution starts the new interval only after that invocation has settled.
#[test]
fn periodic_active_change_rearms_from_completion() {
    let now = Instant::now();
    let mut cadence = Cadence::new(Duration::from_millis(10), now).unwrap();
    assert_eq!(cadence.claim(now + Duration::from_millis(10)), 1);
    assert!(cadence.set_interval(Duration::from_millis(20), now + Duration::from_millis(15)));
    assert!(cadence.complete(now + Duration::from_millis(30)));
    assert_eq!(cadence.claim(now + Duration::from_millis(49)), 0);
    assert_eq!(cadence.claim(now + Duration::from_millis(50)), 1);
}

/// Stop dominates an earlier interval change and cannot be reversed by another setter.
#[test]
fn periodic_stop_dominates_and_allows_settlement() {
    let now = Instant::now();
    let mut cadence = Cadence::new(Duration::from_millis(10), now).unwrap();
    assert_eq!(cadence.claim(now + Duration::from_millis(10)), 1);
    assert!(cadence.set_interval(Duration::from_millis(20), now + Duration::from_millis(12)));
    cadence.stop();
    assert!(!cadence.set_interval(Duration::from_millis(1), now));
    assert!(cadence.complete(now + Duration::from_millis(15)));
    assert_eq!(cadence.claim(now + Duration::from_secs(1)), 2);
    assert!(!cadence.complete(now));
}

/// Retired identities fail closed even when a later registration reuses the same OS descriptor.
#[test]
fn periodic_registration_retirement_never_reuses_ids() {
    let mut registry = Registry::default();
    let now = Instant::now();
    let first = registry.create(1.0, now);
    assert!(first > 0);
    assert_eq!(registry.release(first), 1);
    let second = registry.create(1.0, now);
    assert!(second > first);
    assert_eq!(registry.release(first), -1);
    registry.next_id = i64::MAX;
    assert_eq!(registry.create(1.0, now), -1);
    assert_eq!(registry.release(second), 1);
}

/// Active admission pins native state and endpoints until explicit completion.
#[test]
fn periodic_release_waits_for_active_invocation() {
    let mut registry = Registry::default();
    let now = Instant::now();
    let id = registry.create(0.001, now);
    assert_eq!(registry.entries.get_mut(&id).unwrap().cadence.claim(now + Duration::from_millis(1)), 1);
    assert_eq!(registry.release(id), 0);
    assert!(registry.entries.get(&id).unwrap().wake.fd() >= 0);
    assert!(registry.entries.get_mut(&id).unwrap().cadence.complete(now + Duration::from_millis(2)));
    assert_eq!(registry.release(id), 1);
}

/// Cross-thread interval/stop commands wake only their registration and use no PHP-owned data.
#[test]
fn periodic_scalar_commands_wake_independently_across_threads() {
    let first = elephc_task_periodic_create(1.0);
    let second = elephc_task_periodic_create(1.0);
    assert!(first > 0 && second > first);
    assert_eq!(elephc_task_periodic_drain(first), 0);
    std::thread::spawn(move || {
        assert_eq!(elephc_task_periodic_set_interval(first, 2.0), 1);
        assert_eq!(elephc_task_periodic_stop(first), 1);
    }).join().unwrap();
    assert_eq!(elephc_task_periodic_drain(first), 1);
    assert_eq!(elephc_task_periodic_drain(first), 0);
    assert_eq!(elephc_task_periodic_drain(second), 0);
    assert_eq!(elephc_task_periodic_claim(first), 2);
    assert_eq!(elephc_task_periodic_set_interval(first, 3.0), -1);
    assert_eq!(elephc_task_periodic_release(first), 1);
    assert_eq!(elephc_task_periodic_fd(first), -1);
    assert_eq!(elephc_task_periodic_stop(first), -1);
    assert_eq!(elephc_task_periodic_release(second), 1);
}

/// Different registrations use different deadlines even when they share one coordinator.
#[test]
fn periodic_registered_intervals_are_independent() {
    let mut registry = Registry::default();
    let now = Instant::now();
    let fast = registry.create(0.001, now);
    let slow = registry.create(0.020, now);
    assert_eq!(registry.entries.get_mut(&fast).unwrap().cadence.claim(now + Duration::from_millis(1)), 1);
    assert_eq!(registry.entries.get_mut(&slow).unwrap().cadence.claim(now + Duration::from_millis(1)), 0);
    registry.entries.get_mut(&fast).unwrap().cadence.stop();
    assert!(registry.entries.get_mut(&fast).unwrap().cadence.complete(now + Duration::from_millis(2)));
    assert_eq!(registry.entries.get_mut(&slow).unwrap().cadence.claim(now + Duration::from_millis(20)), 1);
    assert!(registry.entries.get_mut(&slow).unwrap().cadence.complete(now + Duration::from_millis(22)));
    assert_eq!(registry.release(fast), 1);
    assert_eq!(registry.release(slow), 1);
}

/// Exported IDs/statuses stay in PHP's signed 64-bit integer ABI and timers return double seconds.
#[test]
fn periodic_exported_abi_and_invalid_handles() {
    let create: extern "C" fn(f64) -> i64 = elephc_task_periodic_create;
    let remaining: extern "C" fn(i64) -> f64 = elephc_task_periodic_remaining;
    assert_eq!(create(0.0), -2);
    assert_eq!(remaining(-1), -1.0);
    assert_eq!(elephc_task_periodic_claim(-1), -1);
    assert_eq!(elephc_task_periodic_complete(-1), -1);
    assert_eq!(elephc_task_periodic_release(-1), -1);
}
