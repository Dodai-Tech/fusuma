//! Prints the Rust of a palette made from a seed: `cargo run -p fusuma-palette --example gen -- seagreen 46 139 87 [tint]`.

use fusuma_render::Color;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [name, r, g, b, rest @ ..] = args.as_slice() else {
        eprintln!(
            "usage: gen NAME R G B [TINT]   (the seed colour as 0-255 numbers; TINT is the neutrals' chroma, 0.012 by default)"
        );
        std::process::exit(2);
    };
    let byte = |s: &String| {
        s.parse::<u8>().unwrap_or_else(|_| {
            eprintln!("{s:?} is not a number from 0 to 255");
            std::process::exit(2);
        })
    };
    let tint = rest.first().and_then(|t| t.parse().ok()).unwrap_or(0.012);
    print!(
        "{}",
        fusuma_palette::rust_source(name, Color::rgb(byte(r), byte(g), byte(b)), tint)
    );
}
