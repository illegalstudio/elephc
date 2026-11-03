//! Purpose:
//! Bounded binary wire protocol between a web worker, its prestarted handler
//! broker, and the pool/request process that executes one PHP request.
//!
//! Called from:
//! - `crate::handler_broker`, for request dispatch and handler response frames.
//! - `crate::request_state`, when captured PHP output becomes response chunks.
//!
//! Key details:
//! - Metadata and individual body chunks are length-checked before allocation.
//! - Response bodies have no whole-body frame: fixed-size chunks provide
//!   backpressure instead of requiring a second complete response allocation.

use std::io::{self, Read, Write};
use std::os::fd::RawFd;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Maximum request body accepted by the internal broker protocol.
pub(crate) const MAX_REQUEST_BODY_BYTES: usize = 64 * 1024 * 1024;
/// Maximum aggregate request metadata accepted by the broker protocol.
const MAX_REQUEST_METADATA_BYTES: usize = 4 * 1024 * 1024;
/// Maximum aggregate response-header bytes accepted from a handler child.
const MAX_RESPONSE_HEADER_BYTES: usize = 1024 * 1024;
/// Maximum number of request or response headers in one frame.
const MAX_HEADER_COUNT: usize = 1024;
/// Maximum response chunk written atomically by a handler child.
pub(crate) const MAX_RESPONSE_CHUNK_BYTES: usize = 64 * 1024;
/// Protocol marker that rejects accidental or stale request-channel payloads.
/// Bumped when the request frame gained `request_scheme` and `https`.
const REQUEST_MAGIC: &[u8; 8] = b"ELEWEB02";
/// Response frame containing status and headers.
const RESPONSE_START: u8 = 1;
/// Response frame containing one bounded body chunk.
const RESPONSE_CHUNK: u8 = 2;
/// Response frame marking successful handler completion.
const RESPONSE_END: u8 = 3;

/// Owned HTTP request snapshot transferred to the single-threaded broker.
pub(crate) struct HandlerRequest {
    pub(crate) method: String,
    pub(crate) uri: String,
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
    pub(crate) remote_addr: String,
    pub(crate) remote_port: u16,
    pub(crate) server_addr: String,
    pub(crate) server_port: u16,
    pub(crate) protocol: String,
    pub(crate) request_scheme: String,
    pub(crate) https: bool,
}

/// Status and headers committed by the handler before its first body chunk.
pub(crate) struct ResponseStart {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
}

/// One decoded child-to-worker response frame.
pub(crate) enum ResponseFrame {
    Start(ResponseStart),
    Chunk(Vec<u8>),
    End,
}

/// Creates an invalid-data error with a stable broker-protocol message.
fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Adds `amount` to a bounded aggregate byte count.
fn account_bytes(total: &mut usize, amount: usize, limit: usize) -> io::Result<()> {
    *total = total
        .checked_add(amount)
        .filter(|next| *next <= limit)
        .ok_or_else(|| invalid_data("broker frame exceeds its byte limit"))?;
    Ok(())
}

/// Writes one u32-length-prefixed byte slice to a synchronous writer.
fn write_bytes(writer: &mut impl Write, bytes: &[u8]) -> io::Result<()> {
    let len = u32::try_from(bytes.len()).map_err(|_| invalid_data("broker field is too large"))?;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(bytes)
}

