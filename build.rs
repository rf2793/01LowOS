use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const GLYPH_0: [&str; 9] = [
    ".###.",
    "##.##",
    "##.##",
    "##.##",
    "##.##",
    "##.##",
    "##.##",
    "##.##",
    ".###.",
];

const GLYPH_1: [&str; 9] = [
    "..##.",
    ".###.",
    "####.",
    "..##.",
    "..##.",
    "..##.",
    "..##.",
    "..##.",
    "#####",
];

fn draw_glyph(rgba: &mut [u8], glyph: &[&str; 9], ox: usize, oy: usize, scale: usize, color: [u8; 4]) {
    for (gy, row) in glyph.iter().enumerate() {
        for (gx, ch) in row.chars().enumerate() {
            if ch != '#' {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let x = ox + gx * scale + sx;
                    let y = oy + gy * scale + sy;
                    if x < 32 && y < 32 {
                        let idx = (y * 32 + x) * 4;
                        rgba[idx..idx + 4].copy_from_slice(&color);
                    }
                }
            }
        }
    }
}

fn generate_rgba() -> Vec<u8> {
    let mut rgba = vec![0u8; 32 * 32 * 4];
    let bg = [15u8, 23, 42, 255]; // #0F172A
    let border = [6u8, 182, 212, 255]; // #06B6D4
    let inner_border = [14u8, 116, 144, 255];
    let fg = [34u8, 211, 238, 255]; // #22D3EE

    for y in 0..32 {
        for x in 0..32 {
            // Rounded corners (radius 3)
            let corner = !(2..=29).contains(&x) && !(2..=29).contains(&y);
            if corner {
                continue;
            }
            let is_outer = x == 0 || x == 31 || y == 0 || y == 31
                || ((x == 1 || x == 30) && (y == 2 || y == 29))
                || ((y == 1 || y == 30) && (x == 2 || x == 29));
            let is_inner = x == 1 || x == 30 || y == 1 || y == 30;
            let px = if is_outer {
                border
            } else if is_inner {
                inner_border
            } else {
                bg
            };
            let idx = (y * 32 + x) * 4;
            rgba[idx..idx + 4].copy_from_slice(&px);
        }
    }

    draw_glyph(&mut rgba, &GLYPH_0, 5, 7, 2, fg);
    draw_glyph(&mut rgba, &GLYPH_1, 17, 7, 2, fg);
    rgba
}

fn generate_ico(rgba: &[u8]) -> Vec<u8> {
    let xor_size = 32 * 32 * 4;
    let and_size = (32 / 8) * 32; // 128 bytes
    let dib_size = 40 + xor_size + and_size;

    let mut ico = Vec::with_capacity(6 + 16 + dib_size);
    // ICONDIR
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // type = 1 (ICO)
    ico.extend_from_slice(&1u16.to_le_bytes()); // count = 1

    // ICONDIRENTRY
    ico.push(32); // width
    ico.push(32); // height
    ico.push(0); // color count
    ico.push(0); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bpp
    ico.extend_from_slice(&(dib_size as u32).to_le_bytes()); // bytes in res
    ico.extend_from_slice(&22u32.to_le_bytes()); // offset (6 + 16)

    // BITMAPINFOHEADER
    ico.extend_from_slice(&40u32.to_le_bytes()); // biSize
    ico.extend_from_slice(&32i32.to_le_bytes()); // biWidth
    ico.extend_from_slice(&64i32.to_le_bytes()); // biHeight (XOR + AND)
    ico.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    ico.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    ico.extend_from_slice(&0u32.to_le_bytes()); // biCompression (BI_RGB)
    ico.extend_from_slice(&((xor_size + and_size) as u32).to_le_bytes());
    ico.extend_from_slice(&0i32.to_le_bytes());
    ico.extend_from_slice(&0i32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());
    ico.extend_from_slice(&0u32.to_le_bytes());

    // Bottom-up BGRA pixels
    for y in (0..32).rev() {
        for x in 0..32 {
            let idx = (y * 32 + x) * 4;
            ico.push(rgba[idx + 2]); // B
            ico.push(rgba[idx + 1]); // G
            ico.push(rgba[idx]);     // R
            ico.push(rgba[idx + 3]); // A
        }
    }

    // 1-bit AND mask (all 0 since alpha channel handles transparency)
    ico.resize(ico.len() + and_size, 0);
    ico
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR missing"));
    let rgba = generate_rgba();
    let ico = generate_ico(&rgba);

    let rgba_path = out_dir.join("icon.rgba");
    let ico_path = out_dir.join("icon.ico");
    let rc_path = out_dir.join("icon.rc");
    let res_path = out_dir.join("icon_res.o");

    fs::write(&rgba_path, &rgba).expect("write icon.rgba");
    fs::write(&ico_path, &ico).expect("write icon.ico");

    let ico_slash = ico_path.to_string_lossy().replace('\\', "/");
    fs::write(&rc_path, format!("1 ICON \"{ico_slash}\"\n")).expect("write icon.rc");

    match Command::new("windres")
        .arg("-i")
        .arg(&rc_path)
        .arg("-o")
        .arg(&res_path)
        .output()
    {
        Ok(out) if out.status.success() => {
            println!("cargo:rustc-link-arg={}", res_path.display());
        }
        Ok(out) => {
            println!(
                "cargo:warning=windres falhou ao compilar icon.rc: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Err(e) => {
            println!("cargo:warning=windres não encontrado ({e}); ícone .ico não embutido.");
        }
    }
}
