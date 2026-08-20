//! Small helper for replacing a local file without exposing partial contents.

use std::io::Write as _;
use std::path::Path;

/// Writes `data` to a unique sibling temporary file, then renames it over `path`.
///
/// The caller is responsible for creating `path`'s parent directory. The rename is
/// atomic when the parent directory and destination are on the same filesystem.
/// This intentionally does not fsync either file or directory.
pub(crate) fn write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp_path = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        uuid::Uuid::new_v4()
    ));
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp_path)?;

    if let Err(error) = file.write_all(data) {
        drop(file);
        let _ = std::fs::remove_file(&tmp_path);
        return Err(error);
    }
    drop(file);

    std::fs::rename(tmp_path, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_leaves_only_the_complete_new_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.bin");
        std::fs::write(&path, b"old").unwrap();

        write(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