/// Reads one bounded u32-length-prefixed byte vector from a synchronous reader.
fn read_bytes(
    reader: &mut impl Read,
    total: &mut usize,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    reader.read_exact(&mut len)?;
    let len = u32::from_be_bytes(len) as usize;
    account_bytes(total, len, limit)?;
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

/// Converts protocol bytes into a lossy but always-owned HTTP string.
fn wire_string(bytes: Vec<u8>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Validates request sizes before the worker begins sending them to the broker.
fn validate_request(request: &HandlerRequest) -> io::Result<()> {
    if request.body.len() > MAX_REQUEST_BODY_BYTES {
        return Err(invalid_data("request body exceeds the broker limit"));
    }
    if request.headers.len() > MAX_HEADER_COUNT {
        return Err(invalid_data("request has too many headers"));
    }
    let mut metadata = 0usize;
    for value in [
        request.method.as_bytes(),
        request.uri.as_bytes(),
        request.path.as_bytes(),
        request.query.as_bytes(),
        request.remote_addr.as_bytes(),
        request.server_addr.as_bytes(),
        request.protocol.as_bytes(),
        request.request_scheme.as_bytes(),
    ] {
        account_bytes(&mut metadata, value.len(), MAX_REQUEST_METADATA_BYTES)?;
    }
    for (name, value) in &request.headers {
        account_bytes(&mut metadata, name.len(), MAX_REQUEST_METADATA_BYTES)?;
        account_bytes(&mut metadata, value.len(), MAX_REQUEST_METADATA_BYTES)?;
    }
    Ok(())
}

/// Writes a validated request snapshot to its dedicated async broker channel.
pub(crate) async fn write_request_async(
    writer: &mut (impl AsyncWrite + Unpin),
    request: &HandlerRequest,
) -> io::Result<()> {
    validate_request(request)?;
    writer.write_all(REQUEST_MAGIC).await?;
    for value in [
        request.method.as_bytes(),
        request.uri.as_bytes(),
        request.path.as_bytes(),
        request.query.as_bytes(),
    ] {
        let len = u32::try_from(value.len()).map_err(|_| invalid_data("request field is too large"))?;
        writer.write_all(&len.to_be_bytes()).await?;
        writer.write_all(value).await?;
    }
    let header_count = u32::try_from(request.headers.len())
        .map_err(|_| invalid_data("request has too many headers"))?;
    writer.write_all(&header_count.to_be_bytes()).await?;
    for (name, value) in &request.headers {
        for bytes in [name.as_bytes(), value.as_bytes()] {
            let len = u32::try_from(bytes.len())
                .map_err(|_| invalid_data("request header field is too large"))?;
            writer.write_all(&len.to_be_bytes()).await?;
            writer.write_all(bytes).await?;
        }
    }
    let body_len = u64::try_from(request.body.len())
        .map_err(|_| invalid_data("request body is too large"))?;
    writer.write_all(&body_len.to_be_bytes()).await?;
    writer.write_all(&request.body).await?;
    for value in [
        request.remote_addr.as_bytes(),
        request.server_addr.as_bytes(),
        request.protocol.as_bytes(),
        request.request_scheme.as_bytes(),
    ] {
        let len = u32::try_from(value.len()).map_err(|_| invalid_data("request metadata is too large"))?;
        writer.write_all(&len.to_be_bytes()).await?;
        writer.write_all(value).await?;
    }
    writer.write_all(&request.remote_port.to_be_bytes()).await?;
    writer.write_all(&request.server_port.to_be_bytes()).await?;
    writer.write_all(&[u8::from(request.https)]).await
}

/// Reads and validates one complete request snapshot in the broker process.
pub(crate) fn read_request(reader: &mut impl Read) -> io::Result<HandlerRequest> {
    let mut magic = [0u8; 8];
    reader.read_exact(&mut magic).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("request marker was incomplete: {error}"),
        )
    })?;
    if &magic != REQUEST_MAGIC {
        return Err(invalid_data("invalid broker request marker"));
    }
    let mut metadata = 0usize;
    let method = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
    let uri = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
    let path = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
    let query = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
    let mut count = [0u8; 4];
    reader.read_exact(&mut count)?;
    let count = u32::from_be_bytes(count) as usize;
    if count > MAX_HEADER_COUNT {
        return Err(invalid_data("request has too many headers"));
    }
    let mut headers = Vec::with_capacity(count);
    for _ in 0..count {
        let name = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
        let value = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
        headers.push((name, value));
    }
    let mut body_len = [0u8; 8];
    reader.read_exact(&mut body_len)?;
    let body_len = usize::try_from(u64::from_be_bytes(body_len))
        .ok()
        .filter(|len| *len <= MAX_REQUEST_BODY_BYTES)
        .ok_or_else(|| invalid_data("request body exceeds the broker limit"))?;
    let mut body = vec![0; body_len];
    reader.read_exact(&mut body)?;
    let remote_addr = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
    let server_addr = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
    let protocol = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
    let request_scheme = wire_string(read_bytes(reader, &mut metadata, MAX_REQUEST_METADATA_BYTES)?);
    let mut remote_port = [0u8; 2];
    let mut server_port = [0u8; 2];
    reader.read_exact(&mut remote_port)?;
    reader.read_exact(&mut server_port)?;
    let mut https = [0u8; 1];
    reader.read_exact(&mut https)?;
    Ok(HandlerRequest {
        method,
        uri,
        path,
        query,
        headers,
        body,
        remote_addr,
        remote_port: u16::from_be_bytes(remote_port),
        server_addr,
        server_port: u16::from_be_bytes(server_port),
        protocol,
        request_scheme,
        https: https[0] != 0,
    })
}

