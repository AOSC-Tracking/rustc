/// Convert byte `n` into two characters hexadecimal.
pub(crate) fn write_hex_to_string(n: u8, string: &mut String) {
    let tbl = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
    ];
    string.push(tbl[n as usize >> 4]);
    string.push(tbl[n as usize & 0xf]);
}

pub fn write_base64_to_bytes(parts: &[u8], string: &mut Vec<u8>) {
    let tbl = [
        b'A', b'B', b'C', b'D', b'E', b'F', b'G', b'H', b'I', b'J', b'K', b'L', b'M', b'N', b'O',
        b'P', b'Q', b'R', b'S', b'T', b'U', b'V', b'W', b'X', b'Y', b'Z', b'a', b'b', b'c', b'd',
        b'e', b'f', b'g', b'h', b'i', b'j', b'k', b'l', b'm', b'n', b'o', b'p', b'q', b'r', b's',
        b't', b'u', b'v', b'w', b'x', b'y', b'z', b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7',
        b'8', b'9', b'+', b'/', b'=',
    ];
    for chunk in parts.chunks(3) {
        let sextet1 = *chunk.get(0).unwrap_or(&0) >> 2;
        string.push(tbl[sextet1 as usize]);
        let sextet2 =
            ((*chunk.get(0).unwrap_or(&0) & 0x3) << 4) | (*chunk.get(1).unwrap_or(&0) >> 4);
        string.push(tbl[sextet2 as usize]);
        let sextet3 = chunk.get(1).map_or(64, |&x| {
            (*chunk.get(2).unwrap_or(&0) >> 6) | ((x & 0xf) << 2)
        });
        string.push(tbl[sextet3 as usize]);
        let sextet4 = chunk.get(2).map_or(64, |&x| x & 0x3f);
        string.push(tbl[sextet4 as usize]);
    }
}

pub(crate) fn write_48bit_to_bytes_be(number: u64, string: &mut Vec<u8>) {
    assert!(number == (number & 0xFFFF_FFFF_FFFF));
    string.push((number >> 40) as u8);
    string.push((number >> 32) as u8);
    string.push((number >> 24) as u8);
    string.push((number >> 16) as u8);
    string.push((number >> 8) as u8);
    string.push(number as u8);
}

pub(crate) fn write_vlqhex_to_bytes(value: u32, string: &mut Vec<u8>) {
    // https://rust-lang.github.io/rustc-dev-guide/rustdoc-internals/search.html
    // describes the encoding in more detail.
    //
    // This version doesn't use zig-zag encoding, unlike the older version that was used for
    // type signatures (it's not needed).
    let mut shift: u32 = 28;
    let mut mask: u32 = 0xF0_00_00_00;
    // first skip leading zeroes
    while shift < 32 {
        let hexit = (value & mask) >> shift;
        if hexit != 0 || shift == 0 {
            break;
        }
        shift = shift.wrapping_sub(4);
        mask >>= 4;
    }
    // now write the rest
    while shift < 32 {
        let hexit = (value & mask) >> shift;
        let hex = u8::try_from(if shift == 0 { '`' } else { '@' } as u32 + hexit).unwrap();
        string.push(hex);
        shift = shift.wrapping_sub(4);
        mask >>= 4;
    }
}

