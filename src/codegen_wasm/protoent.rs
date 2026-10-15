//! Purpose:
//! The protocol database on `wasm32-wasi`: `getprotobyname()` and `getprotobynumber()`.
//!
//! Called from:
//! - `plan` lays the two tables out and registers the runtime, for a module that calls either.
//! - `builtins` lowers the two `RuntimeFnId` identities onto the helpers below.
//! - `capability` gates each call against the shapes the helpers read.
//!
//! Key details:
//! - The database is CARRIED, not read. The native backend opens `/etc/protocols`; a WASI
//!   module reaches only what the host preopens, and an absolute path outside a preopen cannot
//!   be opened at all, so a module that tried would answer `false` for everything on every
//!   normal host. What `/etc/protocols` contains is the IANA protocol-number registry, which is
//!   a specification rather than a property of the machine — the same reasoning that makes
//!   `readdir()` report `.` and `..` whatever the host enumerates. Elephc already carries the
//!   IANA timezone list on the same grounds.
//! - Lookup by name is CASE-SENSITIVE and matches the canonical name or an alias. Measured:
//!   `getprotobyname("tcp")` and `("TCP")` are both 6, and `("Tcp")` is `false`. So the key
//!   table carries every spelling, and there is no case folding anywhere.
//! - Lookup by number answers the CANONICAL name — the first field of the row, lowercase —
//!   never the alias.
//! - `getprotobyname("ip")` is `0`, which is not `false`. The answer is boxed so the two are
//!   distinguishable, which is what `=== false` in php asks about.

use crate::ir::{Immediate, Module, Op, RuntimeCallTarget, RuntimeFnId};

use super::wat::{DataSegment, Global, ValType, WatModule};

/// Base of the name-keyed rows: `[name_ptr i32][name_len i32][number i32]`, one per spelling.
const KEYS_GLOBAL: &str = "__protoent_keys";

/// Number of name-keyed rows.
const KEY_COUNT_GLOBAL: &str = "__protoent_key_count";

/// Base of the number-keyed rows: `[number i32][name_ptr i32][name_len i32]`, one per entry.
const NUMS_GLOBAL: &str = "__protoent_nums";

/// Number of number-keyed rows.
const NUM_COUNT_GLOBAL: &str = "__protoent_num_count";

/// Bytes per row in either table.
const ROW_BYTES: u32 = 12;

