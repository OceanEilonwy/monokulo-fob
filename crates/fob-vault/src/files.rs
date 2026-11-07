//! Folders and files only this user can read, written whole or not at all.
//!
//! Even sealed, a wallet's files say something to another account on the same
//! machine: which wallets there are, when they change and by how much. So the
//! data folder is 0700 and each file 0600, whatever the umask says.

use std::{
    io::{self, Write},
    path::Path,
};

/// Makes `dir` and any missing parents, readable by this user alone. A folder
/// that's already there keeps the mode it has, as the user may have chosen it.
pub fn private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Writes `bytes` to `path` with mode 0600, replacing what's there only once
/// the new file is complete and on disk, and the rename itself too.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        private_dir(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let _ = std::fs::remove_file(&tmp);
    {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    // The rename lives in the folder: without this a crash could bring back the
    // old file, or neither.
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        std::fs::File::open(dir)?.sync_all()?;
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn folders_and_files_are_this_users_alone_and_replaced_whole() {
        let root = std::env::temp_dir().join(format!("fob-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let dir = root.join("data");
        let file = dir.join("settings.json");
        write_private(&file, b"one").unwrap();
        write_private(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!dir.join("settings.json.tmp").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
