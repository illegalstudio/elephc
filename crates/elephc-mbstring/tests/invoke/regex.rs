//! Purpose:
//! Verifies protected mbregex warning callbacks, nested calls, and pending-error cleanup.
//!
//! Called from:
//! - The focused shared invocation test binary with a managed Oniguruma prefix.
//!
//! Key details:
//! - Callback handlers use the real shared operation engine and retain independent host owners.
//! - Pending PHP exceptions outrank later release failures and ordinary false results.

use super::*;

/// Reenters the actual coordinator from native compilation warnings and injects callback/release failures.
#[test]
#[ignore = "requires a managed Oniguruma test prefix or pinned 6.9.10 native development files"]
fn mbstring_invoke_regex_warning_reentry_and_cleanup() {
    assert_eq!(unsafe { elephc_mbstring_regex_provider_v1(&regex_provider::provider()) }, 0);
    let values = [string(b"["), string(b"a")];
    elephc_mbstring_reset_v1();
    call(RuntimeBuiltinId::MbRegexSetOptions, &[MbArgV1::string(b"i")]);
    let mut host = Host::new("regex_reentry");
    assert_eq!(run(RuntimeBuiltinId::MbEregMatch, &values, false, &mut host), (0, json!(["bool", false])));
    assert_eq!(host.trace, vec![
        json!(["diagnostic", 2, hex(b"mb_ereg_match(): mbregex compile err: premature end of char-class")]),
        json!(["nested_match", 0, ["bool", false]]),
    ]);
    assert_eq!(call(RuntimeBuiltinId::MbRegexEncoding, &[]), json!(["string", hex(b"ASCII")]));
    assert_eq!(call(RuntimeBuiltinId::MbRegexSetOptions, &[]), json!(["string", hex(b"r")]));
    assert!(host.live.is_empty() && host.errors.is_empty(), "{:?}", host.errors);
    for handler in ["throw", ""] {
        for (callback, occurrence) in [("diagnostic", 1), ("release", 1), ("release", 2)] {
            for status in [1, 2, -1] {
                elephc_mbstring_reset_v1();
                let mut host = Host::new(handler);
                host.fault = Some(Fault { callback, occurrence, status, malformed: false });
                let (actual, _) = run(RuntimeBuiltinId::MbEregMatch, &values, false, &mut host);
                assert_eq!(actual, if handler == "throw" || status == 2 { 2 } else { 1 }, "{handler} {callback} {occurrence} {status}");
                assert!(host.live.is_empty() && host.errors.is_empty(), "{handler} {callback}: {:?}", host.errors);
            }
        }
    }
}
