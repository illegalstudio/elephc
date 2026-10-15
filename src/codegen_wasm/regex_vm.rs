//! Purpose:
//! The backtracking VM that runs a compiled `preg_match()` pattern on `wasm32-wasi`.
//!
//! Called from:
//! - `runtime` registers it for a module that matches a pattern.
//! - `builtins` lowers `RuntimeFnId::PregMatch` to `__rt_preg_match`, passing the addresses
//!   `plan` laid the compiled program out at.
//!
//! Key details:
//! - One interpreter, many patterns. Each `preg_match` call site has its own program and its
//!   own class bitmaps in static data, and passes their addresses in — so a module with six
//!   patterns carries six small programs and one VM, not six specialised matchers.
//! - Backtracking uses an EXPLICIT stack rather than recursion: a `Split` pushes the road not
//!   taken and falls through, and a failed instruction pops. That makes the depth a number
//!   this runtime can bound, where WASM call recursion would be bounded by the host's stack
//!   and would fail as a trap rather than an answer.
//! - Both budgets are finite. php's PCRE has its own `pcre.backtrack_limit` and answers `false`
//!   when it is hit; this target's return type is `int`, which cannot carry that, and answering
//!   `0` reported "no match" for a subject that DOES match. So exhaustion stops the program with
//!   a named fatal (`ERR_PREG_LIMIT`) instead of lying.
//! - The search is unanchored, as php's is: the whole program is tried from each starting
//!   offset in turn, including one past the last byte so `/^$/` matches an empty subject.

use super::wat::{Global, ValType, WatModule};

/// Backtrack frames the VM will hold before giving up on a subject.
///
/// A pattern that needs more stops with the named `ERR_PREG_LIMIT` fatal: php's own backtrack
/// limit produces `false`, which this `int` return type cannot carry.
const STACK_FRAMES: u32 = 4096;

/// Bytes per backtrack frame: the program counter, the offset, and the progress marks.
///
/// The marks have to be PER FRAME rather than global. A loop's progress check compares the
/// current offset against the offset its body started at, and backtracking rewinds the offset
/// — so a mark that outlived the frame that set it would compare against a position the
/// thread has already abandoned. Rounded to a power of two so the index is a shift.
const FRAME_BYTES: u32 = 64;

/// Offset of the marks inside a frame.
const FRAME_MARKS: u32 = 8;

/// Instructions the VM will execute for one subject before giving up.
///
/// A catastrophically backtracking pattern — the classic `(?:a+)+b` against a long run of
/// `a` — would otherwise run for longer than anyone is waiting. php bounds the same thing.
const STEP_BUDGET: u32 = 1_000_000;

/// Adds the pattern VM to `wm`. Requires the heap runtime.
pub(super) fn emit_regex_runtime(wm: &mut WatModule) {
    wm.add_global(Global {
        name: "__re_steps".to_string(),
        ty: ValType::I32,
        mutable: true,
        init: 0,
    });
    wm.add_raw_func(RT_RE_LOWER);
    wm.add_raw_func(RT_RE_WORD);
    wm.add_raw_func(&rt_re_run());
    wm.add_raw_func(&rt_preg_match());
}

