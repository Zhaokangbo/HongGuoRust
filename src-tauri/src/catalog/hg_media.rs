//! MediaCrypto (spade URL decryption, spade_a CENC key derivation, MP4 CENC sample
//! table & on-the-fly AES-128-CTR decryption) and PlaybackModels / StreamLadder.
//! Adapted from N3urda/hongguoTV (GPL-3.0-only): MediaCrypto.kt, PlaybackModels.kt,
//! ContentRepository.kt.

use super::hg_crypto::{
    aes128_cbc_decrypt_nopad, aes128_ctr_apply, base64_decode, hex_decode, sha512,
};
use super::hg_signer::KDF_SALT_HEX;
use serde_json::Value;

const MAX_MOOV_END: u64 = 8 * 1024 * 1024;
const MAX_SAMPLE_COUNT: u32 = 200_000;

#[derive(Clone, Debug)]
pub struct CencSample {
    pub start: u64,
    pub end: u64,
    pub iv: [u8; 16],
    pub encrypted_offset: u64,
}

#[derive(Clone, Debug)]
pub struct PreparedCenc {
    pub header: Vec<u8>,
    pub samples: Vec<CencSample>,
}

#[derive(Clone, Debug)]
pub struct Rendition {
    pub quality: u32,
    pub codec: String,
    #[allow(dead_code)]
    pub width: Option<u32>,
    #[allow(dead_code)]
    pub height: Option<u32>,
    pub url: String,
    pub backup_url: Option<String>,
    pub key: Option<[u8; 16]>,
}

pub fn clean_url(raw: &str) -> String {
    raw.trim()
        .replace("\\/", "/")
        .replace("\\u002F", "/")
        .replace("\\u002f", "/")
        .replace("\\u0026", "&")
        .replace("&amp;", "&")
}

/// Derive 16-byte CENC AES-128 content key from `spade_a`.
pub fn derive_content_key(value: &str) -> Result<[u8; 16], String> {
    let raw = base64_decode(value)?;
    if raw.len() < 3 {
        return Err("hg_media_key: spade_a 过短".into());
    }
    let xor_val = (raw[0] ^ raw[1] ^ raw[2]) as isize;
    let calc = (raw.len() as isize) - xor_val + 47;
    let length = calc.min((raw.len() - 1) as isize);
    if length < 33 {
        return Err("hg_media_key: spade_a 长度无效".into());
    }
    let len = length as usize;
    let mut work = raw[1..1 + len].to_vec();
    let mut a: u8 = 85;
    let mut b: u8 = 246;
    for (i, slot) in work.iter_mut().enumerate() {
        let old = *slot;
        let previous = if (i & 1) != 0 { a } else { b };
        if (i & 1) != 0 {
            a = old;
        } else {
            b = old;
        }
        let pop = i.count_ones() as i32;
        let decoded = (-21i32 - pop + i32::from(previous ^ old)) & 0xff;
        *slot = decoded as u8;
    }
    let hex_str = std::str::from_utf8(&work[1..33]).map_err(|_| "hg_media_key: 密钥编码异常")?;
    let bytes = hex_decode(hex_str)?;
    bytes
        .try_into()
        .map_err(|_| "hg_media_key: 密钥长度不是 16 字节".to_string())
}

/// Decrypt spade-framed CDN URL using `key_seed`.
pub fn decrypt_spade_url(value: &str, seed: &[u8]) -> Result<String, String> {
    let raw = base64_decode(value)?;
    if raw.len() < 20 || raw[0] != 0xa8 || raw[2] != 0x01 || raw[3] != 0x00 {
        return Err("hg_media_url: spade 密文头格式异常".into());
    }
    let cipher_len = ((raw.len() - 4) / 16) * 16;
    if cipher_len == 0 {
        return Err("hg_media_url: spade 密文无完整分组".into());
    }
    let constants = hex_decode(KDF_SALT_HEX)?;
    let h1 = sha512(seed);
    let mut h2_in = Vec::with_capacity(h1.len() + constants.len());
    h2_in.extend_from_slice(&h1);
    h2_in.extend_from_slice(&constants);
    let h2 = sha512(&h2_in);
    let key: [u8; 16] = h2[0..16].try_into().unwrap();
    let iv: [u8; 16] = h2[16..32].try_into().unwrap();
    let mut plain = aes128_cbc_decrypt_nopad(&key, &iv, &raw[4..4 + cipher_len])?;
    if let Some(&pad) = plain.last() {
        let p = pad as usize;
        if (1..=16).contains(&p) && p <= plain.len() {
            plain.truncate(plain.len() - p);
        }
    }
    while plain.last() == Some(&0) {
        plain.pop();
    }
    String::from_utf8(plain).map_err(|_| "hg_media_url: 解密后 URL 非合法 UTF-8".to_string())
}

