//! Purpose:
//! WAT source for the digests: `crc32`, `sha1` and `md5`, including their constant tables.
//!
//! Called from:
//! - `super::emit_builtin_runtime` registers every constant.

/// `__rt_crc32`: PHP's `crc32`, the reflected IEEE 802.3 polynomial.
///
/// Computed bitwise rather than through a 256-entry table: eight shifts per byte cost a little
/// time but keep a module that calls this from carrying a kilobyte of static data. PHP's result
/// is the UNSIGNED 32-bit value, which on a 64-bit build is a positive int — so the final
/// complement is masked rather than sign-extended.
pub(super) const RT_CRC32: &str = r#"(func $__rt_crc32 (param $ptr i32) (param $len i64) (result i64)
  (local $crc i32)                                                ;; running remainder
  (local $i i64)                                                  ;; byte cursor
  (local $k i32)                                                  ;; bit counter
  (local.set $crc (i32.const -1))                                 ;; 0xFFFFFFFF
  (local.set $i (i64.const 0))                                    ;; i = 0
  (block $end (loop $bytes
    (br_if $end (i64.ge_s (local.get $i) (local.get $len)))       ;; every byte folded in
    (local.set $crc
      (i32.xor (local.get $crc)
        (i32.load8_u (i32.add (local.get $ptr) (i32.wrap_i64 (local.get $i))))))
    (local.set $k (i32.const 0))                                  ;; eight bits per byte
    (block $bend (loop $bits
      (br_if $bend (i32.ge_u (local.get $k) (i32.const 8)))
      (local.set $crc
        (i32.xor
          (i32.shr_u (local.get $crc) (i32.const 1))
          (i32.and (i32.const 0xEDB88320)
                   (i32.sub (i32.const 0) (i32.and (local.get $crc) (i32.const 1))))))
      (local.set $k (i32.add (local.get $k) (i32.const 1)))       ;; k++
      (br $bits)))
    (local.set $i (i64.add (local.get $i) (i64.const 1)))         ;; i++
    (br $bytes)))
  (i64.and (i64.extend_i32_u (i32.xor (local.get $crc) (i32.const -1)))
           (i64.const 0xFFFFFFFF)))                               ;; PHP's unsigned 32-bit result
"#;

