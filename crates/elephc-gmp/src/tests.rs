//! Purpose:
//! Pins the bridge's operation table to values produced by PHP 8.5.10 + libgmp 6.3.0.
//!
//! Called from:
//! - `cargo test -p elephc-gmp` with `ELEPHC_LIBGMP_DIR` set to a built libgmp.
//!
//! Key details:
//! - The whole module is compiled only with `cfg(elephc_gmp_native)`, because the
//!   symbols are unresolved without a real libgmp archive.
//! - Random tests replay the exact call sequence of php-src's
//!   `ext/gmp/tests/gmp_random_seed.phpt`, which is what pins GMP's generator
//!   stream and `mpz_urandomm` sampling.
//! - Packet assertions check the outcome prefix as well as the payload, so a
//!   silently converted error cannot pass as a value.

#[cfg(elephc_gmp_native)]
mod native {
    use crate::abi::*;
    use crate::ops::*;

    /// Extracts the payload of a successful packet.
    fn ok_packet(packet: String) -> String {
        assert!(packet.starts_with('O'), "expected success packet, got {packet:?}");
        packet[1..].to_string()
    }

    /// Calls a string-returning extern and copies its thread-local result.
    unsafe fn call_string(pointer: *const std::os::raw::c_char) -> String {
        if pointer.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(pointer).to_string_lossy().into_owned()
    }

    /// Verifies arithmetic, bitwise semantics on negatives, and division modes.
    #[test]
    fn arithmetic_and_bitwise_match_gmp() {
        assert_eq!(ok_packet(binary(BIN_ADD, "42", "17")), "59");
        assert_eq!(ok_packet(binary(BIN_SUB, "42", "17")), "25");
        assert_eq!(ok_packet(binary(BIN_MUL, "42", "17")), "714");
        assert_eq!(ok_packet(binary(BIN_TDIV_Q, "42", "17")), "2");
        assert_eq!(ok_packet(binary(BIN_MOD, "42", "17")), "8");
        assert_eq!(ok_packet(binary(BIN_MOD, "-42", "17")), "9");
        assert_eq!(ok_packet(binary(BIN_AND, "-42", "17")), "16");
        assert_eq!(ok_packet(binary(BIN_OR, "-42", "17")), "-41");
        assert_eq!(ok_packet(binary(BIN_XOR, "-42", "17")), "-57");
        assert_eq!(ok_packet(binary(BIN_DIVEXACT, "714", "17")), "42");
        assert_eq!(ok_packet(binary(BIN_ADD, "9223372036854775807", "1")), "9223372036854775808");
    }

    /// Verifies the three division roundings and the packed quotient/remainder shape.
    #[test]
    fn division_modes_return_php_pairs() {
        assert_eq!(ok_packet(binary(BIN_DIV_QR_TDIV, "-7", "2")), "-3|-1");
        assert_eq!(ok_packet(binary(BIN_DIV_QR_CDIV, "-7", "2")), "-3|-1");
        assert_eq!(ok_packet(binary(BIN_DIV_QR_FDIV, "-7", "2")), "-4|1");
        assert_eq!(binary(BIN_TDIV_Q, "1", "0"), "ZDivision by zero");
        assert_eq!(binary(BIN_MOD, "1", "0"), "ZModulo by zero");
    }

    /// Verifies number theory operations and multi-value gcdext order.
    #[test]
    fn number_theory_matches_php_vectors() {
        assert_eq!(ok_packet(binary(BIN_GCD, "42", "30")), "6");
        assert_eq!(ok_packet(binary(BIN_LCM, "42", "30")), "210");
        assert_eq!(ok_packet(binary(BIN_GCDEXT, "42", "30")), "6|-2|3");
        assert_eq!(ok_packet(binary(BIN_INVERT, "3", "11")), "4");
        assert_eq!(binary(BIN_INVERT, "2", "4"), "F");
        assert_eq!(binary(BIN_INVERT, "1", "0"), "ZDivision by zero");
        assert_eq!(ok_packet(binary(BIN_JACOBI, "5", "21")), "1");
        assert_eq!(ok_packet(binary(BIN_LEGENDRE, "5", "11")), "1");
        assert_eq!(ok_packet(binary(BIN_KRONECKER, "5", "21")), "1");
        assert_eq!(ok_packet(binary(BIN_HAMDIST, "1231231", "0")), "13");
        assert_eq!(ok_packet(binary(BIN_HAMDIST, "1231231", "-1")), "-1");
    }

