use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::Expr;
use crate::types::PhpType;

pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    match name {
        "strlen" => {
            emitter.comment("strlen()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("mov x0, x2");
            Some(PhpType::Int)
        }
        "intval" => {
            emitter.comment("intval()");
            let ty = emit_expr(&args[0], emitter, ctx, data);
            if ty == PhpType::Str {
                emitter.instruction("bl __rt_atoi");
            }
            Some(PhpType::Int)
        }
        "number_format" => {
            emitter.comment("number_format()");
            let t0 = emit_expr(&args[0], emitter, ctx, data);
            if t0 != PhpType::Float { emitter.instruction("scvtf d0, x0"); }
            emitter.instruction("str d0, [sp, #-16]!");

            if args.len() >= 2 {
                emit_expr(&args[1], emitter, ctx, data);
                emitter.instruction("str x0, [sp, #-16]!");
            } else {
                emitter.instruction("str xzr, [sp, #-16]!");
            }

            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("ldrb w0, [x1]");
                emitter.instruction("str x0, [sp, #-16]!");
            } else {
                emitter.instruction("mov x0, #46"); // '.'
                emitter.instruction("str x0, [sp, #-16]!");
            }

            if args.len() >= 4 {
                emit_expr(&args[3], emitter, ctx, data);
                emitter.instruction("cbz x2, 1f");
                emitter.instruction("ldrb w0, [x1]");
                emitter.instruction("b 2f");
                emitter.raw("1:");
                emitter.instruction("mov x0, #0");
                emitter.raw("2:");
                emitter.instruction("str x0, [sp, #-16]!");
            } else {
                emitter.instruction("mov x0, #44"); // ','
                emitter.instruction("str x0, [sp, #-16]!");
            }

            emitter.instruction("ldr x3, [sp], #16");
            emitter.instruction("ldr x2, [sp], #16");
            emitter.instruction("ldr x1, [sp], #16");
            emitter.instruction("ldr d0, [sp], #16");
            emitter.instruction("bl __rt_number_format");
            Some(PhpType::Str)
        }
        "substr" => {
            emitter.comment("substr()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!"); // save str
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("str x0, [sp, #-16]!"); // save offset
            if args.len() >= 3 {
                emit_expr(&args[2], emitter, ctx, data);
                emitter.instruction("mov x3, x0"); // length arg
            } else {
                emitter.instruction("mov x3, #-1"); // sentinel: use remaining
            }
            emitter.instruction("ldr x0, [sp], #16"); // offset
            emitter.instruction("ldp x1, x2, [sp], #16"); // str ptr, len
            // Handle negative offset
            emitter.instruction("cmp x0, #0");
            emitter.instruction("b.ge 1f");
            emitter.instruction("add x0, x2, x0"); // offset = len + offset
            emitter.instruction("cmp x0, #0");
            emitter.instruction("csel x0, xzr, x0, lt"); // clamp to 0
            emitter.raw("1:");
            // Clamp offset to len
            emitter.instruction("cmp x0, x2");
            emitter.instruction("csel x0, x2, x0, gt");
            // Adjust ptr and compute result len
            emitter.instruction("add x1, x1, x0");
            emitter.instruction("sub x2, x2, x0");
            // If length arg provided and < remaining, use it
            emitter.instruction("cmn x3, #1"); // check if x3 == -1
            emitter.instruction("b.eq 2f");
            emitter.instruction("cmp x3, #0");
            emitter.instruction("csel x3, xzr, x3, lt"); // clamp negative length to 0
            emitter.instruction("cmp x3, x2");
            emitter.instruction("csel x2, x3, x2, lt"); // min(length, remaining)
            emitter.raw("2:");
            Some(PhpType::Str)
        }
        "strpos" => {
            emitter.comment("strpos()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1"); // needle ptr
            emitter.instruction("mov x4, x2"); // needle len
            emitter.instruction("ldp x1, x2, [sp], #16"); // haystack
            emitter.instruction("bl __rt_strpos");
            Some(PhpType::Int)
        }
        "strrpos" => {
            emitter.comment("strrpos()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            emitter.instruction("ldp x1, x2, [sp], #16");
            emitter.instruction("bl __rt_strrpos");
            Some(PhpType::Int)
        }
        "strstr" => {
            emitter.comment("strstr()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            emitter.instruction("ldp x1, x2, [sp], #16");
            // Find position first
            emitter.instruction("stp x1, x2, [sp, #-16]!"); // save haystack
            emitter.instruction("bl __rt_strpos");
            emitter.instruction("ldp x1, x2, [sp], #16");
            // If not found (x0 < 0), return empty string
            let found = ctx.next_label("strstr_found");
            emitter.instruction("cmp x0, #0");
            emitter.instruction(&format!("b.ge {}", found));
            emitter.instruction("mov x2, #0"); // empty string
            let end = ctx.next_label("strstr_end");
            emitter.instruction(&format!("b {}", end));
            emitter.label(&found);
            emitter.instruction("add x1, x1, x0");
            emitter.instruction("sub x2, x2, x0");
            emitter.label(&end);
            Some(PhpType::Str)
        }
        "strtolower" => {
            emitter.comment("strtolower()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_strtolower");
            Some(PhpType::Str)
        }
        "strtoupper" => {
            emitter.comment("strtoupper()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_strtoupper");
            Some(PhpType::Str)
        }
        "ucfirst" => {
            emitter.comment("ucfirst()");
            emit_expr(&args[0], emitter, ctx, data);
            // Copy to concat_buf, uppercase first char
            emitter.instruction("bl __rt_strcopy");
            emitter.instruction("cbz x2, 1f");
            emitter.instruction("ldrb w9, [x1]");
            emitter.instruction("cmp w9, #97"); // 'a'
            emitter.instruction("b.lt 1f");
            emitter.instruction("cmp w9, #122"); // 'z'
            emitter.instruction("b.gt 1f");
            emitter.instruction("sub w9, w9, #32");
            emitter.instruction("strb w9, [x1]");
            emitter.raw("1:");
            Some(PhpType::Str)
        }
        "lcfirst" => {
            emitter.comment("lcfirst()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_strcopy");
            emitter.instruction("cbz x2, 1f");
            emitter.instruction("ldrb w9, [x1]");
            emitter.instruction("cmp w9, #65"); // 'A'
            emitter.instruction("b.lt 1f");
            emitter.instruction("cmp w9, #90"); // 'Z'
            emitter.instruction("b.gt 1f");
            emitter.instruction("add w9, w9, #32");
            emitter.instruction("strb w9, [x1]");
            emitter.raw("1:");
            Some(PhpType::Str)
        }
        "trim" => {
            emitter.comment("trim()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_trim");
            Some(PhpType::Str)
        }
        "ltrim" => {
            emitter.comment("ltrim()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_ltrim");
            Some(PhpType::Str)
        }
        "rtrim" => {
            emitter.comment("rtrim()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_rtrim");
            Some(PhpType::Str)
        }
        "str_repeat" => {
            emitter.comment("str_repeat()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0"); // times
            emitter.instruction("ldp x1, x2, [sp], #16");
            emitter.instruction("bl __rt_str_repeat");
            Some(PhpType::Str)
        }
        "strrev" => {
            emitter.comment("strrev()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("bl __rt_strrev");
            Some(PhpType::Str)
        }
        "ord" => {
            emitter.comment("ord()");
            emit_expr(&args[0], emitter, ctx, data);
            // x1=ptr, x2=len — return first byte as int
            emitter.instruction("ldrb w0, [x1]");
            Some(PhpType::Int)
        }
        "chr" => {
            emitter.comment("chr()");
            emit_expr(&args[0], emitter, ctx, data);
            // x0 = char code — write single byte to concat_buf
            emitter.instruction("bl __rt_chr");
            Some(PhpType::Str)
        }
        "strcmp" => {
            emitter.comment("strcmp()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            emitter.instruction("ldp x1, x2, [sp], #16");
            emitter.instruction("bl __rt_strcmp");
            Some(PhpType::Int)
        }
        "strcasecmp" => {
            emitter.comment("strcasecmp()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            emitter.instruction("ldp x1, x2, [sp], #16");
            emitter.instruction("bl __rt_strcasecmp");
            Some(PhpType::Int)
        }
        "str_contains" => {
            emitter.comment("str_contains()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            emitter.instruction("ldp x1, x2, [sp], #16");
            emitter.instruction("bl __rt_strpos");
            // x0 >= 0 means found
            emitter.instruction("cmp x0, #0");
            emitter.instruction("cset x0, ge");
            Some(PhpType::Bool)
        }
        "str_starts_with" => {
            emitter.comment("str_starts_with()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            emitter.instruction("ldp x1, x2, [sp], #16");
            emitter.instruction("bl __rt_str_starts_with");
            Some(PhpType::Bool)
        }
        "str_ends_with" => {
            emitter.comment("str_ends_with()");
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!");
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1");
            emitter.instruction("mov x4, x2");
            emitter.instruction("ldp x1, x2, [sp], #16");
            emitter.instruction("bl __rt_str_ends_with");
            Some(PhpType::Bool)
        }
        "str_replace" => {
            emitter.comment("str_replace()");
            // str_replace($search, $replace, $subject)
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!"); // search
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!"); // replace
            emit_expr(&args[2], emitter, ctx, data);
            // x1/x2 = subject, stack has replace then search
            emitter.instruction("mov x5, x1"); // subject ptr
            emitter.instruction("mov x6, x2"); // subject len
            emitter.instruction("ldp x3, x4, [sp], #16"); // replace
            emitter.instruction("ldp x1, x2, [sp], #16"); // search
            // x1/x2=search, x3/x4=replace, x5/x6=subject
            emitter.instruction("bl __rt_str_replace");
            Some(PhpType::Str)
        }
        "explode" => {
            emitter.comment("explode()");
            // explode($delimiter, $string)
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!"); // delimiter
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x1"); // string ptr
            emitter.instruction("mov x4, x2"); // string len
            emitter.instruction("ldp x1, x2, [sp], #16"); // delimiter
            emitter.instruction("bl __rt_explode");
            Some(PhpType::Array(Box::new(PhpType::Str)))
        }
        "implode" => {
            emitter.comment("implode()");
            // implode($glue, $array)
            emit_expr(&args[0], emitter, ctx, data);
            emitter.instruction("stp x1, x2, [sp, #-16]!"); // glue
            emit_expr(&args[1], emitter, ctx, data);
            emitter.instruction("mov x3, x0"); // array ptr
            emitter.instruction("ldp x1, x2, [sp], #16"); // glue
            emitter.instruction("bl __rt_implode");
            Some(PhpType::Str)
        }
        _ => None,
    }
}