/// The protocol database this target carries, as `(canonical name, number, aliases)`.
///
/// One row per assigned IANA protocol number, taken from the registry as `/etc/protocols`
/// ships it, restricted to the numbers IANA can actually assign, and then VERIFIED entry by
/// entry against php-src 8.5.10: every one of the 280 lookup keys below answers the number
/// recorded here, and every number answers the canonical name recorded here.
///
/// The restriction is not cosmetic. An IP protocol number is an 8-BIT field, and macOS's
/// `/etc/protocols` carries a BSD-local `divert 258` line that no registry assigns and that a
/// Linux host does not ship. Carrying it would make this module answer 258 for `divert`
/// wherever it ran, including on hosts whose own php answers `false`. A module that runs
/// anywhere carries the standard, not one operating system's additions — so `divert` is the
/// one entry where this target and php-on-macOS disagree, and it disagrees on purpose.
pub(super) const PROTOCOLS: &[(&str, i32, &[&str])] = &[
    ("ip", 0, &["IP"]),
    ("icmp", 1, &["ICMP"]),
    ("igmp", 2, &["IGMP"]),
    ("ggp", 3, &["GGP"]),
    ("ipencap", 4, &["IP-ENCAP"]),
    ("st2", 5, &["ST2"]),
    ("tcp", 6, &["TCP"]),
    ("cbt", 7, &["CBT"]),
    ("egp", 8, &["EGP"]),
    ("igp", 9, &["IGP"]),
    ("bbn-rcc", 10, &["BBN-RCC-MON"]),
    ("nvp", 11, &["NVP-II"]),
    ("pup", 12, &["PUP"]),
    ("argus", 13, &["ARGUS"]),
    ("emcon", 14, &["EMCON"]),
    ("xnet", 15, &["XNET"]),
    ("chaos", 16, &["CHAOS"]),
    ("udp", 17, &["UDP"]),
    ("mux", 18, &["MUX"]),
    ("dcn", 19, &["DCN-MEAS"]),
    ("hmp", 20, &["HMP"]),
    ("prm", 21, &["PRM"]),
    ("xns-idp", 22, &["XNS-IDP"]),
    ("trunk-1", 23, &["TRUNK-1"]),
    ("trunk-2", 24, &["TRUNK-2"]),
    ("leaf-1", 25, &["LEAF-1"]),
    ("leaf-2", 26, &["LEAF-2"]),
    ("rdp", 27, &["RDP"]),
    ("irtp", 28, &["IRTP"]),
    ("iso-tp4", 29, &["ISO-TP4"]),
    ("netblt", 30, &["NETBLT"]),
    ("mfe-nsp", 31, &["MFE-NSP"]),
    ("merit-inp", 32, &["MERIT-INP"]),
    ("dccp", 33, &["DCCP"]),
    ("3pc", 34, &["3PC"]),
    ("idpr", 35, &["IDPR"]),
    ("xtp", 36, &["XTP"]),
    ("ddp", 37, &["DDP"]),
    ("idpr-cmtp", 38, &["IDPR-CMTP"]),
    ("tp++", 39, &["TP++"]),
    ("il", 40, &["IL"]),
    ("ipv6", 41, &["IPV6"]),
    ("sdrp", 42, &["SDRP"]),
    ("ipv6-route", 43, &["IPV6-ROUTE"]),
    ("ipv6-frag", 44, &["IPV6-FRAG"]),
    ("idrp", 45, &["IDRP"]),
    ("rsvp", 46, &["RSVP"]),
    ("gre", 47, &["GRE"]),
    ("dsr", 48, &["DSR"]),
    ("bna", 49, &["BNA"]),
    ("esp", 50, &["ESP"]),
    ("ah", 51, &["AH"]),
    ("i-nlsp", 52, &["I-NLSP"]),
    ("swipe", 53, &["SWIPE"]),
    ("narp", 54, &["NARP"]),
    ("mobile", 55, &["MOBILE"]),
    ("tlsp", 56, &["TLSP"]),
    ("skip", 57, &["SKIP"]),
    ("ipv6-icmp", 58, &["IPV6-ICMP", "icmp6"]),
    ("ipv6-nonxt", 59, &["IPV6-NONXT"]),
    ("ipv6-opts", 60, &["IPV6-OPTS"]),
    ("cftp", 62, &["CFTP"]),
    ("sat-expak", 64, &["SAT-EXPAK"]),
    ("kryptolan", 65, &["KRYPTOLAN"]),
    ("rvd", 66, &["RVD"]),
    ("ippc", 67, &["IPPC"]),
    ("sat-mon", 69, &["SAT-MON"]),
    ("visa", 70, &["VISA"]),
    ("ipcv", 71, &["IPCV"]),
    ("cpnx", 72, &["CPNX"]),
    ("cphb", 73, &["CPHB"]),
    ("wsn", 74, &["WSN"]),
    ("pvp", 75, &["PVP"]),
    ("br-sat-mon", 76, &["BR-SAT-MON"]),
    ("sun-nd", 77, &["SUN-ND"]),
    ("wb-mon", 78, &["WB-MON"]),
    ("wb-expak", 79, &["WB-EXPAK"]),
    ("iso-ip", 80, &["ISO-IP"]),
    ("vmtp", 81, &["VMTP"]),
    ("secure-vmtp", 82, &["SECURE-VMTP"]),
    ("vines", 83, &["VINES"]),
    ("ttp", 84, &["TTP"]),
    ("nsfnet-igp", 85, &["NSFNET-IGP"]),
    ("dgp", 86, &["DGP"]),
    ("tcf", 87, &["TCF"]),
    ("eigrp", 88, &["EIGRP"]),
    ("ospf", 89, &["OSPFIGP"]),
    ("sprite-rpc", 90, &["Sprite-RPC"]),
    ("larp", 91, &["LARP"]),
    ("mtp", 92, &["MTP"]),
    ("ax.25", 93, &["AX.25"]),
    ("ipip", 94, &["IPIP"]),
    ("micp", 95, &["MICP"]),
    ("scc-sp", 96, &["SCC-SP"]),
    ("etherip", 97, &["ETHERIP"]),
    ("encap", 98, &["ENCAP"]),
    ("gmtp", 100, &["GMTP"]),
    ("ifmp", 101, &["IFMP"]),
    ("pnni", 102, &["PNNI"]),
    ("pim", 103, &["PIM"]),
    ("aris", 104, &["ARIS"]),
    ("scps", 105, &["SCPS"]),
    ("qnx", 106, &["QNX"]),
    ("a/n", 107, &["A/N"]),
    ("ipcomp", 108, &["IPComp"]),
    ("snp", 109, &["SNP"]),
    ("compaq-peer", 110, &["Compaq-Peer"]),
    ("ipx-in-ip", 111, &["IPX-in-IP"]),
    ("carp", 112, &["CARP", "vrrp"]),
    ("pgm", 113, &["PGM"]),
    ("l2tp", 115, &["L2TP"]),
    ("ddx", 116, &["DDX"]),
    ("iatp", 117, &["IATP"]),
    ("stp", 118, &["STP"]),
    ("srp", 119, &["SRP"]),
    ("uti", 120, &["UTI"]),
    ("smp", 121, &["SMP"]),
    ("sm", 122, &["SM"]),
    ("ptp", 123, &["PTP"]),
    ("isis", 124, &["ISIS"]),
    ("fire", 125, &["FIRE"]),
    ("crtp", 126, &["CRTP"]),
    ("crudp", 127, &["CRUDP"]),
    ("sscopmce", 128, &["SSCOPMCE"]),
    ("iplt", 129, &["IPLT"]),
    ("sps", 130, &["SPS"]),
    ("pipe", 131, &["PIPE"]),
    ("sctp", 132, &["SCTP"]),
    ("fc", 133, &["FC"]),
    ("rsvp-e2e-ignore", 134, &["RSVP-E2E-IGNORE"]),
    ("mobility-header", 135, &["Mobility-Header"]),
    ("udplite", 136, &["UDPLite"]),
    ("mpls-in-ip", 137, &["MPLS-IN-IP"]),
    ("manet", 138, &["MANET"]),
    ("hip", 139, &["HIP"]),
    ("shim6", 140, &["SHIM6"]),
    ("wesp", 141, &["WESP"]),
    ("rohc", 142, &["ROHC"]),
    ("pfsync", 240, &["PFSYNC"]),
];

