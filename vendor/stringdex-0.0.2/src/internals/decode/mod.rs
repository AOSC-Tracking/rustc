use std::error::Error;
use std::fmt::{self, Debug, Display, Formatter};

use super::encode::BitmapContainer;

pub fn unescape_ascii(mut string: &[u8], out: &mut Vec<u8>) -> Option<usize> {
    let mut i = 0;
    loop {
        let rem = match string {
            [] => return Some(i),
            [b'\\', b't', rem @ ..] => {
                out.push(b'\t');
                rem
            }
            [b'\\', b'r', rem @ ..] => {
                out.push(b'\r');
                rem
            }
            [b'\\', b'n', rem @ ..] => {
                out.push(b'\n');
                rem
            }
            [b'\\', b'\'', rem @ ..] => {
                out.push(b'\'');
                rem
            }
            [b'\\', b'"', rem @ ..] => {
                out.push(b'\"');
                rem
            }
            [b'\\', b'\\', rem @ ..] => {
                out.push(b'\\');
                rem
            }
            [b'\\', b'x', a, b, rem @ ..] => {
                let tbl = [
                    b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'a', b'b', b'c',
                    b'd', b'e', b'f',
                ];
                out.push(
                    u8::try_from(tbl.iter().position(|x| x == a)? << 4).ok()?
                        | u8::try_from(tbl.iter().position(|x| x == b)?).ok()?,
                );
                rem
            }
            [c, rem @ ..] => {
                out.push(*c);
                rem
            }
        };
        i += string.len() - rem.len();
        string = rem;
    }
}

pub fn read_vlqhex_from_bytes(string: &[u8]) -> Option<(u32, usize)> {
    let mut n = 0u32;
    let mut i = 0;
    while let Some(&c) = string.get(i) {
        i += 1;
        n = (n << 4) | u32::from(c & 0xF);
        if c >= 96 {
            return Some((n, i));
        }
    }
    None
}

pub fn read_48bit_from_bytes_be(string: &[u8]) -> u64 {
    ((string[0] as u64) << 40)
        | ((string[1] as u64) << 32)
        | ((string[2] as u64) << 24)
        | ((string[3] as u64) << 16)
        | ((string[4] as u64) << 8)
        | (string[5] as u64)
}

/// Read two characters hexadecimal from `string`.
pub fn read_hex_from_string(string: &str) -> Result<u8, HexError> {
    let tbl = [
        b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'a', b'b', b'c', b'd', b'e',
        b'f',
    ];
    match string[..].as_bytes() {
        &[a, b, ..] => {
            let ax = tbl
                .iter()
                .position(|x| *x == a)
                .and_then(|pos| u8::try_from(pos).ok())
                .ok_or(HexError)?;
            let bx = tbl
                .iter()
                .position(|x| *x == b)
                .and_then(|pos| u8::try_from(pos).ok())
                .ok_or(HexError)?;
            Ok((ax << 4) | bx)
        }
        _ => Err(HexError),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct HexError;

impl Display for HexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("invalid hexadecimal")
    }
}

impl Debug for HexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("HexError")
    }
}

impl Error for HexError {}

/// Read base64 from `string`.
pub fn read_base64_from_bytes(string: &[u8], out: &mut Vec<u8>) -> Result<(), Base64Error> {
    let tbl = [
        b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K', b'L', b'M', b'N', b'O',
        b'P', b'Q', b'R', b'S', b'T', b'U', b'V', b'W', b'X', b'Y', b'Z', b'a', b'b', b'c', b'd',
        b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l', b'm', b'n', b'o', b'p', b'q', b'r', b's',
        b't', b'u', b'v', b'w', b'x', b'y', b'z', b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7',
        b'8', b'9', b'+', b'/',
    ];
    for wnd in string.chunks(4) {
        match wnd {
            // 8 bits
            &[c1, c2, b'=', b'='] => {
                let sextet1 = tbl
                    .iter()
                    .position(|b| *b == c1)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let sextet2 = tbl
                    .iter()
                    .position(|b| *b == c2)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let byte1 = (sextet1 << 2) | (sextet2 >> 4);
                out.push(byte1);
            }
            // 16 bits
            &[c1, c2, c3, b'='] => {
                let sextet1 = tbl
                    .iter()
                    .position(|b| *b == c1)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let sextet2 = tbl
                    .iter()
                    .position(|b| *b == c2)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let sextet3 = tbl
                    .iter()
                    .position(|b| *b == c3)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let byte1 = (sextet1 << 2) | (sextet2 >> 4);
                let byte2 = ((sextet2 & 0xf) << 4) | (sextet3 >> 2);
                out.push(byte1);
                out.push(byte2);
            }
            // 24 bits
            &[c1, c2, c3, c4] => {
                let sextet1 = tbl
                    .iter()
                    .position(|b| *b == c1)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let sextet2 = tbl
                    .iter()
                    .position(|b| *b == c2)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let sextet3 = tbl
                    .iter()
                    .position(|b| *b == c3)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let sextet4 = tbl
                    .iter()
                    .position(|b| *b == c4)
                    .and_then(|pos| u8::try_from(pos).ok())
                    .ok_or(Base64Error)?;
                let byte1 = (sextet1 << 2) | (sextet2 >> 4);
                let byte2 = ((sextet2 & 0xf) << 4) | (sextet3 >> 2);
                let byte3 = ((sextet3 & 0x3) << 6) | sextet4;
                out.push(byte1);
                out.push(byte2);
                out.push(byte3);
            }
            _ => return Err(Base64Error),
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Base64Error;

impl Display for Base64Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("invalid base64")
    }
}

impl Debug for Base64Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Base64Error")
    }
}

