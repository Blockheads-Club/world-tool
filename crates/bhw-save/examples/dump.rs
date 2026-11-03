use std::io::Read;

fn pp(v: &plist::Value, ind: usize) -> String {
    use plist::Value as V;
    let pad = "  ".repeat(ind + 1);
    match v {
        V::Dictionary(d) => {
            let mut s = String::from("{\n");
            for (k, x) in d {
                s += &format!("{pad}{k}: {}\n", pp(x, ind + 1));
            }
            s + &"  ".repeat(ind) + "}"
        }
        V::Array(a) => {
            if a.len() > 12 {
                return format!(
                    "[{} items, first: {}]",
                    a.len(),
                    a.first().map(|x| pp(x, ind + 1)).unwrap_or_default()
                );
            }
            let mut s = String::from("[\n");
            for x in a {
                s += &format!("{pad}{}\n", pp(x, ind + 1));
            }
            s + &"  ".repeat(ind) + "]"
        }
        V::Data(d) => {
            if d.starts_with(b"bplist00") {
                if let Ok(p) = plist::Value::from_reader(std::io::Cursor::new(d)) {
                    return format!("data(bplist {}B) {}", d.len(), pp(&p, ind + 1));
                }
            }
            let hex: Vec<String> = d.iter().take(48).map(|b| format!("{b:02x}")).collect();
            format!("data({}B) {}", d.len(), hex.join(""))
        }
        V::String(s) => format!("{s:?}"),
        other => format!("{other:?}"),
    }
}

fn show(v: &[u8]) -> String {
    if v.starts_with(b"bplist00") || v.starts_with(b"<?xml") {
        match plist::Value::from_reader(std::io::Cursor::new(v)) {
            Ok(p) => return format!("PLIST({}) {}", if v[0] == b'b' { "bin" } else { "xml" }, pp(&p, 1)),
            Err(e) => return format!("PLIST err {e}"),
        }
    }
    if v.starts_with(&[0x1f, 0x8b]) {
        let mut d = flate2::read::GzDecoder::new(v);
        let mut out = Vec::new();
        let _ = d.read_to_end(&mut out);
        let head: Vec<String> = out.iter().take(80).map(|b| format!("{b:02x}")).collect();
        let tail: Vec<String> = out.iter().rev().take(8).rev().map(|b| format!("{b:02x}")).collect();
        return format!("GZIP len={} head={} tail={}", out.len(), head.join(""), tail.join(""));
    }
    if v.starts_with(b"<?xml") {
        let s = String::from_utf8_lossy(v);
        return format!("XML {}", if s.len() > 4000 { &s[..4000] } else { &s });
    }
    let s = String::from_utf8_lossy(&v[..v.len().min(200)]).to_string();
    let hex: Vec<String> = v.iter().take(64).map(|b| format!("{b:02x}")).collect();
    format!("RAW len={} hex={} str={:?}", v.len(), hex.join(""), s)
}

fn main() {
    let path = std::env::args().nth(1).unwrap();
    let full = std::env::args().nth(2).is_some();
    let c = bhw_lmdb::read_file(&path).unwrap();
    println!("page={} map={} flags={:#x} main_entries={}", c.page_size, c.map_size, c.env_flags, c.main.entries.len());
    for (k, v) in &c.main.entries {
        println!("MAIN {:?} => {}", String::from_utf8_lossy(k), show(v));
    }
    for (name, db) in &c.named {
        println!("== DB {:?} flags={:#x} entries={}", String::from_utf8_lossy(name), db.flags.0, db.entries.len());
        for (i, (k, v)) in db.entries.iter().enumerate() {
            if !full && i >= 6 && name != b"main" {
                break;
            }
            println!("  {:?} => {}", String::from_utf8_lossy(k), show(v));
        }
    }
}