/// Whether any instruction in the module consults the protocol database.
///
/// The gate for laying the tables out and registering the runtime: a module that never asks
/// keeps the static data it had, which is about 3 KiB of names and rows.
pub(super) fn module_uses_protocols(module: &Module) -> bool {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .any(|function| {
            function.instructions.iter().any(|inst| {
                inst.op == Op::RuntimeCall
                    && matches!(
                        inst.immediate.as_ref(),
                        Some(Immediate::RuntimeCall(
                            RuntimeCallTarget::Function(
                                RuntimeFnId::Getprotobyname | RuntimeFnId::Getprotobynumber
                            ) | RuntimeCallTarget::ProfiledFunction {
                                target: RuntimeFnId::Getprotobyname
                                    | RuntimeFnId::Getprotobynumber,
                                ..
                            }
                        ))
                    )
            })
        })
}

/// Lays the name bytes and the two row tables out, and declares their globals.
///
/// Must run before `heap_base` is computed, like every other static table, so the rows sit
/// below the bump allocator. Returns the advanced data cursor.
pub(super) fn emit_protocol_tables(wm: &mut WatModule, module: &Module, mut cursor: u32) -> u32 {
    cursor = (cursor + 3) & !3;
    if !module_uses_protocols(module) {
        for name in [KEYS_GLOBAL, KEY_COUNT_GLOBAL, NUMS_GLOBAL, NUM_COUNT_GLOBAL] {
            wm.add_global(Global {
                name: name.to_string(),
                ty: ValType::I32,
                mutable: false,
                init: 0,
            });
        }
        return cursor;
    }

    // Every spelling's bytes, once each. The canonical name is also a key, so one pass over
    // the table places each string exactly once and both row tables point into it.
    let mut placed: Vec<(&str, u32)> = Vec::new();
    for (canonical, _, aliases) in PROTOCOLS {
        for spelling in std::iter::once(canonical).chain(aliases.iter()) {
            if placed.iter().any(|(name, _)| name == spelling) {
                continue;
            }
            wm.add_data(DataSegment {
                offset: cursor,
                bytes: spelling.as_bytes().to_vec(),
            });
            placed.push((spelling, cursor));
            cursor += spelling.len() as u32;
        }
    }
    let offset_of = |wanted: &str| {
        placed
            .iter()
            .find(|(name, _)| *name == wanted)
            .map(|(_, offset)| *offset)
            .expect("every spelling was placed above")
    };

    cursor = (cursor + 3) & !3;
    let keys_off = cursor;
    let mut keys: Vec<u8> = Vec::new();
    let mut key_count = 0u32;
    for (canonical, number, aliases) in PROTOCOLS {
        for spelling in std::iter::once(canonical).chain(aliases.iter()) {
            keys.extend_from_slice(&(offset_of(spelling) as i32).to_le_bytes());
            keys.extend_from_slice(&(spelling.len() as i32).to_le_bytes());
            keys.extend_from_slice(&number.to_le_bytes());
            key_count += 1;
        }
    }
    wm.add_data(DataSegment {
        offset: keys_off,
        bytes: keys,
    });
    cursor += key_count * ROW_BYTES;

    let nums_off = cursor;
    let mut nums: Vec<u8> = Vec::new();
    for (canonical, number, _) in PROTOCOLS {
        nums.extend_from_slice(&number.to_le_bytes());
        nums.extend_from_slice(&(offset_of(canonical) as i32).to_le_bytes());
        nums.extend_from_slice(&(canonical.len() as i32).to_le_bytes());
    }
    wm.add_data(DataSegment {
        offset: nums_off,
        bytes: nums,
    });
    cursor += PROTOCOLS.len() as u32 * ROW_BYTES;

    for (name, init) in [
        (KEYS_GLOBAL, keys_off as i64),
        (KEY_COUNT_GLOBAL, key_count as i64),
        (NUMS_GLOBAL, nums_off as i64),
        (NUM_COUNT_GLOBAL, PROTOCOLS.len() as i64),
    ] {
        wm.add_global(Global {
            name: name.to_string(),
            ty: ValType::I32,
            mutable: false,
            init,
        });
    }
    cursor
}

