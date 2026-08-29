//! Palworld's outer `.sav` container format.
//!
//! `uesave` only understands already-decompressed GVAS bytes; the container
//! wrapping those bytes (`uncompressed_len u32 LE | compressed_len u32 LE |
//! magic (3 bytes) | save_type (1 byte) | payload`) is Palworld-specific and
//! not part of Unreal's save format. Ported from `decompress_sav`/
//! `compress_sav` in `NFZ-441/Palworld-Co-op-to-Dedicated-Server-Migration-Tool`'s
//! `fix_host_save.py` (MIT), which itself supports three magics:
//!
//! - `PlZ` -- zlib, single-compressed if `save_type == 0x31`, or
//!   double-compressed (nested) if `save_type == 0x32`.
//! - `PlM` -- Oodle, decompressed via `oozextract`. That crate only
//!   implements decompression (confirmed in its source -- no compress/encode
//!   function exists), so [`compress_sav`] cannot write `PlM`; it returns
//!   [`SavError::UnsupportedWrite`] instead of silently doing the wrong
//!   thing.
//! - `CNK` -- a chunked format that re-embeds a fresh 12-byte header at
//!   offset 12 (ignoring the outer header's length fields) and only ever
//!   wraps `PlZ`. Read-only, matching the Python reference, which has no
//!   `CNK` write path either.
//!
//! Note: unlike the Python reference (which always single-compresses on
//! write regardless of `save_type`, so round-tripping a `0x32` file writes a
//! header claiming double-compression over a single-compressed payload),
//! [`compress_sav`] actually double-compresses for `save_type == 0x32`, so
//! read-then-write stays functionally correct.

use std::io::{Read, Write};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

const HEADER_LEN: usize = 12;

#[derive(Debug)]
pub enum SavError {
    TooShort,
    UnknownMagic([u8; 3]),
    UnknownPlzSaveType(u8),
    UnknownCnkSubFormat([u8; 3]),
    Io(std::io::Error),
    Ooz(oozextract::OozError),
    LengthMismatch {
        expected: usize,
        actual: usize,
    },
    // Only reachable from `compress_sav`, which has no caller yet -- see its
    // doc comment.
    #[allow(dead_code)]
    UnsupportedWrite(&'static str),
}

impl std::fmt::Display for SavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SavError::TooShort => write!(f, "save data is shorter than the container header"),
            SavError::UnknownMagic(magic) => {
                write!(
                    f,
                    "unknown save format magic: {:?}",
                    String::from_utf8_lossy(magic)
                )
            }
            SavError::UnknownPlzSaveType(save_type) => {
                write!(f, "unknown PlZ save type: 0x{save_type:02X}")
            }
            SavError::UnknownCnkSubFormat(magic) => write!(
                f,
                "unknown CNK sub-format: {:?}",
                String::from_utf8_lossy(magic)
            ),
            SavError::Io(err) => write!(f, "io error: {err}"),
            SavError::Ooz(err) => write!(f, "oodle decompression failed: {err}"),
            SavError::LengthMismatch { expected, actual } => write!(
                f,
                "decompressed length mismatch: expected {expected}, got {actual}"
            ),
            SavError::UnsupportedWrite(reason) => write!(f, "cannot write save: {reason}"),
        }
    }
}

impl std::error::Error for SavError {}

impl From<std::io::Error> for SavError {
    fn from(err: std::io::Error) -> Self {
        SavError::Io(err)
    }
}

pub struct DecodedSav {
    pub magic: [u8; 3],
    pub save_type: u8,
    pub data: Vec<u8>,
}

fn zlib_decompress(data: &[u8]) -> Result<Vec<u8>, SavError> {
    let mut out = Vec::new();
    ZlibDecoder::new(data).read_to_end(&mut out)?;
    Ok(out)
}

// Only used by `compress_sav`, which has no caller yet -- writing saves back
// out is future work (the actual conversion feature this is groundwork
// for), but the round-trip is exercised by this module's tests.
#[allow(dead_code)]
fn zlib_compress(data: &[u8]) -> Result<Vec<u8>, SavError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    Ok(encoder.finish()?)
}

