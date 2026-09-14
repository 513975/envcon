use std::path::{Path, PathBuf};

pub(crate) fn plain_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    PathBuf::from(value.strip_prefix(r"\\?\").unwrap_or(&value))
}
use std::collections::HashSet;

pub(crate) fn directory_size(path: &Path) -> Option<(u64, bool)> {
    std::fs::read_dir(path).ok()?;
    let mut total = 0;
    let mut complete = true;
    let mut visited = HashSet::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let identity = dir.canonicalize().unwrap_or_else(|_| dir.clone());
        if !visited.insert(identity.to_string_lossy().to_lowercase()) {
            continue;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(v) => v,
            Err(_) => {
                complete = false;
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            let meta = match std::fs::symlink_metadata(entry.path()) {
                Ok(m) => m,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                pending.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    Some((total, complete))
}

#[cfg(test)]
mod tests {
    #[test]
    fn unavailable_size_is_unknown_and_links_do_not_double_count() {
        let dir = crate::test_support::TestDir::new();
        assert_eq!(super::directory_size(&dir.path().join("missing")), None);
        std::fs::create_dir(dir.path().join("data")).unwrap();
        std::fs::write(dir.path().join("data/file"), b"1234").unwrap();
        junction::create(dir.path().join("data"), dir.path().join("alias")).unwrap();
        assert_eq!(super::directory_size(dir.path()), Some((4, true)));
    }
}