/// `__rt_re_run`: runs the program from one starting offset. 1 on a match, 0 otherwise.
///
/// The instruction stream is three little-endian words per instruction — opcode, then two
/// operands — which `regex::Inst::words` writes and this reads back. The opcode numbering is
/// shared between the two and is the one thing that must not drift; `regex`'s layout test
/// pins the first few, and every case below names the construct it serves.
fn rt_re_run() -> String {
    format!(
        r#"(func $__rt_re_run (param $prog i32) (param $classes i32) (param $subj i32) (param $len i32) (param $start i32) (param $stack i32) (param $marks i32) (result i32)
  (local $sp i32) (local $pc i32) (local $at i32) (local $base i32)
  (local $op i32) (local $a i32) (local $b i32) (local $byte i32)
  (local $before i32) (local $after i32) (local $slot i32)
  (i32.store (local.get $stack) (i32.const 0))                     ;; pc = 0
  (i32.store offset=4 (local.get $stack) (local.get $start))       ;; at = start
  ;; -1 in every mark, so a slot never set cannot equal a real offset
  (memory.fill (i32.add (local.get $stack) (i32.const {marks_at})) (i32.const 255)
               (i32.const {marks_bytes}))
  (local.set $sp (i32.const 1))
  (block $exhausted (loop $backtrack
    (br_if $exhausted (i32.eqz (local.get $sp)))
    (local.set $sp (i32.sub (local.get $sp) (i32.const 1)))
    (local.set $slot (i32.add (local.get $stack) (i32.mul (local.get $sp) (i32.const {frame}))))
    (local.set $pc (i32.load (local.get $slot)))
    (local.set $at (i32.load offset=4 (local.get $slot)))
    ;; the frame's marks become the thread's, so a rewound offset rewinds them with it
    (memory.copy (local.get $marks) (i32.add (local.get $slot) (i32.const {marks_at}))
                 (i32.const {marks_bytes}))
    (block $fail (loop $step
      (global.set $__re_steps (i32.add (global.get $__re_steps) (i32.const 1)))
      (if (i32.gt_u (global.get $__re_steps) (i32.const {budget}))
        (then (return (i32.const 2))))                             ;; the step budget, as php bounds its own: 2 = exhausted
      (local.set $base (i32.add (local.get $prog) (i32.mul (local.get $pc) (i32.const 12))))
      (local.set $op (i32.load (local.get $base)))
      (local.set $a (i32.load offset=4 (local.get $base)))
      (local.set $b (i32.load offset=8 (local.get $base)))
      ;; 12 = Match
      (if (i32.eq (local.get $op) (i32.const 12)) (then (return (i32.const 1))))
      ;; 4 = Split: push the alternative, take the preferred branch
      (if (i32.eq (local.get $op) (i32.const 4)) (then
        (if (i32.ge_u (local.get $sp) (i32.const {frames}))
          (then (return (i32.const 2))))                           ;; past the backtrack budget: 2 = exhausted
        (local.set $slot (i32.add (local.get $stack) (i32.mul (local.get $sp) (i32.const {frame}))))
        (i32.store (local.get $slot) (local.get $b))
        (i32.store offset=4 (local.get $slot) (local.get $at))
        (memory.copy (i32.add (local.get $slot) (i32.const {marks_at})) (local.get $marks)
                     (i32.const {marks_bytes}))
        (local.set $sp (i32.add (local.get $sp) (i32.const 1)))
        (local.set $pc (local.get $a))
        (br $step)))
      ;; 5 = Jump
      (if (i32.eq (local.get $op) (i32.const 5)) (then
        (local.set $pc (local.get $a))
        (br $step)))
      ;; 6 = Bol: start of subject, or after a newline when multiline
      (if (i32.eq (local.get $op) (i32.const 6)) (then
        (br_if $fail (i32.eqz (i32.or (i32.eqz (local.get $at))
          (i32.and (local.get $a) (i32.and (i32.gt_u (local.get $at) (i32.const 0))
            (i32.eq (i32.load8_u (i32.add (local.get $subj)
                                          (i32.sub (local.get $at) (i32.const 1))))
                    (i32.const 10)))))))
        (local.set $pc (i32.add (local.get $pc) (i32.const 1)))
        (br $step)))
      ;; 7 = Eol: end, before a trailing newline, or before any newline when multiline
      (if (i32.eq (local.get $op) (i32.const 7)) (then
        (local.set $before (i32.eq (local.get $at) (local.get $len)))
        (if (i32.lt_u (local.get $at) (local.get $len)) (then
          (local.set $byte (i32.load8_u (i32.add (local.get $subj) (local.get $at))))
          (if (i32.eq (local.get $byte) (i32.const 10)) (then
            (if (i32.or (local.get $a)
                        (i32.eq (i32.add (local.get $at) (i32.const 1)) (local.get $len)))
              (then (local.set $before (i32.const 1))))))))
        (br_if $fail (i32.eqz (local.get $before)))
        (local.set $pc (i32.add (local.get $pc) (i32.const 1)))
        (br $step)))
      ;; 8 = EndOfSubject: `\z`, which unlike `$` allows nothing after it
      (if (i32.eq (local.get $op) (i32.const 8)) (then
        (br_if $fail (i32.ne (local.get $at) (local.get $len)))
        (local.set $pc (i32.add (local.get $pc) (i32.const 1)))
        (br $step)))
      ;; 9 = WordBoundary: the two sides differ in word-ness, or agree when negated
      (if (i32.eq (local.get $op) (i32.const 9)) (then
        (local.set $before (i32.const 0))
        (if (i32.gt_u (local.get $at) (i32.const 0)) (then
          (local.set $before (call $__rt_re_word
            (i32.load8_u (i32.add (local.get $subj) (i32.sub (local.get $at) (i32.const 1))))))))
        (local.set $after (i32.const 0))
        (if (i32.lt_u (local.get $at) (local.get $len)) (then
          (local.set $after (call $__rt_re_word
            (i32.load8_u (i32.add (local.get $subj) (local.get $at)))))))
        (br_if $fail (i32.eq (i32.ne (local.get $before) (local.get $after)) (local.get $a)))
        (local.set $pc (i32.add (local.get $pc) (i32.const 1)))
        (br $step)))
      ;; 10 = Mark: this loop body starts here
      (if (i32.eq (local.get $op) (i32.const 10)) (then
        (i32.store (i32.add (local.get $marks) (i32.shl (local.get $a) (i32.const 2)))
                   (local.get $at))
        (local.set $pc (i32.add (local.get $pc) (i32.const 1)))
        (br $step)))
      ;; 11 = Advanced: the body consumed nothing, so going round again never would either
      (if (i32.eq (local.get $op) (i32.const 11)) (then
        (br_if $fail (i32.eq (local.get $at)
          (i32.load (i32.add (local.get $marks) (i32.shl (local.get $a) (i32.const 2))))))
        (local.set $pc (i32.add (local.get $pc) (i32.const 1)))
        (br $step)))
      ;; everything below consumes a byte, so there has to be one
      (br_if $fail (i32.ge_u (local.get $at) (local.get $len)))
      (local.set $byte (i32.load8_u (i32.add (local.get $subj) (local.get $at))))
      ;; 0 = Char
      (if (i32.eqz (local.get $op))
        (then (br_if $fail (i32.ne (local.get $byte) (local.get $a))))
      (else
        ;; 1 = CharFolded: the operand is already lowercase
        (if (i32.eq (local.get $op) (i32.const 1))
          (then (br_if $fail (i32.ne (call $__rt_re_lower (local.get $byte)) (local.get $a))))
        (else
          ;; 2 = Any: a newline only counts under the `s` flag
          (if (i32.eq (local.get $op) (i32.const 2))
            (then (br_if $fail (i32.and (i32.eqz (local.get $a))
                                        (i32.eq (local.get $byte) (i32.const 10)))))
          (else
            ;; 3 = Class: one bit per byte value, 32 bytes per class
            (br_if $fail (i32.eqz (i32.and
              (i32.load8_u (i32.add (i32.add (local.get $classes)
                                             (i32.mul (local.get $a) (i32.const 32)))
                                    (i32.shr_u (local.get $byte) (i32.const 3))))
              (i32.shl (i32.const 1) (i32.and (local.get $byte) (i32.const 7))))))))))))
      (local.set $at (i32.add (local.get $at) (i32.const 1)))
      (local.set $pc (i32.add (local.get $pc) (i32.const 1)))
      (br $step)))
    (br $backtrack)))
  (i32.const 0))
"#,
        budget = STEP_BUDGET,
        frames = STACK_FRAMES,
        frame = FRAME_BYTES,
        marks_at = FRAME_MARKS,
        marks_bytes = FRAME_BYTES - FRAME_MARKS
    )
}

