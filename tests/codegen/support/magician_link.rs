//! Purpose:
//! Mirrors Magician's Apple SDK library requirement in the codegen test linker.
//!
//! Called from:
//! - `super::runner::test_link_plan` after collecting bridge and native archives.
//!
//! Key details:
//! - Magician embeds iconv callers even when the PHP checker did not request iconv.
//! - Matches `src/linker/bridges.rs` by placing one Apple-only iconv after archives.

use elephc::codegen::platform::Platform;
use elephc::link_plan::{LinkItem, LinkPlan};

/// Adds Magician's Apple SDK dependency while retaining the order of other inputs.
pub(super) fn with_apple_libraries(plan: LinkPlan, platform: Platform) -> LinkPlan {
    if platform != Platform::MacOS || !plan.items().iter().any(|item| {
        matches!(item, LinkItem::NamedLibrary { name, .. } if name == "elephc_magician")
    }) {
        return plan;
    }
    let mut items: Vec<_> = plan.items().iter().filter(|item| {
        !matches!(item, LinkItem::NamedLibrary { name, .. } if name == "iconv")
    }).cloned().collect();
    items.push(LinkItem::named_runtime("iconv"));
    LinkPlan::from_items(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc::codegen::platform::Target;

    /// Builds an eval-only plan whose checker did not request the iconv library.
    fn eval_plan() -> LinkPlan {
        LinkPlan::from_items(vec![
            LinkItem::named_runtime("elephc_magician"),
            LinkItem::managed_archive("/cache/native.a", "native"),
        ])
    }

    /// All Apple targets need iconv after archives, while both Linux targets use libc.
    #[test]
    fn magician_apple_libraries_cover_all_supported_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let platform = Target::parse(name).unwrap().platform;
            let plan = with_apple_libraries(eval_plan(), platform);
            let iconv: Vec<_> = plan.items().iter().enumerate().filter(|(_, item)| {
                matches!(item, LinkItem::NamedLibrary { name, .. } if name == "iconv")
            }).map(|(index, _)| index).collect();
            if platform == Platform::MacOS {
                assert_eq!(iconv, vec![2], "{name}: {:?}", plan.items());
            } else {
                assert!(iconv.is_empty(), "{name}: {:?}", plan.items());
                assert_eq!(plan, eval_plan());
            }
        }
    }

    /// Existing checker and XML iconv inputs collapse to one dependency after archives.
    #[test]
    fn existing_iconv_is_deduplicated_after_archives() {
        let mut items = vec![LinkItem::named_extern("iconv")];
        items.extend(eval_plan().items().iter().cloned());
        items.push(LinkItem::named_runtime("iconv"));
        let plan = with_apple_libraries(LinkPlan::from_items(items), Platform::MacOS);
        let mut expected = eval_plan();
        expected.push(LinkItem::named_runtime("iconv"));
        assert_eq!(plan, expected);
    }

    /// Other programs keep their original library order and Linux linking mode.
    #[test]
    fn plans_without_magician_are_preserved() {
        let plan = LinkPlan::from_items(vec![
            LinkItem::named_extern("iconv"),
            LinkItem::managed_archive("/cache/native.a", "native"),
        ]);
        assert_eq!(with_apple_libraries(plan.clone(), Platform::MacOS), plan);
    }
}