pub fn decompress_sav(data: &[u8]) -> Result<DecodedSav, SavError> {
    if data.len() < HEADER_LEN {
        return Err(SavError::TooShort);
    }

    let uncompressed_len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    let compressed_len = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    let magic: [u8; 3] = data[8..11].try_into().unwrap();
    let save_type = data[11];
    let payload = &data[HEADER_LEN..];

    match &magic {
        b"PlZ" => {
            let raw = match save_type {
                0x31 => zlib_decompress(payload)?,
                0x32 => zlib_decompress(&zlib_decompress(payload)?)?,
                other => return Err(SavError::UnknownPlzSaveType(other)),
            };
            Ok(DecodedSav {
                magic,
                save_type,
                data: raw,
            })
        }
        b"PlM" => {
            if payload.len() < compressed_len {
                return Err(SavError::TooShort);
            }
            let mut out = vec![0u8; uncompressed_len];
            let mut extractor = oozextract::Extractor::new();
            let written = extractor
                .read_from_slice(&payload[..compressed_len], &mut out)
                .map_err(SavError::Ooz)?;
            if written != uncompressed_len {
                return Err(SavError::LengthMismatch {
                    expected: uncompressed_len,
                    actual: written,
                });
            }
            Ok(DecodedSav {
                magic,
                save_type,
                data: out,
            })
        }
        b"CNK" => {
            if payload.len() < HEADER_LEN {
                return Err(SavError::TooShort);
            }
            let inner_magic: [u8; 3] = payload[8..11].try_into().unwrap();
            let inner_save_type = payload[11];
            if &inner_magic != b"PlZ" {
                return Err(SavError::UnknownCnkSubFormat(inner_magic));
            }
            let raw = zlib_decompress(&payload[HEADER_LEN..])?;
            Ok(DecodedSav {
                magic,
                save_type: inner_save_type,
                data: raw,
            })
        }
        _ => Err(SavError::UnknownMagic(magic)),
    }
}

// No caller yet -- `inspect.rs` only reads. Writing saves back out is the
// actual conversion feature this container layer is groundwork for.
#[allow(dead_code)]
pub fn compress_sav(data: &[u8], save_type: u8, magic: [u8; 3]) -> Result<Vec<u8>, SavError> {
    match &magic {
        b"PlZ" => {
            let compressed = match save_type {
                0x31 => zlib_compress(data)?,
                0x32 => zlib_compress(&zlib_compress(data)?)?,
                other => return Err(SavError::UnknownPlzSaveType(other)),
            };
            let mut out = Vec::with_capacity(HEADER_LEN + compressed.len());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            out.extend_from_slice(&magic);
            out.push(save_type);
            out.extend_from_slice(&compressed);
            Ok(out)
        }
        b"PlM" => Err(SavError::UnsupportedWrite(
            "PlM (Oodle) compression is not supported - oozextract only implements decompression",
        )),
        _ => Err(SavError::UnsupportedWrite(
            "only PlZ (zlib) saves can be written",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> Vec<u8> {
        (0..2000).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn roundtrip_plz_single() {
        let data = sample_data();
        let container = compress_sav(&data, 0x31, *b"PlZ").unwrap();
        let decoded = decompress_sav(&container).unwrap();
        assert_eq!(decoded.magic, *b"PlZ");
        assert_eq!(decoded.save_type, 0x31);
        assert_eq!(decoded.data, data);
    }

    #[test]
    fn roundtrip_plz_double() {
        let data = sample_data();
        let container = compress_sav(&data, 0x32, *b"PlZ").unwrap();
        let decoded = decompress_sav(&container).unwrap();
        assert_eq!(decoded.save_type, 0x32);
        assert_eq!(decoded.data, data);
    }

    #[test]
    fn decodes_cnk_wrapping_plz() {
        let data = sample_data();
        let inner = compress_sav(&data, 0x31, *b"PlZ").unwrap();
        // CNK's own outer length fields are unused by readers (ours and the
        // Python reference both ignore them), only the magic matters.
        let mut cnk = Vec::new();
        cnk.extend_from_slice(&[0u8; 8]);
        cnk.extend_from_slice(b"CNK");
        cnk.push(0);
        cnk.extend_from_slice(&inner);

        let decoded = decompress_sav(&cnk).unwrap();
        assert_eq!(decoded.magic, *b"CNK");
        assert_eq!(decoded.save_type, 0x31);
        assert_eq!(decoded.data, data);
    }

    #[test]
    fn decompress_rejects_unknown_magic() {
        let mut data = vec![0u8; HEADER_LEN];
        data[8..11].copy_from_slice(b"Xyz");
        assert!(matches!(
            decompress_sav(&data),
            Err(SavError::UnknownMagic(_))
        ));
    }

    #[test]
    fn decompress_rejects_short_input() {
        assert!(matches!(decompress_sav(&[0u8; 4]), Err(SavError::TooShort)));
    }

    #[test]
    fn compress_rejects_plm() {
        assert!(matches!(
            compress_sav(&sample_data(), 0, *b"PlM"),
            Err(SavError::UnsupportedWrite(_))
        ));
    }

    #[test]
    fn compress_rejects_unknown_magic() {
        assert!(matches!(
            compress_sav(&sample_data(), 0, *b"Xyz"),
            Err(SavError::UnsupportedWrite(_))
        ));
    }
}
