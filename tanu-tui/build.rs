use eyre::WrapErr;
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
};

fn main() -> eyre::Result<()> {
    let out_dir =
        std::env::var("OUT_DIR").wrap_err("Failed to get OUT_DIR environment variable")?;
    let dest_path = Path::new(&out_dir).join("themes.rs");
    let mut f =
        File::create(&dest_path).wrap_err(format!("Failed to create file at {dest_path:?}"))?;

    // Write module header
    writeln!(f, "mod themes {{").wrap_err("Failed to write to output file")?;
    writeln!(f, "    use std::collections::HashMap;").wrap_err("Failed to write to output file")?;
    writeln!(f).wrap_err("Failed to write to output file")?;

    // Get all theme files
    let themes_dir = Path::new("../vendors/base16-textmate/Themes");
    let mut theme_names = Vec::new();

    // Define constants for each theme
    if let Ok(entries) = fs::read_dir(themes_dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "tmTheme") {
                if let Some(filename) = path.file_stem() {
                    if let Some(filename_str) = filename.to_str() {
                        let const_name = filename_str.replace('-', "_").to_uppercase();
                        writeln!(
                            f,
                            "    pub const {const_name}: &str = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/../vendors/base16-textmate/Themes/{filename_str}.tmTheme\"));"
                        ).wrap_err("Failed to write theme constant")?;
                        theme_names.push((const_name, filename_str.to_string()));
                    }
                }
            }
        }
    }

    // Write the function to get all themes
    writeln!(f).context("Failed to write to output file")?;
    writeln!(
        f,
        "    pub fn get_all_themes() -> HashMap<&'static str, &'static str> {{"
    )
    .context("Failed to write function header")?;
    writeln!(f, "        let mut themes = HashMap::new();")
        .context("Failed to write to output file")?;

    // Add each theme to the HashMap
    for (const_name, filename) in theme_names {
        writeln!(f, "        themes.insert(\"{filename}\", {const_name});")
            .context("Failed to write theme entry")?;
    }

    writeln!(f, "        themes").context("Failed to write return statement")?;
    writeln!(f, "    }}").context("Failed to write function closing")?;
    writeln!(f, "}}").context("Failed to write module closing")?;

    Ok(())
}
