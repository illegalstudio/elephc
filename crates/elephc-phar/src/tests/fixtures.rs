//! Purpose:
//! Shared native PHAR, tar, ZIP, compression, and listing fixtures.
//!
//! Called from:
//! - Focused PHAR bridge unit-test modules.
//!
//! Key details:
//! - Fixture builders intentionally emit minimal deterministic archive structures.

use super::*;

/// Builds a minimal native PHAR fixture with entries carrying explicit flags.
pub(super) fn build_native_phar_with_flags(entries: &[(&str, &[u8], u32, u32)]) -> Vec<u8> {
    let mut manifest = Vec::new();
    manifest.extend_from_slice(&(entries.len() as u32).to_le_bytes());
    manifest.extend_from_slice(&[0x11, 0x00]);
    manifest.extend_from_slice(&0u32.to_le_bytes());
    manifest.extend_from_slice(&0u32.to_le_bytes());
    manifest.extend_from_slice(&0u32.to_le_bytes());
    for (name, stored, uncompressed_len, flags) in entries {
        manifest.extend_from_slice(&(name.len() as u32).to_le_bytes());
        manifest.extend_from_slice(name.as_bytes());
        manifest.extend_from_slice(&uncompressed_len.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&(stored.len() as u32).to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
        manifest.extend_from_slice(&flags.to_le_bytes());
        manifest.extend_from_slice(&0u32.to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"<?php __HALT_COMPILER(); ?>\r\n");
    out.extend_from_slice(&(manifest.len() as u32).to_le_bytes());
    out.extend_from_slice(&manifest);
    for (_, stored, _, _) in entries {
        out.extend_from_slice(stored);
    }
    out
}

/// Builds a minimal native PHAR fixture with uncompressed entries.
pub(super) fn build_native_phar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let entries = entries
        .iter()
        .map(|(name, content)| (*name, *content, content.len() as u32, PHAR_FILE_MODE_0644))
        .collect::<Vec<_>>();
    build_native_phar_with_flags(&entries)
}

/// Builds a raw-DEFLATE payload for PHAR gzip entry fixtures.
pub(super) fn deflate_payload(content: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(content).unwrap();
    encoder.finish().unwrap()
}

/// Builds a bzip2 payload for PHAR bzip2 entry fixtures.
pub(super) fn bzip2_payload(content: &[u8]) -> Vec<u8> {
    let mut encoder =
        bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
    encoder.write_all(content).unwrap();
    encoder.finish().unwrap()
}

/// Finds one parsed archive entry payload by name.
pub(super) fn entry_payload<'a>(entries: &'a [ArchiveEntry], name: &[u8]) -> Option<&'a [u8]> {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .map(|entry| entry.payload.as_slice())
}

/// Builds the serialized entry-name format returned by `entry_names_bytes`.
pub(super) fn serialized_names(names: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for name in names {
        out.extend_from_slice(&(name.len() as u64).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
    }
    out
}

/// Builds a small tar archive with regular-file entries.
pub(super) fn build_tar(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, content) in entries {
        let mut header = [0u8; 512];
        header[..name.len()].copy_from_slice(name.as_bytes());
        let size = format!("{:011o}\0", content.len());
        header[124..124 + size.len()].copy_from_slice(size.as_bytes());
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        for byte in &mut header[148..156] {
            *byte = b' ';
        }
        let checksum: u32 = header.iter().map(|&b| b as u32).sum();
        let checksum = format!("{:06o}\0 ", checksum);
        header[148..156].copy_from_slice(checksum.as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(content);
        out.resize(out.len() + round_up_to_512(content.len()).unwrap() - content.len(), 0);
    }
    out.extend_from_slice(&[0u8; 1024]);
    out
}

/// Builds a ZIP archive with matching payload CRCs in both header families.
pub(super) fn build_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, content, deflate) in entries {
        let local_offset = out.len() as u32;
        let stored = if *deflate {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(content).unwrap();
            encoder.finish().unwrap()
        } else {
            content.to_vec()
        };
        let method = if *deflate { ZIP_METHOD_DEFLATE } else { ZIP_METHOD_STORE };
        let crc = crc32(content);
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(stored.len() as u32).to_le_bytes());
        out.extend_from_slice(&(content.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&stored);

        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(stored.len() as u32).to_le_bytes());
        central.extend_from_slice(&(content.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u32.to_le_bytes());
        central.extend_from_slice(&local_offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}