impl Error for Base64Error {}

#[derive(Default)]
pub struct RoaringBitmap {
    keys: Vec<u16>,
    containers: Vec<crate::internals::encode::BitmapContainer>,
}

impl RoaringBitmap {
    pub fn from_bytes(u8array: &[u8]) -> Option<(RoaringBitmap, usize)> {
        if u8array.is_empty() {
            return None;
        }
        let mut keys = Vec::new();
        let mut cardinalities = Vec::new();
        let mut containers = Vec::new();
        // https://github.com/RoaringBitmap/RoaringFormatSpec
        //
        // Roaring bitmaps are used for flags that can be kept in their
        // compressed form, even when loaded into memory. This decoder
        // turns the containers into objects, but uses byte array
        // slices of the original format for the data payload.
        let has_runs = u8array[0] == 0x3b;
        let size = if has_runs {
            (usize::from(u8array[2]) | usize::from(u8array[3]) << 8) + 1
        } else {
            usize::from(u8array[4])
                | (usize::from(u8array[5]) << 8)
                | (usize::from(u8array[6]) << 16)
                | (usize::from(u8array[7]) << 24)
        };
        let mut i = if has_runs { 4usize } else { 8 };
        let is_run = if has_runs {
            let is_run_len = (size + 7) / 8;
            let is_run = &u8array[i..i + is_run_len];
            i += is_run_len;
            is_run
        } else {
            &[]
        };
        for _ in 0..size {
            keys.push(u16::from(u8array[i]) | (u16::from(u8array[i + 1]) << 8));
            i += 2;
            cardinalities.push(u16::from(u8array[i]) | (u16::from(u8array[i + 1]) << 8));
            i += 2;
        }
        let mut offsets = None;
        if !has_runs || keys.len() >= 4 {
            let mut new_offsets = Vec::new();
            for _ in 0..size {
                new_offsets.push(
                    usize::from(u8array[i])
                        | (usize::from(u8array[i + 1]) << 8)
                        | (usize::from(u8array[i + 2]) << 16)
                        | (usize::from(u8array[i + 3]) << 24),
                );
                i += 4;
            }
            offsets = Some(new_offsets);
        }
        for j in 0..size {
            if let Some(offsets) = &offsets {
                let off = offsets[j];
                if offsets[j] != i {
                    eprintln!("{containers:#?}");
                    panic!("corrupt bitmap {j}: {i} / {off}");
                }
            }
            let cardinality = usize::from(cardinalities[j]) + 1;
            if !is_run.is_empty() && is_run[j >> 3] & (1 << (j & 0x7)) != 0 {
                let runcount = u16::from(u8array[i]) | u16::from(u8array[i + 1]) << 8;
                i += 2;
                let mut runparts = Vec::with_capacity(usize::from(runcount));
                for _ in 0..runcount {
                    runparts.push((
                        u16::from(u8array[i]) | u16::from(u8array[i + 1]) << 8,
                        u16::from(u8array[i + 2]) | u16::from(u8array[i + 3]) << 8,
                    ));
                    i += 4;
                }
                containers.push(BitmapContainer::Run(runparts));
            } else if cardinality >= 4096 {
                let mut bits = Box::new([0; 1024]);
                for k in 0..1024 {
                    bits[k] = u64::from_le_bytes(u8array[i..i + 8].try_into().ok()?);
                    i += 8;
                }
                containers.push(BitmapContainer::Bits(bits));
            } else {
                let mut entries = Vec::with_capacity(cardinality);
                for _ in 0..cardinality {
                    entries.push(u16::from(u8array[i]) | u16::from(u8array[i + 1]) << 8);
                    i += 2;
                }
                containers.push(BitmapContainer::Array(entries));
            }
        }
        Some((RoaringBitmap { keys, containers }, i))
    }
    pub fn contains(&self, keyvalue: u32) -> bool {
        let key = u16::try_from(keyvalue >> 16).unwrap();
        let value = u16::try_from(keyvalue & 0xFFFF).unwrap();
        if let Ok(idx) = self.keys.binary_search(&key) {
            self.containers[idx].contains(value)
        } else {
            false
        }
    }
    pub fn to_vec(&self) -> Vec<u32> {
        let mut result = Vec::new();
        for (i, container) in self.containers.iter().enumerate() {
            let key = self.keys[i];
            container.push_to_vec(key, &mut result);
        }
        result
    }
}

#[cfg(test)]
mod tests;
