//! Dev tool: writes every key/value of an LMDB file to `<out>/<db>/<key>` files (key bytes percent-escaped).
fn main() {
    let mut a = std::env::args().skip(1);
    let (path, out) = (a.next().unwrap(), std::path::PathBuf::from(a.next().unwrap()));
    let c = bhw_lmdb::read_file(&path).unwrap();
    let esc = |k: &[u8]| -> String {
        k.iter()
            .map(|&b| {
                if b.is_ascii_alphanumeric() || b"._-".contains(&b) {
                    (b as char).to_string()
                } else {
                    format!("%{b:02x}")
                }
            })
            .collect()
    };
    for (name, db) in &c.named {
        let d = out.join(esc(name));
        std::fs::create_dir_all(&d).unwrap();
        for (k, v) in &db.entries {
            std::fs::write(d.join(esc(k)), v).unwrap();
        }
    }
}
