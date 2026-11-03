//! Minimal read-only parser for Android binary XML (AXML), enough to read the `<manifest>` element of an APK's
//! `AndroidManifest.xml`: package name, `versionName`, `versionCode` and `split` (set on split APKs).
//!
//! Layout (all little endian): a file chunk (`0x0003`) holding a string pool (`0x0001`), an optional resource map
//! (`0x0180`, one resource id per leading string) and XML node chunks. A start-element chunk (`0x0102`) carries
//! namespace/name string indices and attribute records `{ns, name, rawValue, size u16, res0 u8, type u8, data u32}`.
//! Attribute names are matched by resource id first (obfuscated manifests may have empty names), then by name.

/// Resource id of `android:versionCode`.
pub const ATTR_VERSION_CODE: u32 = 0x0101_021b;
/// Resource id of `android:versionName`.
pub const ATTR_VERSION_NAME: u32 = 0x0101_021c;

const NONE: u32 = 0xffff_ffff;
const T_XML: u16 = 0x0003;
const T_POOL: u16 = 0x0001;
const T_RESMAP: u16 = 0x0180;
const T_START_EL: u16 = 0x0102;
const FLAG_UTF8: u32 = 0x100;
const TYPE_STRING: u8 = 0x03;
const TYPE_INT_DEC: u8 = 0x10;
const TYPE_INT_HEX: u8 = 0x11;

/// Malformed binary XML.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid AndroidManifest.xml: {0}")]
pub struct AxmlError(pub String);

fn err<T>(msg: impl Into<String>) -> Result<T, AxmlError> {
    Err(AxmlError(msg.into()))
}

/// What the root `<manifest>` element says about the APK.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManifestInfo {
    /// `package` attribute.
    pub package: String,
    /// `android:versionName`, if present as a literal string.
    pub version_name: Option<String>,
    /// `android:versionCode`, if present as an integer.
    pub version_code: Option<u32>,
    /// `split` attribute: present on split APKs, absent on the base APK.
    pub split: Option<String>,
}

fn u16_at(b: &[u8], off: usize) -> Result<u16, AxmlError> {
    match b.get(off..off.wrapping_add(2)) {
        Some(s) => Ok(u16::from_le_bytes([s[0], s[1]])),
        None => err(format!("truncated at offset {off}")),
    }
}

fn u32_at(b: &[u8], off: usize) -> Result<u32, AxmlError> {
    match b.get(off..off.wrapping_add(4)) {
        Some(s) => Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]])),
        None => err(format!("truncated at offset {off}")),
    }
}

fn u8_at(b: &[u8], off: usize) -> Result<u8, AxmlError> {
    match b.get(off) {
        Some(v) => Ok(*v),
        None => err(format!("truncated at offset {off}")),
    }
}

/// Decoded string pool.
struct Pool(Vec<String>);

impl Pool {
    fn get(&self, idx: u32) -> Option<&str> {
        self.0.get(idx as usize).map(String::as_str)
    }
}

fn utf8_len(b: &[u8], p: &mut usize) -> Result<usize, AxmlError> {
    let b0 = u8_at(b, *p)? as usize;
    *p += 1;
    if b0 & 0x80 != 0 {
        let b1 = u8_at(b, *p)? as usize;
        *p += 1;
        return Ok(((b0 & 0x7f) << 8) | b1);
    }
    Ok(b0)
}

fn parse_pool(b: &[u8], off: usize) -> Result<Pool, AxmlError> {
    let header_size = u16_at(b, off + 2)? as usize;
    let count = u32_at(b, off + 8)? as usize;
    let flags = u32_at(b, off + 16)?;
    let str_start = u32_at(b, off + 20)? as usize;
    let utf8 = flags & FLAG_UTF8 != 0;
    let mut out = Vec::with_capacity(count.min(4096));
    for i in 0..count {
        let mut p = off + str_start + u32_at(b, off + header_size + i * 4)? as usize;
        let s = if utf8 {
            utf8_len(b, &mut p)?; // character count, unused
            let len = utf8_len(b, &mut p)?;
            match b.get(p..p + len) {
                Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                None => return err("string pool entry out of bounds"),
            }
        } else {
            let mut len = u16_at(b, p)? as usize;
            p += 2;
            if len & 0x8000 != 0 {
                len = ((len & 0x7fff) << 16) | u16_at(b, p)? as usize;
                p += 2;
            }
            let units = (0..len).map(|k| u16_at(b, p + k * 2)).collect::<Result<Vec<u16>, _>>()?;
            String::from_utf16_lossy(&units)
        };
        out.push(s);
    }
    Ok(Pool(out))
}