    /// Verifies unary operations, roots, binomial and negative inputs.
    #[test]
    fn unary_operations_match_php_vectors() {
        assert_eq!(ok_packet(unary(UN_NEG, "42", 0)), "-42");
        assert_eq!(ok_packet(unary(UN_ABS, "-42", 0)), "42");
        assert_eq!(ok_packet(unary(UN_COM, "42", 0)), "-43");
        assert_eq!(ok_packet(unary(UN_NEXTPRIME, "100", 0)), "101");
        assert_eq!(ok_packet(unary(UN_SQRT, "17", 0)), "4");
        assert_eq!(ok_packet(unary(UN_ROOT, "28", 3)), "3");
        assert_eq!(ok_packet(unary(UN_ROOTREM, "28", 3)), "3|1");
        assert_eq!(ok_packet(unary(UN_SQRTREM, "17", 0)), "4|1");
        assert_eq!(ok_packet(unary(UN_FACT, "10", 0)), "3628800");
        assert_eq!(ok_packet(unary(UN_BINOMIAL, "10", 3)), "120");
        assert_eq!(ok_packet(unary(UN_PROB_PRIME, "17", 10)), "2");
        assert_eq!(ok_packet(unary(UN_PROB_PRIME, "10", 10)), "0");
        assert_eq!(ok_packet(unary(UN_PERFECT_SQUARE, "16", 0)), "1");
        assert_eq!(ok_packet(unary(UN_PERFECT_POWER, "27", 0)), "1");
        assert_eq!(ok_packet(unary(UN_SIGN, "-42", 0)), "-1");
        assert_eq!(ok_packet(unary(UN_POPCOUNT, "-1", 0)), "-1");
        assert_eq!(ok_packet(unary(UN_SCAN0, "8", 0)), "0");
        assert_eq!(ok_packet(unary(UN_SCAN1, "8", 0)), "3");
        assert_eq!(ok_packet(unary(UN_TESTBIT, "8", 3)), "1");
    }

    /// Verifies powers and shifts, including arithmetic floor shift of negatives.
    #[test]
    fn powers_and_shifts_match_gmp() {
        assert_eq!(ok_packet(unary(UN_POW, "42", 17)), "3937657486715347520027492352");
        assert_eq!(ok_packet(unary(UN_SHL, "42", 17)), "5505024");
        assert_eq!(ok_packet(unary(UN_SHR, "-42", 2)), "-11");
        assert_eq!(ok_packet(powm("4", "13", "497")), "445");        assert_eq!(powm("4", "-1", "497"), "Vmust be greater than or equal to 0");
        assert_eq!(powm("4", "1", "0"), "ZModulo by zero");
    }

    /// Verifies setbit/clrbit mutate the returned value.
    #[test]
    fn bits_mutate_like_php() {
        assert_eq!(ok_packet(bit("0", 10, true)), "1024");
        assert_eq!(ok_packet(bit("1024", 10, false)), "0");
        assert_eq!(bit("0", -1, true), "Vmust be between 0 and 2147483647 * 64");
    }

    /// Verifies PHP's base-0 prefixes and negative output bases.
    #[test]
    fn parsing_and_formatting_follow_php() {
        unsafe {
            assert_eq!(ok_packet(call_string(elephc_gmp_parse(c_str("0x2A"), 0))), "42");
            assert_eq!(ok_packet(call_string(elephc_gmp_parse(c_str("  42"), 0))), "42");
            assert_eq!(ok_packet(call_string(elephc_gmp_parse(c_str("0b101"), 0))), "5");
            assert_eq!(ok_packet(call_string(elephc_gmp_parse(c_str("0o52"), 0))), "42");
            assert_eq!(call_string(elephc_gmp_parse(c_str("42.0"), 0)), "VNumber is not an integer string");
            assert_eq!(ok_packet(call_string(elephc_gmp_strval(c_str("255"), 16))), "ff");
            assert_eq!(ok_packet(call_string(elephc_gmp_strval(c_str("255"), -16))), "FF");
            assert_eq!(elephc_gmp_intval(c_str("-9223372036854775808")), i64::MIN);
            assert_eq!(elephc_gmp_double(c_str("42")), 42.0);
            assert_eq!(elephc_gmp_fits_slong(c_str("9223372036854775807")), 1);
            assert_eq!(elephc_gmp_fits_slong(c_str("9223372036854775808")), 0);
            assert_eq!(call_string(elephc_gmp_version()), "6.3.0");
        }
    }