// Used during bitmap encoding
#[derive(Debug)]
pub enum BitmapContainer {
    /// number of ones, bits
    Bits(Box<[u64; 1024]>),
    /// list of entries
    Array(Vec<u16>),
    /// list of (start, len-1)
    Run(Vec<(u16, u16)>),
}
impl BitmapContainer {
    pub fn contains(&self, value: u16) -> bool {
        match self {
            BitmapContainer::Bits(bits) => {
                let hunk = bits[value as usize >> 6];
                hunk & (1 << (value & 0x3F)) != 0
            }
            BitmapContainer::Array(array) => array.binary_search(&value).is_ok(),
            BitmapContainer::Run(runs) => runs
                .binary_search_by(|&(start, lenm1)| {
                    if start + lenm1 < value {
                        std::cmp::Ordering::Less
                    } else if start > value {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .is_ok(),
        }
    }
    fn popcount(&self) -> u32 {
        match self {
            BitmapContainer::Bits(bits) => bits.iter().copied().map(|x| x.count_ones()).sum(),
            BitmapContainer::Array(array) => array
                .len()
                .try_into()
                .expect("array can't be bigger than 2**32"),
            BitmapContainer::Run(runs) => runs
                .iter()
                .copied()
                .map(|(_, lenm1)| u32::from(lenm1) + 1)
                .sum(),
        }
    }
    fn push(&mut self, value: u16) {
        match self {
            BitmapContainer::Bits(bits) => bits[value as usize >> 6] |= 1 << (value & 0x3F),
            BitmapContainer::Array(array) => {
                array.push(value);
                if array.len() >= 4096 {
                    let array = std::mem::take(array);
                    *self = BitmapContainer::Bits(Box::new([0; 1024]));
                    for value in array {
                        self.push(value);
                    }
                }
            }
            BitmapContainer::Run(runs) => {
                if let Some(r) = runs.last_mut().filter(|r| r.0 + r.1 + 1 == value) {
                    r.1 += 1;
                } else {
                    runs.push((value, 0));
                }
            }
        }
    }
    fn try_make_run(&mut self) -> bool {
        match self {
            BitmapContainer::Bits(bits) => {
                let mut r: u64 = 0;
                for (i, chunk) in bits.iter().copied().enumerate() {
                    let next_chunk = i
                        .checked_add(1)
                        .and_then(|i| bits.get(i))
                        .copied()
                        .unwrap_or(0);
                    r += !chunk & u64::from((chunk << 1).count_ones());
                    r += !next_chunk & u64::from((chunk >> 63).count_ones());
                }
                if (2 + 4 * r) >= 8192 {
                    return false;
                }
                let bits = std::mem::replace(bits, Box::new([0; 1024]));
                *self = BitmapContainer::Run(Vec::new());
                for (i, bits) in bits.iter().copied().enumerate() {
                    if bits == 0 {
                        continue;
                    }
                    for j in 0..64 {
                        let value = (u16::try_from(i).unwrap() << 6) | j;
                        if bits & (1 << j) != 0 {
                            self.push(value);
                        }
                    }
                }
                true
            }
            BitmapContainer::Array(array) if array.len() <= 5 => false,
            BitmapContainer::Array(array) => {
                let mut r = 0;
                let mut prev = None;
                for value in array.iter().copied() {
                    if value.checked_sub(1) != prev {
                        r += 1;
                    }
                    prev = Some(value);
                }
                if 2 + 4 * r >= 2 * array.len() + 2 {
                    return false;
                }
                let array = std::mem::take(array);
                *self = BitmapContainer::Run(Vec::new());
                for value in array {
                    self.push(value);
                }
                true
            }
            BitmapContainer::Run(_) => true,
        }
    }
    pub(crate) fn push_to_vec(&self, key: u16, result: &mut Vec<u32>) {
        let key: u32 = key.into();
        match self {
            BitmapContainer::Run(v) => {
                for (start, lenm1) in v.iter().copied() {
                    let mut value = u32::from(start);
                    let mut j = 0;
                    while j <= lenm1 {
                        result.push((key << 16) | value);
                        value += 1;
                        j += 1;
                    }
                }
            }
            BitmapContainer::Array(values) => {
                for value in values.iter().copied() {
                    let value = u32::from(value);
                    result.push((key << 16) | value);
                }
            }
            BitmapContainer::Bits(_) => {
                let mut value = 0u32;
                while value < 65536 {
                    if self.contains(u16::try_from(value).unwrap()) {
                        result.push((key << 16) | value);
                    }
                    value += 1;
                }
            }
        }
    }
}

// checked against roaring-rs in
// https://gitlab.com/notriddle/roaring-test
pub fn write_bitmap_to_bytes(domain: &[u32], mut out: impl std::io::Write) -> std::io::Result<()> {
    if domain.len() <= 2
        && (domain.len() <= 1 || domain[0] & 0xffff_0000 == domain[1] & 0xffff_0000)
    {
        // This shows up in benchmarks.
        let size = if domain.len() == 0 { 0 } else { 1 };
        let count_minus_one = u32::try_from(domain.len()).unwrap().checked_sub(1);
        out.write_all(&u32::to_le_bytes(SERIAL_COOKIE_NO_RUNCONTAINER))?;
        out.write_all(&u32::to_le_bytes(size))?;
        let start_offset = 4 + 4 + 4 * size + 4 * size;
        if let Some(count_minus_one) = count_minus_one {
            out.write_all(&u32::to_le_bytes(
                (count_minus_one << 16) | (domain[0] & 0xffff_0000) >> 16,
            ))?;
            out.write_all(&u32::to_le_bytes(start_offset))?;
            for value in domain.iter() {
                let value = u16::try_from(value & 0x0000_ffff).unwrap();
                out.write_all(&u16::to_le_bytes(value))?;
            }
        }
        return Ok(());
    }
    // https://arxiv.org/pdf/1603.06549.pdf
    let mut keys = Vec::<u16>::new();
    let mut containers = Vec::<BitmapContainer>::new();
    let mut key: u16;
    let mut domain_iter = domain.iter().copied().peekable();
    let mut has_run = false;
    let mut prev = None;
    while let Some(entry) = domain_iter.next() {
        assert!(prev <= Some(entry));
        prev = Some(entry);
        key = (entry >> 16)
            .try_into()
            .expect("shifted off the top 16 bits, so it should fit");
        let value: u16 = (entry & 0x00_00_FF_FF)
            .try_into()
            .expect("AND 16 bits, so it should fit");
        let mut container = BitmapContainer::Array(vec![value]);
        while let Some(entry) = domain_iter.peek().copied() {
            let entry_key: u16 = (entry >> 16)
                .try_into()
                .expect("shifted off the top 16 bits, so it should fit");
            if entry_key != key {
                break;
            }
            domain_iter.next().expect("peeking just succeeded");
            container.push(
                (entry & 0x00_00_FF_FF)
                    .try_into()
                    .expect("AND 16 bits, so it should fit"),
            );
        }
        keys.push(key);
        has_run = container.try_make_run() || has_run;
        containers.push(container);
    }
    // https://github.com/RoaringBitmap/RoaringFormatSpec
    const SERIAL_COOKIE_NO_RUNCONTAINER: u32 = 12346;
    const SERIAL_COOKIE: u32 = 12347;
    const NO_OFFSET_THRESHOLD: u32 = 4;
    let size: u32 = containers.len().try_into().unwrap();
    let start_offset = if has_run {
        out.write_all(&u32::to_le_bytes(SERIAL_COOKIE | ((size - 1) << 16)))?;
        for set in containers.chunks(8) {
            let mut b = 0;
            for (i, container) in set.iter().enumerate() {
                if matches!(container, &BitmapContainer::Run(..)) {
                    b |= 1 << i;
                }
            }
            out.write_all(&[b])?;
        }
        if size < NO_OFFSET_THRESHOLD {
            4 + 4 * size + ((size + 7) / 8)
        } else {
            4 + 8 * size + ((size + 7) / 8)
        }
    } else {
        out.write_all(&u32::to_le_bytes(SERIAL_COOKIE_NO_RUNCONTAINER))?;
        out.write_all(&u32::to_le_bytes(size))?;
        4 + 4 + 4 * size + 4 * size
    };
    for (&key, container) in keys.iter().zip(&containers) {
        // descriptive header
        let key: u32 = key.into();
        let count: u32 = container.popcount() - 1;
        out.write_all(&u32::to_le_bytes((count << 16) | key))?;
    }
    if !has_run || size >= NO_OFFSET_THRESHOLD {
        // offset header
        let mut starting_offset = start_offset;
        for container in &containers {
            out.write_all(&u32::to_le_bytes(starting_offset))?;
            starting_offset += match container {
                BitmapContainer::Bits(_) => 8192u32,
                BitmapContainer::Array(array) => u32::try_from(array.len()).unwrap() * 2,
                BitmapContainer::Run(runs) => 2 + u32::try_from(runs.len()).unwrap() * 4,
            };
        }
    }
    for container in &containers {
        match container {
            BitmapContainer::Bits(bits) => {
                for chunk in bits.iter() {
                    out.write_all(&u64::to_le_bytes(*chunk))?;
                }
            }
            BitmapContainer::Array(array) => {
                for value in array.iter() {
                    out.write_all(&u16::to_le_bytes(*value))?;
                }
            }
            BitmapContainer::Run(runs) => {
                out.write_all(&u16::to_le_bytes(runs.len().try_into().unwrap()))?;
                for (start, lenm1) in runs.iter().copied() {
                    out.write_all(&u16::to_le_bytes(start))?;
                    out.write_all(&u16::to_le_bytes(lenm1))?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