/// Parses the root element of a binary `AndroidManifest.xml`.
pub fn parse_manifest(b: &[u8]) -> Result<ManifestInfo, AxmlError> {
    if u16_at(b, 0)? != T_XML {
        return err("not a binary XML file");
    }
    let file_header = u16_at(b, 2)? as usize;
    let end = (u32_at(b, 4)? as usize).min(b.len());
    let mut pool: Option<Pool> = None;
    let mut resmap: Vec<u32> = Vec::new();
    let mut off = file_header;
    while off + 8 <= end {
        let ty = u16_at(b, off)?;
        let hsize = u16_at(b, off + 2)? as usize;
        let size = u32_at(b, off + 4)? as usize;
        if size < 8 || off + size > end {
            return err(format!("bad chunk size {size} at offset {off}"));
        }
        match ty {
            T_POOL if pool.is_none() => pool = Some(parse_pool(b, off)?),
            T_RESMAP => {
                resmap = (off + hsize..off + size).step_by(4).map(|p| u32_at(b, p)).collect::<Result<_, _>>()?
            }
            T_START_EL => {
                let Some(pool) = pool else { return err("element before string pool") };
                return read_manifest(b, off, &pool, &resmap);
            }
            _ => {}
        }
        off += size;
    }
    err("no root element")
}

fn read_manifest(b: &[u8], off: usize, pool: &Pool, resmap: &[u32]) -> Result<ManifestInfo, AxmlError> {
    let hsize = u16_at(b, off + 2)? as usize;
    let ext = off + hsize;
    let name = pool.get(u32_at(b, ext + 4)?).unwrap_or_default();
    if name != "manifest" {
        return err(format!("root element is <{name}>, not <manifest>"));
    }
    let attr_start = u16_at(b, ext + 8)? as usize;
    let attr_size = u16_at(b, ext + 10)? as usize;
    let attr_count = u16_at(b, ext + 12)? as usize;
    let mut info = ManifestInfo::default();
    let mut have_package = false;
    for i in 0..attr_count {
        let a = ext + attr_start + i * attr_size;
        let name_idx = u32_at(b, a + 4)?;
        let raw = u32_at(b, a + 8)?;
        let vtype = u8_at(b, a + 15)?;
        let data = u32_at(b, a + 16)?;
        let res_id = resmap.get(name_idx as usize).copied();
        let attr_name = pool.get(name_idx).unwrap_or_default();
        let string_value = || {
            let idx = if raw != NONE {
                raw
            } else if vtype == TYPE_STRING {
                data
            } else {
                NONE
            };
            pool.get(idx).map(str::to_owned)
        };
        match (res_id, attr_name) {
            (Some(ATTR_VERSION_CODE), _) | (None, "versionCode") => {
                if matches!(vtype, TYPE_INT_DEC | TYPE_INT_HEX) {
                    info.version_code = Some(data);
                } else {
                    info.version_code = string_value().and_then(|s| s.trim().parse().ok());
                }
            }
            (Some(ATTR_VERSION_NAME), _) | (None, "versionName") => info.version_name = string_value(),
            (None, "package") => {
                info.package = string_value().unwrap_or_default();
                have_package = true;
            }
            (None, "split") => info.split = string_value(),
            _ => {}
        }
    }
    if !have_package {
        return err("<manifest> has no package attribute");
    }
    Ok(info)
}

#[cfg(test)]
pub(crate) mod build {
    //! Synthesizes binary manifests for tests.

    use super::*;