// ---------- ISO BMFF (MP4) Box & CENC Parsing ----------

#[derive(Clone, Copy)]
struct Mp4Box {
    start: usize,
    end: usize,
    body: usize,
    kind: [u8; 4],
}

fn read_u32_be(data: &[u8], at: usize) -> Option<u32> {
    data.get(at..at + 4)
        .map(|s| u32::from_be_bytes(s.try_into().unwrap()))
}

fn read_u64_be(data: &[u8], at: usize) -> Option<u64> {
    data.get(at..at + 8)
        .map(|s| u64::from_be_bytes(s.try_into().unwrap()))
}

fn read_box(data: &[u8], at: usize, limit: usize) -> Option<Mp4Box> {
    if at + 8 > limit || limit > data.len() {
        return None;
    }
    let mut size = u64::from(read_u32_be(data, at)?);
    let mut header = 8usize;
    if size == 1 {
        if at + 16 > limit {
            return None;
        }
        size = read_u64_be(data, at + 8)?;
        header = 16;
    } else if size == 0 {
        size = (limit - at) as u64;
    }
    if size < header as u64 || size > (limit - at) as u64 {
        return None;
    }
    let kind: [u8; 4] = data[at + 4..at + 8].try_into().ok()?;
    Some(Mp4Box {
        start: at,
        end: at + size as usize,
        body: at + header,
        kind,
    })
}

fn find_box(data: &[u8], kind: &[u8; 4], start: usize, limit: usize) -> Option<Mp4Box> {
    if limit < start + 8 || limit > data.len() {
        return None;
    }
    for at in start..=limit - 8 {
        if data[at + 4] == kind[0] {
            if let Some(b) = read_box(data, at, limit) {
                if &b.kind == kind {
                    return Some(b);
                }
            }
        }
    }
    None
}

/// Validate `ftyp` + `moov` at start of progressive MP4 and return byte offset where `moov` ends.
pub fn header_end(first: &[u8], total: u64) -> Result<usize, String> {
    if first.len() < 16 || &first[4..8] != b"ftyp" {
        return Err("hg_mp4: 不支持的视频容器格式".into());
    }
    let ftyp = u64::from(read_u32_be(first, 0).ok_or("hg_mp4: ftyp 读取失败")?);
    if ftyp < 8 || (ftyp + 8) as usize > first.len() {
        return Err("hg_mp4: ftyp 尺寸异常".into());
    }
    let ftyp_u = ftyp as usize;
    if &first[ftyp_u + 4..ftyp_u + 8] != b"moov" {
        return Err("hg_mp4: 视频索引 moov 不在文件开头".into());
    }
    let moov_size = u64::from(read_u32_be(first, ftyp_u).ok_or("hg_mp4: moov 读取失败")?);
    let end = ftyp.checked_add(moov_size).ok_or("hg_mp4: moov 偏移溢出")?;
    if moov_size < 8 || end > total || end > MAX_MOOV_END {
        return Err("hg_mp4: 视频索引大小异常".into());
    }
    Ok(end as usize)
}

fn read_bounded_count(data: &[u8], at: usize, limit: usize) -> Result<usize, String> {
    if at + 4 > limit {
        return Err("hg_mp4: 索引越界".into());
    }
    let v = read_u32_be(data, at).ok_or("hg_mp4: 索引读取失败")?;
    if v > MAX_SAMPLE_COUNT {
        return Err("hg_mp4: 索引样本数过大".into());
    }
    Ok(v as usize)
}