/// `__rt_sha1_hex`: PHP's `sha1`, returning the 40-character lowercase hex digest.
///
/// The message is padded into an owned buffer rather than processed in place with a special-cased
/// tail: one `0x80` byte, zeros up to 56 bytes past a 64-byte boundary, then the BIT length as a
/// big-endian 64-bit word. Every word in SHA-1 is big-endian, which is the difference from MD5
/// and the usual place an implementation goes wrong. The 80-word message schedule lives in the
/// same buffer's tail so a single allocation covers both, and it is freed before returning.
pub(super) const RT_SHA1_HEX: &str = r#"(func $__rt_sha1_hex (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $blocks i32)                                             ;; padded length in 64-byte blocks
  (local $buf i32)                                                ;; padded message
  (local $w i32)                                                  ;; 80-word schedule
  (local $i i32)                                                  ;; general cursor
  (local $t i32)                                                  ;; round counter
  (local $n i32)                                                  ;; source length as i32
  (local $base i32)                                               ;; current block offset
  (local $h0 i32) (local $h1 i32) (local $h2 i32) (local $h3 i32) (local $h4 i32)
  (local $a i32) (local $b i32) (local $c i32) (local $d i32) (local $e i32)
  (local $f i32) (local $k i32) (local $tmp i32)
  (local $out i32) (local $ow i32) (local $nib i32)
  (local.set $n (i32.wrap_i64 (local.get $len)))
  (local.set $blocks
    (i32.div_u (i32.add (local.get $n) (i32.const 72)) (i32.const 64)))  ;; room for 0x80 + 8 length bytes
  (local.set $buf (call $__rt_heap_alloc
    (i32.add (i32.mul (local.get $blocks) (i32.const 64)) (i32.const 320))))  ;; message + schedule
  (local.set $w (i32.add (local.get $buf) (i32.mul (local.get $blocks) (i32.const 64))))
  (local.set $i (i32.const 0))
  (block $cend (loop $copy
    (br_if $cend (i32.ge_u (local.get $i) (local.get $n)))
    (i32.store8 (i32.add (local.get $buf) (local.get $i))
      (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copy)))
  (i32.store8 (i32.add (local.get $buf) (local.get $i)) (i32.const 128))  ;; the 0x80 terminator
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (block $zend (loop $zeros
    (br_if $zend (i32.ge_u (local.get $i) (i32.mul (local.get $blocks) (i32.const 64))))
    (i32.store8 (i32.add (local.get $buf) (local.get $i)) (i32.const 0))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $zeros)))
  (local.set $i (i32.const 0))
  (block $lend (loop $lenbytes                                    ;; big-endian bit length in the last 8 bytes
    (br_if $lend (i32.ge_u (local.get $i) (i32.const 8)))
    (i32.store8
      (i32.sub (i32.add (local.get $buf) (i32.mul (local.get $blocks) (i32.const 64)))
               (i32.sub (i32.const 8) (local.get $i)))
      (i32.wrap_i64 (i64.and
        (i64.shr_u (i64.mul (local.get $len) (i64.const 8))
                   (i64.extend_i32_u (i32.mul (i32.sub (i32.const 7) (local.get $i)) (i32.const 8))))
        (i64.const 255))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $lenbytes)))
  (local.set $h0 (i32.const 0x67452301))
  (local.set $h1 (i32.const 0xEFCDAB89))
  (local.set $h2 (i32.const 0x98BADCFE))
  (local.set $h3 (i32.const 0x10325476))
  (local.set $h4 (i32.const 0xC3D2E1F0))
  (local.set $base (i32.const 0))
  (block $bend (loop $block
    (br_if $bend (i32.ge_u (local.get $base) (i32.mul (local.get $blocks) (i32.const 64))))
    (local.set $t (i32.const 0))
    (block $wend (loop $w16                                       ;; the first 16 words, big-endian
      (br_if $wend (i32.ge_u (local.get $t) (i32.const 16)))
      (i32.store (i32.add (local.get $w) (i32.mul (local.get $t) (i32.const 4)))
        (i32.or (i32.or
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (local.get $base) (i32.mul (local.get $t) (i32.const 4))))) (i32.const 24))
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $t) (i32.const 4))) (i32.const 1)))) (i32.const 16)))
          (i32.or
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $t) (i32.const 4))) (i32.const 2)))) (i32.const 8))
          (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $t) (i32.const 4))) (i32.const 3)))))))
      (local.set $t (i32.add (local.get $t) (i32.const 1)))
      (br $w16)))
    (block $xend (loop $expand                                    ;; w[t] = rotl(w[t-3]^w[t-8]^w[t-14]^w[t-16], 1)
      (br_if $xend (i32.ge_u (local.get $t) (i32.const 80)))
      (local.set $tmp (i32.xor (i32.xor
        (i32.load (i32.add (local.get $w) (i32.mul (i32.sub (local.get $t) (i32.const 3)) (i32.const 4))))
        (i32.load (i32.add (local.get $w) (i32.mul (i32.sub (local.get $t) (i32.const 8)) (i32.const 4)))))
        (i32.xor
        (i32.load (i32.add (local.get $w) (i32.mul (i32.sub (local.get $t) (i32.const 14)) (i32.const 4))))
        (i32.load (i32.add (local.get $w) (i32.mul (i32.sub (local.get $t) (i32.const 16)) (i32.const 4)))))))
      (i32.store (i32.add (local.get $w) (i32.mul (local.get $t) (i32.const 4)))
        (i32.rotl (local.get $tmp) (i32.const 1)))
      (local.set $t (i32.add (local.get $t) (i32.const 1)))
      (br $expand)))
    (local.set $a (local.get $h0)) (local.set $b (local.get $h1))
    (local.set $c (local.get $h2)) (local.set $d (local.get $h3))
    (local.set $e (local.get $h4))
    (local.set $t (i32.const 0))
    (block $rend (loop $rounds
      (br_if $rend (i32.ge_u (local.get $t) (i32.const 80)))
      (if (i32.lt_u (local.get $t) (i32.const 20))
        (then
          (local.set $f (i32.or (i32.and (local.get $b) (local.get $c))
                                (i32.and (i32.xor (local.get $b) (i32.const -1)) (local.get $d))))
          (local.set $k (i32.const 0x5A827999)))
        (else (if (i32.lt_u (local.get $t) (i32.const 40))
          (then
            (local.set $f (i32.xor (i32.xor (local.get $b) (local.get $c)) (local.get $d)))
            (local.set $k (i32.const 0x6ED9EBA1)))
          (else (if (i32.lt_u (local.get $t) (i32.const 60))
            (then
              (local.set $f (i32.or (i32.or
                (i32.and (local.get $b) (local.get $c))
                (i32.and (local.get $b) (local.get $d)))
                (i32.and (local.get $c) (local.get $d))))
              (local.set $k (i32.const 0x8F1BBCDC)))
            (else
              (local.set $f (i32.xor (i32.xor (local.get $b) (local.get $c)) (local.get $d)))
              (local.set $k (i32.const 0xCA62C1D6))))))))
      (local.set $tmp (i32.add (i32.add (i32.add (i32.add
        (i32.rotl (local.get $a) (i32.const 5)) (local.get $f)) (local.get $e)) (local.get $k))
        (i32.load (i32.add (local.get $w) (i32.mul (local.get $t) (i32.const 4))))))
      (local.set $e (local.get $d))
      (local.set $d (local.get $c))
      (local.set $c (i32.rotl (local.get $b) (i32.const 30)))
      (local.set $b (local.get $a))
      (local.set $a (local.get $tmp))
      (local.set $t (i32.add (local.get $t) (i32.const 1)))
      (br $rounds)))
    (local.set $h0 (i32.add (local.get $h0) (local.get $a)))
    (local.set $h1 (i32.add (local.get $h1) (local.get $b)))
    (local.set $h2 (i32.add (local.get $h2) (local.get $c)))
    (local.set $h3 (i32.add (local.get $h3) (local.get $d)))
    (local.set $h4 (i32.add (local.get $h4) (local.get $e)))
    (local.set $base (i32.add (local.get $base) (i32.const 64)))
    (br $block)))
  (call $__rt_heap_free (local.get $buf))                         ;; the scratch is done with
  (local.set $out (call $__rt_str_alloc (i64.const 40)))          ;; 5 words, 8 hex digits each
  (local.set $ow (i32.const 0))
  (local.set $i (i32.const 0))
  (block $hend (loop $hex
    (br_if $hend (i32.ge_u (local.get $i) (i32.const 40)))
    (local.set $tmp (local.get $h0))
    (if (i32.ge_u (local.get $i) (i32.const 8))  (then (local.set $tmp (local.get $h1))))
    (if (i32.ge_u (local.get $i) (i32.const 16)) (then (local.set $tmp (local.get $h2))))
    (if (i32.ge_u (local.get $i) (i32.const 24)) (then (local.set $tmp (local.get $h3))))
    (if (i32.ge_u (local.get $i) (i32.const 32)) (then (local.set $tmp (local.get $h4))))
    (local.set $nib (i32.and
      (i32.shr_u (local.get $tmp)
        (i32.mul (i32.sub (i32.const 7) (i32.rem_u (local.get $i) (i32.const 8))) (i32.const 4)))
      (i32.const 15)))
    (i32.store8 (i32.add (local.get $out) (local.get $ow))
      (i32.add (local.get $nib)
        (select (i32.const 48) (i32.const 87) (i32.lt_u (local.get $nib) (i32.const 10)))))
    (local.set $ow (i32.add (local.get $ow) (i32.const 1)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $hex)))
  (local.get $out) (i64.const 40))                                ;; owned 40-character digest
