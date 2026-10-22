//! Narrow write boundary shared by importers.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(crate) trait SourceBackend {
    fn source_digest(&mut self, path: &Path) -> io::Result<String>;
    fn read_image(&mut self, path: &Path) -> Result<crate::image_reader::Files, String>;
}

pub(crate) trait WriteBackend {
    type File: Write;

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()>;
    fn create_file(&mut self, path: &Path) -> io::Result<Self::File>;
}

pub(crate) struct NativeWriteBackend;

pub(crate) trait StageBackend {
    fn create_dir(&mut self, path: &Path) -> io::Result<()>;
}

pub(crate) struct NativeStageBackend;

impl StageBackend for NativeStageBackend {
    fn create_dir(&mut self, path: &Path) -> io::Result<()> {
        fs::create_dir(path)
    }
}

impl WriteBackend for NativeWriteBackend {
    type File = File;

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn create_file(&mut self, path: &Path) -> io::Result<Self::File> {
        File::create(path)
    }
}

#[derive(Debug)]
pub(crate) enum WriteError {
    Directory(io::Error),
    File(io::Error),
}

pub(crate) fn write_bytes_with<B: WriteBackend>(
    backend: &mut B,
    path: &Path,
    bytes: &[u8],
) -> Result<(), WriteError> {
    if let Some(parent) = path.parent() {
        backend
            .create_dir_all(parent)
            .map_err(WriteError::Directory)?;
    }
    let mut file = backend.create_file(path).map_err(WriteError::File)?;
    file.write_all(bytes).map_err(WriteError::File)
}

pub(crate) trait CleanupBackend {
    fn remove_owned_stage(&mut self, path: &Path) -> io::Result<()>;
}

pub(crate) struct NativeCleanup;

impl CleanupBackend for NativeCleanup {
    fn remove_owned_stage(&mut self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }
}

/// Owns a staging directory created exclusively by this import invocation.
/// Construct only after `create_dir` has successfully created this exact path;
/// never construct for an existing profile, cache directory, or collision.
pub(crate) struct OwnedStage<B: CleanupBackend = NativeCleanup> {
    path: PathBuf,
    backend: B,
}

impl OwnedStage<NativeCleanup> {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            backend: NativeCleanup,
        }
    }
}

#[cfg(test)]
impl<B: CleanupBackend> OwnedStage<B> {
    fn with_backend(path: PathBuf, backend: B) -> Self {
        Self { path, backend }
    }
}

impl<B: CleanupBackend> OwnedStage<B> {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl<B: CleanupBackend> Drop for OwnedStage<B> {
    fn drop(&mut self) {
        let _ = self.backend.remove_owned_stage(&self.path);
    }
}

pub(crate) trait PublishBackend {
    fn rename(&mut self, stage: &Path, destination: &Path) -> io::Result<()>;
    fn destination_exists(&mut self, destination: &Path) -> bool;
}

pub(crate) trait CopyBackend {
    fn copy(&mut self, source: &Path, destination: &Path) -> io::Result<u64>;
}

pub(crate) trait MigrationScanner {
    type Entries: Iterator<Item = io::Result<fs::DirEntry>>;

    fn read_dir(&mut self, path: &Path) -> io::Result<Self::Entries>;
    fn symlink_metadata(&mut self, path: &Path) -> io::Result<fs::Metadata>;
}

pub(crate) struct NativeMigrationScanner;

impl MigrationScanner for NativeMigrationScanner {
    type Entries = fs::ReadDir;

    fn read_dir(&mut self, path: &Path) -> io::Result<Self::Entries> {
        fs::read_dir(path)
    }

    fn symlink_metadata(&mut self, path: &Path) -> io::Result<fs::Metadata> {
        fs::symlink_metadata(path)
    }
}

pub(crate) struct NativePublishBackend;

impl PublishBackend for NativePublishBackend {
    fn rename(&mut self, stage: &Path, destination: &Path) -> io::Result<()> {
        fs::rename(stage, destination)
    }

    fn destination_exists(&mut self, destination: &Path) -> bool {
        fs::symlink_metadata(destination).is_ok()
    }
}

/// Native adapter for an import's write, publish, and migration-copy effects.
/// Each operation delegates to the existing primitive rather than duplicating it.
pub(crate) struct NativeImportBackend;

impl WriteBackend for NativeImportBackend {
    type File = File;

    fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        NativeWriteBackend.create_dir_all(path)
    }

    fn create_file(&mut self, path: &Path) -> io::Result<Self::File> {
        NativeWriteBackend.create_file(path)
    }
}

impl PublishBackend for NativeImportBackend {
    fn rename(&mut self, stage: &Path, destination: &Path) -> io::Result<()> {
        NativePublishBackend.rename(stage, destination)
    }

    fn destination_exists(&mut self, destination: &Path) -> bool {
        NativePublishBackend.destination_exists(destination)
    }
}

impl CopyBackend for NativeImportBackend {
    fn copy(&mut self, source: &Path, destination: &Path) -> io::Result<u64> {
        fs::copy(source, destination)
    }
}

