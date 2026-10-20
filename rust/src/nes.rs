//! Bounded validation of stock NES cartridge images. ROM bytes stay unchanged;
//! console Exec fields and bundled emulator binaries are never used.

pub fn prepare(bytes: &[u8]) -> Result<(Vec<u8>, u16, &'static str), String> {
    if bytes.starts_with(b"NES\x1a") {
        let mapper = validate(bytes)?;
        return Ok((bytes.to_vec(), mapper, "validated-ines"));
    }
    // The supplied US/Japanese Excitebike revisions use a small Wii U VC
    // wrapper, not compression. Only these exact, inspected carriers are
    // accepted: the iNES payload is [48,41024), with a NUL magic terminator.
    // Unknown wrappers must not be guessed from a coincidental NES signature.
    if bytes.len() != 41072 {
        return Err("Not an iNES cartridge image or recognized stock NES wrapper".into());
    }
    let hash = crate::hash::digest(bytes);
    if ![
        "5d84fd1713020e563d24683a5bdc04d7ef785c1d9a61b1cbf543ff1f41370842",
        "ea48dd6c080ee81ca756bd11f411fa53001c62618b17be3d793cd6153d774e11",
    ]
    .contains(&hash.as_str())
    {
        return Err("Not an iNES cartridge image or recognized stock NES wrapper".into());
    }
    let mut rom = bytes[48..41024].to_vec();
    rom[3] = 0x1a;
    let mapper = validate(&rom)?;
    Ok((rom, mapper, "restored-ines-wrapper"))
}

pub fn validate(bytes: &[u8]) -> Result<u16, String> {
    if bytes.len() < 16 || &bytes[..4] != b"NES\x1a" {
        return Err("Not an iNES cartridge image".into());
    }
    if bytes[7] & 0x0f != 0 || bytes[10..16] != [0; 6] || bytes[9] & 0xfe != 0 || bytes[6] & 4 != 0
    {
        return Err("Only standard iNES 1.0 cartridges are supported".into());
    }
    let mapper = u16::from(bytes[6] >> 4) | u16::from(bytes[7] & 0xf0);
    if ![0, 1, 2, 3, 4, 9, 184].contains(&mapper) {
        return Err(format!("NES mapper {mapper} is not enabled"));
    }
    let prg = usize::from(bytes[4]) * 16384;
    let chr = usize::from(bytes[5]) * 8192;
    let max_prg = match mapper {
        1 | 4 => 512 * 1024,
        2 | 9 => 256 * 1024,
        _ => 32768,
    };
    let max_chr = match mapper {
        3 | 184 => 32768,
        4 => 256 * 1024,
        1 | 9 => 128 * 1024,
        _ => 8192,
    };
    if prg > max_prg
        || chr > max_chr
        || (mapper == 2 && chr != 0)
        || (matches!(mapper, 3 | 9 | 184) && chr == 0)
        || (bytes[6] & 8 != 0 && (mapper != 4 || chr == 0))
        || bytes[8] > 1
        || ((bytes[6] & 2 != 0 || bytes[8] != 0) && !matches!(mapper, 0 | 1 | 4 | 9))
    {
        return Err("Unsupported memory or mirroring variant for stock NES mapper".into());
    }
    let size = 16
        + usize::from(bytes[6] & 4 != 0) * 512
        + usize::from(bytes[4]) * 16384
        + usize::from(bytes[5]) * 8192;
    if bytes[4] == 0 || bytes.len() != size {
        return Err("Truncated NES cartridge or unexpected trailing data".into());
    }
    Ok(mapper)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cartridge() -> Vec<u8> {
        let mut bytes = vec![0; 16 + 16384];
        bytes[..4].copy_from_slice(b"NES\x1a");
        bytes[4] = 1;
        bytes
    }
    #[test]
    fn validates_exact_stock_cartridges() {
        let bytes = cartridge();
        assert_eq!(validate(&bytes), Ok(0));
        assert!(validate(&bytes[..bytes.len() - 1]).is_err());
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(validate(&extra).is_err());
        extra = bytes;
        extra[7] = 8;
        assert!(validate(&extra).is_err());
    }
    #[test]
    fn rejects_unknown_mapper_and_trainers() {
        let mut bytes = cartridge();
        bytes[6] = 0xf0;
        assert!(validate(&bytes).is_err());
        bytes[6] = 4;
        assert!(validate(&bytes).is_err());
        bytes.resize(bytes.len() + 512, 0);
        assert!(validate(&bytes).is_err());
    }
}