    /// Verifies import/export hex transport and word-order flags against php-src.
    #[test]
    fn import_export_match_php_flags() {
        // php -r '$n=gmp_init(0x1234); echo bin2hex(gmp_export($n, 2, GMP_MSW_FIRST|GMP_BIG_ENDIAN));' => 1234
        assert_eq!(ok_packet(export("4660", 2, 1, 1)), "1234");
        // MSW first with little-endian words reverses the two bytes.
        assert_eq!(ok_packet(export("4660", 2, 1, -1)), "3412");
        // One-byte words expose the exact bit length without a zero-word probe.
        assert_eq!(ok_packet(export("4660", 1, 1, 1)), "1234");
        // A wider word size pads the value with leading zero words, as PHP does.
        assert_eq!(ok_packet(export("4660", 8, 1, 1)), "0000000000001234");
        assert_eq!(ok_packet(export("0", 8, 1, 1)), "");
        assert_eq!(ok_packet(import(&[0x12, 0x34], 2, 1, 1)), "4660");
        assert_eq!(ok_packet(import(&[0x34, 0x12], 2, 1, -1)), "4660");
    }

    /// Replays php-src's `gmp_random_seed.phpt` sequence exactly.
    #[test]
    fn random_stream_matches_php_src_vectors() {
        unsafe {
            assert_eq!(call_string(elephc_gmp_random_seed(c_str("0"))), "O");
            assert_eq!(
                ok_packet(call_string(elephc_gmp_random_bits(10))),
                "107"
            );
            assert_eq!(
                ok_packet(call_string(elephc_gmp_random_bits(100))),
                "576055025228722307492589900056"
            );
            assert_eq!(
                ok_packet(call_string(elephc_gmp_random_bits(1000))),
                "5075491613651149525976453192895895253653438900772590630831858908690082668789318258254821002217677675804439098856210618572534955562143303188483908287009522532300439665975877709754914215718998849272363858786685187951932478210775857465448084868199807983919191214972626993925394176279001074206804955195464"
            );
            assert_eq!(ok_packet(call_string(elephc_gmp_random_range(c_str("0"), c_str("10000")))), "4098");
            assert_eq!(ok_packet(call_string(elephc_gmp_random_range(c_str("-10000"), c_str("10000")))), "866");
            assert_eq!(ok_packet(call_string(elephc_gmp_random_range(c_str("-10000"), c_str("0")))), "-4602");

            // Re-seeding the same process generator replays the same stream.
            assert_eq!(call_string(elephc_gmp_random_seed(c_str("42"))), "O");
            let first = ok_packet(call_string(elephc_gmp_random_bits(256)));
            assert_eq!(call_string(elephc_gmp_random_seed(c_str("42"))), "O");
            assert_eq!(ok_packet(call_string(elephc_gmp_random_bits(256))), first);

            // A seed applied on another thread must affect the caller's next draw.
            assert_eq!(call_string(elephc_gmp_random_seed(c_str("987654321"))), "O");
            let expected = ok_packet(call_string(elephc_gmp_random_bits(256)));
            std::thread::spawn(|| unsafe {
                assert_eq!(call_string(elephc_gmp_random_seed(c_str("987654321"))), "O");
            })
            .join()
            .expect("seed worker");
            assert_eq!(ok_packet(call_string(elephc_gmp_random_bits(256))), expected);
        }
    }

    /// Builds one NUL-terminated fixture string.
    fn c_str(value: &str) -> *const std::os::raw::c_char {
        // Leaking every fixture keeps the pointer valid for the duration of the test.
        Box::leak(std::ffi::CString::new(value).unwrap().into_boxed_c_str()).as_ptr()
    }
}

#[cfg(not(elephc_gmp_native))]
mod native_skip {
    /// Reports why the native operation suite is absent from a plain `cargo test` run.
    #[test]
    fn native_suite_requires_managed_libgmp() {
        eprintln!(
            "skipping elephc-gmp native operation tests: set ELEPHC_LIBGMP_DIR to the managed \
             gmp lib directory (scripts/ci/gmp_lib_dir.sh <target>)"
        );
    }
}
