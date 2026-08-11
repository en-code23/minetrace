use std::{
    fs::{self, File, Metadata, OpenOptions},
    io::{BufReader, Read, Seek, SeekFrom, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    ScanError,
    model::{FileFingerprint, FingerprintedLog, InventoryReport, LogCandidate},
};

#[derive(Debug, Clone)]
pub(crate) struct FingerprintOptions {
    pub prefix_bytes: usize,
    pub buffer_bytes: usize,
}

impl Default for FingerprintOptions {
    fn default() -> Self {
        Self {
            prefix_bytes: 64 * 1024,
            buffer_bytes: 64 * 1024,
        }
    }
}

pub(crate) fn fingerprint_inventory(
    report: &InventoryReport,
    options: &FingerprintOptions,
) -> Result<Vec<FingerprintedLog>, ScanError> {
    report
        .candidates
        .iter()
        .map(|candidate| {
            fingerprint_log(candidate, options).map(|fingerprint| FingerprintedLog {
                candidate: candidate.clone(),
                fingerprint,
            })
        })
        .collect()
}

pub(crate) fn fingerprint_log(
    candidate: &LogCandidate,
    options: &FingerprintOptions,
) -> Result<FileFingerprint, ScanError> {
    fingerprint_log_with_previous_size(candidate, options, None)
}

pub(crate) fn fingerprint_log_with_previous_size(
    candidate: &LogCandidate,
    options: &FingerprintOptions,
    previous_size: Option<u64>,
) -> Result<FileFingerprint, ScanError> {
    fingerprint_log_with_previous_size_and_control(candidate, options, previous_size, || false)
}

pub(crate) fn fingerprint_log_with_previous_size_and_control<C>(
    candidate: &LogCandidate,
    options: &FingerprintOptions,
    previous_size: Option<u64>,
    mut is_cancelled: C,
) -> Result<FileFingerprint, ScanError>
where
    C: FnMut() -> bool,
{
    let path = &candidate.absolute_path;
    check_cancelled(&mut is_cancelled)?;
    let before = fs::symlink_metadata(path)
        .map_err(|error| ScanError::io("read candidate metadata", path, error))?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(ScanError::NotRegularFile(
            path.to_string_lossy().into_owned(),
        ));
    }

    let file = open_log_read_only_no_follow(candidate)?;
    let opened = file
        .metadata()
        .map_err(|error| ScanError::io("read open log metadata", path, error))?;
    if !same_observation(&before, &opened) {
        return Err(ScanError::FileChanged(path.to_string_lossy().into_owned()));
    }
    check_cancelled(&mut is_cancelled)?;

    let mut reader = BufReader::with_capacity(options.buffer_bytes.max(4 * 1024), file);
    let mut buffer = vec![0_u8; options.buffer_bytes.max(4 * 1024)];
    let mut prefix_remaining = options.prefix_bytes;
    let mut prefix_hasher = blake3::Hasher::new();
    let mut comparison_hasher = previous_size.map(|_| blake3::Hasher::new());
    let mut comparison_remaining = previous_size.unwrap_or(0);
    let mut full_hasher = blake3::Hasher::new();
    let mut bytes_read = 0_u64;

    loop {
        check_cancelled(&mut is_cancelled)?;
        let read = reader
            .read(&mut buffer)
            .map_err(|error| ScanError::io("fingerprint log", path, error))?;
        if read == 0 {
            break;
        }
        full_hasher.update(&buffer[..read]);
        if prefix_remaining > 0 {
            let prefix_read = read.min(prefix_remaining);
            prefix_hasher.update(&buffer[..prefix_read]);
            prefix_remaining -= prefix_read;
        }
        if comparison_remaining > 0 {
            let comparison_read =
                read.min(usize::try_from(comparison_remaining).unwrap_or(usize::MAX));
            if let Some(hasher) = comparison_hasher.as_mut() {
                hasher.update(&buffer[..comparison_read]);
            }
            comparison_remaining = comparison_remaining.saturating_sub(comparison_read as u64);
        }
        bytes_read = bytes_read.saturating_add(read as u64);
    }
    check_cancelled(&mut is_cancelled)?;

    let after = fs::symlink_metadata(path)
        .map_err(|error| ScanError::io("recheck candidate metadata", path, error))?;
    if !same_observation(&opened, &after) || bytes_read != opened.len() {
        return Err(ScanError::FileChanged(path.to_string_lossy().into_owned()));
    }

    let comparison_prefix_hash = comparison_hasher
        .filter(|_| previous_size.is_some_and(|length| length <= bytes_read))
        .map(|hasher| *hasher.finalize().as_bytes());
    Ok(FileFingerprint {
        size_bytes: opened.len(),
        modified_at_ms: metadata_time_ms(opened.modified().ok()).unwrap_or_default(),
        birthtime_ms: metadata_time_ms(opened.created().ok()),
        prefix_hash: *prefix_hasher.finalize().as_bytes(),
        full_hash: *full_hasher.finalize().as_bytes(),
        comparison_prefix_len: comparison_prefix_hash.map(|_| previous_size.unwrap_or(0)),
        comparison_prefix_hash,
    })
}