    /// An attribute value for [`manifest`].
    #[derive(Clone)]
    pub enum Val {
        Str(&'static str),
        /// String typed value only (raw value index = NONE).
        TypedStr(&'static str),
        Int(u32),
        /// Reference typed value (unsupported for versionName).
        Ref(u32),
    }

    /// `(name, resource id, value)`.
    pub type Attr = (&'static str, Option<u32>, Val);

    fn push16(v: &mut Vec<u8>, x: u16) {
        v.extend_from_slice(&x.to_le_bytes());
    }
    fn push32(v: &mut Vec<u8>, x: u32) {
        v.extend_from_slice(&x.to_le_bytes());
    }

    /// Builds an AXML document whose root element is `root` with `attrs`.
    pub fn document(root: &'static str, attrs: &[Attr], utf8: bool) -> Vec<u8> {
        // Strings with resource ids must come first (resource map is index-aligned).
        let mut strings: Vec<&str> = Vec::new();
        let mut ids: Vec<u32> = Vec::new();
        for (n, id, _) in attrs {
            if let Some(id) = id {
                strings.push(n);
                ids.push(*id);
            }
        }
        let idx = |s: &'static str, strings: &mut Vec<&str>| -> u32 {
            if let Some(p) = strings.iter().position(|x| *x == s) {
                return p as u32;
            }
            strings.push(s);
            (strings.len() - 1) as u32
        };
        let root_idx = idx(root, &mut strings);
        let mut recs = Vec::new();
        for (n, id, v) in attrs {
            let name =
                if id.is_some() { strings.iter().position(|x| x == n).unwrap() as u32 } else { idx(n, &mut strings) };
            let (raw, ty, data) = match v {
                Val::Str(s) => {
                    let i = idx(s, &mut strings);
                    (i, TYPE_STRING, i)
                }
                Val::TypedStr(s) => (NONE, TYPE_STRING, idx(s, &mut strings)),
                Val::Int(x) => (NONE, TYPE_INT_DEC, *x),
                Val::Ref(x) => (NONE, 0x01, *x),
            };
            recs.push((name, raw, ty, data));
        }
        // String pool.
        let mut data = Vec::new();
        let mut offsets = Vec::new();
        for s in &strings {
            offsets.push(data.len() as u32);
            if utf8 {
                let bytes = s.as_bytes();
                for len in [s.chars().count(), bytes.len()] {
                    if len > 0x7f {
                        data.push(0x80 | (len >> 8) as u8);
                        data.push(len as u8);
                    } else {
                        data.push(len as u8);
                    }
                }
                data.extend_from_slice(bytes);
                data.push(0);
            } else {
                let units: Vec<u16> = s.encode_utf16().collect();
                if units.len() > 0x7fff {
                    push16(&mut data, 0x8000 | (units.len() >> 16) as u16);
                    push16(&mut data, units.len() as u16);
                } else {
                    push16(&mut data, units.len() as u16);
                }
                for u in units {
                    push16(&mut data, u);
                }
                push16(&mut data, 0);
            }
        }
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let mut pool = Vec::new();
        let str_start = 28 + 4 * strings.len() as u32;
        push16(&mut pool, T_POOL);
        push16(&mut pool, 28);
        push32(&mut pool, str_start + data.len() as u32);
        push32(&mut pool, strings.len() as u32);
        push32(&mut pool, 0);
        push32(&mut pool, if utf8 { FLAG_UTF8 } else { 0 });
        push32(&mut pool, str_start);
        push32(&mut pool, 0);
        for o in offsets {
            push32(&mut pool, o);
        }
        pool.extend_from_slice(&data);
        // Resource map.
        let mut resmap = Vec::new();
        push16(&mut resmap, T_RESMAP);
        push16(&mut resmap, 8);
        push32(&mut resmap, 8 + 4 * ids.len() as u32);
        for id in &ids {
            push32(&mut resmap, *id);
        }
        // Start element.
        let mut el = Vec::new();
        push16(&mut el, T_START_EL);
        push16(&mut el, 16);
        push32(&mut el, 16 + 20 + 20 * recs.len() as u32);
        push32(&mut el, 1);
        push32(&mut el, NONE);
        push32(&mut el, NONE);
        push32(&mut el, root_idx);
        push16(&mut el, 20);
        push16(&mut el, 20);
        push16(&mut el, recs.len() as u16);
        push16(&mut el, 0);
        push16(&mut el, 0);
        push16(&mut el, 0);
        for (name, raw, ty, d) in recs {
            push32(&mut el, NONE);
            push32(&mut el, name);
            push32(&mut el, raw);
            push16(&mut el, 8);
            el.push(0);
            el.push(ty);
            push32(&mut el, d);
        }
        let mut out = Vec::new();
        push16(&mut out, T_XML);
        push16(&mut out, 8);
        push32(&mut out, (8 + pool.len() + resmap.len() + el.len()) as u32);
        out.extend_from_slice(&pool);
        out.extend_from_slice(&resmap);
        out.extend_from_slice(&el);
        out
    }

    /// A typical manifest for `package`/`version_name`/`version_code`, optionally a split.
    pub fn manifest(
        package: &'static str,
        version_name: &'static str,
        code: u32,
        split: Option<&'static str>,
    ) -> Vec<u8> {
        let mut attrs: Vec<Attr> = vec![
            ("versionCode", Some(ATTR_VERSION_CODE), Val::Int(code)),
            ("versionName", Some(ATTR_VERSION_NAME), Val::Str(version_name)),
            ("package", None, Val::Str(package)),
        ];
        if let Some(s) = split {
            attrs.push(("split", None, Val::Str(s)));
        }
        document("manifest", &attrs, false)
    }
}

#[cfg(test)]
mod tests {
    use super::build::*;
    use super::*;