/// Writes the status/header frame that commits a handler response.
pub(crate) unsafe fn write_response_start(
    fd: RawFd,
    status: u16,
    headers: &[(String, String)],
) -> bool {
    if headers.len() > MAX_HEADER_COUNT {
        return false;
    }
    let mut total = 0usize;
    if headers.iter().any(|(name, value)| {
        account_bytes(&mut total, name.len(), MAX_RESPONSE_HEADER_BYTES).is_err()
            || account_bytes(&mut total, value.len(), MAX_RESPONSE_HEADER_BYTES).is_err()
    }) {
        return false;
    }
    if !write_all_fd(fd, &[RESPONSE_START])
        || !write_all_fd(fd, &status.to_be_bytes())
        || !write_all_fd(fd, &(headers.len() as u32).to_be_bytes())
    {
        return false;
    }
    for (name, value) in headers {
        let mut frame = Vec::with_capacity(8 + name.len() + value.len());
        if write_bytes(&mut frame, name.as_bytes()).is_err()
            || write_bytes(&mut frame, value.as_bytes()).is_err()
            || !write_all_fd(fd, &frame)
        {
            return false;
        }
    }
    true
}

/// Writes body bytes as independently bounded response-chunk frames.
pub(crate) unsafe fn write_response_chunks(fd: RawFd, mut body: &[u8]) -> bool {
    while !body.is_empty() {
        let chunk_len = body.len().min(MAX_RESPONSE_CHUNK_BYTES);
        let (chunk, rest) = body.split_at(chunk_len);
        if !write_all_fd(fd, &[RESPONSE_CHUNK])
            || !write_all_fd(fd, &(chunk_len as u32).to_be_bytes())
            || !write_all_fd(fd, chunk)
        {
            return false;
        }
        body = rest;
    }
    true
}

/// Writes the successful end-of-response frame.
pub(crate) unsafe fn write_response_end(fd: RawFd) -> bool {
    write_all_fd(fd, &[RESPONSE_END])
}

/// Repeats `write(2)` until every byte is delivered or the peer closes.
unsafe fn write_all_fd(fd: RawFd, mut bytes: &[u8]) -> bool {
    while !bytes.is_empty() {
        let written = libc::write(fd, bytes.as_ptr().cast(), bytes.len());
        if written < 0 {
            if io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return false;
        }
        if written == 0 {
            return false;
        }
        bytes = &bytes[written as usize..];
    }
    true
}

/// Reads one child response frame without buffering subsequent body chunks.
pub(crate) async fn read_response_frame(
    reader: &mut (impl AsyncRead + Unpin),
) -> io::Result<ResponseFrame> {
    let tag = reader.read_u8().await?;
    match tag {
        RESPONSE_START => {
            let status = reader.read_u16().await?;
            let count = reader.read_u32().await? as usize;
            if count > MAX_HEADER_COUNT {
                return Err(invalid_data("response has too many headers"));
            }
            let mut total = 0usize;
            let mut headers = Vec::with_capacity(count);
            for _ in 0..count {
                let name = read_bytes_async(reader, &mut total, MAX_RESPONSE_HEADER_BYTES).await?;
                let value = read_bytes_async(reader, &mut total, MAX_RESPONSE_HEADER_BYTES).await?;
                headers.push((wire_string(name), wire_string(value)));
            }
            Ok(ResponseFrame::Start(ResponseStart { status, headers }))
        }
        RESPONSE_CHUNK => {
            let len = reader.read_u32().await? as usize;
            if len > MAX_RESPONSE_CHUNK_BYTES {
                return Err(invalid_data("response chunk exceeds its byte limit"));
            }
            let mut bytes = vec![0; len];
            reader.read_exact(&mut bytes).await?;
            Ok(ResponseFrame::Chunk(bytes))
        }
        RESPONSE_END => Ok(ResponseFrame::End),
        _ => Err(invalid_data("unknown broker response frame")),
    }
}

/// Reads one bounded u32-length-prefixed vector from an async reader.
async fn read_bytes_async(
    reader: &mut (impl AsyncRead + Unpin),
    total: &mut usize,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let len = reader.read_u32().await? as usize;
    account_bytes(total, len, limit)?;
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes).await?;
    Ok(bytes)
}
