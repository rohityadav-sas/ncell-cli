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
    fn update<T>(&self, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T>;
    fn clear(&self) -> Result<()>;
}

pub struct FileStore {
    path: PathBuf,
    lock_path: PathBuf,
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

        Ok(Self {
            path: dir.join("session.json"),
            lock_path: dir.join("session.lock"),
        })
    }

    fn acquire_lock(&self) -> Result<fs::File> {
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)
            .map_err(|_| Error::Storage)?;
        for _ in 0..10 {
            match lock.try_lock() {
                Ok(()) => return Ok(lock),
                Err(fs::TryLockError::WouldBlock) => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(fs::TryLockError::Error(_)) => return Err(Error::Storage),
            }
        }
        match lock.try_lock() {
            Ok(()) => Ok(lock),
            Err(fs::TryLockError::WouldBlock) => Err(Error::SessionBusy),
            Err(fs::TryLockError::Error(_)) => Err(Error::Storage),
        }
    }

    fn write_to_file(&self, session: &Session) -> Result<()> {
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
        let _lock = self.acquire_lock()?;
        self.write_to_file(session)
    }

    fn update<T>(&self, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
        let _lock = self.acquire_lock()?;
        let mut session = self.load()?.unwrap_or_default();
        let res = f(&mut session)?;
        self.write_to_file(&session)?;
        Ok(res)
    }

    fn clear(&self) -> Result<()> {
        let _lock = self.acquire_lock()?;
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