/// Parse CENC sample/subsample tables and patch `encv`/`enca` -> original `frma` codec + clear `sinf`.
pub fn prepare_cenc_header(data: &[u8], key: Option<[u8; 16]>) -> Result<PreparedCenc, String> {
    if key.is_none() {
        return Ok(PreparedCenc {
            header: data.to_vec(),
            samples: Vec::new(),
        });
    }
    let ftyp_size = read_u32_be(data, 0).ok_or("hg_mp4: 头部不完整")? as usize;
    let moov = read_box(data, ftyp_size, data.len()).ok_or("hg_mp4: 视频索引不完整")?;
    if &moov.kind != b"moov" {
        return Err("hg_mp4: 缺少 moov 索引".into());
    }

    let mut samples = Vec::new();
    let mut header = data.to_vec();
    let mut cursor = moov.body;
    while cursor < moov.end {
        let track = read_box(data, cursor, moov.end).ok_or("hg_mp4: 视频轨道格式异常")?;
        cursor = track.end;
        if &track.kind != b"trak" {
            continue;
        }
        let Some(stbl) = find_box(data, b"stbl", track.body, track.end) else {
            continue;
        };
        let Some(saiz) = find_box(data, b"saiz", stbl.body, stbl.end) else {
            continue;
        };
        let saio =
            find_box(data, b"saio", stbl.body, stbl.end).ok_or("hg_mp4: 视频加密索引 saio 缺失")?;
        let stsz =
            find_box(data, b"stsz", stbl.body, stbl.end).ok_or("hg_mp4: 样本索引 stsz 缺失")?;
        let stsc =
            find_box(data, b"stsc", stbl.body, stbl.end).ok_or("hg_mp4: 块索引 stsc 缺失")?;
        let chunks = find_box(data, b"stco", stbl.body, stbl.end)
            .or_else(|| find_box(data, b"co64", stbl.body, stbl.end))
            .ok_or("hg_mp4: 块偏移 stco/co64 缺失")?;

        let n = read_bounded_count(data, stsz.body + 8, stsz.end)?;
        let default_size = u64::from(read_u32_be(data, stsz.body + 4).ok_or("hg_mp4: stsz 截断")?);
        if default_size == 0 && stsz.body + 12 + n * 4 > stsz.end {
            return Err("hg_mp4: 样本索引截断".into());
        }
        let mut sizes = Vec::with_capacity(n);
        for i in 0..n {
            if default_size > 0 {
                sizes.push(default_size);
            } else {
                sizes.push(u64::from(
                    read_u32_be(data, stsz.body + 12 + i * 4).unwrap(),
                ));
            }
        }

        let chunk_count = read_bounded_count(data, chunks.body + 4, chunks.end)?;
        let width64 = &chunks.kind == b"co64";
        let width = if width64 { 8 } else { 4 };
        if chunks.body + 8 + chunk_count * width > chunks.end {
            return Err("hg_mp4: 块偏移截断".into());
        }
        let mut offsets = Vec::with_capacity(chunk_count);
        for i in 0..chunk_count {
            let at = chunks.body + 8 + i * width;
            let off = if width64 {
                read_u64_be(data, at).unwrap()
            } else {
                u64::from(read_u32_be(data, at).unwrap())
            };
            offsets.push(off);
        }

        let entry_count = read_bounded_count(data, stsc.body + 4, stsc.end)?;
        if entry_count == 0 || stsc.body + 8 + entry_count * 12 > stsc.end {
            return Err("hg_mp4: 块索引截断".into());
        }
        let mut entries = Vec::with_capacity(entry_count);
        for i in 0..entry_count {
            let at = stsc.body + 8 + i * 12;
            let first_chunk = read_u32_be(data, at).unwrap() as usize;
            let spc = read_u32_be(data, at + 4).unwrap() as usize;
            entries.push((first_chunk, spc));
        }

        let saiz_flags = read_u32_be(data, saiz.body).ok_or("hg_mp4: saiz 截断")?;
        let size_base = saiz.body + 4 + if (saiz_flags & 1) != 0 { 8 } else { 0 };
        if size_base + 5 > saiz.end {
            return Err("hg_mp4: 辅助索引截断".into());
        }
        let aux_default = data[size_base] as usize;
        let aux_count = read_bounded_count(data, size_base + 1, saiz.end)?;
        if aux_count != n || (aux_default == 0 && size_base + 5 + n > saiz.end) {
            return Err("hg_mp4: 辅助索引数量不一致".into());
        }
        let mut aux_sizes = Vec::with_capacity(n);
        for i in 0..n {
            aux_sizes.push(if aux_default > 0 {
                aux_default
            } else {
                data[size_base + 5 + i] as usize
            });
        }

        let saio_flags = read_u32_be(data, saio.body).ok_or("hg_mp4: saio 截断")?;
        let offset_base = saio.body + 4 + if (saio_flags & 1) != 0 { 8 } else { 0 };
        let offset_width = if data[saio.body] == 1 { 8 } else { 4 };
        if read_bounded_count(data, offset_base, saio.end)? != 1
            || offset_base + 4 + offset_width > saio.end
        {
            return Err("hg_mp4: 不支持的辅助索引布局".into());
        }
        let mut aux_pos = if offset_width == 8 {
            read_u64_be(data, offset_base + 4).unwrap()
        } else {
            u64::from(read_u32_be(data, offset_base + 4).unwrap())
        };
        let aux_total: u64 = aux_sizes.iter().map(|&s| s as u64).sum();
        let aux_end = aux_pos
            .checked_add(aux_total)
            .ok_or("hg_mp4: 辅助数据偏移溢出")?;
        if aux_end > data.len() as u64 {
            return Err("hg_mp4: 辅助数据超出索引范围".into());
        }

        let mut sample_index = 0usize;
        let mut entry = 0usize;
        for (chunk_idx, &chunk_offset) in offsets.iter().enumerate() {
            while entry + 1 < entries.len() && entries[entry + 1].0 <= chunk_idx + 1 {
                entry += 1;
            }
            let count = entries[entry].1;
            if count > n - sample_index || entries[entry].0 > chunk_idx + 1 {
                return Err("hg_mp4: 样本与块数量不一致".into());
            }
            let mut position = chunk_offset;
            for _ in 0..count {
                let length = sizes[sample_index];
                let aux_size = aux_sizes[sample_index];
                if length == 0 || position.checked_add(length).is_none() || aux_size < 8 {
                    return Err("hg_mp4: 样本索引异常".into());
                }
                let aux_u = aux_pos as usize;
                let mut iv = [0u8; 16];
                iv[0..8].copy_from_slice(&data[aux_u..aux_u + 8]);
                if aux_size == 8 {
                    samples.push(CencSample {
                        start: position,
                        end: position + length,
                        iv,
                        encrypted_offset: 0,
                    });
                } else {
                    if aux_size < 10 {
                        return Err("hg_mp4: 不支持的加密样本格式".into());
                    }
                    let subs = u16::from_be_bytes([data[aux_u + 8], data[aux_u + 9]]) as usize;
                    if aux_size != 10 + subs * 6 {
                        return Err("hg_mp4: 加密子样本索引异常".into());
                    }
                    let mut offset = position;
                    let mut encrypted = 0u64;
                    for sub in 0..subs {
                        let at = aux_u + 10 + sub * 6;
                        let clear = u64::from(u16::from_be_bytes([data[at], data[at + 1]]));
                        let enc_size = u64::from(read_u32_be(data, at + 2).unwrap());
                        offset = offset.checked_add(clear).ok_or("hg_mp4: 子样本偏移溢出")?;
                        let sub_end = offset
                            .checked_add(enc_size)
                            .ok_or("hg_mp4: 子样本长度溢出")?;
                        if sub_end > position + length {
                            return Err("hg_mp4: 加密子样本越界".into());
                        }
                        if enc_size > 0 {
                            samples.push(CencSample {
                                start: offset,
                                end: sub_end,
                                iv,
                                encrypted_offset: encrypted,
                            });
                        }
                        encrypted += enc_size;
                        offset = sub_end;
                    }
                    if offset != position + length {
                        return Err("hg_mp4: 加密子样本长度异常".into());
                    }
                }
                position += length;
                aux_pos += aux_size as u64;
                sample_index += 1;
            }
        }
        if sample_index != n {
            return Err("hg_mp4: 视频样本不完整".into());
        }

        // Restore original codec FourCC from `sinf -> frma` (e.g. avc1, hvc1, mp4a)
        // and neutralize `sinf` box to `free`.
        let mut at = stbl.body;
        while at + 8 <= stbl.end {
            let Some(sinf) = find_box(data, b"sinf", at, stbl.end) else {
                break;
            };
            let frma = find_box(data, b"frma", sinf.body, sinf.end)
                .ok_or("hg_mp4: 原始编码格式 frma 缺失")?;
            if frma.body + 4 > frma.end {
                return Err("hg_mp4: 原始编码格式无效".into());
            }
            let original = &data[frma.body..frma.body + 4];
            if sinf.start > stbl.body {
                let mut entry_at = sinf.start - 1;
                loop {
                    if entry_at + 4 <= data.len() {
                        let tag = &data[entry_at..entry_at + 4];
                        if tag == b"encv" || tag == b"enca" {
                            header[entry_at..entry_at + 4].copy_from_slice(original);
                            break;
                        }
                    }
                    if entry_at == stbl.body {
                        break;
                    }
                    entry_at -= 1;
                }
            }
            header[sinf.start + 4..sinf.start + 8].copy_from_slice(b"free");
            header[sinf.body..sinf.end].fill(0);
            at = sinf.end;
        }
    }

    if samples.is_empty() {
        return Err("hg_mp4: 视频加密索引不可用".into());
    }
    samples.sort_by_key(|s| s.start);
    if samples.windows(2).any(|w| w[0].end > w[1].start) {
        return Err("hg_mp4: 视频样本区间重叠".into());
    }
    Ok(PreparedCenc { header, samples })
}

