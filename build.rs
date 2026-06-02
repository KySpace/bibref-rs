#[cfg(target_os = "windows")]
fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let svg_path = manifest_dir.join("icon.svg");
    let ico_path = out_dir.join("app.ico");
    let rc_path = out_dir.join("app.rc");

    println!("cargo:rerun-if-changed={}", svg_path.display());

    render_svg_icon(&svg_path, &ico_path);

    let ico_resource_path = ico_path.to_string_lossy().replace('\\', "/");
    std::fs::write(&rc_path, format!("1 ICON \"{}\"\n", ico_resource_path)).unwrap();

    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}

#[cfg(not(target_os = "windows"))]
fn main() {}

#[cfg(target_os = "windows")]
fn render_svg_icon(svg_path: &std::path::Path, ico_path: &std::path::Path) {
    let mut options = usvg::Options {
        resources_dir: svg_path.parent().map(std::path::Path::to_path_buf),
        ..usvg::Options::default()
    };
    options.fontdb_mut().load_system_fonts();

    let svg = std::fs::read(svg_path).unwrap();
    let tree = usvg::Tree::from_data(&svg, &options).unwrap();

    let icon_size = 256u32;
    let mut pixmap = tiny_skia::Pixmap::new(icon_size, icon_size).unwrap();
    let svg_size = tree.size();
    let scale = (icon_size as f32 / svg_size.width()).min(icon_size as f32 / svg_size.height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    let png = pixmap.encode_png().unwrap();
    write_png_backed_ico(ico_path, icon_size, icon_size, &png);
}

#[cfg(target_os = "windows")]
fn write_png_backed_ico(path: &std::path::Path, width: u32, height: u32, png: &[u8]) {
    let mut ico = Vec::with_capacity(22 + png.len());

    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());

    ico.push(if width >= 256 { 0 } else { width as u8 });
    ico.push(if height >= 256 { 0 } else { height as u8 });
    ico.push(0);
    ico.push(0);
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&32u16.to_le_bytes());
    ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(png);

    std::fs::write(path, ico).unwrap();
}