"#;

/// `__rt_md5_k`: MD5's per-round additive constant, `floor(|sin(i+1)| * 2^32)`.
///
/// A branch chain rather than a data segment: WebAssembly has no `sin`, so the 64 values have to
/// be materialized somehow, and a segment would mean threading a new reservation through
/// `plan_module`'s static-data cursor for a table only this builtin reads.
pub(super) const RT_MD5_K: &str = r#"(func $__rt_md5_k (param $i i32) (result i32)
  (if (i32.eq (local.get $i) (i32.const 0)) (then (return (i32.const 0xd76aa478))))
  (if (i32.eq (local.get $i) (i32.const 1)) (then (return (i32.const 0xe8c7b756))))
  (if (i32.eq (local.get $i) (i32.const 2)) (then (return (i32.const 0x242070db))))
  (if (i32.eq (local.get $i) (i32.const 3)) (then (return (i32.const 0xc1bdceee))))
  (if (i32.eq (local.get $i) (i32.const 4)) (then (return (i32.const 0xf57c0faf))))
  (if (i32.eq (local.get $i) (i32.const 5)) (then (return (i32.const 0x4787c62a))))
  (if (i32.eq (local.get $i) (i32.const 6)) (then (return (i32.const 0xa8304613))))
  (if (i32.eq (local.get $i) (i32.const 7)) (then (return (i32.const 0xfd469501))))
  (if (i32.eq (local.get $i) (i32.const 8)) (then (return (i32.const 0x698098d8))))
  (if (i32.eq (local.get $i) (i32.const 9)) (then (return (i32.const 0x8b44f7af))))
  (if (i32.eq (local.get $i) (i32.const 10)) (then (return (i32.const 0xffff5bb1))))
  (if (i32.eq (local.get $i) (i32.const 11)) (then (return (i32.const 0x895cd7be))))
  (if (i32.eq (local.get $i) (i32.const 12)) (then (return (i32.const 0x6b901122))))
  (if (i32.eq (local.get $i) (i32.const 13)) (then (return (i32.const 0xfd987193))))
  (if (i32.eq (local.get $i) (i32.const 14)) (then (return (i32.const 0xa679438e))))
  (if (i32.eq (local.get $i) (i32.const 15)) (then (return (i32.const 0x49b40821))))
  (if (i32.eq (local.get $i) (i32.const 16)) (then (return (i32.const 0xf61e2562))))
  (if (i32.eq (local.get $i) (i32.const 17)) (then (return (i32.const 0xc040b340))))
  (if (i32.eq (local.get $i) (i32.const 18)) (then (return (i32.const 0x265e5a51))))
  (if (i32.eq (local.get $i) (i32.const 19)) (then (return (i32.const 0xe9b6c7aa))))
  (if (i32.eq (local.get $i) (i32.const 20)) (then (return (i32.const 0xd62f105d))))
  (if (i32.eq (local.get $i) (i32.const 21)) (then (return (i32.const 0x02441453))))
  (if (i32.eq (local.get $i) (i32.const 22)) (then (return (i32.const 0xd8a1e681))))
  (if (i32.eq (local.get $i) (i32.const 23)) (then (return (i32.const 0xe7d3fbc8))))
  (if (i32.eq (local.get $i) (i32.const 24)) (then (return (i32.const 0x21e1cde6))))
  (if (i32.eq (local.get $i) (i32.const 25)) (then (return (i32.const 0xc33707d6))))
  (if (i32.eq (local.get $i) (i32.const 26)) (then (return (i32.const 0xf4d50d87))))
  (if (i32.eq (local.get $i) (i32.const 27)) (then (return (i32.const 0x455a14ed))))
  (if (i32.eq (local.get $i) (i32.const 28)) (then (return (i32.const 0xa9e3e905))))
  (if (i32.eq (local.get $i) (i32.const 29)) (then (return (i32.const 0xfcefa3f8))))
  (if (i32.eq (local.get $i) (i32.const 30)) (then (return (i32.const 0x676f02d9))))
  (if (i32.eq (local.get $i) (i32.const 31)) (then (return (i32.const 0x8d2a4c8a))))
  (if (i32.eq (local.get $i) (i32.const 32)) (then (return (i32.const 0xfffa3942))))
  (if (i32.eq (local.get $i) (i32.const 33)) (then (return (i32.const 0x8771f681))))
  (if (i32.eq (local.get $i) (i32.const 34)) (then (return (i32.const 0x6d9d6122))))
  (if (i32.eq (local.get $i) (i32.const 35)) (then (return (i32.const 0xfde5380c))))
  (if (i32.eq (local.get $i) (i32.const 36)) (then (return (i32.const 0xa4beea44))))
  (if (i32.eq (local.get $i) (i32.const 37)) (then (return (i32.const 0x4bdecfa9))))
  (if (i32.eq (local.get $i) (i32.const 38)) (then (return (i32.const 0xf6bb4b60))))
  (if (i32.eq (local.get $i) (i32.const 39)) (then (return (i32.const 0xbebfbc70))))
  (if (i32.eq (local.get $i) (i32.const 40)) (then (return (i32.const 0x289b7ec6))))
  (if (i32.eq (local.get $i) (i32.const 41)) (then (return (i32.const 0xeaa127fa))))
  (if (i32.eq (local.get $i) (i32.const 42)) (then (return (i32.const 0xd4ef3085))))
  (if (i32.eq (local.get $i) (i32.const 43)) (then (return (i32.const 0x04881d05))))
  (if (i32.eq (local.get $i) (i32.const 44)) (then (return (i32.const 0xd9d4d039))))
  (if (i32.eq (local.get $i) (i32.const 45)) (then (return (i32.const 0xe6db99e5))))
  (if (i32.eq (local.get $i) (i32.const 46)) (then (return (i32.const 0x1fa27cf8))))
  (if (i32.eq (local.get $i) (i32.const 47)) (then (return (i32.const 0xc4ac5665))))
  (if (i32.eq (local.get $i) (i32.const 48)) (then (return (i32.const 0xf4292244))))
  (if (i32.eq (local.get $i) (i32.const 49)) (then (return (i32.const 0x432aff97))))
  (if (i32.eq (local.get $i) (i32.const 50)) (then (return (i32.const 0xab9423a7))))
  (if (i32.eq (local.get $i) (i32.const 51)) (then (return (i32.const 0xfc93a039))))
  (if (i32.eq (local.get $i) (i32.const 52)) (then (return (i32.const 0x655b59c3))))
  (if (i32.eq (local.get $i) (i32.const 53)) (then (return (i32.const 0x8f0ccc92))))
  (if (i32.eq (local.get $i) (i32.const 54)) (then (return (i32.const 0xffeff47d))))
  (if (i32.eq (local.get $i) (i32.const 55)) (then (return (i32.const 0x85845dd1))))
  (if (i32.eq (local.get $i) (i32.const 56)) (then (return (i32.const 0x6fa87e4f))))
  (if (i32.eq (local.get $i) (i32.const 57)) (then (return (i32.const 0xfe2ce6e0))))
  (if (i32.eq (local.get $i) (i32.const 58)) (then (return (i32.const 0xa3014314))))
  (if (i32.eq (local.get $i) (i32.const 59)) (then (return (i32.const 0x4e0811a1))))
  (if (i32.eq (local.get $i) (i32.const 60)) (then (return (i32.const 0xf7537e82))))
  (if (i32.eq (local.get $i) (i32.const 61)) (then (return (i32.const 0xbd3af235))))
  (if (i32.eq (local.get $i) (i32.const 62)) (then (return (i32.const 0x2ad7d2bb))))
  (if (i32.eq (local.get $i) (i32.const 63)) (then (return (i32.const 0xeb86d391))))
  (i32.const 0))
