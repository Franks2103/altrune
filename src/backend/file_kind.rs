//! Looking at a file the person gives the app: the first bytes are read here, and the domain says
//! what they mean.

use crate::domain::install::{classify_bytes, unreadable, Detected};
use std::path::Path;

/// Reads the first bytes of `path` and says what it is. Touches the disk once.
pub fn classify_file(path: &Path) -> Detected {
    use std::io::Read;
    let mut head = vec![0u8; 4096];
    let Ok(read) = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)) else {
        return unreadable();
    };
    head.truncate(read);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    classify_bytes(&name, &head)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file that cannot be read says so; it is not taken for a bad download.
    #[test]
    fn an_unreadable_file_is_not_a_bad_download() {
        let d = classify_file(Path::new("/no/such/folder/setup.exe"));
        assert!(!d.supported());
        assert!(d.warning().unwrap().contains("leer"));
    }

    /// What is read from the disk is judged like the same bytes given directly.
    #[test]
    fn a_file_is_judged_by_its_name_and_first_bytes() {
        let dir = std::env::temp_dir().join(format!("bl-classify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("SteamSetup.exe");
        let mut bytes = b"MZ".to_vec();
        bytes.resize(512, 0);
        std::fs::write(&file, &bytes).unwrap();
        assert_eq!(
            classify_file(&file),
            classify_bytes("SteamSetup.exe", &bytes)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
