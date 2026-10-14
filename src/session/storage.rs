use super::Session;
use crate::error::{Error, Result};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub trait SessionStore {
    fn load(&self) -> Result<Option<Session>>;
    fn save(&self, session: &Session) -> Result<()>;
    fn clear(&self) -> Result<()>;
}

pub struct FileStore {
    path: PathBuf,
    _lock: fs::File,
}

fn session_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    base.filter(|p| p.is_absolute())
        .map(|p| p.join("ncell-cli"))
}

impl FileStore {
    pub fn new() -> Result<Self> {
        Self::in_dir(&session_dir().ok_or(Error::Storage)?)
    }

    /// Also used by tests, so they can run on any OS with a scratch directory.
    pub fn in_dir(dir: &Path) -> Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(dir).map_err(|_| Error::Storage)?;

        // Lock lives next to the data, not in the shared temp dir.
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(dir.join("session.lock"))
            .map_err(|_| Error::Storage)?;
        lock.try_lock().map_err(|e| match e {
            fs::TryLockError::WouldBlock => Error::SessionBusy,
            fs::TryLockError::Error(_) => Error::Storage,
        })?;

        Ok(Self {
            path: dir.join("session.json"),
            _lock: lock,
        })
    }
}

impl SessionStore for FileStore {
    fn load(&self) -> Result<Option<Session>> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|_| Error::InvalidSession),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(Error::Storage),
        }
    }

    fn save(&self, session: &Session) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(session).map_err(|_| Error::InvalidSession)?;
        let tmp = self.path.with_extension("json.tmp");
        let _ = fs::remove_file(&tmp); // stale leftover from a crash

        let write = || -> io::Result<()> {
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600); // 0600 from creation, no chmod race
            let mut file = opts.open(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&tmp, &self.path) // atomic replace on Unix and Windows
        };
        write().map_err(|_| {
            let _ = fs::remove_file(&tmp);
            Error::Storage
        })
    }

    fn clear(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(Error::Storage),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/session_storage.rs"]
mod tests;