    #[test]
    fn parses_utf16_and_utf8_manifests() {
        for utf8 in [false, true] {
            let doc = document(
                "manifest",
                &[
                    ("versionCode", Some(ATTR_VERSION_CODE), Val::Int(1_548_230_154)),
                    ("versionName", Some(ATTR_VERSION_NAME), Val::Str("1.7.6")),
                    ("package", None, Val::Str("com.noodlecake.blockheads")),
                    ("installLocation", None, Val::Int(0)),
                ],
                utf8,
            );
            let m = parse_manifest(&doc).unwrap();
            assert_eq!(m.package, "com.noodlecake.blockheads");
            assert_eq!(m.version_name.as_deref(), Some("1.7.6"));
            assert_eq!(m.version_code, Some(1_548_230_154));
            assert_eq!(m.split, None);
        }
    }

    #[test]
    fn names_without_resource_ids_and_typed_strings() {
        let doc = document(
            "manifest",
            &[
                ("versionCode", None, Val::Str(" 42 ")),
                ("versionName", None, Val::TypedStr("1.7.5")),
                ("package", None, Val::Str("p")),
                ("split", None, Val::Str("config.armeabi_v7a")),
            ],
            true,
        );
        let m = parse_manifest(&doc).unwrap();
        assert_eq!((m.version_code, m.version_name.as_deref()), (Some(42), Some("1.7.5")));
        assert_eq!(m.split.as_deref(), Some("config.armeabi_v7a"));
    }

    #[test]
    fn obfuscated_names_match_by_resource_id() {
        let doc = document(
            "manifest",
            &[
                ("", Some(ATTR_VERSION_CODE), Val::Int(7)),
                ("x", Some(ATTR_VERSION_NAME), Val::Ref(0x7f0a0000)),
                ("package", None, Val::Str("p")),
            ],
            false,
        );
        let m = parse_manifest(&doc).unwrap();
        assert_eq!((m.version_code, m.version_name), (Some(7), None));
    }

    #[test]
    fn long_strings_use_extended_lengths() {
        let long: &'static str = Box::leak("é".repeat(300).into_boxed_str());
        let huge: &'static str = Box::leak("a".repeat(0x8001).into_boxed_str());
        for (s, utf8) in [(long, true), (huge, false)] {
            let doc = document("manifest", &[("package", None, Val::Str(s))], utf8);
            assert_eq!(parse_manifest(&doc).unwrap().package, s);
        }
    }

    #[test]
    fn rejects_malformed_input() {
        let ok = manifest("p", "1", 1, None);
        assert!(parse_manifest(&[]).is_err());
        assert!(parse_manifest(&[0, 0, 8, 0]).unwrap_err().0.contains("not a binary XML"));
        // Truncated everywhere: never panics.
        for n in 0..ok.len() {
            let mut t = ok[..n].to_vec();
            if t.len() >= 8 {
                t[4..8].copy_from_slice(&(ok.len() as u32).to_le_bytes());
            }
            let _ = parse_manifest(&t);
        }
        // Bad chunk size.
        let mut bad = ok.clone();
        bad[12..16].copy_from_slice(&4u32.to_le_bytes());
        assert!(parse_manifest(&bad).unwrap_err().0.contains("bad chunk size"));
        // Only the file header: no root element.
        let mut empty = ok[..8].to_vec();
        empty[4..8].copy_from_slice(&8u32.to_le_bytes());
        assert!(parse_manifest(&empty).unwrap_err().0.contains("no root element"));
        // Element before the pool.
        let pool_size = u32::from_le_bytes(ok[12..16].try_into().unwrap()) as usize;
        let mut no_pool = ok[..8].to_vec();
        no_pool.extend_from_slice(&ok[8 + pool_size..]);
        let len = no_pool.len() as u32;
        no_pool[4..8].copy_from_slice(&len.to_le_bytes());
        assert!(parse_manifest(&no_pool).unwrap_err().0.contains("before string pool"));
        // Wrong root, missing package.
        let wrong = document("application", &[("package", None, Val::Str("p"))], true);
        assert!(parse_manifest(&wrong).unwrap_err().0.contains("<application>"));
        let nopkg = document("manifest", &[("versionName", None, Val::Str("1"))], true);
        assert!(parse_manifest(&nopkg).unwrap_err().0.contains("no package"));
        // String entry pointing past the end (utf8).
        let mut oob = document("manifest", &[("package", None, Val::Str("p"))], true);
        let str_start = u32::from_le_bytes(oob[28..32].try_into().unwrap()) as usize;
        oob[8 + str_start + 1] = 0x7f;
        assert!(parse_manifest(&oob).unwrap_err().0.contains("out of bounds"));
        assert_eq!(AxmlError("x".into()).to_string(), "invalid AndroidManifest.xml: x");
    }
}
