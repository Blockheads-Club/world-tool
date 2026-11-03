//! Dev tool: `inspect <world dir> [game_db dir] [out dir]` prints the summary, writes map.png and the stored
//! thumbnail, and converts the world to the other kind into `<out dir>`.
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let dir = Path::new(&a[0]);
    let gdb = a.get(1).filter(|s| !s.is_empty()).map(|g| bhw_save::GameDb::open(Path::new(g)).unwrap());
    let s = bhw_save::summarize(dir, gdb.as_ref()).unwrap();
    println!("{s:#?}");
    let Some(out) = a.get(2) else { return };
    let out = Path::new(out);
    std::fs::create_dir_all(out).unwrap();
    let save = bhw_save::AnySave::open(dir).unwrap();
    let (img, bad) =
        bhw_save::render_map(save.world_db(), bhw_save::MapOptions { tiles_per_pixel: 2, crop_to_content: true });
    println!("map {}x{} bad={bad}", img.width, img.height);
    std::fs::write(out.join("map.png"), img.to_png()).unwrap();
    if let Some(p) = save.worldv2().and_then(bhw_save::stored_preview) {
        std::fs::write(out.join("thumb.png"), p.to_png()).unwrap();
    }
    let dst = out.join("converted");
    let _ = std::fs::remove_dir_all(&dst);
    let r = match save {
        bhw_save::AnySave::Client(_) => bhw_save::client_to_server(dir, &dst, &Default::default()),
        bhw_save::AnySave::Server(_) => bhw_save::server_to_client(dir, &dst, &Default::default()),
    };
    println!("{r:#?}");
}