/// Open a candidate without following a final-component symbolic link and
/// prove the opened handle is the regular file just observed at `path`.
pub(crate) fn open_log_read_only_no_follow(candidate: &LogCandidate) -> Result<File, ScanError> {
    let path = &candidate.absolute_path;
    if candidate.approved_root.join(&candidate.relative_path) != *path {
        return Err(ScanError::FileChanged(path.to_string_lossy().into_owned()));
    }
    let before = fs::symlink_metadata(path)
        .map_err(|error| ScanError::io("read candidate metadata", path, error))?;
    if before.file_type().is_symlink() || !before.is_file() {
        return Err(ScanError::NotRegularFile(
            path.to_string_lossy().into_owned(),
        ));
    }
    let file = open_candidate_platform(candidate).map_err(|error| {
        ScanError::io("open log read-only without following links", path, error)
    })?;
    let opened = file
        .metadata()
        .map_err(|error| ScanError::io("read open log metadata", path, error))?;
    if !opened.is_file() || !same_observation(&before, &opened) {
        return Err(ScanError::FileChanged(path.to_string_lossy().into_owned()));
    }
    Ok(file)
}

/// Copy an already-open source into a private temporary file while hashing the
/// exact copied byte stream. Only a byte-for-byte match with the staged
/// fingerprint is returned, so the parser consumes an immutable snapshot even
/// if another process mutates and later restores the live source.
pub(crate) fn create_verified_file_snapshot_with_control<C>(
    file: &mut File,
    path: &Path,
    expected: &FileFingerprint,
    options: &FingerprintOptions,
    mut is_cancelled: C,
) -> Result<Option<File>, ScanError>
where
    C: FnMut() -> bool,
{
    check_cancelled(&mut is_cancelled)?;
    let before = file
        .metadata()
        .map_err(|error| ScanError::io("read open log metadata", path, error))?;
    if !metadata_matches_fingerprint(&before, expected) {
        return Ok(None);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| ScanError::io("rewind open log", path, error))?;

    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; options.buffer_bytes.max(4 * 1024)];
    let mut bytes_read = 0_u64;
    let mut snapshot = tempfile::tempfile()
        .map_err(|error| ScanError::io("create private log snapshot", path, error))?;
    loop {
        check_cancelled(&mut is_cancelled)?;
        let read = file
            .read(&mut buffer)
            .map_err(|error| ScanError::io("verify open log snapshot", path, error))?;
        if read == 0 {
            break;
        }
        snapshot
            .write_all(&buffer[..read])
            .map_err(|error| ScanError::io("write private log snapshot", path, error))?;
        hasher.update(&buffer[..read]);
        bytes_read = bytes_read.saturating_add(read as u64);
    }
    check_cancelled(&mut is_cancelled)?;
    let after = file
        .metadata()
        .map_err(|error| ScanError::io("recheck open log metadata", path, error))?;
    let matches = same_observation(&before, &after)
        && metadata_matches_fingerprint(&after, expected)
        && bytes_read == expected.size_bytes
        && hasher.finalize().as_bytes() == &expected.full_hash;
    if matches {
        snapshot
            .seek(SeekFrom::Start(0))
            .map_err(|error| ScanError::io("rewind private log snapshot", path, error))?;
        Ok(Some(snapshot))
    } else {
        Ok(None)
    }
}

fn metadata_matches_fingerprint(metadata: &Metadata, expected: &FileFingerprint) -> bool {
    metadata.len() == expected.size_bytes
        && metadata_time_ms(metadata.modified().ok()).unwrap_or_default() == expected.modified_at_ms
}

#[cfg(unix)]
fn open_candidate_platform(candidate: &LogCandidate) -> std::io::Result<File> {
    use std::{
        ffi::CString,
        os::unix::{
            ffi::OsStrExt,
            fs::OpenOptionsExt,
            io::{AsRawFd, FromRawFd},
        },
        path::Component,
    };

    let mut options = OpenOptions::new();
    options.read(true);
    options.custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY);
    let mut current = options.open(&candidate.approved_root)?;
    let components = candidate.relative_path.components().collect::<Vec<_>>();
    if components.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "log candidate has an empty relative path",
        ));
    }

    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "log candidate path must remain beneath its approved root",
            ));
        };
        let name = CString::new(name.as_bytes()).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "log candidate path contains a NUL byte",
            )
        })?;
        let last = index + 1 == components.len();
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | if last { 0 } else { libc::O_DIRECTORY };
        // SAFETY: `current` is a live directory handle, `name` is NUL-terminated,
        // and a successful descriptor is immediately owned by `File`.
        let descriptor = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: `openat` returned a fresh owned descriptor above.
        current = unsafe { File::from_raw_fd(descriptor) };
    }
    Ok(current)
}