impl SourceBackend for NativeImportBackend {
    fn source_digest(&mut self, path: &Path) -> io::Result<String> {
        crate::hash::file_digest(path)
    }

    fn read_image(&mut self, path: &Path) -> Result<crate::image_reader::Files, String> {
        crate::image_reader::read_image(path)
    }
}

#[derive(Debug)]
pub(crate) enum Publication<T> {
    Published,
    Existing(T),
}

pub(crate) fn publish_with<B, T>(
    backend: &mut B,
    stage: &Path,
    destination: &Path,
    check_existing: impl FnOnce() -> Result<Option<T>, String>,
) -> Result<Publication<T>, String>
where
    B: PublishBackend,
{
    match backend.rename(stage, destination) {
        Ok(()) => Ok(Publication::Published),
        Err(error) if backend.destination_exists(destination) => match check_existing()? {
            Some(existing) => Ok(Publication::Existing(existing)),
            None => Err(format!(
                "Cannot publish profile because its destination already exists: {error}"
            )),
        },
        Err(error) => Err(format!("Cannot publish imported profile: {error}")),
    }
}

#[cfg(test)]
pub(crate) fn publish<T>(
    stage: &Path,
    destination: &Path,
    check_existing: impl FnOnce() -> Result<Option<T>, String>,
) -> Result<Publication<T>, String> {
    publish_with(
        &mut NativePublishBackend,
        stage,
        destination,
        check_existing,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};

    #[derive(Clone)]
    struct FakeCleanup {
        paths: Rc<RefCell<Vec<std::path::PathBuf>>>,
        failure: Option<io::ErrorKind>,
    }

    impl CleanupBackend for FakeCleanup {
        fn remove_owned_stage(&mut self, path: &Path) -> io::Result<()> {
            self.paths.borrow_mut().push(path.to_path_buf());
            match self.failure {
                Some(kind) => Err(io::Error::from(kind)),
                None => Ok(()),
            }
        }
    }

    #[test]
    fn owned_stage_cleanup_logs_exact_path_once() {
        let path = Path::new("cache/.owned-stage").to_path_buf();
        let paths = Rc::new(RefCell::new(Vec::new()));
        {
            let stage = OwnedStage::with_backend(
                path.clone(),
                FakeCleanup {
                    paths: Rc::clone(&paths),
                    failure: None,
                },
            );
            assert_eq!(stage.path(), path);
        }
        assert_eq!(*paths.borrow(), vec![path]);
    }

    #[test]
    fn failed_cleanup_is_ignored_and_called_once() {
        let path = Path::new("cache/.orphan-stage").to_path_buf();
        let paths = Rc::new(RefCell::new(Vec::new()));
        {
            let _stage = OwnedStage::with_backend(
                path.clone(),
                FakeCleanup {
                    paths: Rc::clone(&paths),
                    failure: Some(io::ErrorKind::PermissionDenied),
                },
            );
        }
        assert_eq!(*paths.borrow(), vec![path]);
    }

    #[derive(Clone, Copy)]
    enum Writes {
        Partial,
        Zero,
        PartialThenDiskFull,
    }

    #[derive(Default)]
    struct State {
        bytes: Vec<u8>,
        calls: usize,
        file_creations: usize,
        fail_directory: bool,
        fail_file: bool,
    }

    struct FakeBackend {
        state: Rc<RefCell<State>>,
        writes: Writes,
    }

    struct FakeFile {
        state: Rc<RefCell<State>>,
        writes: Writes,
    }

    impl Write for FakeFile {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let mut state = self.state.borrow_mut();
            state.calls += 1;
            match (self.writes, state.calls) {
                (Writes::Zero, _) => Ok(0),
                (Writes::PartialThenDiskFull, 1) => {
                    let count = bytes.len().min(2);
                    state.bytes.extend_from_slice(&bytes[..count]);
                    Ok(count)
                }
                (Writes::PartialThenDiskFull, _) => {
                    Err(io::Error::from(io::ErrorKind::StorageFull))
                }
                (Writes::Partial, _) => {
                    let count = bytes.len().min(2);
                    state.bytes.extend_from_slice(&bytes[..count]);
                    Ok(count)
                }
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl WriteBackend for FakeBackend {
        type File = FakeFile;

        fn create_dir_all(&mut self, _path: &Path) -> io::Result<()> {
            if self.state.borrow().fail_directory {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                Ok(())
            }
        }

        fn create_file(&mut self, _path: &Path) -> io::Result<Self::File> {
            self.state.borrow_mut().file_creations += 1;
            if self.state.borrow().fail_file {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            Ok(FakeFile {
                state: Rc::clone(&self.state),
                writes: self.writes,
            })
        }
    }

    fn backend(writes: Writes) -> (FakeBackend, Rc<RefCell<State>>) {
        let state = Rc::new(RefCell::new(State::default()));
        (
            FakeBackend {
                state: Rc::clone(&state),
                writes,
            },
            state,
        )
    }

    #[test]
    fn write_all_recovers_from_partial_writes() {
        let (mut backend, state) = backend(Writes::Partial);
        write_bytes_with(&mut backend, Path::new("stage/output"), b"success").unwrap();
        assert_eq!(state.borrow().bytes, b"success");
        assert!(state.borrow().calls > 1);
    }

    #[test]
    fn zero_write_is_write_zero() {
        let (mut backend, _) = backend(Writes::Zero);
        let error = write_bytes_with(&mut backend, Path::new("stage/output"), b"data").unwrap_err();
        assert!(
            matches!(error, WriteError::File(error) if error.kind() == io::ErrorKind::WriteZero)
        );
    }

    #[test]
    fn partial_write_then_disk_full_is_an_error() {
        let (mut backend, state) = backend(Writes::PartialThenDiskFull);
        let error = write_bytes_with(&mut backend, Path::new("stage/output"), b"data");
        assert!(
            matches!(error, Err(WriteError::File(error)) if error.kind() == io::ErrorKind::StorageFull)
        );
        assert_eq!(state.borrow().bytes, b"da");
    }

    #[test]
    fn directory_failure_does_not_create_file() {
        let (mut backend, state) = backend(Writes::Partial);
        state.borrow_mut().fail_directory = true;
        let error = write_bytes_with(&mut backend, Path::new("stage/output"), b"data");
        assert!(matches!(error, Err(WriteError::Directory(_))));
        assert!(state.borrow().bytes.is_empty());
        assert_eq!(state.borrow().calls, 0);
        assert_eq!(state.borrow().file_creations, 0);
    }

    #[test]
    fn file_creation_failure_is_typed() {
        let (mut backend, state) = backend(Writes::Partial);
        state.borrow_mut().fail_file = true;
        let error = write_bytes_with(&mut backend, Path::new("stage/output"), b"data");
        assert!(
            matches!(error, Err(WriteError::File(error)) if error.kind() == io::ErrorKind::PermissionDenied)
        );
    }

    #[test]
    fn failed_stage_write_does_not_touch_published_profile_or_userdata() {
        let root = std::env::temp_dir().join(format!(
            "clover-import-storage-{}-{}",
            std::process::id(),
            STAGE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let published = root.join("profile");
        let stage = root.join("stage");
        fs::create_dir_all(&published).unwrap();
        fs::write(published.join("manifest.json"), b"existing profile").unwrap();
        fs::write(published.join("userdata"), b"existing userdata").unwrap();

        let (mut backend, _) = backend(Writes::PartialThenDiskFull);
        let failed = write_bytes_with(&mut backend, &stage.join("asset"), b"replacement");
        assert!(matches!(failed, Err(WriteError::File(_))));
        assert_eq!(
            fs::read(published.join("manifest.json")).unwrap(),
            b"existing profile"
        );
        assert_eq!(
            fs::read(published.join("userdata")).unwrap(),
            b"existing userdata"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn native_guard_removes_failed_stage_and_preserves_published_profile_and_userdata() {
        let root = temp_root("native-cleanup");
        let published = root.join("profile");
        let stage_path = root.join(".owned-stage");
        fs::create_dir_all(&published).unwrap();
        fs::write(published.join("profile.json"), b"existing profile").unwrap();
        fs::write(published.join("userdata"), b"existing userdata").unwrap();
        fs::create_dir(&stage_path).unwrap();

        {
            let _stage = OwnedStage::new(stage_path.clone());
            let (mut backend, _) = backend(Writes::PartialThenDiskFull);
            let result = write_bytes_with(&mut backend, &stage_path.join("asset"), b"replacement");
            assert!(matches!(result, Err(WriteError::File(_))));
        }

        assert!(!stage_path.exists());
        assert_eq!(
            fs::read(published.join("profile.json")).unwrap(),
            b"existing profile"
        );
        assert_eq!(
            fs::read(published.join("userdata")).unwrap(),
            b"existing userdata"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cleanup_failure_leaves_owned_orphan_without_touching_published_files() {
        let root = temp_root("cleanup-failure");
        let published = root.join("profile");
        let stage_path = root.join(".orphan-stage");
        fs::create_dir_all(&published).unwrap();
        fs::write(published.join("profile.json"), b"existing profile").unwrap();
        fs::write(published.join("userdata"), b"existing userdata").unwrap();
        fs::create_dir(&stage_path).unwrap();
        fs::write(stage_path.join("partial"), b"partial stage output").unwrap();

        let paths = Rc::new(RefCell::new(Vec::new()));
        {
            let _stage = OwnedStage::with_backend(
                stage_path.clone(),
                FakeCleanup {
                    paths: Rc::clone(&paths),
                    failure: Some(io::ErrorKind::PermissionDenied),
                },
            );
        }

        assert_eq!(*paths.borrow(), vec![stage_path.clone()]);
        assert_eq!(
            fs::read(stage_path.join("partial")).unwrap(),
            b"partial stage output"
        );
        assert_eq!(
            fs::read(published.join("profile.json")).unwrap(),
            b"existing profile"
        );
        assert_eq!(
            fs::read(published.join("userdata")).unwrap(),
            b"existing userdata"
        );
        let _ = fs::remove_dir_all(root);
    }

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "clover-import-storage-{label}-{}-{}",
            std::process::id(),
            STAGE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ))
    }

    static STAGE_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
}