/// Decrypt an arbitrary byte range `[start, start + data.len())` of the progressive MP4.
pub fn decrypt_range(
    data: &[u8],
    start: u64,
    key: Option<[u8; 16]>,
    prepared: &PreparedCenc,
) -> Vec<u8> {
    let mut out = data.to_vec();
    let end = start + out.len() as u64;
    let header_len = prepared.header.len() as u64;
    if start < header_len {
        let copy_end = end.min(header_len) as usize;
        let start_u = start as usize;
        let len = copy_end - start_u;
        out[..len].copy_from_slice(&prepared.header[start_u..copy_end]);
    }
    let Some(key_bytes) = key else {
        return out;
    };
    let idx = prepared
        .samples
        .partition_point(|sample| sample.end <= start);
    for sample in &prepared.samples[idx..] {
        if sample.start >= end {
            break;
        }
        let begin = start.max(sample.start);
        let stop = end.min(sample.end);
        if begin >= stop {
            continue;
        }
        let relative = (begin - sample.start) + sample.encrypted_offset;
        let slice_start = (begin - start) as usize;
        let slice_end = (stop - start) as usize;
        aes128_ctr_apply(
            &key_bytes,
            &sample.iv,
            relative,
            &mut out[slice_start..slice_end],
        );
    }
    out
}