/// `__rt_preg_match`: php's answer, 1 or 0, for a compiled pattern against a subject.
///
/// The search is unanchored, so the program is tried from every offset up to and INCLUDING
/// one past the last byte — without that last try, `/^$/` would not match an empty subject
/// and `/a*$/` would not match at the end of one.
///
/// The step budget is shared across all the starting offsets rather than reset for each, so a
/// pathological pattern cannot multiply its cost by the subject's length.
fn rt_preg_match() -> String {
    format!(
        r#"(func $__rt_preg_match (param $prog i32) (param $classes i32) (param $subj i32) (param $len i64) (result i64)
  (local $n i32) (local $start i32) (local $stack i32) (local $marks i32) (local $hit i32)
  (local.set $n (i32.wrap_i64 (local.get $len)))
  ;; one block for the frames and the thread's own mark scratch, which sits past them
  (local.set $stack (call $__rt_heap_alloc (i32.const {bytes})))
  (if (i32.eqz (local.get $stack)) (then (return (i64.const 0))))
  (local.set $marks (i32.add (local.get $stack) (i32.const {frames_bytes})))
  (global.set $__re_steps (i32.const 0))
  (block $done (loop $try
    (br_if $done (i32.gt_u (local.get $start) (local.get $n)))
    (local.set $hit (call $__rt_re_run (local.get $prog) (local.get $classes)
      (local.get $subj) (local.get $n) (local.get $start) (local.get $stack)
      (local.get $marks)))
    (br_if $done (local.get $hit))
    (local.set $start (i32.add (local.get $start) (i32.const 1)))
    (br $try)))
  (call $__rt_heap_free (local.get $stack))
  ;; 2 = a budget ran out, which is NOT "no match": php answers false and flags the error.
  (if (i32.eq (local.get $hit) (i32.const 2)) (then (call $__rt_fail (i32.const 26))))
  (i64.extend_i32_u (local.get $hit)))
"#,
        bytes = STACK_FRAMES * FRAME_BYTES + (FRAME_BYTES - FRAME_MARKS),
        frames_bytes = STACK_FRAMES * FRAME_BYTES
    )
}

/// `__rt_re_lower`: ASCII case folding, which is the only folding a byte engine can do.
pub(super) const RT_RE_LOWER: &str = r#"(func $__rt_re_lower (param $byte i32) (result i32)
  (select (i32.add (local.get $byte) (i32.const 32)) (local.get $byte)
          (i32.and (i32.ge_u (local.get $byte) (i32.const 65))
                   (i32.le_u (local.get $byte) (i32.const 90)))))
"#;

/// `__rt_re_word`: whether a byte is one `\w` accepts, which is what `\b` compares.
pub(super) const RT_RE_WORD: &str = r#"(func $__rt_re_word (param $byte i32) (result i32)
  (i32.or (i32.or (i32.or
    (i32.and (i32.ge_u (local.get $byte) (i32.const 48)) (i32.le_u (local.get $byte) (i32.const 57)))
    (i32.and (i32.ge_u (local.get $byte) (i32.const 65)) (i32.le_u (local.get $byte) (i32.const 90))))
    (i32.and (i32.ge_u (local.get $byte) (i32.const 97)) (i32.le_u (local.get $byte) (i32.const 122))))
    (i32.eq (local.get $byte) (i32.const 95))))
"#;