/// Adds the two lookups to `wm`. They reference only `__rt_mixed_from_value` and the globals
/// above, so they can be registered anywhere after the mixed runtime.
pub(super) fn emit_protocol_runtime(wm: &mut WatModule) {
    wm.add_raw_func(RT_GETPROTOBYNAME);
    wm.add_raw_func(RT_GETPROTOBYNUMBER);
}

/// `__rt_getprotobyname`: a spelling to its number, boxed as php's `int|false`.
///
/// The comparison is byte-exact, because php's is: the row table carries `tcp` and `TCP` as
/// separate keys and answers `false` for `Tcp`. A zero-length name matches nothing, which is
/// also php's answer for `getprotobyname("")`.
const RT_GETPROTOBYNAME: &str = r#"(func $__rt_getprotobyname (param $ptr i32) (param $len i64) (result i32)
  (local $i i32) (local $row i32) (local $rlen i32) (local $rptr i32) (local $j i32)
  (if (i64.eqz (local.get $len))
    (then (return (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))))
  (block $done (loop $scan
    (br_if $done (i32.ge_u (local.get $i) (global.get $__protoent_key_count)))
    (local.set $row (i32.add (global.get $__protoent_keys) (i32.mul (local.get $i) (i32.const 12))))
    (local.set $rlen (i32.load offset=4 (local.get $row)))
    (if (i32.eq (local.get $rlen) (i32.wrap_i64 (local.get $len))) (then
      (local.set $rptr (i32.load (local.get $row)))
      (local.set $j (i32.const 0))
      (block $ne (block $eq (loop $bytes
        (br_if $eq (i32.ge_u (local.get $j) (local.get $rlen)))       ;; ran out of bytes: equal
        (br_if $ne (i32.ne (i32.load8_u (i32.add (local.get $rptr) (local.get $j)))
                           (i32.load8_u (i32.add (local.get $ptr) (local.get $j)))))
        (local.set $j (i32.add (local.get $j) (i32.const 1)))
        (br $bytes)))
        (return (call $__rt_mixed_from_value (i64.const 0)
          (i64.extend_i32_s (i32.load offset=8 (local.get $row))) (i64.const 0))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))
"#;

/// `__rt_getprotobynumber`: a number to its canonical name, boxed as php's `string|false`.
///
/// A number outside the table — negative, past 255, or an unassigned value the registry skips
/// — is `false`, which is what php answers for a number `/etc/protocols` has no line for.
/// `__rt_mixed_from_value` persists its own copy of the name, so handing it a pointer into
/// static data is safe.
const RT_GETPROTOBYNUMBER: &str = r#"(func $__rt_getprotobynumber (param $number i64) (result i32)
  (local $i i32) (local $row i32)
  (block $done (loop $scan
    (br_if $done (i32.ge_u (local.get $i) (global.get $__protoent_num_count)))
    (local.set $row (i32.add (global.get $__protoent_nums) (i32.mul (local.get $i) (i32.const 12))))
    (if (i64.eq (i64.extend_i32_s (i32.load (local.get $row))) (local.get $number)) (then
      (return (call $__rt_mixed_from_value (i64.const 1)
        (i64.extend_i32_u (i32.load offset=4 (local.get $row)))
        (i64.extend_i32_u (i32.load offset=8 (local.get $row)))))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $scan)))
  (call $__rt_mixed_from_value (i64.const 3) (i64.const 0) (i64.const 0)))
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// The carried table is the registry's, and it is internally consistent.
    ///
    /// The row-by-row comparison against php-src happens outside the test suite (php is not a
    /// build dependency); what is pinned here is everything a later edit could break without
    /// php noticing: a number claimed twice would make `getprotobynumber` answer whichever row
    /// came first, and a key claimed twice with different numbers would do the same to
    /// `getprotobyname`.
    #[test]
    fn protocol_table_agrees_with_the_registry() {
        // Both identities are served by this table, so a change here is a change to both.
        for target in [RuntimeFnId::Getprotobyname, RuntimeFnId::Getprotobynumber] {
            assert!(
                crate::codegen_wasm::capability::runtime_function_is_supported(target),
                "{target:?} must be supported on this target"
            );
        }

        assert_eq!(PROTOCOLS.len(), 139, "one row per assigned protocol number");

        let mut numbers: Vec<i32> = PROTOCOLS.iter().map(|(_, number, _)| *number).collect();
        numbers.sort_unstable();
        let unique = numbers.len();
        numbers.dedup();
        assert_eq!(numbers.len(), unique, "a number must name exactly one protocol");
        assert!(
            numbers.iter().all(|number| (0..=255).contains(number)),
            "a protocol number is an 8-bit field"
        );

        // Every spelling resolves to exactly one number, canonical names and aliases alike.
        let mut keys: Vec<(&str, i32)> = Vec::new();
        for (canonical, number, aliases) in PROTOCOLS {
            for spelling in std::iter::once(canonical).chain(aliases.iter()) {
                assert!(
                    !keys.iter().any(|(name, _)| name == spelling),
                    "{spelling:?} is claimed twice"
                );
                keys.push((spelling, *number));
            }
        }
        assert_eq!(keys.len(), 280, "every spelling the registry lists is a key");

        // The three the example asks about, and the one that is zero rather than false.
        for (name, number) in [("tcp", 6), ("udp", 17), ("icmp", 1), ("ip", 0), ("TCP", 6)] {
            assert_eq!(
                keys.iter().find(|(key, _)| *key == name).map(|(_, n)| *n),
                Some(number),
                "{name} must answer {number}"
            );
        }
        // Case folding is NOT php's rule here: only the spellings the registry lists match.
        assert!(!keys.iter().any(|(key, _)| *key == "Tcp"));
    }
}