// ---------- PlaybackModels & StreamLadder ----------

pub fn quality_number(raw: &str) -> u32 {
    let text = raw.trim();
    for &known in &[2160u32, 1440, 1080, 720, 576, 540, 480, 360] {
        if text.contains(&known.to_string()) {
            return known;
        }
    }
    match text {
        "1920" => 1080,
        "1280" => 720,
        "1024" => 576,
        "854" => 480,
        "640" => 360,
        _ => {
            if (3..=4).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_digit()) {
                text.parse().unwrap_or(0)
            } else {
                0
            }
        }
    }
}

fn json_str(obj: &Value, key: &str) -> String {
    match obj.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn json_u32(obj: &Value, key: &str) -> u32 {
    match obj.get(key) {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0) as u32,
        Some(Value::String(s)) => s.trim().parse().unwrap_or(0),
        _ => 0,
    }
}

pub fn parse_json_object(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::Object(_) => value.cloned(),
        Value::String(s) => serde_json::from_str::<Value>(s)
            .ok()
            .filter(|v| v.is_object()),
        _ => None,
    }
}

fn is_valid_media_url(raw: &str) -> Option<String> {
    let cleaned = clean_url(raw);
    if !(cleaned.starts_with("https://") || cleaned.starts_with("http://")) {
        return None;
    }
    let after = cleaned.split_once("://")?.1;
    let authority = after.split(['/', '?']).next()?;
    if authority.is_empty() || authority.contains('@') || cleaned.chars().any(char::is_control) {
        return None;
    }
    Some(cleaned)
}

fn decode_media_url(entry: &Value, field: &str, seed: Option<&[u8]>) -> Option<String> {
    let raw = clean_url(&json_str(entry, field));
    if raw.is_empty() {
        return None;
    }
    if let Some(direct) = is_valid_media_url(&raw) {
        return Some(direct);
    }
    if let Some(seed_bytes) = seed {
        if let Ok(decrypted) = decrypt_spade_url(&raw, seed_bytes) {
            if let Some(valid) = is_valid_media_url(&decrypted) {
                return Some(valid);
            }
        }
    }
    if let Ok(decoded) = base64_decode(&raw) {
        if let Ok(text) = String::from_utf8(decoded) {
            if let Some(valid) = is_valid_media_url(&text) {
                return Some(valid);
            }
        }
    }
    None
}

fn is_bytevc2(codec_text: &str) -> bool {
    let lower = codec_text.to_ascii_lowercase();
    if lower.contains("bytevc2") {
        return true;
    }
    // Check standalone token `bvc2`
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| tok == "bvc2")
}

