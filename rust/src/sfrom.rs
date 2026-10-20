//! Strict parser and signal-exact PCM-to-BRR restoration for stock SFROM.
//!
//! BRR encoding is solved by exhaustive inversion of the SNES DSP decoder.
//! Each restored block is checked against its original PCM samples, including
//! every recorded loop history. This does not claim retail-byte identity.

use serde_json::{Value, json};

const HEADER_SIZE: usize = 48;
const FOOTER_MINIMUM: usize = 36;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conversion {
    pub rom: Vec<u8>,
    pub status: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfromError(pub String);

impl std::fmt::Display for SfromError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SfromError {}

fn malformed(reason: impl AsRef<str>) -> SfromError {
    SfromError(format!("SFROM malformed: {}", reason.as_ref()))
}

fn unsupported(reason: impl AsRef<str>) -> SfromError {
    SfromError(format!("SFROM unsupported: {}", reason.as_ref()))
}

fn le16(bytes: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([bytes[p], bytes[p + 1]])
}

fn le32(bytes: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(bytes[p..p + 4].try_into().expect("validated u32 range"))
}

fn le24(bytes: &[u8], p: usize) -> usize {
    usize::from(bytes[p]) | (usize::from(bytes[p + 1]) << 8) | (usize::from(bytes[p + 2]) << 16)
}

fn header_valid(rom: &[u8], h: usize, size: usize, checksum: u16, check_sum: bool) -> bool {
    if h > size || size - h < 64 {
        return false;
    }
    if !rom[h..h + 21]
        .iter()
        .all(|&c| (0x20..=0x7e).contains(&c) || (0xa1..=0xdf).contains(&c))
    {
        return false;
    }
    let map = rom[h + 21];
    let hirom = map == 0x21 || map == 0x31;
    let lorom = matches!(map, 0x20 | 0x30 | 0x23 | 0x33 | 0x32);
    if (!hirom && !lorom) || (hirom != (h == 0xffc0)) {
        return false;
    }
    let exponent = rom[h + 23];
    if !(5..=13).contains(&exponent) {
        return false;
    }
    let declared_size = 1024usize << exponent;
    if declared_size < size || declared_size / 2 >= size || le16(rom, h + 60) < 0x8000 {
        return false;
    }
    let inverse = le16(rom, h + 28);
    let stored = le16(rom, h + 30);
    inverse ^ stored == 0xffff && (!check_sum || stored == checksum)
}

fn floor_div(a: i32, b: i32) -> i32 {
    a.div_euclid(b)
}

fn decode_nibble(nibble: i32, scale: u32, filter: u32, p1: i32, p2: i32) -> i32 {
    let mut sample = if scale <= 12 {
        floor_div(nibble * (1 << scale), 2)
    } else if nibble < 0 {
        -2048
    } else {
        0
    };
    match filter {
        1 => sample += floor_div(p1, 2) + floor_div(-p1, 32),
        2 => {
            sample +=
                p1 - floor_div(p2, 2) + floor_div(floor_div(p2, 2), 16) + floor_div(-3 * p1, 64)
        }
        3 => {
            sample += p1 - floor_div(p2, 2)
                + floor_div(-13 * p1, 128)
                + floor_div(3 * floor_div(p2, 2), 16)
        }
        _ => {}
    }
    sample = sample.clamp(-32768, 32767) * 2;
    if sample > 32767 {
        sample -= 65536;
    }
    if sample < -32768 {
        sample += 65536;
    }
    sample
}

fn rom_checksum(rom: &[u8]) -> u16 {
    let mut span = 1usize;
    while span < rom.len() {
        span *= 2;
    }
    let mut sum = 0u32;
    for address in 0..span {
        let mut a = address;
        let mut base = 0usize;
        let mut remaining = rom.len();
        let mut mask = span;
        while a >= remaining {
            while mask > a {
                mask >>= 1;
            }
            a -= mask;
            if remaining > mask {
                remaining -= mask;
                base += mask;
            }
        }
        sum += u32::from(rom[base + a]);
    }
    sum as u16
}

#[derive(Default)]
struct AudioReport {
    blocks: usize,
    auxiliary: usize,
    resets: usize,
    loop_blocks: usize,
    loop_inexact: usize,
    loop_max_error: i32,
}

fn pcm_sample(bytes: &[u8], pcm_start: usize, index: usize) -> Result<i32, SfromError> {
    let p = pcm_start
        .checked_add(
            index
                .checked_mul(2)
                .ok_or_else(|| malformed("PCM index overflow"))?,
        )
        .ok_or_else(|| malformed("PCM index overflow"))?;
    let raw = bytes
        .get(p..p + 2)
        .ok_or_else(|| malformed("PCM sample outside input"))?;
    Ok(i32::from(i16::from_be_bytes([raw[0], raw[1]])))
}

fn restore_audio(
    bytes: &[u8],
    rom: &mut [u8],
    pcm_start: usize,
    pcm_end: usize,
    footer: usize,
) -> Result<AudioReport, SfromError> {
    if pcm_start == pcm_end
        || !(pcm_end - pcm_start).is_multiple_of(2)
        || !(footer - pcm_end).is_multiple_of(6)
    {
        return Err(malformed(
            "external PCM / PCMF audio section alignment or missing samples",
        ));
    }
    let samples = (pcm_end - pcm_start) / 2;
    let entries = (footer - pcm_end) / 6;
    if entries > 65536 || samples > 16 * 1024 * 1024 {
        return Err(unsupported("external PCM exceeds stock profile limits"));
    }
    let mut report = AudioReport::default();
    let mut previous = rom.len();
    let (mut p1, mut p2) = (0i32, 0i32);
    let mut ended = true;
    let mut chain_work = 0usize;
    let mut p = 0usize;
    while p + 4 <= rom.len() {
        if rom[p..p + 4] != *b"PCMF" {
            p += 1;
            continue;
        }
        if rom.len() - p < 9 {
            return Err(malformed("truncated PCMF record"));
        }
        let control = rom[p + 7];
        if control & 0xe0 != 0 {
            return Err(unsupported("unknown PCMF control bits"));
        }
        let mut offset = le24(rom, p + 4);
        let mut auxiliary_index = entries;
        if control & 0x10 != 0 {
            let index = usize::from(le16(rom, p + 4));
            auxiliary_index = index;
            if index >= entries {
                return Err(malformed("PCMF auxiliary index outside table"));
            }
            let entry = pcm_end + index * 6;
            if bytes[entry] != rom[p + 8] {
                return Err(malformed("PCMF auxiliary identity mismatch"));
            }
            offset = le24(bytes, entry + 3);
            report.auxiliary += 1;
        }
        if offset > samples || samples - offset < 16 {
            return Err(malformed("PCMF sample offset outside PCM"));
        }
        let mut target = [0i32; 16];
        for (i, value) in target.iter_mut().enumerate() {
            *value = pcm_sample(bytes, pcm_start, offset + i)?;
        }
        let mut alternates = Vec::new();
        if auxiliary_index != entries {
            let mut index = usize::from(le16(bytes, pcm_end + auxiliary_index * 6 + 1));
            while index != 0 {
                if index >= entries || alternates.len() >= entries {
                    return Err(malformed("PCMF auxiliary chain is cyclic or out of range"));
                }
                chain_work += 1;
                if chain_work > entries * 16 {
                    return Err(unsupported("PCMF auxiliary reference budget exceeded"));
                }
                let alternate = le24(bytes, pcm_end + index * 6 + 3);
                if alternate < 2 || alternate > samples || samples - alternate < 16 {
                    return Err(malformed("PCMF alternate sample offset outside PCM"));
                }
                alternates.push(alternate);
                index = usize::from(le16(bytes, pcm_end + index * 6 + 1));
            }
        }
        if ended || previous + 9 != p {
            p1 = 0;
            p2 = 0;
            report.resets += 1;
        }
        let filter = u32::from((control >> 2) & 3);
        let mut best = [0u8; 9];
        let mut found = false;
        for scale in 0..=13u32 {
            let mut block = [0u8; 9];
            block[0] = ((scale as u8) << 4) | (control & 15);
            let (mut a, mut b) = (p1, p2);
            let mut exact = true;
            for i in 0..16 {
                let mut chosen = 0i32;
                let mut matched = false;
                for n in -8..=7 {
                    if decode_nibble(n, scale, filter, a, b) != target[i] {
                        continue;
                    }
                    let mut matches = true;
                    for &alternate in &alternates {
                        let prior1 = pcm_sample(bytes, pcm_start, alternate + i - 1)?;
                        let prior2 = pcm_sample(bytes, pcm_start, alternate + i - 2)?;
                        if decode_nibble(n, scale, filter, prior1, prior2)
                            != pcm_sample(bytes, pcm_start, alternate + i)?
                        {
                            matches = false;
                            break;
                        }
                    }
                    if matches {
                        chosen = n;
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    exact = false;
                    break;
                }
                block[1 + i / 2] |= ((chosen as u8) & 15) << if i % 2 == 0 { 4 } else { 0 };
                b = a;
                a = target[i];
            }
            if exact {
                best = block;
                found = true;
                break;
            }
        }
        if !found {
            return Err(unsupported(format!(
                "PCMF block cannot be restored exactly against first-pass and loop PCM at ROM offset {p}"
            )));
        }
        report.blocks += 1;
        for (i, target_value) in target.iter().enumerate() {
            let mut nibble = i32::from((best[1 + i / 2] >> if i % 2 == 0 { 4 } else { 0 }) & 15);
            if nibble >= 8 {
                nibble -= 16;
            }
            let value = decode_nibble(nibble, u32::from(best[0] >> 4), filter, p1, p2);
            if value != *target_value {
                return Err(malformed("BRR first-pass verification failed"));
            }
            p2 = p1;
            p1 = value;
        }
        for &alternate in &alternates {
            let (mut a, mut b) = (
                pcm_sample(bytes, pcm_start, alternate - 1)?,
                pcm_sample(bytes, pcm_start, alternate - 2)?,
            );
            let mut maximum = 0;
            for i in 0..16 {
                let mut nibble =
                    i32::from((best[1 + i / 2] >> if i % 2 == 0 { 4 } else { 0 }) & 15);
                if nibble >= 8 {
                    nibble -= 16;
                }
                let value = decode_nibble(nibble, u32::from(best[0] >> 4), filter, a, b);
                maximum = maximum.max((value - pcm_sample(bytes, pcm_start, alternate + i)?).abs());
                b = a;
                a = value;
            }
            report.loop_blocks += 1;
            if maximum != 0 {
                report.loop_inexact += 1;
            }
            report.loop_max_error = report.loop_max_error.max(maximum);
        }
        if report.loop_inexact != 0 {
            return Err(unsupported("BRR loop verification failed"));
        }
        rom[p..p + 9].copy_from_slice(&best);
        previous = p;
        ended = control & 1 != 0;
        p += 9;
    }
    if report.blocks == 0 {
        return Err(unsupported("external PCM / PCMF audio has no records"));
    }
    Ok(report)
}

pub fn convert_sfrom(bytes: &[u8]) -> Result<Conversion, SfromError> {
    if bytes.len() < HEADER_SIZE {
        return Err(malformed("truncated 48-byte header"));
    }
    if bytes.len() > u32::MAX as usize {
        return Err(malformed("container exceeds 32-bit format limits"));
    }
    if le32(bytes, 0) != 0x100 {
        return Err(unsupported("unrecognized container version"));
    }
    if le32(bytes, 4) as usize != bytes.len() {
        return Err(malformed("declared file size differs from input size"));
    }
    let rom_start = le32(bytes, 8) as usize;
    let rom_end = le32(bytes, 12) as usize;
    let pcm_end = le32(bytes, 16) as usize;
    let footer = le32(bytes, 20) as usize;
    let extension = le32(bytes, 24) as usize;
    let footer_flag = le32(bytes, 32) as usize;
    if rom_start != HEADER_SIZE {
        return Err(unsupported(
            "only the official trailing-footer, 48-byte-header layout is supported",
        ));
    }
    if !(rom_start < rom_end && rom_end <= pcm_end && pcm_end <= footer && footer <= bytes.len()) {
        return Err(malformed(
            "ROM / PCM / PCM-footer offsets are unordered or outside the container",
        ));
    }
    if bytes.len() - footer < FOOTER_MINIMUM {
        return Err(malformed("truncated footer"));
    }
    if extension < footer || extension > bytes.len() {
        return Err(malformed("extension offset outside footer"));
    }
    if footer_flag < footer || footer_flag >= bytes.len() {
        return Err(malformed("footer flag offset outside footer"));
    }
    if le32(bytes, 28) != 0 || le32(bytes, 44) != 0 {
        return Err(unsupported("unknown nonzero header flags"));
    }
    if !bytes[36..44].iter().all(|&c| (0x20..=0x7e).contains(&c)) {
        return Err(malformed("non-ASCII Virtual Console game ID"));
    }
    let rom_size = rom_end - rom_start;
    let pcm_size = pcm_end - rom_end;
    let pcm_footer_size = footer - pcm_end;
    if le32(bytes, footer + 1) as usize != rom_size
        || le32(bytes, footer + 5) as usize != pcm_size
        || le32(bytes, footer + 9) as usize != pcm_footer_size
    {
        return Err(malformed(
            "footer sizes disagree with header section boundaries",
        ));
    }
    let mut markers = 0usize;
    for p in rom_start..rom_end.saturating_sub(3) {
        if bytes[p..p + 4] == *b"PCMF" {
            markers += 1;
        }
    }
    let audio = pcm_size != 0 || pcm_footer_size != 0 || markers != 0;
    let mut empty_extension = extension.checked_add(16) == Some(bytes.len())
        && extension + 4 <= bytes.len()
        && le32(bytes, extension) as usize == bytes.len();
    if empty_extension {
        empty_extension = bytes[extension + 4..].iter().all(|&byte| byte == 0);
    }
    if extension != bytes.len() && !empty_extension {
        return Err(unsupported(
            "S-DD1 data or an unknown footer extension requires restoration",
        ));
    }
    if footer_flag != footer + 27 {
        return Err(unsupported("unknown footer flag layout"));
    }
    if (!audio && bytes.len() - footer != 48) || extension - footer < 32 || extension - footer > 64
    {
        return Err(unsupported("unrecognized non-PCM footer profile"));
    }
    let preset = le16(bytes, footer + 13);
    let alternate_footer = matches!(preset, 0x10b0 | 0x10b2 | 0x10dc | 0x10dd);
    for p in 19..extension - footer {
        let expected = u8::from(p == 31 || (p == 27 && !alternate_footer));
        if bytes[footer + p] != expected {
            return Err(unsupported("unrecognized footer flags"));
        }
    }
    if bytes[footer] != 60 && bytes[footer] != 50 {
        return Err(unsupported("unrecognized footer frame rate"));
    }
    if bytes[footer + 17] != 0x14 && bytes[footer + 17] != 0x15 {
        return Err(unsupported("unrecognized footer ROM mapping"));
    }
    if bytes[footer + 18] != 0 && bytes[footer + 18] != 0x0c {
        return Err(unsupported("unrecognized footer enhancement-chip flags"));
    }
    if !(32768..=8 * 1024 * 1024).contains(&rom_size) || !rom_size.is_multiple_of(32768) {
        return Err(unsupported(
            "only standard ROM payloads from 32 KiB to 8 MiB are supported",
        ));
    }
    let mut rom = bytes[rom_start..rom_end].to_vec();
    let contra_header = audio
        && preset == 0x1036
        && rom_size == 1_048_576
        && rom[0x7fd5] == 0x53
        && &rom[0x7fc0..0x7fd5] == b"CONTRA3 THE ALIEN WAR";
    let soccer_header = !audio
        && preset == 0x1240
        && rom_size == 524_288
        && rom[0x7fd5] == 0x52
        && &rom[0x7fc0..0x7fd5] == b"SUPER FORMATION SOCCE";
    if soccer_header && rom_checksum(&rom) != le16(&rom, 0x7fde) {
        return Err(unsupported(
            "Super Formation Soccer payload checksum mismatch",
        ));
    }
    let header_repaired = contra_header || soccer_header;
    if header_repaired {
        rom[0x7fd5] = 0x20;
    }
    let audio_report = if audio {
        restore_audio(bytes, &mut rom, rom_end, pcm_end, footer)?
    } else {
        AudioReport::default()
    };
    if audio && audio_report.blocks != markers {
        return Err(malformed("overlapping PCMF markers"));
    }
    let checksum = rom_checksum(&rom);
    let mut header = None;
    for candidate in [0x7fc0usize, 0xffc0] {
        if !header_valid(
            &rom,
            candidate,
            rom_size,
            checksum,
            !audio && !header_repaired,
        ) {
            continue;
        }
        if header.is_some() {
            return Err(unsupported("ambiguous SNES cartridge headers"));
        }
        header = Some(candidate);
    }
    let header = header.ok_or_else(|| unsupported(
        "no supported standard SNES header with matching full-ROM checksum; the payload may be patched, damaged, or use an unsupported mapping",
    ))?;
    let mut title = String::new();
    for &c in &rom[header..header + 21] {
        if c < 0x80 {
            title.push(char::from(c));
        } else {
            title.push(char::from_u32(0xff61 + u32::from(c) - 0xa1).expect("katakana code point"));
        }
    }
    while title.ends_with(' ') {
        title.pop();
    }
    let cartridge_type = bytes[rom_start + header + 22];
    let super_fx = matches!(cartridge_type, 0x13 | 0x14 | 0x15 | 0x1a);
    if super_fx != (bytes[footer + 18] == 0x0c) {
        return Err(unsupported(
            "footer enhancement flags disagree with the cartridge header",
        ));
    }
    let original_checksum = le16(&rom, header + 30);
    let original_match = checksum == original_checksum;
    if audio || header_repaired {
        rom[header + 28..header + 30].copy_from_slice(&(!checksum).to_le_bytes());
        rom[header + 30..header + 32].copy_from_slice(&checksum.to_le_bytes());
        if rom_checksum(&rom) != checksum {
            return Err(malformed("restored checksum is not stable"));
        }
    }
    let status = if audio {
        "converted-restored-pcm"
    } else if header_repaired {
        "converted-normalized-header"
    } else {
        "converted-validated-non-pcm"
    }
    .to_owned();
    let metadata = json!({
        "format": "sfrom", "conversion": status,
        "rom_bytes": rom_size, "rom_start": rom_start, "rom_end": rom_end,
        "pcm_bytes": pcm_size, "pcm_footer_bytes": pcm_footer_size, "pcmf_markers": markers,
        "footer_start": footer, "preset_id": preset,
        "virtual_console_id": String::from_utf8_lossy(&bytes[36..44]),
        "title": title, "map_mode": rom[header + 21],
        "cartridge_type": cartridge_type, "super_fx": super_fx,
        "checksum": checksum, "checksum_verified": true, "rom_payload_unchanged": !audio && !header_repaired,
        "original_checksum": original_checksum, "original_checksum_matches": original_match,
        "pcm_restored": audio, "patches_reversed": false, "runtime_tested": false,
        "brr_blocks": audio_report.blocks, "brr_inexact_blocks": 0,
        "brr_max_sample_error": 0, "brr_squared_error": 0,
        "brr_auxiliary_records": audio_report.auxiliary, "brr_streams": audio_report.resets,
        "brr_loop_blocks": audio_report.loop_blocks, "brr_loop_inexact_blocks": audio_report.loop_inexact,
        "brr_loop_max_sample_error": audio_report.loop_max_error, "header_map_normalized": header_repaired,
        "pcm_signal_exact": audio, "retail_byte_exact_verified": false,
        "non_audio_patches_verified": false,
        "checksum_recomputed": audio || header_repaired, "empty_extension_validated": empty_extension,
        "firmware_policy": "No firmware is extracted or bundled; any coprocessor firmware required by ares must be supplied by the user.",
        "compatibility": "Validated standard ROM structure, not a claim of runtime or byte-identical retail-ROM verification."
    });
    Ok(Conversion {
        rom,
        status,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put32(bytes: &mut [u8], p: usize, value: u32) {
        bytes[p..p + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn standard_fixture() -> Vec<u8> {
        let size = 32 * 1024;
        let footer = HEADER_SIZE + size;
        let mut bytes = vec![0; footer + 48];
        let total_size = bytes.len() as u32;
        put32(&mut bytes, 0, 0x100);
        put32(&mut bytes, 4, total_size);
        put32(&mut bytes, 8, HEADER_SIZE as u32);
        put32(&mut bytes, 12, footer as u32);
        put32(&mut bytes, 16, footer as u32);
        put32(&mut bytes, 20, footer as u32);
        put32(&mut bytes, 24, total_size);
        put32(&mut bytes, 32, (footer + 27) as u32);
        bytes[36..44].copy_from_slice(b"WUP-TEST");
        let h = HEADER_SIZE + 0x7fc0;
        bytes[h..h + 21].fill(b' ');
        bytes[h..h + 18].copy_from_slice(b"SELF AUTHORED TEST");
        bytes[h + 21] = 0x20;
        bytes[h + 23] = 5;
        bytes[h + 28] = 0xff;
        bytes[h + 29] = 0xff;
        bytes[h + 60] = 0x00;
        bytes[h + 61] = 0x80;
        let sum: u32 = bytes[HEADER_SIZE..footer]
            .iter()
            .map(|&value| u32::from(value))
            .sum();
        let checksum = sum as u16;
        bytes[h + 28..h + 30].copy_from_slice(&(!checksum).to_le_bytes());
        bytes[h + 30..h + 32].copy_from_slice(&checksum.to_le_bytes());
        bytes[footer] = 60;
        put32(&mut bytes, footer + 1, size as u32);
        bytes[footer + 17] = 0x14;
        bytes[footer + 27] = 1;
        bytes[footer + 31] = 1;
        bytes
    }

    #[test]
    fn converts_self_authored_non_pcm_stock_container() {
        let converted = convert_sfrom(&standard_fixture()).unwrap();
        assert_eq!(converted.status, "converted-validated-non-pcm");
        assert_eq!(converted.rom.len(), 32 * 1024);
        assert_eq!(converted.metadata["title"], "SELF AUTHORED TEST");
        assert_eq!(converted.metadata["checksum_verified"], true);
    }

    #[test]
    fn rejects_invalid_magic_and_size() {
        let short = convert_sfrom(b"short").err().unwrap().to_string();
        assert!(short.contains("truncated 48-byte header"));
        let mut invalid = standard_fixture();
        invalid[1] = 0;
        let error = convert_sfrom(&invalid).err().unwrap().to_string();
        assert!(error.contains("unrecognized container version"), "{error}");
    }

    #[test]
    fn decoder_uses_floor_rounding_for_negative_predictors() {
        assert_eq!(decode_nibble(0, 2, 1, -3, 0), -4);
        assert_eq!(decode_nibble(0, 2, 3, -3, 0), -6);
    }
}
