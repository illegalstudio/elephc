//! Tests for the in-tree Mach-O assembler: line handling, layout, and an end-to-end program
//! assembled both ways (here and by `as`), linked and run.

use super::*;

/// `.ascii` operands decode the escapes the data section writes.
/// An instruction text with no symbolic operand is parsed once per slice: repeats take the word
/// from the cache, and a text with a fixup is parsed every time, since its target resolves per
/// line. The words written are the ones a fresh parse gives.
#[test]
fn repeated_plain_instructions_are_parsed_once() {
    let mut text = String::from(".globl _main\n_main:\n");
    for _ in 0..50 {
        text.push_str("    ldr x0, [x29, #-16]\n    add x1, x0, #1\n    bl _main\n");
    }
    text.push_str("    ret\n");
    super::INSTRUCTIONS_PARSED.with(|count| count.set(0));
    let object = assemble(&text).expect("assembles");
    let parsed = super::INSTRUCTIONS_PARSED.with(std::cell::Cell::get);
    // Two plain texts once each, `bl _main` 50 times, `ret` once.
    assert_eq!(parsed, 53, "{parsed} instruction texts parsed");
    // Every repeat carries the freshly parsed word: the 50 `ldr`/`add` pairs sit in the text
    // section, three words apart.
    let pair: Vec<u8> = ["ldr x0, [x29, #-16]", "add x1, x0, #1"]
        .iter()
        .flat_map(|line| super::encode::encode_instruction(line).unwrap().word.to_le_bytes())
        .collect();
    let pairs = object.windows(pair.len()).filter(|window| *window == pair.as_slice()).count();
    assert_eq!(pairs, 50, "{pairs} encoded ldr/add pairs");
}