/// Extract playable renditions from a `video_model` (or `video_info.data`) JSON object.
/// Excludes `bytevc2` / `bvc2` variants that libmpv/FFmpeg cannot decode.
pub fn parse_renditions(model: &Value) -> Vec<Rendition> {
    let seed = {
        let s = json_str(model, "key_seed");
        if s.is_empty() {
            None
        } else {
            base64_decode(&s).ok()
        }
    };
    let entries: Vec<(String, &Value)> = match model.get("video_list") {
        Some(Value::Object(map)) => map
            .iter()
            .filter(|(_, v)| v.is_object())
            .map(|(k, v)| (k.clone(), v))
            .collect(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter(|v| v.is_object())
            .map(|v| (String::new(), v))
            .collect(),
        _ => Vec::new(),
    };

    let mut out = Vec::new();
    for (name, entry) in entries {
        let meta = entry.get("video_meta").filter(|v| v.is_object());
        let codec_all = format!(
            "{} {} {}",
            json_str(entry, "codec_type"),
            json_str(entry, "gear_des_key"),
            meta.map(|m| json_str(m, "codec_type")).unwrap_or_default()
        );
        if is_bytevc2(&codec_all) {
            continue;
        }
        let encryption = entry.get("encrypt_info").filter(|v| v.is_object());
        let spade_a = encryption
            .map(|e| json_str(e, "spade_a"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| json_str(entry, "spade_a"));
        let key = if spade_a.is_empty() {
            None
        } else {
            match derive_content_key(&spade_a) {
                Ok(k) => Some(k),
                Err(_) => continue,
            }
        };
        if let Some(enc_obj) = encryption {
            if key.is_none() {
                let not_encrypted = matches!(
                    enc_obj.get("encrypt"),
                    Some(Value::Bool(false))
                        | Some(Value::Number(_)) if enc_obj.get("encrypt").and_then(Value::as_i64) == Some(0)
                ) || matches!(
                    enc_obj.get("encrypt").and_then(Value::as_str),
                    Some("0") | Some("false")
                );
                if !not_encrypted {
                    continue;
                }
            }
        }
        let backup = decode_media_url(entry, "backup_url_1", seed.as_deref());
        let Some(resolved) = decode_media_url(entry, "main_url", seed.as_deref())
            .or_else(|| decode_media_url(entry, "play_url", seed.as_deref()))
            .or_else(|| backup.clone())
        else {
            continue;
        };
        let width = {
            let w = json_u32(entry, "vwidth");
            if w > 0 {
                w
            } else {
                meta.map(|m| {
                    let mw = json_u32(m, "vwidth");
                    if mw > 0 {
                        mw
                    } else {
                        json_u32(m, "width")
                    }
                })
                .unwrap_or(0)
            }
        };
        let height = {
            let h = json_u32(entry, "vheight");
            if h > 0 {
                h
            } else {
                meta.map(|m| {
                    let mh = json_u32(m, "vheight");
                    if mh > 0 {
                        mh
                    } else {
                        json_u32(m, "height")
                    }
                })
                .unwrap_or(0)
            }
        };
        let dim_str = if width > 0 && height > 0 {
            width.min(height).to_string()
        } else {
            String::new()
        };
        let q_source = [
            json_str(entry, "quality_desc"),
            meta.map(|m| json_str(m, "definition")).unwrap_or_default(),
            dim_str,
            json_str(entry, "height"),
            json_str(entry, "vheight"),
            json_str(entry, "quality"),
            name,
        ]
        .into_iter()
        .find(|s| !s.is_empty())
        .unwrap_or_default();
        let quality = quality_number(&q_source);
        let codec_label = {
            let c = json_str(entry, "codec_type");
            if !c.is_empty() {
                c.to_ascii_lowercase()
            } else {
                meta.map(|m| json_str(m, "codec_type").to_ascii_lowercase())
                    .unwrap_or_default()
            }
        };
        let backup_dedup = backup.filter(|b| b != &resolved);
        out.push(Rendition {
            quality,
            codec: codec_label,
            width: (width > 0).then_some(width),
            height: (height > 0).then_some(height),
            url: resolved,
            backup_url: backup_dedup,
            key,
        });
    }
    out
}

pub fn parse_fallback_api(model: &Value) -> Option<String> {
    let raw = model.get("fallback_api")?;
    let url = match raw {
        Value::Array(arr) => arr.first().and_then(Value::as_str).unwrap_or("").to_owned(),
        Value::Object(obj) => obj
            .get("fallback_api")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
        Value::String(s) => serde_json::from_str::<Value>(s)
            .ok()
            .and_then(|v| {
                v.get("fallback_api")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| s.clone()),
        _ => return None,
    };
    is_valid_media_url(&url).filter(|u| u.starts_with("https://"))
}

fn is_avc(codec: &str) -> bool {
    matches!(codec, "h264" | "avc" | "avc1" | "avc3")
}

/// Capability-first rendition selector (highest quality <= max_quality, preferring AVC on ties).
pub fn select_rendition(renditions: &[Rendition], max_quality: u32) -> Option<Rendition> {
    let rank = |r: &Rendition| (r.quality, if is_avc(&r.codec) { 1u8 } else { 0u8 });
    renditions
        .iter()
        .filter(|r| r.quality >= 1 && r.quality <= max_quality)
        .max_by_key(|r| rank(r))
        .or_else(|| {
            renditions
                .iter()
                .filter(|r| r.quality == 0)
                .max_by_key(|r| rank(r))
        })
        .or_else(|| renditions.iter().min_by_key(|r| rank(r)))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::hg_crypto::hex_decode;

    fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(body.len() + 8);
        out.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    fn cenc_fixture(codec: &[u8; 4], aux_size: u8, iv8: &[u8]) -> Vec<u8> {
        let ftyp = mp4_box(b"ftyp", b"isom0000");
        let build_moov = |saio_offset: u32| -> Vec<u8> {
            let mut encv_body = vec![0u8; 78];
            encv_body.extend_from_slice(&mp4_box(b"sinf", &mp4_box(b"frma", codec)));
            let entry = mp4_box(b"encv", &encv_body);
            let mut stsd_body = vec![0u8; 4];
            stsd_body.extend_from_slice(&1u32.to_be_bytes());
            stsd_body.extend_from_slice(&entry);
            let stsd = mp4_box(b"stsd", &stsd_body);

            let mut stsz_body = vec![0u8; 4];
            stsz_body.extend_from_slice(&16u32.to_be_bytes());
            stsz_body.extend_from_slice(&1u32.to_be_bytes());
            let stsz = mp4_box(b"stsz", &stsz_body);

            let mut stsc_body = vec![0u8; 4];
            stsc_body.extend_from_slice(&1u32.to_be_bytes());
            stsc_body.extend_from_slice(&1u32.to_be_bytes());
            stsc_body.extend_from_slice(&1u32.to_be_bytes());
            stsc_body.extend_from_slice(&1u32.to_be_bytes());
            let stsc = mp4_box(b"stsc", &stsc_body);

            let mut stco_body = vec![0u8; 4];
            stco_body.extend_from_slice(&1u32.to_be_bytes());
            stco_body.extend_from_slice(&4096u32.to_be_bytes());
            let stco = mp4_box(b"stco", &stco_body);

            let mut saiz_body = vec![0u8; 4];
            saiz_body.push(aux_size);
            saiz_body.extend_from_slice(&1u32.to_be_bytes());
            let saiz = mp4_box(b"saiz", &saiz_body);

            let mut saio_body = vec![0u8; 4];
            saio_body.extend_from_slice(&1u32.to_be_bytes());
            saio_body.extend_from_slice(&saio_offset.to_be_bytes());
            let saio = mp4_box(b"saio", &saio_body);

            let mut stbl_body = Vec::new();
            stbl_body.extend_from_slice(&stsd);
            stbl_body.extend_from_slice(&stsz);
            stbl_body.extend_from_slice(&stsc);
            stbl_body.extend_from_slice(&stco);
            stbl_body.extend_from_slice(&saiz);
            stbl_body.extend_from_slice(&saio);
            let trak = mp4_box(
                b"trak",
                &mp4_box(b"mdia", &mp4_box(b"minf", &mp4_box(b"stbl", &stbl_body))),
            );
            let mut moov_body = trak;
            moov_body.extend_from_slice(&mp4_box(b"free", iv8));
            mp4_box(b"moov", &moov_body)
        };
        let initial_len = ftyp.len() + build_moov(0).len();
        let mut full = ftyp;
        full.extend_from_slice(&build_moov((initial_len - 8) as u32));
        full
    }

    #[test]
    fn nist_ctr_unaligned_and_subsample_decryption_matches() {
        let key: [u8; 16] = hex_decode("2b7e151628aed2a6abf7158809cf4f3c")
            .unwrap()
            .try_into()
            .unwrap();
        let iv: [u8; 16] = hex_decode("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff")
            .unwrap()
            .try_into()
            .unwrap();
        let plain = hex_decode(
            "6bc1bee22e409f96e93d7e117393172aae2d8a571e03ac9c9eb76fac45af8e5130c81c46a35ce411e5fbc1191a0a52eff69f2445df4f9b17ad2b417be66c3710",
        )
        .unwrap();
        let encrypted = hex_decode(
            "874d6191b620e3261bef6864990db6ce9806f66b7970fdff8617187bb9fffdff5ae4df3edbd5d35e5b4f09020db03eab1e031dda2fbe03d1792170a0f3009cee",
        )
        .unwrap();

        let mut source = vec![0x66u8; 23];
        source.extend_from_slice(&encrypted);
        source.extend_from_slice(&[0x77u8; 12]);
        let mut expected = vec![0x66u8; 23];
        expected.extend_from_slice(&plain);
        expected.extend_from_slice(&[0x77u8; 12]);
        let prepared = PreparedCenc {
            header: vec![],
            samples: vec![CencSample {
                start: 23,
                end: 87,
                iv,
                encrypted_offset: 0,
            }],
        };
        for start in 0..source.len() {
            for len in 1..=(source.len() - start).min(29) {
                let dec = decrypt_range(
                    &source[start..start + len],
                    start as u64,
                    Some(key),
                    &prepared,
                );
                assert_eq!(dec, expected[start..start + len]);
            }
        }

        // Subsample test: clear(3) + enc(21) + clear(2) + enc(43)
        let mut sub_src = vec![1, 2, 3];
        sub_src.extend_from_slice(&encrypted[0..21]);
        sub_src.extend_from_slice(&[4, 5]);
        sub_src.extend_from_slice(&encrypted[21..64]);
        let mut sub_exp = vec![1, 2, 3];
        sub_exp.extend_from_slice(&plain[0..21]);
        sub_exp.extend_from_slice(&[4, 5]);
        sub_exp.extend_from_slice(&plain[21..64]);
        let sub_prep = PreparedCenc {
            header: vec![],
            samples: vec![
                CencSample {
                    start: 3,
                    end: 24,
                    iv,
                    encrypted_offset: 0,
                },
                CencSample {
                    start: 26,
                    end: 69,
                    iv,
                    encrypted_offset: 21,
                },
            ],
        };
        assert_eq!(decrypt_range(&sub_src, 0, Some(key), &sub_prep), sub_exp);
        assert_eq!(
            decrypt_range(&sub_src[22..47], 22, Some(key), &sub_prep),
            sub_exp[22..47]
        );
    }

    #[test]
    fn header_restores_original_codec_and_rejects_malformed() {
        let key = [1u8; 16];
        let iv8 = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7];
        let data = cenc_fixture(b"avc1", 8, &iv8);
        let prep = prepare_cenc_header(&data, Some(key)).unwrap();
        assert!(prep.header.windows(4).any(|w| w == b"avc1"));
        assert!(!prep.header.windows(4).any(|w| w == b"encv"));
        assert!(!prep.header.windows(4).any(|w| w == b"sinf"));
        assert_eq!(prep.samples.len(), 1);
        assert_eq!(prep.samples[0].start, 4096);
        assert_eq!(prep.samples[0].end, 4112);
        assert_eq!(header_end(&data, 5000).unwrap(), data.len());

        // Truncated aux fails closed
        let truncated = cenc_fixture(b"avc1", 16, &iv8);
        assert!(prepare_cenc_header(&truncated, Some(key)).is_err());

        // Oversized moov rejected
        let mut huge = mp4_box(b"ftyp", b"isom0000");
        huge.extend_from_slice(&9_000_000u32.to_be_bytes());
        huge.extend_from_slice(b"moov");
        assert!(header_end(&huge, 10_000_000).is_err());
    }

    #[test]
    fn stream_ladder_excludes_bytevc2_and_prefers_avc_at_equal_quality() {
        let model = serde_json::json!({
            "video_list": [
                {
                    "main_url": "https://cdn.example.com/bvc2.mp4",
                    "video_meta": { "codec_type": "bytevc2", "definition": "1080p", "vwidth": 1080, "vheight": 1920 }
                },
                {
                    "main_url": "https://cdn.example.com/hevc.mp4",
                    "backup_url_1": "https://backup.example.com/hevc.mp4",
                    "video_meta": { "codec_type": "bytevc1", "definition": "1080p", "vwidth": 1080, "vheight": 1920 }
                },
                {
                    "main_url": "https://cdn.example.com/avc.mp4",
                    "backup_url_1": "https://backup.example.com/avc.mp4",
                    "video_meta": { "codec_type": "h264", "definition": "1080p", "vwidth": 1080, "vheight": 1920 }
                }
            ]
        });
        let rows = parse_renditions(&model);
        assert_eq!(rows.len(), 2);
        let picked = select_rendition(&rows, 1080).unwrap();
        assert_eq!(picked.codec, "h264");
        assert_eq!(picked.url, "https://cdn.example.com/avc.mp4");
        assert_eq!(
            picked.backup_url.as_deref(),
            Some("https://backup.example.com/avc.mp4")
        );
    }
}
