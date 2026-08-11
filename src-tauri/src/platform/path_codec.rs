use std::{ffi::OsString, path::Path};

pub fn native_path_key(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    }
}

pub fn path_from_native_key(key: &[u8]) -> Option<std::path::PathBuf> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Some(OsString::from_vec(key.to_vec()).into())
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;

        if !key.len().is_multiple_of(2) {
            return None;
        }
        let wide = key
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        Some(OsString::from_wide(&wide).into())
    }
}

pub fn stable_location_id(path_key: &[u8]) -> String {
    let digest = blake3::hash(path_key).to_hex().to_string();
    format!("loc_{}", &digest[..24])
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{native_path_key, path_from_native_key, stable_location_id};

    #[test]
    fn native_path_keys_round_trip() {
        let path = PathBuf::from("Minecraft data").join("世界").join("logs");
        let key = native_path_key(&path);
        assert_eq!(path_from_native_key(&key), Some(path));
    }

    #[test]
    fn stable_ids_do_not_change_for_the_same_key() {
        assert_eq!(stable_location_id(b"same"), stable_location_id(b"same"));
        assert_ne!(stable_location_id(b"same"), stable_location_id(b"other"));
    }
}