#[test]
fn ascii_operands_decode_the_emitted_escapes() {
    let mut out = Vec::new();
    decode_ascii(r#""a\n\t\\\"\000\177z""#, &mut out).expect("decodes");
    assert_eq!(out, b"a\n\t\\\"\0\x7fz");
    assert!(decode_ascii(r#""unterminated"#, &mut Vec::new()).is_none());
}

/// A `.quad` above the signed range is a bit pattern: the Demo's data has `u64::MAX`.
#[test]
fn integers_cover_the_whole_unsigned_range() {
    assert_eq!(integer("18446744073709551615"), Some(-1));
    assert_eq!(integer("-1"), Some(-1));
    assert_eq!(integer("0xffffffffffffffff"), Some(-1));
    assert_eq!(integer("18446744073709551616"), None);
}

/// Numeric labels resolve forward and backward from the referencing line.
#[test]
fn numeric_labels_resolve_by_direction() {
    let text = "1:\n    mov x0, x1\n1:\n    mov x0, x2\n    mov x0, x3\n1:\n";
    let layout = layout(text).expect("layout");
    // Line indexes: 0 `1:`, 1 mov, 2 `1:`, 3 mov, 4 mov, 5 `1:`.
    assert_eq!(numeric_target(&layout, "1b", 4).map(|p| p.offset), Some(4));
    assert_eq!(numeric_target(&layout, "1f", 4).map(|p| p.offset), Some(12));
    assert_eq!(numeric_target(&layout, "1b", 1).map(|p| p.offset), Some(0));
    assert!(numeric_target(&layout, "1f", 5).is_none());
}

/// Alignment pads code with `nop` and data with zeros, and `L1 - L2` folds to a constant.
#[test]
fn layout_aligns_and_folds_label_differences() {
    let text = ".data\nLa:\n    .byte 1\n.p2align 3\nLb:\n    .quad 7\n    .long Lb - La\n";
    let layout = layout(text).expect("layout");
    assert_eq!(layout.labels["Lb"].offset, 8);
    assert_eq!(label_difference(&layout, "Lb - La", Names::IDENTITY), Some(8));
    assert_eq!(padding(13, 3), 3);
    assert_eq!(padding(16, 3), 0);
}

/// The single pass places labels while it emits, and resolves what needed a later label after:
/// forward and backward branches (named and numeric), external calls, `A - B` data operands in
/// either order, several operands on one line, commons and alignment. Its object is byte for byte
/// the two-pass assembler's, relocations and their order included; so is its refusal of a
/// listing it cannot assemble.
#[test]
fn one_pass_matches_two_pass() {
    let listings = [
        r#".globl _main
.p2align 2
_main:
    stp x29, x30, [sp, #-32]!
    b.eq Llater
    cbz x1, 2f
    tbnz x2, #3, Llater
    bl _external
    bl _helper
    adrp x9, _counter@PAGE
    add x9, x9, _counter@PAGEOFF
    adrp x10, Ltable@PAGE
    ldr x1, [x10, Ltable@PAGEOFF]
1:
    subs x3, x3, #1
    b.ne 1b
2:
    b 3f
    mov x0, #9
3:
    adr x4, Llater
Llater:
    ldp x29, x30, [sp], #32
    ret
_helper:
    b _main
    mov x0, #42
    ret
.data
.comm _counter, 8, 3
Lmsg:
    .ascii "hello\n"
    .long Ltable - Lmsg, Lend - Lmsg
.p2align 3
Ltable:
    .quad 0, Lmsg, _external+8, Lend - Ltable
    .quad Lmsg
    .long Ltable - Lmsg
Lend:
    .byte 1, 2, 3
"#,
        "1:\n    mov x0, x1\n1:\n    mov x0, x2\n    b 1b\n    b 1f\n1:\n    ret\n",
        ".section __TEXT,__cstring\n",
        "    b.eq _undefined_short\n",
        ".data\n    mov x0, x1\n",
    ];
    for listing in listings {
        let one = super::assemble_parts(&[listing], Names::IDENTITY);
        let two = super::assemble_parts_two_pass(&[listing], Names::IDENTITY);
        match (&one, &two) {
            (Ok(one), Ok(two)) => assert!(one == two, "objects differ for:\n{listing}"),
            (Err(_), Err(_)) => {}
            _ => panic!("one pass {:?} vs two pass {:?} for:\n{listing}", one.is_ok(), two.is_ok()),
        }
    }
    // The same listing cut into parts at line boundaries assembles the same.
    let whole = super::assemble_parts(&[listings[0]], Names::IDENTITY).expect("assembles");
    let lines: Vec<&str> = listings[0].split_inclusive('\n').collect();
    let parts: Vec<String> = lines.chunks(5).map(|chunk| chunk.concat()).collect();
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    assert!(super::assemble_parts(&parts, Names::IDENTITY).expect("assembles") == whole);
}

/// A directive or section the assembler does not model hands the slice back to `as`.
#[test]
fn unknown_directives_are_unsupported() {
    assert!(assemble(".section __TEXT,__cstring\n").is_err());
    assert!(assemble(".data\n    mov x0, x1\n").is_err());
}

/// A small program that exercises every relocation the compiler's listings use, local and
/// numeric branches, `.quad` pointers, `.comm` and alignment: assembled here and by `as`, both
/// linked and run, the two must behave identically.
#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn inhouse_object_runs_like_the_assembler_one() {
    let listing = r#".globl _main
.p2align 2
_main:
    stp x29, x30, [sp, #-32]!
    mov x29, sp
    bl _helper
    adrp x9, _counter@PAGE
    add x9, x9, _counter@PAGEOFF
    str x0, [x9]
    adrp x10, _table@PAGE
    add x10, x10, _table@PAGEOFF
    ldr x1, [x10, #8]
    mov x2, #6
    mov x0, #1
    bl _write
    mov x3, #3
1:
    subs x3, x3, #1
    b.ne 1b
    cbz x3, 2f
    mov x0, #9
2:
    adrp x9, _counter@PAGE
    ldr x0, [x9, _counter@PAGEOFF]
    sub x0, x0, #42
    ldp x29, x30, [sp], #32
    ret
_helper:
    mov x0, #42
    ret
.data
.comm _counter, 8, 3
Lmsg:
    .ascii "hello\n"
.p2align 3
_table:
    .quad 0
    .quad Lmsg
    .long _table - Lmsg
"#;
    let dir = std::env::temp_dir().join(format!("elephc-inhouse-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("prog.s");
    std::fs::write(&source, listing).unwrap();
    let ours = dir.join("ours.o");
    std::fs::write(&ours, assemble(listing).expect("the in-tree assembler handles the listing")).unwrap();
    let theirs = dir.join("theirs.o");
    let status = std::process::Command::new("as")
        .args(["-arch", "arm64", "-o"])
        .arg(&theirs)
        .arg(&source)
        .status()
        .unwrap();
    assert!(status.success());
    let mut outputs = Vec::new();
    for (object, exe) in [(&ours, dir.join("ours")), (&theirs, dir.join("theirs"))] {
        let link = std::process::Command::new("cc").arg(object).arg("-o").arg(&exe).output().unwrap();
        if std::env::var_os("ELEPHC_INHOUSE_KEEP").is_some() && !link.status.success() {
            eprintln!("kept {}", dir.display());
            continue;
        }
        assert!(link.status.success(), "link {}: {}", object.display(), String::from_utf8_lossy(&link.stderr));
        assert!(link.stderr.is_empty(), "the linker warned: {}", String::from_utf8_lossy(&link.stderr));
        let run = std::process::Command::new(&exe).output().unwrap();
        outputs.push((run.status.code(), run.stdout));
    }
    if std::env::var_os("ELEPHC_INHOUSE_KEEP").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0], (Some(0), b"hello\n".to_vec()));
}

/// Times split + in-tree assembly of every slice of a real listing, read straight from the
/// listing or, with `ELEPHC_INHOUSE_BENCH_RENDERED=1`, from rendered slices with the split's
/// pre-encoding on or off (`ELEPHC_INHOUSE_BENCH_RAW=1`):
/// `ELEPHC_INHOUSE_BENCH=<file.s> cargo test --release --lib inhouse_bench -- --ignored --nocapture`.
#[test]
#[ignore]
fn inhouse_bench() {
    let Ok(path) = std::env::var("ELEPHC_INHOUSE_BENCH") else {
        return;
    };
    let text = std::fs::read_to_string(path).expect("listing");
    let encode = std::env::var_os("ELEPHC_INHOUSE_BENCH_RAW").is_none();
    let started = std::time::Instant::now();
    let plan = super::super::asm_split::plan_split(&text, 6, "L", encode).expect("a legal cut");
    let (mut render, mut assemble_time, mut bytes) = (0f64, 0f64, 0usize);
    let mut two_pass_time = 0f64;
    let direct = std::env::var_os("ELEPHC_INHOUSE_BENCH_RENDERED").is_none();
    let names = super::Names::promoted(plan.promoted(), plan.local_prefix());
    for part in 0..plan.slices() {
        let mark = std::time::Instant::now();
        if direct {
            let (prologue, body, footer) = plan.assembler_parts(part);
            render += mark.elapsed().as_secs_f64();
            let mark = std::time::Instant::now();
            let object = super::assemble_parts(&[prologue.as_str(), body, footer.as_str()], names);
            assemble_time += mark.elapsed().as_secs_f64();
            // `ELEPHC_INHOUSE_BENCH_COMPARE=1` also times the two-pass reference and requires the
            // same object from it.
            if std::env::var_os("ELEPHC_INHOUSE_BENCH_COMPARE").is_some() {
                let mark = std::time::Instant::now();
                let reference = super::assemble_parts_two_pass(&[prologue.as_str(), body, footer.as_str()], names);
                two_pass_time += mark.elapsed().as_secs_f64();
                assert!(
                    matches!((&object, &reference), (Ok(a), Ok(b)) if a == b)
                        || (object.is_err() && reference.is_err()),
                    "slice {part}: the one-pass object differs from the two-pass one"
                );
            }
            bytes += object.map(|object| object.len()).unwrap_or(0);
            continue;
        }
        let mut slice = String::new();
        plan.render_slice(part, &mut |chunk| {
            slice.push_str(chunk);
            Ok(())
        })
        .expect("render");
        render += mark.elapsed().as_secs_f64();
        let mark = std::time::Instant::now();
        bytes += assemble(&slice).map(|object| object.len()).unwrap_or(0);
        assemble_time += mark.elapsed().as_secs_f64();
    }
    eprintln!(
        "encode={encode} total {:.2}s render {render:.2}s assemble {assemble_time:.2}s two-pass {two_pass_time:.2}s object bytes {bytes}",
        started.elapsed().as_secs_f64()
    );
}
