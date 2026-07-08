//! Purpose:
//! Declarative eval registry entry for `gzcompress`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime dispatch is declared here and implemented through the gzip/zlib hook.

use super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "gzcompress",
    area: String,
    params: [data, level = EvalBuiltinDefaultValue::Int(-1)],
    direct: Gzip,
    values: Gzip,
}

use super::super::super::*;
use flate2::read::{DeflateDecoder, ZlibDecoder};
use flate2::write::{DeflateEncoder, ZlibEncoder};
use flate2::Compression;
use std::io::{Read, Write};

/// Evaluates PHP `gzcompress(...)` over eval expressions.
pub(in crate::interpreter) fn eval_builtin_gzcompress(
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::gzcompress::eval_builtin_gzip_named("gzcompress", args, context, scope, values)
}

/// Applies PHP `gzcompress(...)` to already evaluated arguments.
pub(in crate::interpreter) fn eval_gzcompress_result(
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    super::gzcompress::eval_gzip_named_result("gzcompress", evaluated_args, values)
}

/// Evaluates a named gzip/zlib builtin over eval expressions.
pub(in crate::interpreter) fn eval_builtin_gzip_named(
    name: &str,
    args: &[EvalExpr],
    context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let mut evaluated_args = Vec::with_capacity(args.len());
    for arg in args {
        evaluated_args.push(eval_expr(arg, context, scope, values)?);
    }
    eval_gzip_named_result(name, &evaluated_args, values)
}

/// Dispatches one materialized gzip/zlib builtin call.
pub(in crate::interpreter) fn eval_gzip_named_result(
    name: &str,
    evaluated_args: &[RuntimeCellHandle],
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let (data, option) = match evaluated_args {
        [data] => (*data, None),
        [data, option] => (*data, Some(*option)),
        _ => return Err(EvalStatus::RuntimeFatal),
    };
    let data = values.string_bytes(data)?;
    match name {
        "gzcompress" => eval_gz_encode(data, option, true, values),
        "gzdeflate" => eval_gz_encode(data, option, false, values),
        "gzuncompress" => eval_gz_decode(data, true, values),
        "gzinflate" => eval_gz_decode(data, false, values),
        _ => Err(EvalStatus::UnsupportedConstruct),
    }
}

/// Encodes data as zlib-wrapped or raw-DEFLATE bytes.
fn eval_gz_encode(
    data: Vec<u8>,
    level: Option<RuntimeCellHandle>,
    zlib_wrapped: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let compression = eval_gz_compression(level, values)?;
    let compressed = if zlib_wrapped {
        let mut encoder = ZlibEncoder::new(Vec::new(), compression);
        eval_gz_write_all(&mut encoder, &data)?;
        encoder.finish().map_err(|_| EvalStatus::RuntimeFatal)?
    } else {
        let mut encoder = DeflateEncoder::new(Vec::new(), compression);
        eval_gz_write_all(&mut encoder, &data)?;
        encoder.finish().map_err(|_| EvalStatus::RuntimeFatal)?
    };
    values.string_bytes_value(&compressed)
}

/// Decodes zlib-wrapped or raw-DEFLATE bytes and returns false on inflate errors.
fn eval_gz_decode(
    data: Vec<u8>,
    zlib_wrapped: bool,
    values: &mut impl RuntimeValueOps,
) -> Result<RuntimeCellHandle, EvalStatus> {
    let decoded = if zlib_wrapped {
        eval_gz_read(ZlibDecoder::new(data.as_slice()))
    } else {
        eval_gz_read(DeflateDecoder::new(data.as_slice()))
    };
    match decoded {
        Ok(decoded) => values.string_bytes_value(&decoded),
        Err(_) => values.bool_value(false),
    }
}

/// Converts PHP's optional compression level to a flate2 compression value.
fn eval_gz_compression(
    level: Option<RuntimeCellHandle>,
    values: &mut impl RuntimeValueOps,
) -> Result<Compression, EvalStatus> {
    let Some(level) = level else {
        return Ok(Compression::default());
    };
    let level = eval_int_value(level, values)?;
    if level < 0 {
        return Ok(Compression::default());
    }
    let level = u32::try_from(level).map_err(|_| EvalStatus::RuntimeFatal)?;
    Ok(Compression::new(level.min(9)))
}

/// Writes all source bytes into a compression stream.
fn eval_gz_write_all<W: Write>(
    encoder: &mut W,
    data: &[u8],
) -> Result<(), EvalStatus> {
    encoder
        .write_all(data)
        .map_err(|_| EvalStatus::RuntimeFatal)
}

/// Reads all bytes from a decompression stream.
fn eval_gz_read<R: Read>(mut decoder: R) -> std::io::Result<Vec<u8>> {
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded)?;
    Ok(decoded)
}
