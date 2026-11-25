//! Prints a keymap as JSON (or a short summary with --summary).
//! cargo run -p zmk-core --example dump -- path/to/file.keymap [--summary]
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: dump <file.keymap> [--summary]");
    let summary = args.next().as_deref() == Some("--summary");
    let km = zmk_core::load_keymap(&path).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    if summary {
        println!(
            "{} | {} keys | rows {:?} | split {}",
            km.layout.name, km.layout.total_keys, km.layout.keys_per_row, km.layout.is_split
        );
        for (i, l) in km.layers.iter().enumerate() {
            println!("L{i} {:?}", l.name);
            let mut row = usize::MAX;
            for b in &l.bindings {
                if b.row != row {
                    row = b.row;
                    println!();
                    print!("   ");
                }
                print!("{:>8}", b.display_text.replace('\n', "/"));
            }
            println!("\n");
        }
    } else {
        println!("{}", serde_json::to_string(&km).unwrap());
    }
}
