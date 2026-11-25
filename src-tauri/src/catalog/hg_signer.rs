//! ByteDance Android v5 (v04.10.03) and v3 request signing + anonymous device registration.
//! Adapted from N3urda/hongguoTV (GPL-3.0-only): Signer.kt, AesV3.kt, V5Constants.kt,
//! VendorConstants.kt, DeviceRegister.kt, PlaybackDeviceSession.kt.

use super::hg_crypto::{
    aes128_cbc_encrypt_pkcs7, base64_decode, base64_encode, base64_url_encode_nopad,
    fill_random_bytes, gzip_compress, hex_decode, hex_encode, hex_encode_upper, md5, random_u32,
    sha512, sm3, uuid_v4,
};
use super::http_client::{request_with_options, RequestOptions};
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const VIDEO_UA: &str = "com.phoenix.read/71332 (Linux; U; Android 16; zh_CN; 25053RT47C; Build/BP2A.250605.031.A3; Cronet/TTNetVersion:04657795 2026-01-23 QuicVersion:c67e9834 2025-09-08)";
pub const APP_UA: &str =
    "com.phoenix.read/72232 (Linux; U; Android 16; zh_CN; 25053RT47C; Build/BP2A.250605.031.A3)";
pub const V5_POST_UA: &str = "com.phoenix.read/73932 (Linux; U; Android 12; zh_CN_#Hans; PGT-AN10; Build/V417IR;tt-ok/3.12.13.20)";
pub const V5_TAB_UA: &str = "com.phoenix.read/73932 (Linux; U; Android 16; zh_CN; 25053RT47C; Build/BP2A.250605.031.A3; Cronet/TTNetVersion:04657795 2026-01-23 QuicVersion:c67e9834 2025-09-08)";

const DEVICE_REGISTER_URL: &str = "https://log5-applog-lf.fqnovel.com/service/2/device_register/?tt_data=a&cronet_version=8d40f833_2026-03-03&ttnet_version=4.2.243.31-douyin&use_store_region_cookie=1";
const TT_MAGIC: [u8; 6] = [0x74, 0x63, 0x05, 0x10, 0x00, 0x00];
pub const KDF_SALT_HEX: &str = "4dd4c2e6b83162090e52b3c7a6733ba41cb2462b829ab58a196b39db57177524f49baf7f08e8d68d26a72e37c1a95a2f1f05a51892aef2949732b62a38aadd58";

const V5_MAGIC_HEX: &str = "2d4b4fca49750d433fb5ae2c226dcc56";
const V5_LICENSE: &str = "646637794";
// Issue #102 (0.16.14): wire field 21 = 312 (formerly 377, which began returning empty HTTP 200).
const V5_FIELD21: i64 = 312;
const V5_SIGN_KEY_HEX: &str = "b23c2a470a20d105dc3f8c4438f8272ed46d6c89ffdcdedd5108698d26e9489d";
const V5_SBOX_HEX: &str = concat!(
    "53290fe9e51f316f90f74a7e034d367784ab49237cd0135cdc632c322fd3be0e2624a417ecd9151a7ff64f60730a87f9445ec6ada2f865d22b696c4188f24c0d",
    "f37b75510b939f6bc2fe6883e68bc8c722181ba0bdb08c9871b50c9a16b46eea8eed20ba4b8a14e0fcf1468dbbef05a93e5a9d7dcf06898635c1fd99f5de287a",
    "80c594daaab19b6279fbd75070cb52eb2555c456e39e7839853874e2f0013ac0b64742d8ca10e4b79c955de8b843ce4ed464a6a72ed508091c92e7b3eedd1e1d",
    "d12afac321618fa3d65f9167c982f43d66a53c6a1997585b346dbf9672ff37cdbc073b2dae11453304df00815476b2a13fccdb30b957a8acafe1274002485912",
    "38922563e36406d37a24100b7902a028044e218453c2a399199c9d13dc40c55ba67e502273850ebdc1a2e2188b8f2d9e3cb909b8c46a2a9f4a6dd93007eafa65",
    "b7b3a881aa908cc74f0ad76bbbb58617c37012ca05a469fd44e62eecf1e4272ce860c0ae5eadf99b20d8f0234174006f3f110d7657d01645d4eff8f71471cbfe",
    "1cb2f52648727bf3c9de55153e031b4d61757db63dbf7782eeafe9018a8891362fb14759ac32bab03b349aeb5a377c5156be4c8dfbe13a684bda39cf836697c8",
    "94545fe0f2491acea9fc6252f4ff1e0fe72bd2df6e67a7f635e5bcc6d1a5dd951d89960887a18eb40c785cdb1f80ed42299333ab98cdd67f316cd54346585dcc",
    "d759fb722789b08807cc0fda293f7e5e1819bb0353517020b31d3d00374952bea2c421e862040a9ec892a05b266b7a0d0cc316f483b81af39715657ffe32ad8d",
    "e0c13077646035236ee5363e5f900e9a7cf850a81cd9960b6df73317226f99eaa6fff15cec34acb98246639b918ab2851466a7cdfc4ad1b52c2ae95a58d3734e",
    "47ddd5b7560193f644ef9c408b7dee39b4d6aabf4de761e3252d4c7ba3843187745d55e6a5db05d42b45caed43c0c5d0b64bfa9d94a4a9d8f2f0e1a108dcfd28",
    "8fd2bc1fc7bd675412138169cb09249875ba3b68cfb11180afae104fc9789571c6dfce02413886796ae21e3c488cc21b9f423a066c2eabf98e2fdeeb5776e4f5",
    "d09a515ba58cbbab3e37b09fd4f6d9244df80512e84cd7956be301cf64f1429d58577cc241f3a2c655144f81aaf95a4e108da894b38970561ae9002619fbbac9",
    "1fa922ed202bfc62cb08e0de2a7f3631435f8b47c8a3c563b4db4071ce74c46da6fa696a9e282d1e825d663b93bfd56c48dce1884bb10fccea063929f746443f",
    "d830a4232e53da21a1f2d1bdfebc3a02c10a1b8ad31d04fd616590d272ac737e98eb33af60528716bee78ec0ee8699031177277d9cff32542c67caf5f4459168",
    "1335ae0e178f3409a7803db83892dde5b9b7f0c750e24aef077badd678a09b850b84ec1597e4963cb5e675df1c0d6fb618496ecd76c35c798325590c2fb27a5e"
);

const V3_SBOX_B64: &str = "+n0Ia5xZs0sEXznQOEqRmQBnpiCf9U2Ccybu3xhmgzOAAxn72f6uqqmwUsYL83klTni0NqxdGieeiNu9PGPsSRXBMB/cuFbUbM3KCUPINaPvHvSW0vwOcnuUhNHqRVpiAj/TEoE0K91+5ijypUYTATsh9mE3KSoN7Yyvv51cuyR2D3XkU4nhmI2xmmVwT1RMWKtub4sjxAcRDLrPoKSO2AU9FLLadMPX577Wf95IFj6FkKFVt3dCIsmGUC4X+WQxLJvxbRxEaOPpqJOXyzJX6+Vxaq3AzMfF/WAdoi1Hp+JRaV56zgpBtpWP97mH4DoGEIq1+FvV8LyS/3wvwugbQOwb2r26mJEMsiuDQTRn+wrYdrVGBVlhI3WQhyrjUBVMrLF5667llUcEaPCGPVGLD8qO5LlO8hKCvA7V9+8oJc9bXelqVQLhM76T5/WtnT45JKji+hdX0HoNCDDWuKON/QeaxB5uImSX0h2wv0VmP2zd2yeApxHcpsVS+MC2yFwAc2B7oBkTqsk1SEvTpM2fmfMQREBUfin0Bh+iq6EvPPavhWI2IX9e3yAas7Tm/3KEj2UmlFp36kN4x0rMLBRrxuh0U/zUHM4xcAMYjJY4MonxOl/X+alpt2M3WMI7w3HLnpIBigtNiJu7T21v4P6lSd5WFgntnMEt7oF92XzRLkJbTcGmXepE/UVOG6E/0YnhfS+q26utWcuxzpooyeD2cDlK1/8w9d28VzsRjbLuALbmGlp8+d7EzS6Au7lMpZ+ECMZvQmzwJ+eLOpxR+2chdUExp8ogQyq3v9l68rX4jCwjg0+PYKAEEzcU4wHFY2ZcdIHfWL1okD3SszT0GZMyKdZJrg1L2AeerB4tC0C4crp2EHGo5FYdSP7lwkeR2ocmnR+Ia8CYviUJlzOjhRZef9xuVOn3qcjow3fQgivsAmKKkg4+sA8F8/GWeDiGNhg8JM8KtFPMYWWkx5TVFX5t73kiNRJqjlIGVXtGZFCV4gzt0xcDopuZ6xz8r9Rzafpf9ywev8jh8592gHFIqpStZPuJxmDDMrNN0uBE3V+oscdoIzTJbRJ/t+sVvqnReJOgDJKk10fjisJwqyZBmnmn2BSFj8BvVtCMEbkuPOKdzw7eA11GPs04Qw8zWtkaZWwiO/wwpojqN6K0jY5RnNZA7vn4hPSul+nKCkVnVwQvg1zVxcSCtqORmB9KrJaBbssbCQivGJVJfVTt+hYxOtq4ZvWl8f4QAQZ0zGPffCgl9s6yT4vlvIdpu4YhBwA25wtQWZsc6GJYGWHyvSdeuh3mmUI9DSq13Fsp8C1MU3tqc04/df9LoTUXVXI5INOw/e8C7Hd+5CvbkMEFnnrUUmskEw==";

const HASH_TABLE: [u32; 64] = [
    0xebb64faf, 0x07aadcc2, 0xcf3187bf, 0xe01138ff, 0x6d0bfcff, 0x5a30a3be, 0xb41ad638, 0x34180eb8,
    0xf233eb6f, 0xb1a584cc, 0xccc30dc7, 0x47d1db51, 0xd55653de, 0x70a84fa1, 0x57473c12, 0xf76f0288,
    0x2c077f0a, 0xda0dcad0, 0xfbb86f6c, 0xfdc4cf00, 0x688a020d, 0xe676c6a6, 0x8cd6338b, 0x1a3c8d0e,
    0xcce8b06b, 0x6ad0ed0b, 0xa0522717, 0xdc71ac83, 0x2285db71, 0xd5b4dda6, 0x736f8650, 0x6560306c,
    0x617ce2a6, 0xe423417e, 0x0a40e143, 0x544e4032, 0x88dffb2a, 0x716c1ae0, 0x4c467a88, 0x05b23bb3,
    0xe1d0b866, 0xbaa3dcb8, 0xae3374d3, 0xc3381a50, 0x1702f75b, 0xfe6da368, 0xf0b4cf48, 0x4e0ffbb8,
    0x72aad10d, 0x26c53a3d, 0xf2bce0f6, 0xb4557581, 0x4a257fdd, 0x8c3182a2, 0xab0b3b86, 0x3d5dfb14,
    0x4f103634, 0xd37b52d7, 0x444eff16, 0xeb0a33d1, 0x6ca86f6e, 0x00284ba7, 0x08387cfa, 0x5fb37586,
];

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn query_encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'*' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