#[cfg(windows)]
fn open_candidate_platform(candidate: &LogCandidate) -> std::io::Result<File> {
    use std::{
        os::windows::fs::{MetadataExt, OpenOptionsExt},
        path::Component,
    };

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;

    let mut cursor = candidate.approved_root.clone();
    for component in candidate.relative_path.components() {
        let Component::Normal(component) = component else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "log candidate path must remain beneath its approved root",
            ));
        };
        cursor.push(component);
        let metadata = fs::symlink_metadata(&cursor)?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "log candidate path contains a reparse point",
            ));
        }
    }
    let canonical_root = candidate.approved_root.canonicalize()?;
    let canonical_path = candidate.absolute_path.canonicalize()?;
    if !canonical_path.starts_with(canonical_root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "log candidate escaped its approved root",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(&candidate.absolute_path)
}

#[cfg(not(any(unix, windows)))]
fn open_candidate_platform(candidate: &LogCandidate) -> std::io::Result<File> {
    OpenOptions::new().read(true).open(&candidate.absolute_path)
}

fn check_cancelled<C>(is_cancelled: &mut C) -> Result<(), ScanError>
where
    C: FnMut() -> bool,
{
    if is_cancelled() {
        Err(ScanError::Cancelled)
    } else {
        Ok(())
    }
}

fn same_observation(left: &Metadata, right: &Metadata) -> bool {
    left.len() == right.len()
        && metadata_time_ms(left.modified().ok()) == metadata_time_ms(right.modified().ok())
}

fn metadata_time_ms(time: Option<SystemTime>) -> Option<i64> {
    let time = time?;
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => i64::try_from(duration.as_millis()).ok(),
        Err(error) => i64::try_from(error.duration().as_millis())
            .ok()
            .map(|value| -value),
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, fs, path::PathBuf};

    use tempfile::tempdir;

    use super::{
        FingerprintOptions, fingerprint_log, fingerprint_log_with_previous_size,
        fingerprint_log_with_previous_size_and_control,
    };
    use crate::{
        platform::native_path_key,
        scan::{LogCandidate, LogFileKind},
    };

    fn candidate(path: PathBuf) -> LogCandidate {
        let relative_path = PathBuf::from("latest.log");
        let approved_root = path.parent().expect("candidate parent").to_path_buf();
        LogCandidate {
            approved_root,
            observed_size_bytes: fs::metadata(&path).expect("metadata").len(),
            absolute_path: path,
            relative_path_key: native_path_key(&relative_path),
            relative_path,
            kind: LogFileKind::Log,
        }
    }

    #[test]
    fn fingerprints_are_stable_and_change_with_content() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("latest.log");
        fs::write(&path, "first log body").expect("first");
        let first = fingerprint_log(&candidate(path.clone()), &FingerprintOptions::default())
            .expect("fingerprint");
        let repeated = fingerprint_log(&candidate(path.clone()), &FingerprintOptions::default())
            .expect("fingerprint");
        assert_eq!(first, repeated);

        fs::write(&path, "second log body").expect("second");
        let second =
            fingerprint_log(&candidate(path), &FingerprintOptions::default()).expect("fingerprint");
        assert_ne!(first.full_hash, second.full_hash);
    }

    #[test]
    fn append_preserves_the_prefix_hash() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("latest.log");
        fs::write(&path, "prefix-body").expect("first");
        let options = FingerprintOptions {
            prefix_bytes: 6,
            ..FingerprintOptions::default()
        };
        let first = fingerprint_log(&candidate(path.clone()), &options).expect("first hash");
        fs::write(&path, "prefix-body-and-appended-data").expect("append");
        let appended = fingerprint_log(&candidate(path), &options).expect("second hash");

        assert_eq!(first.prefix_hash, appended.prefix_hash);
        assert_ne!(first.full_hash, appended.full_hash);
        assert!(appended.size_bytes > first.size_bytes);
    }

    #[test]
    fn short_append_can_be_proven_against_the_exact_previous_size() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("latest.log");
        fs::write(&path, "short original log").expect("first");
        let options = FingerprintOptions::default();
        let first = fingerprint_log(&candidate(path.clone()), &options).expect("first hash");
        fs::write(&path, "short original log plus appended evidence").expect("append");
        let appended =
            fingerprint_log_with_previous_size(&candidate(path), &options, Some(first.size_bytes))
                .expect("second hash");

        assert_eq!(appended.comparison_prefix_len, Some(first.size_bytes));
        assert_eq!(appended.comparison_prefix_hash, Some(first.full_hash));
    }

    #[test]
    fn cancellation_is_checked_between_bounded_fingerprint_reads() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("latest.log");
        fs::write(&path, vec![b'x'; 64 * 1024]).expect("generated log");
        let calls = Cell::new(0_u32);
        let options = FingerprintOptions {
            buffer_bytes: 4 * 1024,
            ..FingerprintOptions::default()
        };

        let error = fingerprint_log_with_previous_size_and_control(
            &candidate(path),
            &options,
            None,
            || {
                let next = calls.get().saturating_add(1);
                calls.set(next);
                next >= 5
            },
        )
        .expect_err("fingerprint must stop cooperatively");

        assert!(matches!(error, crate::scan::ScanError::Cancelled));
        assert_eq!(calls.get(), 5);
    }
}