"#;

/// `__rt_md5_s`: MD5's per-round rotation amount.
///
/// The four groups repeat `[7,12,17,22]`, `[5,9,14,20]`, `[4,11,16,23]`, `[6,10,15,21]`, so this
/// IS computable from `(i/16, i%4)` — it is written out for the same reason the constants are,
/// which is that one lookup shape is easier to check against the spec than two.
pub(super) const RT_MD5_S: &str = r#"(func $__rt_md5_s (param $i i32) (result i32)
  (if (i32.eq (local.get $i) (i32.const 0)) (then (return (i32.const 0x00000007))))
  (if (i32.eq (local.get $i) (i32.const 1)) (then (return (i32.const 0x0000000c))))
  (if (i32.eq (local.get $i) (i32.const 2)) (then (return (i32.const 0x00000011))))
  (if (i32.eq (local.get $i) (i32.const 3)) (then (return (i32.const 0x00000016))))
  (if (i32.eq (local.get $i) (i32.const 4)) (then (return (i32.const 0x00000007))))
  (if (i32.eq (local.get $i) (i32.const 5)) (then (return (i32.const 0x0000000c))))
  (if (i32.eq (local.get $i) (i32.const 6)) (then (return (i32.const 0x00000011))))
  (if (i32.eq (local.get $i) (i32.const 7)) (then (return (i32.const 0x00000016))))
  (if (i32.eq (local.get $i) (i32.const 8)) (then (return (i32.const 0x00000007))))
  (if (i32.eq (local.get $i) (i32.const 9)) (then (return (i32.const 0x0000000c))))
  (if (i32.eq (local.get $i) (i32.const 10)) (then (return (i32.const 0x00000011))))
  (if (i32.eq (local.get $i) (i32.const 11)) (then (return (i32.const 0x00000016))))
  (if (i32.eq (local.get $i) (i32.const 12)) (then (return (i32.const 0x00000007))))
  (if (i32.eq (local.get $i) (i32.const 13)) (then (return (i32.const 0x0000000c))))
  (if (i32.eq (local.get $i) (i32.const 14)) (then (return (i32.const 0x00000011))))
  (if (i32.eq (local.get $i) (i32.const 15)) (then (return (i32.const 0x00000016))))
  (if (i32.eq (local.get $i) (i32.const 16)) (then (return (i32.const 0x00000005))))
  (if (i32.eq (local.get $i) (i32.const 17)) (then (return (i32.const 0x00000009))))
  (if (i32.eq (local.get $i) (i32.const 18)) (then (return (i32.const 0x0000000e))))
  (if (i32.eq (local.get $i) (i32.const 19)) (then (return (i32.const 0x00000014))))
  (if (i32.eq (local.get $i) (i32.const 20)) (then (return (i32.const 0x00000005))))
  (if (i32.eq (local.get $i) (i32.const 21)) (then (return (i32.const 0x00000009))))
  (if (i32.eq (local.get $i) (i32.const 22)) (then (return (i32.const 0x0000000e))))
  (if (i32.eq (local.get $i) (i32.const 23)) (then (return (i32.const 0x00000014))))
  (if (i32.eq (local.get $i) (i32.const 24)) (then (return (i32.const 0x00000005))))
  (if (i32.eq (local.get $i) (i32.const 25)) (then (return (i32.const 0x00000009))))
  (if (i32.eq (local.get $i) (i32.const 26)) (then (return (i32.const 0x0000000e))))
  (if (i32.eq (local.get $i) (i32.const 27)) (then (return (i32.const 0x00000014))))
  (if (i32.eq (local.get $i) (i32.const 28)) (then (return (i32.const 0x00000005))))
  (if (i32.eq (local.get $i) (i32.const 29)) (then (return (i32.const 0x00000009))))
  (if (i32.eq (local.get $i) (i32.const 30)) (then (return (i32.const 0x0000000e))))
  (if (i32.eq (local.get $i) (i32.const 31)) (then (return (i32.const 0x00000014))))
  (if (i32.eq (local.get $i) (i32.const 32)) (then (return (i32.const 0x00000004))))
  (if (i32.eq (local.get $i) (i32.const 33)) (then (return (i32.const 0x0000000b))))
  (if (i32.eq (local.get $i) (i32.const 34)) (then (return (i32.const 0x00000010))))
  (if (i32.eq (local.get $i) (i32.const 35)) (then (return (i32.const 0x00000017))))
  (if (i32.eq (local.get $i) (i32.const 36)) (then (return (i32.const 0x00000004))))
  (if (i32.eq (local.get $i) (i32.const 37)) (then (return (i32.const 0x0000000b))))
  (if (i32.eq (local.get $i) (i32.const 38)) (then (return (i32.const 0x00000010))))
  (if (i32.eq (local.get $i) (i32.const 39)) (then (return (i32.const 0x00000017))))
  (if (i32.eq (local.get $i) (i32.const 40)) (then (return (i32.const 0x00000004))))
  (if (i32.eq (local.get $i) (i32.const 41)) (then (return (i32.const 0x0000000b))))
  (if (i32.eq (local.get $i) (i32.const 42)) (then (return (i32.const 0x00000010))))
  (if (i32.eq (local.get $i) (i32.const 43)) (then (return (i32.const 0x00000017))))
  (if (i32.eq (local.get $i) (i32.const 44)) (then (return (i32.const 0x00000004))))
  (if (i32.eq (local.get $i) (i32.const 45)) (then (return (i32.const 0x0000000b))))
  (if (i32.eq (local.get $i) (i32.const 46)) (then (return (i32.const 0x00000010))))
  (if (i32.eq (local.get $i) (i32.const 47)) (then (return (i32.const 0x00000017))))
  (if (i32.eq (local.get $i) (i32.const 48)) (then (return (i32.const 0x00000006))))
  (if (i32.eq (local.get $i) (i32.const 49)) (then (return (i32.const 0x0000000a))))
  (if (i32.eq (local.get $i) (i32.const 50)) (then (return (i32.const 0x0000000f))))
  (if (i32.eq (local.get $i) (i32.const 51)) (then (return (i32.const 0x00000015))))
  (if (i32.eq (local.get $i) (i32.const 52)) (then (return (i32.const 0x00000006))))
  (if (i32.eq (local.get $i) (i32.const 53)) (then (return (i32.const 0x0000000a))))
  (if (i32.eq (local.get $i) (i32.const 54)) (then (return (i32.const 0x0000000f))))
  (if (i32.eq (local.get $i) (i32.const 55)) (then (return (i32.const 0x00000015))))
  (if (i32.eq (local.get $i) (i32.const 56)) (then (return (i32.const 0x00000006))))
  (if (i32.eq (local.get $i) (i32.const 57)) (then (return (i32.const 0x0000000a))))
  (if (i32.eq (local.get $i) (i32.const 58)) (then (return (i32.const 0x0000000f))))
  (if (i32.eq (local.get $i) (i32.const 59)) (then (return (i32.const 0x00000015))))
  (if (i32.eq (local.get $i) (i32.const 60)) (then (return (i32.const 0x00000006))))
  (if (i32.eq (local.get $i) (i32.const 61)) (then (return (i32.const 0x0000000a))))
  (if (i32.eq (local.get $i) (i32.const 62)) (then (return (i32.const 0x0000000f))))
  (if (i32.eq (local.get $i) (i32.const 63)) (then (return (i32.const 0x00000015))))
  (i32.const 0))
