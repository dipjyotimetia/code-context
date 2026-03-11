use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use super::languages::LanguageRegistry;

pub fn walk_repository(root: &Path, registry: &LanguageRegistry) -> Vec<PathBuf> {
    let mut files = Vec::new();

    let walker = WalkBuilder::new(root)
        .hidden(true) // respect hidden-file defaults
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .max_depth(Some(50))
        .build();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let path = entry.path();

        // Skip very large files (> 1 MB)
        if let Ok(meta) = std::fs::metadata(path) {
            if meta.len() > 1_048_576 {
                continue;
            }
        }

        // Only include files whose extension maps to a known language
        if registry.detect_language(path).is_none() {
            continue;
        }

        // Skip binary files by checking for null bytes in the first 8KB
        if let Ok(mut f) = std::fs::File::open(path) {
            use std::io::Read;
            let mut buf = [0u8; 8192];
            if let Ok(n) = f.read(&mut buf) {
                if buf[..n].contains(&0) {
                    continue;
                }
            }
        }

        files.push(path.to_path_buf());
    }

    files.sort();
    files
}