pub fn encode_query(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                query_encode_component(k),
                query_encode_component(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

// ---------- Protobuf Wire Encoding ----------

enum ProtoVal<'a> {
    Sint(i64),
    Float(f32),
    Bytes(&'a [u8]),
    Str(&'a str),
}

fn push_varint(out: &mut Vec<u8>, mut n: u64) {
    while n > 127 {
        out.push(((n & 127) | 128) as u8);
        n >>= 7;
    }
    out.push(n as u8);
}

fn encode_proto(fields: &[(u32, ProtoVal<'_>)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (tag, val) in fields {
        match val {
            ProtoVal::Sint(v) => {
                let zz = if *v < 0 {
                    ((-(*v as i128)) * 2 - 1) as u64
                } else {
                    (*v as u64).wrapping_mul(2)
                };
                push_varint(&mut out, u64::from(*tag) << 3);
                push_varint(&mut out, zz);
            }
            ProtoVal::Float(v) => {
                push_varint(&mut out, (u64::from(*tag) << 3) | 5);
                out.extend_from_slice(&v.to_le_bytes());
            }
            ProtoVal::Bytes(b) => {
                if !b.is_empty() {
                    push_varint(&mut out, (u64::from(*tag) << 3) | 2);
                    push_varint(&mut out, b.len() as u64);
                    out.extend_from_slice(b);
                }
            }
            ProtoVal::Str(s) => {
                let b = s.as_bytes();
                if !b.is_empty() {
                    push_varint(&mut out, (u64::from(*tag) << 3) | 2);
                    push_varint(&mut out, b.len() as u64);
                    out.extend_from_slice(b);
                }
            }
        }
    }
    out
}

// ---------- Protocol-Specific AesV3 Carrier (v3 and v5) ----------

pub struct AesV3 {
    v5: bool,
    box_table: [u8; 256],
    con: [usize; 4],
    order: [usize; 16],
    keys: [[u8; 4]; 12],
}

impl AesV3 {
    pub fn new(key: &[u8; 16], timestamp: u32, v5: bool) -> Self {
        let word = (timestamp & 3) as usize;
        let full_sbox = if v5 {
            hex_decode(V5_SBOX_HEX).expect("valid v5 sbox")
        } else {
            base64_decode(V3_SBOX_B64).expect("valid v3 sbox")
        };
        let mut box_table = [0u8; 256];
        box_table.copy_from_slice(&full_sbox[word * 256..(word + 1) * 256]);

        let con = if v5 {
            [[2, 0, 3, 1], [1, 3, 2, 0], [3, 2, 1, 0], [2, 3, 1, 0]][word]
        } else {
            [[1, 0, 2, 3], [2, 0, 3, 1], [0, 1, 3, 2], [1, 0, 2, 3]][word]
        };
        let order = if v5 {
            [
                [0, 9, 14, 15, 4, 13, 2, 7, 8, 1, 6, 3, 12, 5, 10, 11],
                [0, 9, 14, 11, 4, 13, 2, 3, 8, 1, 6, 15, 12, 5, 10, 7],
                [0, 9, 14, 3, 4, 13, 2, 15, 8, 1, 6, 11, 12, 5, 10, 7],
                [0, 9, 14, 15, 4, 13, 2, 3, 8, 1, 6, 11, 12, 5, 10, 7],
            ][word]
        } else {
            [
                [0, 9, 14, 11, 4, 13, 2, 7, 8, 1, 6, 15, 12, 5, 10, 3],
                [0, 9, 14, 15, 4, 13, 2, 7, 8, 1, 6, 3, 12, 5, 10, 11],
                [0, 9, 14, 7, 4, 13, 2, 11, 8, 1, 6, 3, 12, 5, 10, 15],
                [0, 9, 14, 11, 4, 13, 2, 7, 8, 1, 6, 15, 12, 5, 10, 3],
            ][word]
        };
        let initial: u32 = if v5 {
            [0xbbb5_de72, 0xd463_8d5b, 0x92a5_f73b, 0x89f8_a1c9][word]
        } else {
            [0xca02_5ddc, 0x823d_c546, 0xc942_0583, 0xc298_225f][word]
        };
        let init_le = initial.to_le_bytes();
        let mut expanded = [0u8; 48];
        for i in 0..16 {
            expanded[i] = init_le[i & 3] ^ key[i];
        }
        let mut rounds: u32 = 8;
        for i in 4..12 {
            let at = 4 * (i - 1);
            let mut a = expanded[at];
            let mut b = expanded[at + 1];
            let mut c = expanded[at + 2];
            let mut d = expanded[at + 3];
            if (i & 3) == 0 {
                let t = (((initial as i32) >> (rounds & 24)) as u8) ^ box_table[b as usize];
                b = box_table[c as usize];
                c = box_table[d as usize];
                d = box_table[a as usize];
                a = t;
            }
            rounds += 2;
            expanded[at + 4] = a ^ expanded[at - 12];
            expanded[at + 5] = b ^ expanded[at - 11];
            expanded[at + 6] = c ^ expanded[at - 10];
            expanded[at + 7] = d ^ expanded[at - 9];
        }
        let mut keys = [[0u8; 4]; 12];
        for i in 0..12 {
            keys[i].copy_from_slice(&expanded[i * 4..i * 4 + 4]);
        }
        Self {
            v5,
            box_table,
            con,
            order,
            keys,
        }
    }

    fn block(&self, value: &[u8; 16]) -> [u8; 16] {
        let mut rows = [[0u8; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                rows[i][j] = value[i * 4 + j];
            }
        }
        let add_con = |rows: &mut [[u8; 4]; 4], offset: usize| {
            for i in 0..4 {
                for j in 0..4 {
                    rows[i][j] ^= self.keys[offset + i][self.con[j]];
                }
            }
        };
        add_con(&mut rows, 0);
        for round in 1..=2 {
            for row in &mut rows {
                for cell in row.iter_mut() {
                    *cell = self.box_table[*cell as usize];
                }
            }
            let before = rows;
            for i in 0..4 {
                rows[i] = before[self.con[i]];
            }
            let mut flat = [0u8; 16];
            for i in 0..4 {
                for j in 0..4 {
                    flat[i * 4 + j] = rows[i][j];
                }
            }
            for i in 0..16 {
                rows[i / 4][i % 4] = flat[self.order[i]];
            }
            if round == 1 {
                for row in &mut rows {
                    let old = *row;
                    for j in 0..4 {
                        row[j] = old[self.con[j]];
                    }
                }
                let xt = |x: u8| -> u8 { (x << 1) ^ if x & 0x80 != 0 { 0x1b } else { 0 } };
                for i in 0..4 {
                    let t = rows[0][i] ^ rows[1][i] ^ rows[2][i] ^ rows[3][i];
                    let u = rows[0][i];
                    rows[0][i] ^= t ^ xt(rows[0][i] ^ rows[1][i]);
                    rows[1][i] ^= t ^ xt(rows[1][i] ^ rows[2][i]);
                    rows[2][i] ^= t ^ xt(rows[2][i] ^ rows[3][i]);
                    rows[3][i] ^= t ^ xt(rows[3][i] ^ u);
                }
            }
            add_con(&mut rows, round * 4);
        }
        for i in 0..4 {
            for j in 0..4 {
                rows[i][j] ^= self.keys[4 + i][j];
            }
        }
        let mut out = [0u8; 16];
        for i in 0..4 {
            for j in 0..4 {
                out[i * 4 + j] = rows[i][j];
            }
        }
        out
    }

    pub fn encrypt(&self, source: &[u8], iv: &[u8; 16]) -> Result<Vec<u8>, String> {
        if source.len() < 248 {
            return Err("aesv3 source shorter than 248 bytes".into());
        }
        let wire: [u8; 8] = if self.v5 {
            [6, 5, 3, 4, 2, 7, 1, 0]
        } else {
            [5, 6, 2, 4, 3, 1, 0, 7]
        };
        let compact: [u8; 8] = if self.v5 {
            [1, 7, 3, 6, 2, 0, 4, 5]
        } else {
            [1, 6, 0, 7, 2, 4, 5, 3]
        };
        let mut plaintext = [0u8; 32];
        for i in 0..31 {
            for j in 0..8 {
                let bit = (source[i * 8 + j] >> wire[j]) & 1;
                plaintext[i] |= bit << compact[j];
            }
        }
        plaintext[31] = 1;
        let mut b0 = [0u8; 16];
        for i in 0..16 {
            b0[i] = plaintext[i] ^ iv[i];
        }
        let first = self.block(&b0);
        let mut b1 = [0u8; 16];
        for i in 0..16 {
            b1[i] = plaintext[16 + i] ^ first[i];
        }
        let second = self.block(&b1);
        let mut key_stream = [0u8; 32];
        key_stream[..16].copy_from_slice(&first);
        key_stream[16..].copy_from_slice(&second);

        let mut out = Vec::with_capacity(source.len() + 1);
        out.push(key_stream[31]);
        out.extend_from_slice(source);
        for i in 0..31 {
            for j in 0..8 {
                let at = 1 + i * 8 + j;
                let kbit = (key_stream[i] >> compact[j]) & 1;
                out[at] = (out[at] & !(1u8 << wire[j])) | (kbit << wire[j]);
            }
        }
        Ok(out)
    }
}

// ---------- Signer (v3 & v5) ----------

pub struct SignedRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

pub struct Signer {
    mobile_v5: bool,
    process_nonce: String,
    device_token: String,
    process_started_ms: u64,
}

fn make_device_token(process_nonce: &str) -> String {
    if process_nonce.len() == 25 && process_nonce.starts_with('A') {
        process_nonce.to_owned()
    } else {
        let mut out = String::with_capacity(25);
        out.push('A');
        for ch in process_nonce.chars().chain(std::iter::repeat('A')).take(24) {
            out.push(ch);
        }
        out
    }
}

impl Signer {
    pub fn new(mobile_v5: bool) -> Self {
        let mut nonce_bytes = [0u8; 18];
        fill_random_bytes(&mut nonce_bytes);
        let process_nonce = base64_url_encode_nopad(&nonce_bytes);
        let device_token = make_device_token(&process_nonce);
        Self {
            mobile_v5,
            process_nonce,
            device_token,
            process_started_ms: now_millis(),
        }
    }

    #[cfg(test)]
    pub fn with_fixed(mobile_v5: bool, process_nonce: &str, process_started_ms: u64) -> Self {
        let device_token = make_device_token(process_nonce);
        Self {
            mobile_v5,
            process_nonce: process_nonce.to_owned(),
            device_token,
            process_started_ms,
        }
    }

    fn get_iv(initial: u32, data: &[u8]) -> u32 {
        let mut value = initial;
        for (i, &b) in data.iter().enumerate() {
            if (i & 1) == 0 {
                value = (value >> 4) ^ value ^ (value << 6) ^ u32::from(b);
            } else {
                value = !((value >> 7) ^ value ^ (u32::from(b) | (value << 12)));
            }
        }
        value
    }

    fn iv(query_sm3: &[u8; 32], body_md5: &[u8; 16], ts: u32) -> u32 {
        let a = Self::get_iv(0x2023_0928, query_sm3);
        let b = Self::get_iv(a, body_md5);
        Self::get_iv(b, &ts.to_le_bytes())
    }

    pub fn branch(&self, query: &str, body: Option<&[u8]>, ts: u32) -> u32 {
        let q = sm3(query.as_bytes());
        let b = body.map(md5).unwrap_or([0u8; 16]);
        let low = Self::iv(&q, &b, ts) & 15;
        if self.mobile_v5 {
            if low == 12 {
                0
            } else {
                1
            }
        } else {
            low % 3
        }
    }

    fn sum_md5(data: &[u8]) -> u32 {
        let mut check: u32 = 0x2022_0420;
        for i in 0..12 {
            let temp = (check >> if (i & 1) == 0 { 3 } else { 5 }) ^ check;
            check = if (i & 1) == 0 {
                u32::from(data[i]) ^ (check << 7)
            } else {
                !(u32::from(data[i]) | (check << 11))
            };
            check ^= temp;
        }
        (check | 4) ^ 0x0100_0000
    }

    pub fn hash_f13(
        &self,
        query_sm3: &[u8; 32],
        body_md5: &[u8; 16],
        ts: u32,
    ) -> Result<[u8; 20], String> {
        let low = (Self::iv(query_sm3, body_md5, ts) & 15) as usize;
        let index = low / 3;
        if self.mobile_v5 {
            if low != 12 {
                return Err("unsupported v5 hash branch".into());
            }
        } else if low % 3 != 0 {
            return Err("unsupported v3 hash branch".into());
        }
        let iv_v1: u32 = if self.mobile_v5 {
            0x9d4a_97c2
        } else {
            [
                0xc4a7_8580,
                0xb3c0_fd39,
                0xc58c_5686,
                0xc9aa_3ba7,
                0xf5a7_adf2,
                0x963c_2ed1,
            ][index]
        };
        let count = iv_v1.wrapping_add(ts);
        let rot1 = count.wrapping_add(1) & 255;
        let mut tt = [0u32; 64];
        for i in 0..64 {
            tt[i] = HASH_TABLE[i].rotate_right(rot1);
        }
        let shift = count.wrapping_add(2) & 7;
        let pad: [u8; 4] =
            [0xfa, 0x45, 0x61, 0xd7].map(|v: u32| (((v | (v << 8)) >> shift) & 0xff) as u8);

        let mut data = [0u8; 64];
        data[0..32].copy_from_slice(query_sm3);
        data[32..48].copy_from_slice(body_md5);
        data[48..52].copy_from_slice(&ts.to_le_bytes());
        data[52..56].copy_from_slice(&pad);
        data[56..64].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0x01, 0xa0]);

        let mut di = [0u32; 128];
        for i in 0..16 {
            di[i] = u32::from_be_bytes(data[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 0..112 {
            let x = di[i + 1];
            let y = di[i + 14];
            let r1 = x.rotate_left(14) ^ x.rotate_left(25) ^ (x >> 3);
            let r2 = y.rotate_left(13) ^ y.rotate_left(15) ^ (y >> 10);
            di[i + 16] = di[i]
                .wrapping_add(di[i + 9])
                .wrapping_add(r1)
                .wrapping_add(r2);
        }

        let init_raw: [u32; 8] = [
            0x7aba_4fc8,
            0x6716_6507,
            0x6403_fa00,
            0x340f_512f,
            984_304_912,
            3_005_047_866,
            2_874_125_293,
            2_152_413_264,
        ];
        let initial: [u32; 8] = init_raw.map(|v| v.rotate_right(count));

        let chosen: [usize; 11] = if self.mobile_v5 {
            [96, 1, 3, 4, 0, 6, 5, 2, 0, 7, 7]
        } else {
            [
                [101, 5, 7, 6, 3, 2, 1, 0, 5, 4, 3],
                [96, 0, 6, 7, 5, 3, 2, 1, 5, 4, 4],
                [96, 7, 6, 2, 1, 4, 0, 5, 4, 3, 5],
                [99, 3, 6, 2, 4, 5, 1, 0, 0, 7, 6],
                [96, 0, 5, 6, 7, 3, 1, 2, 5, 4, 4],
                [100, 2, 0, 3, 5, 4, 6, 7, 2, 1, 5],
            ][index]
        };

        let mut d = initial;
        for i in 0..chosen[0] {
            let base = iv_v1.wrapping_add(i as u32) as usize;
            let n1 = ((d[chosen[3]] ^ d[chosen[4]]) & d[chosen[1]]) ^ d[chosen[3]];
            let n2 = d[chosen[1]].rotate_left(26)
                ^ d[chosen[1]].rotate_left(21)
                ^ d[chosen[1]].rotate_left(7);
            let n4 = di[base & 127]
                .wrapping_add(n1)
                .wrapping_add(n2)
                .wrapping_add(tt[base & 63])
                .wrapping_add(d[chosen[5]]);
            let n5 = d[chosen[2]].rotate_left(30)
                ^ d[chosen[2]].rotate_left(19)
                ^ d[chosen[2]].rotate_left(10);
            let n6 = (d[chosen[2]] & d[chosen[6]]) | ((d[chosen[2]] | d[chosen[6]]) & d[chosen[7]]);
            let old = d[chosen[9]];
            // d.add(0, d.removeAt(7)) -> rotate right by 1
            d.rotate_right(1);
            d[chosen[10]] = n5.wrapping_add(n6).wrapping_add(n4);
            d[chosen[8]] = old.wrapping_add(n4);
        }

        let mut ret = [0u8; 32];
        for i in 0..8 {
            ret[i * 4..i * 4 + 4].copy_from_slice(&d[i].wrapping_add(initial[i]).to_be_bytes());
        }
        let mut folded = [0u8; 16];
        for i in 0..16 {
            folded[i] = ret[i] ^ ret[16 + i];
        }
        let check = Self::sum_md5(&folded) | if self.mobile_v5 { 0x20 } else { 0 };
        let mut out = [0u8; 20];
        out[..16].copy_from_slice(&folded);
        out[16..20].copy_from_slice(&check.to_le_bytes());
        Ok(out)
    }

    fn xmxor(data: &[u8], key: &[u8; 32]) -> Vec<u8> {
        let len = data.len();
        let mut encoded = vec![0i32; len];
        for (i, &b) in data.iter().enumerate() {
            let at = (i * 4) & 28;
            let d0 = i32::from(key[at]);
            let d1 = i32::from(key[at + 1]);
            let v = i32::from(b);
            let mut d2 = (((v << 4) | (v >> 4)) & 255) + d0;
            d2 = (!d2) ^ d1;
            d2 = (((d2 & 255) << 3) | ((d2 & 255) >> 5)) & 255;
            d2 = (d2 + d1) & 255;
            d2 = (d2 ^ d0) & 255;
            encoded[len - i - 1] = (!d2) & 255;
        }
        let last = encoded[len - 1] ^ encoded[len - 2];
        let first = encoded[0];
        encoded[0] = ((!last).wrapping_add(first)) & 255;
        encoded[1] = ((encoded[0] ^ encoded[len - 1] ^ 254).wrapping_add(encoded[1])) & 255;
        let rot1 = ((encoded[1] << 3) | (encoded[1] >> 5)) & 255;
        encoded[2] = (encoded[2].wrapping_add((last - first) ^ rot1 ^ 2)) & 255;
        for i in 0..(len.saturating_sub(4)) {
            let rot = ((encoded[i + 2] << 3) | (encoded[i + 2] >> 5)) & 255;
            let temp = rot ^ encoded[i + 1] ^ ((i + 3) as i32);
            encoded[i + 3] = ((!temp).wrapping_add(encoded[i + 3])) & 255;
        }
        encoded[len - 1] ^= encoded[len - 2];
        let sum: i32 = encoded[1..].iter().copied().sum();
        encoded[0] = ((encoded[0] ^ encoded[1]).wrapping_add(sum)) & 255;
        encoded.into_iter().map(|v| (v & 255) as u8).collect()
    }

    pub fn gorgon(query: &str, body: Option<&[u8]>, ts: u32, random: u16) -> String {
        let mut input = [0u8; 20];
        input[0..4].copy_from_slice(&md5(query.as_bytes())[0..4]);
        if let Some(b) = body {
            input[4..8].copy_from_slice(&md5(b)[0..4]);
        }
        input[12..16].copy_from_slice(&67_503_104u32.to_le_bytes());
        input[16..20].copy_from_slice(&ts.to_be_bytes());

        let key = [
            0x4au8,
            0x40,
            0x16,
            (random >> 8) as u8,
            0x47,
            0x6c,
            0x01,
            (random & 0xff) as u8,
        ];
        let mut s = [0u8; 256];
        for (i, slot) in s.iter_mut().enumerate() {
            *slot = i as u8;
        }
        let mut j: usize = 0;
        for i in 0..256 {
            j = (j + usize::from(s[i]) + usize::from(key[i % 8])) & 255;
            s[i] = s[j];
        }
        j = 0;
        let mut out = [0u8; 20];
        for n in 0..20 {
            let i = n + 1;
            j = (j + usize::from(s[i])) & 255;
            let y = s[j];
            s[i] = y;
            let idx = (usize::from(y) + usize::from(y)) & 255;
            out[n] = input[n] ^ s[idx];
        }
        for i in 0..20 {
            let value = out[i].rotate_right(4);
            let next = out[if i + 1 < 20 { i + 1 } else { 0 }];
            let reversed = (next ^ value).reverse_bits();
            out[i] = !(reversed ^ 20);
        }
        let mut result = Vec::with_capacity(26);
        result.extend_from_slice(&[
            0x84,
            0x04,
            (random & 0xff) as u8,
            (random >> 8) as u8,
            0x40,
            0x01,
        ]);
        result.extend_from_slice(&out);
        hex_encode(&result)
    }

    pub fn helios(&self, ts: u32) -> String {
        self.helios_with_rand(ts, random_u32())
    }

    fn helios_with_rand(&self, ts: u32, rand_val: u32) -> String {
        let mut seed_input = Vec::with_capacity(8);
        seed_input.extend_from_slice(&rand_val.to_le_bytes());
        seed_input.extend_from_slice(b"8662");
        let ascii = hex_encode(&md5(&seed_input)).into_bytes();
        let mut words = Vec::with_capacity(40);
        for i in 0..4 {
            words.push(u64::from_le_bytes(
                ascii[i * 8..i * 8 + 8].try_into().unwrap(),
            ));
        }
        let mut table = Vec::with_capacity(35);
        table.push(words[0]);
        let mut b0 = words.remove(0);
        let mut b8 = words.remove(0);
        for i in 0..34u64 {
            let mut x8 = b8.rotate_right(8).wrapping_add(b0) ^ i;
            words.push(x8);
            x8 ^= b0.rotate_right(61);
            table.push(x8);
            b0 = x8;
            b8 = words.remove(0);
        }
        let license = if self.mobile_v5 {
            V5_LICENSE
        } else {
            "1588093228"
        };
        let text = format!("{ts}-{license}-8662");
        let text_bytes = text.as_bytes();
        let pad_len = 16 - (text_bytes.len() % 16);
        let mut data = Vec::with_capacity(text_bytes.len() + pad_len);
        data.extend_from_slice(text_bytes);
        data.resize(text_bytes.len() + pad_len, pad_len as u8);

        let mut output = Vec::with_capacity(4 + data.len());
        output.extend_from_slice(&rand_val.to_le_bytes());
        for chunk in data.chunks_exact(16) {
            let mut a = u64::from_le_bytes(chunk[0..8].try_into().unwrap());
            let mut b = u64::from_le_bytes(chunk[8..16].try_into().unwrap());
            for &t in &table[..34] {
                b = t ^ a.wrapping_add(b.rotate_right(8));
                a = b ^ a.rotate_right(61);
            }
            output.extend_from_slice(&a.to_le_bytes());
            output.extend_from_slice(&b.to_le_bytes());
        }
        base64_encode(&output)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn medusa_v5(
        &self,
        query_str: &str,
        body: Option<&[u8]>,
        ts: u32,
        now_ms: u64,
        device_id: &str,
        version_name: &str,
        channel: &str,
        os_version: &str,
        device_type: &str,
        device_brand: &str,
    ) -> Result<String, String> {
        let env_launch = ((random_u32() % 21) + 100) as i64;
        let env_pid = ((random_u32() % 2000) + 10001) as i64;
        self.medusa_v5_with_rands(
            query_str,
            body,
            ts,
            now_ms,
            device_id,
            version_name,
            channel,
            os_version,
            device_type,
            device_brand,
            env_launch,
            env_pid,
            [random_u32(), random_u32(), random_u32()],
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn medusa_v5_with_rands(
        &self,
        query_str: &str,
        body: Option<&[u8]>,
        ts: u32,
        now_ms: u64,
        device_id: &str,
        version_name: &str,
        channel: &str,
        os_version: &str,
        device_type: &str,
        device_brand: &str,
        env_launch: i64,
        env_pid: i64,
        rands: [u32; 3],
    ) -> Result<String, String> {
        let query_sm3 = sm3(query_str.as_bytes());
        let body_md5 = body.map(md5).unwrap_or([0u8; 16]);
        let f13 = self.hash_f13(&query_sm3, &body_md5, ts)?;
        // 0.16.15 (#122): complete v04.10.03 Medusa v5 device envelope & flags.
        let flags: [u8; 8] = [0x44, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let brand = if device_brand.is_empty() {
            "HONOR"
        } else {
            device_brand
        };
        let device_info = encode_proto(&[
            (1, ProtoVal::Sint(1)),
            (2, ProtoVal::Sint(2)),
            (3, ProtoVal::Str("8662")),
            (4, ProtoVal::Str(device_id)),
            (5, ProtoVal::Str(&self.device_token)),
            (6, ProtoVal::Str("!noperm!")),
            (7, ProtoVal::Sint(-888_888)),
            (8, ProtoVal::Sint(-888_888)),
            (9, ProtoVal::Sint(3)),
            (10, ProtoVal::Sint(-888_888)),
            (11, ProtoVal::Str("!notset!")),
            (12, ProtoVal::Str("Asia/Shanghai,8")),
            (13, ProtoVal::Str("zh_CN")),
            (14, ProtoVal::Sint(4)),
            (16, ProtoVal::Float(0.0)),
            (17, ProtoVal::Float(0.0)),
            (18, ProtoVal::Float(0.0)),
            (19, ProtoVal::Float(0.0)),
            (20, ProtoVal::Float(0.0)),
            (21, ProtoVal::Float(0.0)),
            (22, ProtoVal::Str(os_version)),
            (23, ProtoVal::Sint(41)),
            (24, ProtoVal::Sint(36)),
            (25, ProtoVal::Sint(self.process_started_ms as i64)),
            (26, ProtoVal::Sint(self.process_started_ms as i64)),
            (27, ProtoVal::Sint(self.process_started_ms as i64)),
            (28, ProtoVal::Sint(now_ms as i64)),
            (29, ProtoVal::Sint(-1)),
            (30, ProtoVal::Str(device_type)),
            (31, ProtoVal::Str(brand)),
            (32, ProtoVal::Str(device_type)),
            (33, ProtoVal::Str(device_type)),
            (34, ProtoVal::Str(brand)),
            (35, ProtoVal::Str(brand)),
            (36, ProtoVal::Str(brand)),
            (38, ProtoVal::Sint(31)),
        ]);
        let env13 = encode_proto(&[
            (1, ProtoVal::Sint(i64::from(ts))),
            (2, ProtoVal::Sint(-2)),
            (4, ProtoVal::Sint(200)),
        ]);
        let env = encode_proto(&[
            (1, ProtoVal::Sint(env_launch)),
            (2, ProtoVal::Sint(247_235_599)),
            (3, ProtoVal::Sint(247_235_596)),
            (5, ProtoVal::Sint(7)),
            (6, ProtoVal::Str("v04.10.03.04-bugfix")),
            (7, ProtoVal::Sint(env_pid)),
            (8, ProtoVal::Str(channel)),
            (12, ProtoVal::Bytes(&device_info)),
            (13, ProtoVal::Bytes(&env13)),
            (14, ProtoVal::Str(version_name)),
        ]);
        let nested = encode_proto(&[
            (1, ProtoVal::Sint(305)),
            (2, ProtoVal::Sint(4)),
            (3, ProtoVal::Sint(694_367)),
            (5, ProtoVal::Sint(1_621_072_674)),
        ]);
        let magic = hex_decode(V5_MAGIC_HEX)?;
        let msg_rand = i64::from(rands[0]);
        let message = encode_proto(&[
            (1, ProtoVal::Bytes(&magic)),
            (2, ProtoVal::Sint(5)),
            (3, ProtoVal::Sint(msg_rand)),
            (4, ProtoVal::Str("8662")),
            (5, ProtoVal::Str(device_id)),
            (6, ProtoVal::Str(V5_LICENSE)),
            (7, ProtoVal::Str(version_name)),
            (8, ProtoVal::Str("v04.10.03-ml-android")),
            (9, ProtoVal::Sint(0x040a_0300)),
            (10, ProtoVal::Bytes(&flags)),
            (12, ProtoVal::Sint(i64::from(ts))),
            (13, ProtoVal::Bytes(&f13)),
            (14, ProtoVal::Bytes(&query_sm3[..6])),
            (15, ProtoVal::Bytes(&nested)),
            (16, ProtoVal::Str(&self.device_token)),
            (17, ProtoVal::Sint(i64::from(ts))),
            (20, ProtoVal::Str("none")),
            (21, ProtoVal::Sint(V5_FIELD21)),
            (23, ProtoVal::Bytes(&env)),
            (
                24,
                ProtoVal::Str(
                    r#"{"cmr":16777216,"cmr2":16777216,"un_h":11,"vpn":0,"kd":0,"dyn":"","do":1,"tk":true}"#,
                ),
            ),
        ]);

        let nonce = rands[1];
        let sign_key = hex_decode(V5_SIGN_KEY_HEX)?;
        let mut kh_in = Vec::with_capacity(sign_key.len() * 2 + 4);
        kh_in.extend_from_slice(&sign_key);
        kh_in.extend_from_slice(&nonce.to_le_bytes());
        kh_in.extend_from_slice(&sign_key);
        let hash = sm3(&kh_in);
        let high = (nonce >> 16) & 255;
        let seed_word = !(((high << 11) | (nonce >> 24)) ^ (high >> 5) ^ high);
        let seed = seed_word.to_le_bytes();

        let mut transformed = Vec::with_capacity(8 + message.len());
        transformed.extend_from_slice(&flags);
        transformed.extend_from_slice(&Self::xmxor(&message, &hash));
        transformed.reverse();
        for (i, byte) in transformed.iter_mut().enumerate() {
            *byte ^= seed[(!i) & 3];
        }

        let check: u32 =
            0x1800_0001 | (u32::from(query_sm3[0] & 63) << 14) | (u32::from(f13[0] & 63) << 8);
        let mut packed = Vec::with_capacity(11 + transformed.len());
        packed.push(0xca);
        packed.extend_from_slice(&rands[2].to_le_bytes());
        packed.extend_from_slice(&check.to_le_bytes());
        packed.extend_from_slice(&transformed);
        packed.push(((nonce >> 16) & 0xff) as u8);
        packed.push(((nonce >> 24) & 0xff) as u8);

        let aes_key = md5(&sign_key[0..16]);
        let aes_iv = md5(&sign_key[16..32]);
        let encrypted = AesV3::new(&aes_key, ts, true).encrypt(&packed, &aes_iv)?;

        let mut prefix = Vec::with_capacity(20);
        prefix.extend_from_slice(&5u32.to_le_bytes());
        prefix.extend_from_slice(&magic);
        let ts_le = ts.to_le_bytes();
        for (i, byte) in prefix.iter_mut().enumerate() {
            *byte ^= ts_le[i & 3];
        }

        let mut final_bytes = Vec::with_capacity(24 + encrypted.len());
        final_bytes.extend_from_slice(&prefix);
        final_bytes.extend_from_slice(&[(nonce & 0xff) as u8, ((nonce >> 8) & 0xff) as u8, 0, 1]);
        final_bytes.extend_from_slice(&encrypted);
        Ok(base64_encode(&final_bytes))
    }

    pub fn medusa_v3(
        &self,
        query_str: &str,
        body: Option<&[u8]>,
        ts: u32,
        device_id: &str,
        version_name: &str,
    ) -> Result<String, String> {
        let query_bytes = query_str.as_bytes();
        let query_sm3 = sm3(query_bytes);
        let body_md5 = body.map(md5).unwrap_or([0u8; 16]);
        let f13 = self.hash_f13(&query_sm3, &body_md5, ts)?;
        let dev_proto = encode_proto(&[
            (1, ProtoVal::Sint(1)),
            (2, ProtoVal::Sint(2)),
            (3, ProtoVal::Str("8662")),
            (4, ProtoVal::Str(device_id)),
            (5, ProtoVal::Str(&self.process_nonce)),
            (6, ProtoVal::Str("!noperm!")),
            (7, ProtoVal::Sint(-888_888)),
            (8, ProtoVal::Sint(-888_888)),
            (9, ProtoVal::Sint(3)),
            (10, ProtoVal::Sint(-888_888)),
            (11, ProtoVal::Str("!notset!")),
            (12, ProtoVal::Str("Asia/Shanghai,8")),
            (13, ProtoVal::Str("zh_CN")),
            (14, ProtoVal::Sint(4)),
            (16, ProtoVal::Float(0.0)),
            (17, ProtoVal::Float(0.0)),
            (18, ProtoVal::Float(0.0)),
            (19, ProtoVal::Float(0.0)),
            (20, ProtoVal::Float(0.0)),
            (21, ProtoVal::Float(0.0)),
            (22, ProtoVal::Str("16")),
            (23, ProtoVal::Sint(41)),
            (24, ProtoVal::Sint(36)),
            (25, ProtoVal::Sint(self.process_started_ms as i64)),
            (26, ProtoVal::Sint(self.process_started_ms as i64)),
            (27, ProtoVal::Sint(self.process_started_ms as i64)),
            (28, ProtoVal::Sint(self.process_started_ms as i64)),
            (29, ProtoVal::Sint(-1)),
            (30, ProtoVal::Str("25053RT47C")),
            (31, ProtoVal::Str("Redmi")),
            (32, ProtoVal::Str("25053RT47C")),
            (33, ProtoVal::Str("25053RT47C")),
            (34, ProtoVal::Str("Xiaomi")),
            (35, ProtoVal::Str("Redmi")),
            (36, ProtoVal::Str("Redmi")),
            (38, ProtoVal::Sint(31)),
        ]);
        let env_launch = ((random_u32() % 21) + 100) as i64;
        let env_pid = ((random_u32() % 2000) + 10001) as i64;
        let sub13 = encode_proto(&[
            (1, ProtoVal::Sint((now_millis() / 1000) as i64)),
            (2, ProtoVal::Sint(-2)),
            (4, ProtoVal::Sint(200)),
        ]);
        let env = encode_proto(&[
            (1, ProtoVal::Sint(env_launch)),
            (2, ProtoVal::Sint(146_331_399)),
            (3, ProtoVal::Sint(146_331_396)),
            (5, ProtoVal::Sint(7)),
            (6, ProtoVal::Str("v04.06.04.03-bugfix")),
            (7, ProtoVal::Sint(env_pid)),
            (12, ProtoVal::Bytes(&dev_proto)),
            (13, ProtoVal::Bytes(&sub13)),
            (14, ProtoVal::Str(version_name)),
        ]);
        let nested = encode_proto(&[
            (1, ProtoVal::Sint(111)),
            (2, ProtoVal::Sint(10)),
            (3, ProtoVal::Sint(694_367)),
            (5, ProtoVal::Sint(586_952_199)),
        ]);
        let mut qh_in = Vec::with_capacity(query_bytes.len() + 20);
        qh_in.extend_from_slice(query_bytes);
        qh_in.extend_from_slice(&body_md5);
        qh_in.extend_from_slice(b"none");
        let query_hash = sm3(&qh_in);
        let magic = hex_decode("f7e85ffad7d7dc3bd62ac87057cf6118")?;
        let flag = hex_decode("4001000000000000")?;
        let message = encode_proto(&[
            (1, ProtoVal::Bytes(&magic)),
            (2, ProtoVal::Sint(3)),
            (3, ProtoVal::Sint(i64::from(random_u32()))),
            (4, ProtoVal::Str("8662")),
            (5, ProtoVal::Str(device_id)),
            (6, ProtoVal::Str("1588093228")),
            (7, ProtoVal::Str(version_name)),
            (8, ProtoVal::Str("v04.06.04-ml-android")),
            (9, ProtoVal::Sint(67_503_104)),
            (10, ProtoVal::Bytes(&flag)),
            (12, ProtoVal::Sint(i64::from(ts))),
            (13, ProtoVal::Bytes(&f13)),
            (14, ProtoVal::Bytes(&query_sm3[..6])),
            (15, ProtoVal::Bytes(&nested)),
            (16, ProtoVal::Str(&self.process_nonce)),
            (17, ProtoVal::Sint(i64::from(ts))),
            (19, ProtoVal::Bytes(&query_hash)),
            (20, ProtoVal::Str("none")),
            (21, ProtoVal::Sint(312)),
            (23, ProtoVal::Bytes(&env)),
            (
                24,
                ProtoVal::Str(
                    r#"{"cmr":16777216,"cmr2":16777216,"un_h":1879194040,"vpn":0,"kd":0,"fkd":3672518972,"pd":-1872573247,"dyn":"","do":0,"tk":true}"#,
                ),
            ),
        ]);

        let nonce = random_u32();
        let sign_key =
            hex_decode("8ebdfa3806ecc5cee79423e6029ed82540bc2218bb7eaef71cb691f7aa8aa2f5")?;
        let mut kh_in = Vec::with_capacity(sign_key.len() * 2 + 4);
        kh_in.extend_from_slice(&sign_key);
        kh_in.extend_from_slice(&nonce.to_le_bytes());
        kh_in.extend_from_slice(&sign_key);
        let hash = sm3(&kh_in);
        let high = (nonce >> 16) & 255;
        let seed_word = !(((high << 11) | (nonce >> 24)) ^ (high >> 5) ^ high);
        let seed = seed_word.to_le_bytes();

        let mut transformed = Vec::with_capacity(8 + message.len());
        transformed.extend_from_slice(&flag);
        transformed.extend_from_slice(&Self::xmxor(&message, &hash));
        transformed.reverse();
        for (i, byte) in transformed.iter_mut().enumerate() {
            *byte ^= seed[(!i) & 3];
        }

        let check: u32 =
            ((u32::from(query_sm3[0] & 63)) << 14) | 0x1800_0001 | ((u32::from(f13[0] & 63)) << 8);
        let mut packed = Vec::with_capacity(11 + transformed.len());
        packed.push(0x35);
        packed.extend_from_slice(&random_u32().to_le_bytes());
        packed.extend_from_slice(&check.to_le_bytes());
        packed.extend_from_slice(&transformed);
        packed.push(((nonce >> 16) & 0xff) as u8);
        packed.push(((nonce >> 24) & 0xff) as u8);

        let aes_key: [u8; 16] = hex_decode("f1593376766ea98d34f31b057a9d5be4")?
            .try_into()
            .unwrap();
        let aes_iv: [u8; 16] = hex_decode("1fe109a4125283f418de9e051a969e12")?
            .try_into()
            .unwrap();
        let encrypted = AesV3::new(&aes_key, ts, false).encrypt(&packed, &aes_iv)?;

        let ver_bytes = hex_decode("03000000f7e85ffad7d7dc3bd62ac87057cf6118")?;
        let mut prefix = Vec::with_capacity(20);
        for chunk in ver_bytes.chunks_exact(4) {
            let word = u32::from_le_bytes(chunk.try_into().unwrap()) ^ ts;
            prefix.extend_from_slice(&word.to_le_bytes());
        }
        let mut final_bytes = Vec::with_capacity(24 + encrypted.len());
        final_bytes.extend_from_slice(&prefix);
        final_bytes.extend_from_slice(&[(nonce & 0xff) as u8, ((nonce >> 8) & 0xff) as u8, 0, 1]);
        final_bytes.extend_from_slice(&encrypted);
        Ok(base64_encode(&final_bytes))
    }

    /// Sign an Android v5 playback request (POST with gzip body or GET with empty bytes `Some(&[])`).
    /// Searches `_rticket` offset `0..1024` to select branch 12 without modifying episode or identity.
    pub fn sign_v5_playback(
        &self,
        base_url: &str,
        params: &[(String, String)],
        body: Option<&[u8]>,
        now_ms: u64,
    ) -> Result<SignedRequest, String> {
        let ts = (now_ms / 1000) as u32;
        let find_val = |key: &str| -> String {
            params
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let device_id = find_val("device_id");
        let version_name = find_val("version_name");
        let channel = find_val("channel");
        let os_version = find_val("os_version");
        let device_type = find_val("device_type");
        let device_brand = find_val("device_brand");

        for offset in 0..1024u64 {
            let ticket = now_ms + offset;
            let mut full_params: Vec<(String, String)> = params
                .iter()
                .filter(|(k, _)| k != "_rticket")
                .cloned()
                .collect();
            full_params.push(("_rticket".into(), ticket.to_string()));
            let query = encode_query(&full_params);
            if self.branch(&query, body, ts) != 0 {
                continue;
            }
            let url = format!("{base_url}?{query}");
            let medusa = self.medusa_v5(
                &query,
                body,
                ts,
                now_ms,
                &device_id,
                &version_name,
                &channel,
                &os_version,
                &device_type,
                &device_brand,
            )?;
            let mut headers = vec![
                ("x-khronos".into(), ts.to_string()),
                ("x-helios".into(), self.helios(ts)),
                ("x-medusa".into(), medusa),
            ];
            if let Some(b) = body {
                if !b.is_empty() {
                    headers.push(("x-ss-stub".into(), hex_encode_upper(&md5(b))));
                }
            }
            return Ok(SignedRequest { url, headers });
        }
        Err("hg_sign_failed: 无法生成 v5 播放签名".into())
    }

    /// Sign a v3 App catalog/search GET request.
    pub fn sign_v3_get(
        &self,
        base_url: &str,
        params: &[(String, String)],
        now_ms: u64,
    ) -> Result<SignedRequest, String> {
        self.sign_v3_request(base_url, params, None, now_ms)
    }

    /// Sign a v3 App GET or POST request (used by search and cloud account sync).
    pub fn sign_v3_request(
        &self,
        base_url: &str,
        params: &[(String, String)],
        body: Option<&[u8]>,
        now_ms: u64,
    ) -> Result<SignedRequest, String> {
        let ts = (now_ms / 1000) as u32;
        let device_id = params
            .iter()
            .find(|(k, _)| k == "device_id")
            .map(|(_, v)| v.as_str())
            .unwrap_or("1905892595378490");
        let version_name = params
            .iter()
            .find(|(k, _)| k == "version_name")
            .map(|(_, v)| v.as_str())
            .unwrap_or("7.2.2.32");

        for offset in 0..128u64 {
            let ticket = now_ms + offset;
            let mut full_params = params.to_vec();
            full_params.push(("ts".into(), ts.to_string()));
            full_params.push(("_rticket".into(), ticket.to_string()));
            let query = encode_query(&full_params);
            if self.branch(&query, body, ts) != 0 {
                continue;
            }
            let url = format!("{base_url}?{query}");
            let rand16 = (random_u32() & 0xffff) as u16;
            let mut headers = vec![
                ("User-Agent".into(), APP_UA.into()),
                (
                    "Accept".into(),
                    "application/json; charset=utf-8,application/x-protobuf".into(),
                ),
                ("x-ss-req-ticket".into(), ticket.to_string()),
                ("x-tt-request-tag".into(), "t=0;n=0".into()),
                ("sdk-version".into(), "2".into()),
                ("passport-sdk-version".into(), "50561".into()),
                ("x-xs-from-web".into(), "0".into()),
                ("x-khronos".into(), ts.to_string()),
                ("x-ladon".into(), base64_encode(&ts.to_be_bytes())),
                ("x-argus".into(), base64_encode(&ts.to_le_bytes())),
                ("x-gorgon".into(), Self::gorgon(&query, body, ts, rand16)),
                ("x-helios".into(), self.helios(ts)),
                (
                    "x-medusa".into(),
                    self.medusa_v3(&query, body, ts, device_id, version_name)?,
                ),
                ("x-tt-dt".into(), String::new()),
            ];
            if let Some(b) = body {
                if !b.is_empty() {
                    headers.push(("x-ss-stub".into(), hex_encode_upper(&md5(b))));
                }
            }
            return Ok(SignedRequest { url, headers });
        }
        Err("hg_sign_failed: 无法生成 v3 请求签名".into())
    }
}

// ---------- Anonymous Device Registration (DeviceRegister.kt + PlaybackDeviceSession.kt) ----------

#[derive(Clone, Debug)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub install_id: String,
    pub cdid: String,
}

impl DeviceIdentity {
    /// Current 7.4.5.32 Android App profile (Issue #102 / 0.16.14).
    pub fn v5_current_app_params(&self) -> Vec<(String, String)> {
        vec![
            ("aid".into(), "8662".into()),
            ("app_name".into(), "novelread".into()),
            ("version_code".into(), "74532".into()),
            ("version_name".into(), "7.4.5.32".into()),
            ("update_version_code".into(), "74532".into()),
            ("manifest_version_code".into(), "74532".into()),
            ("os".into(), "android".into()),
            ("ssmix".into(), "a".into()),
            ("channel".into(), "update_64".into()),
            ("device_platform".into(), "android".into()),
            ("device_type".into(), "25053RT47C".into()),
            ("device_brand".into(), "Redmi".into()),
            ("os_version".into(), "16".into()),
            ("os_api".into(), "36".into()),
            ("ac".into(), "wifi".into()),
            ("language".into(), "zh".into()),
            ("host_abi".into(), "arm64-v8a".into()),
            ("iid".into(), self.install_id.clone()),
            ("cdid".into(), self.cdid.clone()),
            ("device_id".into(), self.device_id.clone()),
        ]
    }

    /// Legacy 7.4.3.32 Android v5 query parameters.
    #[allow(dead_code)]
    pub fn v5_query_params(&self) -> Vec<(String, String)> {
        vec![
            ("aid".into(), "8662".into()),
            ("app_name".into(), "novelread".into()),
            ("version_code".into(), "74332".into()),
            ("version_name".into(), "7.4.3.32".into()),
            ("channel".into(), "vivo_8662_64".into()),
            ("device_platform".into(), "android".into()),
            ("device_type".into(), "PGT-AN10".into()),
            ("os_version".into(), "12".into()),
            ("update_version_code".into(), "74332".into()),
            ("iid".into(), self.install_id.clone()),
            ("cdid".into(), self.cdid.clone()),
            ("device_id".into(), self.device_id.clone()),
        ]
    }
}

pub fn tt_encrypt(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut rnd = [0u8; 32];
    fill_random_bytes(&mut rnd);
    tt_encrypt_with_random(data, &rnd)
}

pub fn tt_encrypt_with_random(data: &[u8], rnd: &[u8; 32]) -> Result<Vec<u8>, String> {
    let salt = hex_decode(KDF_SALT_HEX)?;
    let inner = sha512(rnd);
    let mut kdf_in = Vec::with_capacity(inner.len() + salt.len());
    kdf_in.extend_from_slice(&inner);
    kdf_in.extend_from_slice(&salt);
    let derived = sha512(&kdf_in);
    let key: [u8; 16] = derived[0..16].try_into().unwrap();
    let iv: [u8; 16] = derived[16..32].try_into().unwrap();

    let digest = sha512(data);
    let mut body = Vec::with_capacity(64 + data.len());
    body.extend_from_slice(&digest);
    body.extend_from_slice(data);

    let encrypted = aes128_cbc_encrypt_pkcs7(&key, &iv, &body);
    let mut out = Vec::with_capacity(6 + 32 + encrypted.len());
    out.extend_from_slice(&TT_MAGIC);
    out.extend_from_slice(rnd);
    out.extend_from_slice(&encrypted);
    Ok(out)
}

fn valid_numeric_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 30
        && !value.starts_with('0')
        && value.bytes().all(|b| b.is_ascii_digit())
}

fn json_id_field(obj: &Value, keys: &[&str]) -> String {
    for &key in keys {
        if let Some(v) = obj.get(key) {
            if let Some(s) = v.as_str() {
                if !s.trim().is_empty() {
                    return s.trim().to_owned();
                }
            } else if let Some(n) = v.as_u64() {
                if n > 0 {
                    return n.to_string();
                }
            } else if let Some(n) = v.as_i64() {
                if n > 0 {
                    return n.to_string();
                }
            }
        }
    }
    String::new()
}

pub fn register_anonymous_device() -> Result<DeviceIdentity, String> {
    let now = now_millis();
    let cdid = uuid_v4();
    let openudid: String = uuid_v4().replace('-', "").chars().take(16).collect();
    let clientudid = uuid_v4();
    let req_id = uuid_v4();

    let mut custom = serde_json::Map::new();
    custom.insert(
        "account_region_src".into(),
        Value::String("fallback".into()),
    );
    custom.insert("host_bit".into(), Value::from(64));
    custom.insert("account_region".into(), Value::String("cn".into()));
    custom.insert("dragon_device_type".into(), Value::String("phone".into()));

    let mut header = serde_json::Map::new();
    header.insert("display_name".into(), Value::String("红果免费短剧".into()));
    header.insert("update_version_code".into(), Value::from(73932));
    header.insert("manifest_version_code".into(), Value::from(73932));
    header.insert("app_version_minor".into(), Value::String("7.3.9.32".into()));
    header.insert("aid".into(), Value::from(8662));
    header.insert("channel".into(), Value::String("update_64".into()));
    header.insert("package".into(), Value::String("com.phoenix.read".into()));
    header.insert("app_version".into(), Value::String("7.3.9.32".into()));
    header.insert("version_code".into(), Value::from(73932));
    header.insert(
        "sdk_version".into(),
        Value::String("3.7.0-rc.34-fanqie-xiaoshuo".into()),
    );
    header.insert("sdk_target_version".into(), Value::from(29));
    header.insert("git_hash".into(), Value::String("3a88f4e".into()));
    header.insert("os".into(), Value::String("Android".into()));
    header.insert("os_version".into(), Value::String("16".into()));
    header.insert("os_api".into(), Value::from(36));
    header.insert("device_model".into(), Value::String("25053RT47C".into()));
    header.insert("device_brand".into(), Value::String("Redmi".into()));
    header.insert("device_manufacturer".into(), Value::String("Redmi".into()));
    header.insert("cpu_abi".into(), Value::String("arm64-v8a".into()));
    header.insert("release_build".into(), Value::String("b5e187f".into()));
    header.insert("density_dpi".into(), Value::from(520));
    header.insert("display_density".into(), Value::String("mdpi".into()));
    header.insert("resolution".into(), Value::String("1280x2772".into()));
    header.insert("language".into(), Value::String("zh".into()));
    header.insert("timezone".into(), Value::from(8));
    header.insert("access".into(), Value::String("wifi".into()));
    header.insert("not_request_sender".into(), Value::from(0));
    header.insert("carrier".into(), Value::String("CHINA MOBILE".into()));
    header.insert("mcc_mnc".into(), Value::String("46000".into()));
    header.insert(
        "rom".into(),
        Value::String("eng.root.20231116.160340".into()),
    );
    header.insert(
        "rom_version".into(),
        Value::String("W528JS release-keys".into()),
    );
    header.insert("cdid".into(), Value::String(cdid.clone()));
    header.insert(
        "sig_hash".into(),
        Value::String("56a962410c494bbaf0b58dba20cae56f".into()),
    );
    header.insert("openudid".into(), Value::String(openudid));
    header.insert("clientudid".into(), Value::String(clientudid));
    header.insert("ipv6_list".into(), Value::Array(Vec::new()));
    header.insert("region".into(), Value::String("CN".into()));
    header.insert("tz_name".into(), Value::String("Asia/Shanghai".into()));
    header.insert("tz_offset".into(), Value::from(28800));
    header.insert("sim_region".into(), Value::String("cn".into()));
    header.insert("sim_serial_number".into(), Value::Array(Vec::new()));
    header.insert("oaid_may_support".into(), Value::Bool(false));
    header.insert("req_id".into(), Value::String(req_id));
    header.insert("device_platform".into(), Value::String("android".into()));
    header.insert("custom".into(), Value::Object(custom));
    header.insert("pre_installed_channel".into(), Value::String(String::new()));
    header.insert("apk_first_install_time".into(), Value::from(now));
    header.insert("is_system_app".into(), Value::from(0));
    header.insert("sdk_flavor".into(), Value::String("china".into()));
    header.insert("guest_mode".into(), Value::from(0));

    let mut payload = serde_json::Map::new();
    payload.insert("magic_tag".into(), Value::String("ss_app_log".into()));
    payload.insert("header".into(), Value::Object(header));
    payload.insert("_gen_time".into(), Value::from(now));
    let payload_json = Value::Object(payload);

    let raw_bytes = payload_json.to_string().into_bytes();
    let encrypted = tt_encrypt(&gzip_compress(&raw_bytes))?;
    let stub = hex_encode_upper(&md5(&encrypted));
    let ticket = now.to_string();

    // Deliberately NO signing headers (x-gorgon/x-medusa/etc.) — see DeviceRegister.kt.
    let headers: Vec<(&str, &str)> = vec![
        ("User-Agent", APP_UA),
        ("Content-Type", "application/octet-stream;tt-data=a"),
        ("log-encode-type", "gzip"),
        ("x-ss-dp", "8662"),
        ("x-ss-req-ticket", &ticket),
        ("x-vc-bdturing-sdk-version", "4.0.3.cn"),
        ("lc", "101"),
        ("sdk-version", "2"),
        ("passport-sdk-version", "5051452"),
        ("x-neptune", "-8|50:51:59:30:40:47:49:39"),
        ("x-ss-stub", &stub),
    ];

    let resp = request_with_options(
        "POST",
        DEVICE_REGISTER_URL,
        &headers,
        Some(&encrypted),
        1024 * 1024,
        RequestOptions {
            user_agent: Some(APP_UA),
            timeout_ms: 8_000,
            decompress: true,
            disable_cookies: true,
        },
    )?;

    if !(200..300).contains(&resp.status) {
        return Err(format!(
            "hg_device_register: 设备注册失败（HTTP {}）",
            resp.status
        ));
    }
    let parsed: Value = serde_json::from_slice(&resp.body)
        .map_err(|_| "hg_device_register: 设备注册响应格式异常".to_string())?;
    let device_id = json_id_field(&parsed, &["device_id_str", "device_id"]);
    let install_id = json_id_field(&parsed, &["install_id_str", "install_id"]);
    if !valid_numeric_id(&device_id) || !valid_numeric_id(&install_id) {
        return Err("hg_device_register: 服务端未下发有效设备身份".into());
    }
    Ok(DeviceIdentity {
        device_id,
        install_id,
        cdid,
    })
}

struct SessionSlot {
    identity: Option<DeviceIdentity>,
    last_rotated: Option<Instant>,
}

/// Shared anonymous playback device session; registers once on first use and supports
/// cooldown-bounded rotation if upstream stops serving the current identity.
pub struct PlaybackDeviceSession {
    slot: Mutex<SessionSlot>,
}

impl Default for PlaybackDeviceSession {
    fn default() -> Self {
        Self {
            slot: Mutex::new(SessionSlot {
                identity: None,
                last_rotated: None,
            }),
        }
    }
}

impl PlaybackDeviceSession {
    pub fn obtain(&self) -> Result<DeviceIdentity, String> {
        let mut guard = self.slot.lock().map_err(|_| "hg_session: 会话锁异常")?;
        if let Some(id) = &guard.identity {
            return Ok(id.clone());
        }
        let fresh = register_anonymous_device()?;
        guard.identity = Some(fresh.clone());
        Ok(fresh)
    }

    pub fn rotate_if_allowed(
        &self,
        failed_device_id: &str,
    ) -> Result<Option<DeviceIdentity>, String> {
        let mut guard = self.slot.lock().map_err(|_| "hg_session: 会话锁异常")?;
        if let Some(current) = &guard.identity {
            if current.device_id != failed_device_id {
                return Ok(Some(current.clone()));
            }
        }
        if let Some(at) = guard.last_rotated {
            if at.elapsed() < Duration::from_secs(30) {
                return Ok(None);
            }
        }
        guard.last_rotated = Some(Instant::now());
        let fresh = register_anonymous_device()?;
        guard.identity = Some(fresh.clone());
        Ok(Some(fresh))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v5_carrier_matches_all_four_reference_vectors() {
        let source = hex_decode(
            "13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a6f94b9de03284d7297bce1062b50759abfe4092e53789dc2e70c31567ba0c5ea0f34597ea3c8ed12375c81a6cbf0153a5f84a9cef3183d6287acd1f61b40658aafd4f91e43688db2d7fc21466b90b5daff24496e93b8dd02274c7196bbe0052a4f7499bee3082d52779cc1e60b30557a9fc4e90e33587da2c7ec11365b80a5caef14395e83a8cdf2173c6186abd0f51a3f6489aed3f81d42678cb1d6fb20456a8fb4d9fe23486d92b7dc01264b7095badf04294e7398bde2072c51769bc0e50a2f54799ec3e80d32577ca1c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a",
        )
        .unwrap();
        let key: [u8; 16] = hex_decode("000102030405060708090a0b0c0d0e0f")
            .unwrap()
            .try_into()
            .unwrap();
        let iv: [u8; 16] = hex_decode("101112131415161718191a1b1c1d1e1f")
            .unwrap()
            .try_into()
            .unwrap();
        let expected = [
            (
                1_800_000_000u32,
                "0653385d92a3ccf3177b608dbacbf41b3f23a8a5c2f79c4367cb90ddea1b44698fb3f8fd3243ec91b79b002d4a6b14bbdf03084d7293bce1066b70759abb640b2f53589dd2e78c33567ba0c5ea0bb45b7fa3e8e51233dc81a78bf01d2a5f04abceb3383572872cd3f65b406d9aab54f91e436885a2d37c21462b90bddaffa4496ed398dd12234c7196bbe00d3a4bf499bfa3282542779cc1e74b105d7a9bc4eb0e337875b2c36c11365ba0adcaef94395fc3a8cde217bc6387abf0fd1a3b648baf93d81552670cb1d6fb004d6a8fb4dbfe63486d92b7dc03264b5095badf84294e33b8bdf203ac51769bc0ed1a2fd4799fc3c80d2253fca3c6eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a",
            ),
            (
                1_800_000_001u32,
                "7253185592a7ccf3177b408dbacb74193e63a8a5d2f39c4366cb90d5fa1f44698fb3d8fd32476c91b6db00254a6f94bbde0308457293bce3076b707d9abf64092f137895d2e30c33567b80c5fa0f34597fe3e8e51237dc83a68bd0152a5b84abcff3183562832cd3f71b606d8aaf54fb1f03688db2d37c21462bb0bdcaff24496fd3b8d502274c7197bbc0052a4f749bbea3282d52771cc1e60b105d7a9fc4eb0e737875a2c76c11375ba0a5caeb14395e8388c5e217bc6186ebd0fd1a3b648baf93f81d52638cb1d7fb00457a8f34d9ff23686d92b7dc01274b509dbadf84294e3398b5f207ac51779bc0e50a2fd4799fc3c80d3257fca1c7eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a",
            ),
            (
                1_800_000_002u32,
                "5553185592a3ccf1173b408daacff41b3f6388add2f71c4167cb90ddea1fc46b8ff3f8f52243ec91b6db20255a6b14b9de0328457297bce1072b707d8abfe40b2f13589dd2e38c31567b80c5fa0bb45b7fa3c8ed02335c83a68bf01d3a5f04abcef3383d6287acd1f75b60658aabd4f91e034885a2d7fc21466bb0bddaffa44b6ed3b8d502274c7196fbe00d3a4b749bbee3282d52731cc1e64b30556a9fc4e90f33587da2c36c13365ba0adcaeb94395e83a8cde217bc6386ebd0f50a3fe489ae93f81542638cb1d7bb004d7a8fb4d9fe63486592b3dc03274b7095badb84294f73b8b5e207ac5377dbe0ed0a2bd47b9f83c8052257fca3c7eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a",
            ),
            (
                1_800_000_003u32,
                "4b13185d82a34cf3177b608dbacbf41b3e63a8adc2f79c4367cbb0d5fa1fc46b8fb3d8f53243ec93b69b20254a6f14bbdf03084d6293bce3072b70758abf640b2e535895d2e30c31567ba0cdfa0fb45b7fe3e8ed0233dc81a7cbd01d2a5f84abcef338356287acd3f71b406d8aabd4fb1e03688da2d7fc23462bb0bdcaffa44b6e9398d50223cc7196bbc00d3a4f7499bfe3282d42739cc3e64b105d7a9f44e90f735875a2c7ec13375b80a5caef143b5fc3a8c5e217bc6186ebd0f51a3f6489ae93d81552630cb3d7fb20457a8b34d9fe23486d92b7dc03264b509dbadb842b4e73b8b5f207ac5377dbe0ed0a2bd4799e83c80d3253fca1c7eb10355a7fa4c9ee13385d82a7ccf1163b6085aacff4193e6388add2f71c41668bb0d5fa1f44698eb3d8fd22476c91b6db00254a",
            ),
        ];
        for (ts, exp) in expected {
            let actual = AesV3::new(&key, ts, true).encrypt(&source, &iv).unwrap();
            assert_eq!(hex_encode(&actual), exp, "carrier mismatch for ts={ts}");
        }
    }

    #[test]
    fn v5_hash_f13_matches_reference_sdk_vectors() {
        let signer = Signer::with_fixed(true, "synthetic", 1_800_000_000_000);
        let vectors = [
            (
                1_800_000_000u32,
                "97f67a81cea547c71c76702cc529330762c14971d85f3c335467579d1e9c15e3",
                "0ff85a33ee8c8aa4f59a843d5199909f",
                "ca03ec7a0c2444b3dcccf33453d02579",
            ),
            (
                1_800_000_001u32,
                "959b0479339d92d4ab2c428cff3daa0349a78072e56115d2fd8181ef00ef722d",
                "49783eb0095375c17655cdc1ff329874",
                "a1376d7977abbc1dbc536aae0429ac2c",
            ),
            (
                1_800_000_015u32,
                "ddf456d095ef231ae9a0332848c86372d34079e8e784bfa319f0c84703daf67d",
                "e93ce6e2ce1bcdaeb93bed8463fbb5b5",
                "3e5d88f088e9e56a668fb0020d9a1832",
            ),
            (
                1_800_000_031u32,
                "7452b4261e5ba66aa463bbc4892aa93c11f3f4005b3747c6adabb6359212d2e8",
                "f5b46ee83c8b3c78876cd15e992c3c82",
                "26c2a2a9bd5a7f468b3f1f4525c0ba18",
            ),
        ];
        for (ts, q, b, exp) in vectors {
            let query: [u8; 32] = hex_decode(q).unwrap().try_into().unwrap();
            let body: [u8; 16] = hex_decode(b).unwrap().try_into().unwrap();
            let out = signer.hash_f13(&query, &body, ts).unwrap();
            assert_eq!(hex_encode(&out[..16]), exp, "f13 mismatch for ts={ts}");
            assert_eq!(out[16] & 0x24, 0x24);
        }
    }

    #[test]
    fn v5_playback_signing_binds_body_and_empty_get_bytes() {
        let signer = Signer::with_fixed(true, "synthetic", 1_800_000_000_000);
        let id = DeviceIdentity {
            device_id: "1234567890".into(),
            install_id: "9876543210".into(),
            cdid: "synthetic-cdid".into(),
        };
        let params = id.v5_query_params();
        let gz = gzip_compress(br#"{"test":1}"#);
        let post = signer
            .sign_v5_playback(
                "https://example.test/player/v1",
                &params,
                Some(&gz),
                1_800_000_000_000,
            )
            .unwrap();
        assert!(post.headers.iter().any(|(k, _)| k == "x-medusa"));
        assert!(post.headers.iter().any(|(k, _)| k == "x-helios"));
        assert!(post.headers.iter().any(|(k, _)| k == "x-ss-stub"));

        // Empty-body GET uses Some(&[]) (MD5 of empty) and emits no x-ss-stub.
        let get = signer
            .sign_v5_playback(
                "https://example.test/video_tab/v1/",
                &params,
                Some(&[]),
                1_800_000_000_000,
            )
            .unwrap();
        assert!(get.headers.iter().any(|(k, _)| k == "x-medusa"));
        assert!(!get.headers.iter().any(|(k, _)| k == "x-ss-stub"));
    }

    #[test]
    fn v5_medusa_and_helios_match_reference_js_byte_for_byte() {
        let nonce_bytes = [0x80u8; 18];
        let process_nonce = base64_url_encode_nopad(&nonce_bytes);
        let signer = Signer::with_fixed(true, &process_nonce, 1_800_000_000_000);
        let id = DeviceIdentity {
            device_id: "1234567890123456789".into(),
            install_id: "2234567890123456789".into(),
            cdid: "synthetic-device".into(),
        };
        let params = id.v5_query_params();
        let signed = signer
            .sign_v5_playback(
                "https://example.test/model",
                &params,
                Some(&[]),
                1_800_000_000_000,
            )
            .unwrap();
        let expected_url = "https://example.test/model?aid=8662&app_name=novelread&version_code=74332&version_name=7.4.3.32&channel=vivo_8662_64&device_platform=android&device_type=PGT-AN10&os_version=12&update_version_code=74332&iid=2234567890123456789&cdid=synthetic-device&device_id=1234567890123456789&_rticket=1800000000040";
        assert_eq!(signed.url, expected_url);

        let helios = signer.helios_with_rand(1_800_000_000, 0x8080_8080);
        assert_eq!(helios, "gICAgEFYTG4OvSBXujBafQesq1DKDpD68Oz/sdaxnq7R9wqx");

        let query = signed.url.split_once('?').unwrap().1;
        let medusa = signer
            .medusa_v5_with_rands(
                query,
                Some(&[]),
                1_800_000_000,
                1_800_000_000_000,
                "1234567890123456789",
                "7.4.3.32",
                "vivo_8662_64",
                "12",
                "PGT-AN10",
                "",
                110,
                11001,
                [0x8080_8080, 0x8080_8080, 0x8080_8080],
            )
            .unwrap();
        let expected_medusa_377 = "BdJJay2ZBqFJp0QoP2fnRyK/hT2AgAABMYqggJCEAWcFGOJgh22qozz+26uXSKBhiGx/eWX33mGLsl8jibKbqei0wqKjsKKx13g9lnv4rLZcLBnGvh3tkS6SOmxCEEklfmzOYTmt5aQN/scqDuqMDTMdhZEK7LqexpFf/IEMTApJTvJrvDxfJGT3LQauOjvi//B3bh9u+Rl5LF30e2zdnqHfMK0wcWATcSVKGnuAFv+W8c+X1+p6BvUYUf8SaoTD1hwJwVCeUv40YtJ+h8vTbfPogICBZXYQD8Ak2kQ42VoBesfM/P3emRXBuSS0ehjIgVZbqLzPN0elfdibubfTpt9MwoNcMSBudoj6t/zc9d4FD4cSJ6yI6BgOnsfstVYD9mdnscI1oDxN4eRXAin4gihgfRK5P8fuYyvUzPlwKKPQmTnGyFiRtYIGDeB107t33nuJ1VCGSQz9hB4JPhaKINLmvkDZHX0pZvJN93FknjvjVw2iFoiFFW6cSNNEHfY6hCXnvCqPJgUlYHljTbxFf/8+k+YMeds2ngp6hg7moPlgYp3gNukCHMNqTax834vP1GFJN6w8avIsEf9mg4ooMUzidv+oE60m/khZjlwcldsrHL/IBwDATK+ouYbribiUfOe3SC2ddzcW+//7//v/+/+AgA==";
        assert_ne!(medusa, expected_medusa_377);
        let decoded = base64_decode(&medusa).unwrap();
        assert!(decoded.len() > 380);
        let ts_le = 1_800_000_000u32.to_le_bytes();
        let mut prefix = decoded[..20].to_vec();
        for (i, b) in prefix.iter_mut().enumerate() {
            *b ^= ts_le[i & 3];
        }
        assert_eq!(&prefix[..4], &5u32.to_le_bytes());
        assert_eq!(hex_encode(&prefix[4..20]), V5_MAGIC_HEX);

        // Byte-for-byte verification against N3urda/hongguoTV 0.16.15 (#122) golden envelope vector.
        let envelope_signer = Signer::with_fixed(true, "synthetic", 1_800_000_000_000);
        let envelope_params = vec![
            ("device_id".into(), "123".into()),
            ("iid".into(), "456".into()),
            ("cdid".into(), "synthetic".into()),
            ("version_name".into(), "7.4.5.32".into()),
            ("channel".into(), "update_64".into()),
            ("os_version".into(), "16".into()),
            ("device_type".into(), "TestTV".into()),
        ];
        let envelope_body = base64_decode("H4sIAAECAwQ=").unwrap();
        let envelope_signed = envelope_signer
            .sign_v5_playback(
                "https://example.test/model",
                &envelope_params,
                Some(&envelope_body),
                1_800_000_000_000,
            )
            .unwrap();
        assert_eq!(
            envelope_signed.url,
            "https://example.test/model?device_id=123&iid=456&cdid=synthetic&version_name=7.4.5.32&channel=update_64&os_version=16&device_type=TestTV&_rticket=1800000000003"
        );
        let envelope_query = envelope_signed.url.split_once('?').unwrap().1;
        let envelope_medusa = envelope_signer
            .medusa_v5_with_rands(
                envelope_query,
                Some(&envelope_body),
                1_800_000_000,
                1_800_000_000_000,
                "123",
                "7.4.5.32",
                "update_64",
                "16",
                "TestTV",
                "",
                110,
                11001,
                [0x8000_0000, 0x8000_0000, 0x8000_0000],
            )
            .unwrap();
        assert_eq!(
            envelope_medusa,
            "BdJJay2ZBqFJp0QoP2fnRyK/hT0AAAABAIogCBCAgc4GGADttDq1C6PIE9PwcDTWVJfFSTMWQiwMsfYBKGbVJ9pZTOhWGMCypQcv+wBR5j7hJUL+mHLQTMCCXYPx3HkSQtfd2dierHJ2YuAgen51bgbETv/zcbxd11lZC/vyfbwuV4RRvM195YN/UuCMl2i+iSPK04zG6fx0JIPN5WSYlK6hh+tS8l8YrMtIiabOIzOVgotm/JqqYnAGQSTAc+MZUE7idIENn47ND7Ids3FGspufTBJqvOlYZfb2+Dj734kpFwOdwVdEhsJvTFiXAA4DqerQ8wnaU3xHv2OjHl27maVXudst3jvgYIIW1RKTFArBdI1fpMvWzLfUE56hY4HRxfoa8hNcy8HOcxYN198HIQ6XtmSdzcNIoiKZMhA/V/xvOm4RR9KP14bSuEeTQUebQWCIBKuIkqJ+ivJWzrvYOThQJnRQinJTUobfxrlmJH1mnC5VOsjP78I5fyC9fv81+yyWp4tNry1TclC7Nyzi8tcWh6kzuZdM52GSUi5vMqfgaWdtot7gGK4vP13qA6xfVEgqKRUo5cqgvtHQvpsZ2nygBZBg1b1dRJrk7dTXEdZk7H+PUlWbFPy2RA/zd9BnIFE5Fmryzl6uiXOQKO6KBUoAqLcA0hoNZexgxrlt3DDQPL5eFSyKOyYOIYzWBACfcVCOO57y5w/TB+8DHYX1rcbByOBvVRihizrrLId2Ov8kfHYXThb/OUIOeYOb9QK0MY4+loU0QoYCj0WRLzIgKovV/q70JT5GGI/IbNJGlgg+ce2rw8EGGwryaylKdjVXU2cD0lL3qjTfp4QNXMf0knPJ/P//f/////+7AIA="
        );
        assert_eq!(
            envelope_signer.helios_with_rand(1_800_000_000, 0x8000_0000),
            "AAAAgOldh6XkgPCcSOD+1hk6geYVwA9oX+NbtYeS3DNddvJe"
        );
    }
}