"#;

/// `__rt_md5_hex`: PHP's `md5`, returning the 32-character lowercase hex digest.
///
/// The padding rule matches SHA-1's shape — one `0x80` byte, zeros up to 56 bytes past a 64-byte
/// boundary, then the BIT length — but every word here is LITTLE-endian, which is the single
/// biggest difference between the two and the usual place a port of one into the other goes
/// wrong. The digest bytes come out little-endian per word too, so `a0`'s low byte is printed
/// first.
pub(super) const RT_MD5_HEX: &str = r#"(func $__rt_md5_hex (param $ptr i32) (param $len i64) (result i32) (result i64)
  (local $blocks i32)                                             ;; padded length in 64-byte blocks
  (local $buf i32)                                                ;; padded message
  (local $i i32)                                                  ;; general cursor
  (local $t i32)                                                  ;; round counter
  (local $g i32)                                                  ;; message word index
  (local $n i32)                                                  ;; source length as i32
  (local $base i32)                                               ;; current block offset
  (local $h0 i32) (local $h1 i32) (local $h2 i32) (local $h3 i32)
  (local $a i32) (local $b i32) (local $c i32) (local $d i32)
  (local $f i32) (local $tmp i32)
  (local $out i32) (local $ow i32) (local $nib i32) (local $word i32)
  (local.set $n (i32.wrap_i64 (local.get $len)))
  (local.set $blocks
    (i32.div_u (i32.add (local.get $n) (i32.const 72)) (i32.const 64)))
  (local.set $buf (call $__rt_heap_alloc (i32.mul (local.get $blocks) (i32.const 64))))
  (local.set $i (i32.const 0))
  (block $cend (loop $copy
    (br_if $cend (i32.ge_u (local.get $i) (local.get $n)))
    (i32.store8 (i32.add (local.get $buf) (local.get $i))
      (i32.load8_u (i32.add (local.get $ptr) (local.get $i))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $copy)))
  (i32.store8 (i32.add (local.get $buf) (local.get $i)) (i32.const 128))
  (local.set $i (i32.add (local.get $i) (i32.const 1)))
  (block $zend (loop $zeros
    (br_if $zend (i32.ge_u (local.get $i) (i32.mul (local.get $blocks) (i32.const 64))))
    (i32.store8 (i32.add (local.get $buf) (local.get $i)) (i32.const 0))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $zeros)))
  (local.set $i (i32.const 0))
  (block $lend (loop $lenbytes                                    ;; LITTLE-endian bit length
    (br_if $lend (i32.ge_u (local.get $i) (i32.const 8)))
    (i32.store8
      (i32.add (i32.sub (i32.add (local.get $buf) (i32.mul (local.get $blocks) (i32.const 64)))
                        (i32.const 8))
               (local.get $i))
      (i32.wrap_i64 (i64.and
        (i64.shr_u (i64.mul (local.get $len) (i64.const 8))
                   (i64.extend_i32_u (i32.mul (local.get $i) (i32.const 8))))
        (i64.const 255))))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $lenbytes)))
  (local.set $h0 (i32.const 0x67452301))
  (local.set $h1 (i32.const 0xEFCDAB89))
  (local.set $h2 (i32.const 0x98BADCFE))
  (local.set $h3 (i32.const 0x10325476))
  (local.set $base (i32.const 0))
  (block $bend (loop $block
    (br_if $bend (i32.ge_u (local.get $base) (i32.mul (local.get $blocks) (i32.const 64))))
    (local.set $a (local.get $h0)) (local.set $b (local.get $h1))
    (local.set $c (local.get $h2)) (local.set $d (local.get $h3))
    (local.set $t (i32.const 0))
    (block $rend (loop $rounds
      (br_if $rend (i32.ge_u (local.get $t) (i32.const 64)))
      (if (i32.lt_u (local.get $t) (i32.const 16))
        (then
          (local.set $f (i32.or (i32.and (local.get $b) (local.get $c))
                                (i32.and (i32.xor (local.get $b) (i32.const -1)) (local.get $d))))
          (local.set $g (local.get $t)))
        (else (if (i32.lt_u (local.get $t) (i32.const 32))
          (then
            (local.set $f (i32.or (i32.and (local.get $d) (local.get $b))
                                  (i32.and (i32.xor (local.get $d) (i32.const -1)) (local.get $c))))
            (local.set $g (i32.rem_u (i32.add (i32.mul (i32.const 5) (local.get $t)) (i32.const 1))
                                     (i32.const 16))))
          (else (if (i32.lt_u (local.get $t) (i32.const 48))
            (then
              (local.set $f (i32.xor (i32.xor (local.get $b) (local.get $c)) (local.get $d)))
              (local.set $g (i32.rem_u (i32.add (i32.mul (i32.const 3) (local.get $t)) (i32.const 5))
                                       (i32.const 16))))
            (else
              (local.set $f (i32.xor (local.get $c)
                                     (i32.or (local.get $b) (i32.xor (local.get $d) (i32.const -1)))))
              (local.set $g (i32.rem_u (i32.mul (i32.const 7) (local.get $t)) (i32.const 16)))))))))
      (local.set $word                                            ;; M[g], little-endian
        (i32.or (i32.or
          (i32.load8_u (i32.add (local.get $buf)
            (i32.add (local.get $base) (i32.mul (local.get $g) (i32.const 4)))))
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $g) (i32.const 4))) (i32.const 1)))) (i32.const 8)))
          (i32.or
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $g) (i32.const 4))) (i32.const 2)))) (i32.const 16))
          (i32.shl (i32.load8_u (i32.add (local.get $buf)
            (i32.add (i32.add (local.get $base) (i32.mul (local.get $g) (i32.const 4))) (i32.const 3)))) (i32.const 24)))))
      (local.set $f (i32.add (i32.add (i32.add (local.get $f) (local.get $a))
                                      (call $__rt_md5_k (local.get $t)))
                             (local.get $word)))
      (local.set $a (local.get $d))
      (local.set $d (local.get $c))
      (local.set $c (local.get $b))
      (local.set $b (i32.add (local.get $b)
        (i32.rotl (local.get $f) (call $__rt_md5_s (local.get $t)))))
      (local.set $t (i32.add (local.get $t) (i32.const 1)))
      (br $rounds)))
    (local.set $h0 (i32.add (local.get $h0) (local.get $a)))
    (local.set $h1 (i32.add (local.get $h1) (local.get $b)))
    (local.set $h2 (i32.add (local.get $h2) (local.get $c)))
    (local.set $h3 (i32.add (local.get $h3) (local.get $d)))
    (local.set $base (i32.add (local.get $base) (i32.const 64)))
    (br $block)))
  (call $__rt_heap_free (local.get $buf))
  (local.set $out (call $__rt_str_alloc (i64.const 32)))
  (local.set $ow (i32.const 0))
  (local.set $i (i32.const 0))
  (block $hend (loop $hex                                         ;; each word's bytes, low first
    (br_if $hend (i32.ge_u (local.get $i) (i32.const 32)))
    (local.set $tmp (local.get $h0))
    (if (i32.ge_u (local.get $i) (i32.const 8))  (then (local.set $tmp (local.get $h1))))
    (if (i32.ge_u (local.get $i) (i32.const 16)) (then (local.set $tmp (local.get $h2))))
    (if (i32.ge_u (local.get $i) (i32.const 24)) (then (local.set $tmp (local.get $h3))))
    (local.set $tmp
      (i32.and (i32.shr_u (local.get $tmp)
        (i32.mul (i32.div_u (i32.rem_u (local.get $i) (i32.const 8)) (i32.const 2)) (i32.const 8)))
        (i32.const 255)))                                         ;; the byte this pair prints
    (local.set $nib
      (select (i32.shr_u (local.get $tmp) (i32.const 4))
              (i32.and (local.get $tmp) (i32.const 15))
              (i32.eqz (i32.and (local.get $i) (i32.const 1)))))  ;; high nibble first within a byte
    (i32.store8 (i32.add (local.get $out) (local.get $ow))
      (i32.add (local.get $nib)
        (select (i32.const 48) (i32.const 87) (i32.lt_u (local.get $nib) (i32.const 10)))))
    (local.set $ow (i32.add (local.get $ow) (i32.const 1)))
    (local.set $i (i32.add (local.get $i) (i32.const 1)))
    (br $hex)))
  (local.get $out) (i64.const 32))                                ;; owned 32-character digest
"#;
