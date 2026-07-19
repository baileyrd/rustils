//! `ls` over the platform traits: name listing, sorted for determinism
//! (the trait leaves order unspecified — see `docs/behavior/fs.md`).

use platform::error::Result;
use platform::fs::{Dir, DirEntry, FileType};

/// List entries of `dir`, sorted by name.
pub fn ls(dir: &dyn Dir) -> Result<Vec<DirEntry>> {
    let mut entries = dir.read_dir()?;
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

/// Render an entry the way the CLI prints it (dirs get a trailing slash).
pub fn render(entry: &DirEntry) -> String {
    let name = entry.name.to_string_lossy();
    match entry.file_type {
        FileType::Dir => format!("{name}/"),
        _ => name.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use platform_mock::MockDir;
    use std::ffi::OsStr;

    #[test]
    fn ls_sorts_and_types() {
        let root = MockDir::root().with_file("b.txt", "x").with_file("a.txt", "y");
        root.create_dir(OsStr::new("z-dir")).expect("mkdir");
        let entries = ls(&root).expect("ls");
        let rendered: Vec<_> = entries.iter().map(render).collect();
        assert_eq!(rendered, vec!["a.txt", "b.txt", "z-dir/"]);
    }
}
