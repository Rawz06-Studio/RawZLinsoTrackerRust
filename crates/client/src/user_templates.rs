//! Templates the user drops in `Documents/RawZLinsoTracker/templates`: as
//! LinSoTracker's `.template` zip files, or as folders.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::SystemTime;

const EXTENSION: &str = "template";

enum Location {
    Folder(PathBuf),
    Archive(PathBuf),
}

/// A template's files, keyed by their path inside the archive.
type Files = Arc<HashMap<String, Vec<u8>>>;

/// Archives already read, with the time they were changed at.
type Cache = Mutex<HashMap<PathBuf, (Option<SystemTime>, Files)>>;

/// Archives already read, until they change on disk.
fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn locate(dir: &Path, name: &str) -> Option<Location> {
    let folder = dir.join(name);
    if folder.join("tracker.json").is_file() {
        return Some(Location::Folder(folder));
    }
    let archive = dir.join(format!("{name}.{EXTENSION}"));
    archive.is_file().then_some(Location::Archive(archive))
}

/// Names of the templates found in `dir`, sorted.
pub fn names_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = if path.is_dir() {
                path.join("tracker.json")
                    .is_file()
                    .then(|| path.file_name())?
            } else if path.extension().is_some_and(|ext| ext == EXTENSION) {
                path.file_stem()
            } else {
                None
            }?;
            name.to_str().map(str::to_owned)
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

pub fn names() -> Vec<String> {
    crate::storage::templates_dir()
        .map(|dir| names_in(&dir))
        .unwrap_or_default()
}

fn read_archive(path: &Path) -> Result<Files, String> {
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if let Some((when, files)) = cache().lock().ok().and_then(|c| c.get(path).cloned())
        && when == modified
    {
        return Ok(files);
    }
    let error = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let file = std::fs::File::open(path).map_err(|e| error(&e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| error(&e))?;
    let mut files = HashMap::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| error(&e))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().map_err(|e| error(&e))?.replace('\\', "/");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| error(&e))?;
        files.insert(name, bytes);
    }
    // An archive made from a folder has the template one level down.
    if !files.contains_key("tracker.json")
        && let Some(prefix) = files
            .keys()
            .find_map(|name| {
                name.strip_suffix("tracker.json")
                    .filter(|p| p.ends_with('/'))
            })
            .map(str::to_owned)
    {
        files = files
            .into_iter()
            .filter_map(|(name, bytes)| Some((name.strip_prefix(&prefix)?.to_owned(), bytes)))
            .collect();
    }
    let files = Arc::new(files);
    if let Ok(mut cache) = cache().lock() {
        cache.insert(path.to_owned(), (modified, files.clone()));
    }
    Ok(files)
}

/// `file` of the user's template `name`, or `None` if there is no such
/// template, so the built-in one is used.
pub fn read_in(dir: &Path, name: &str, file: &str) -> Option<Result<Vec<u8>, String>> {
    Some(match locate(dir, name)? {
        Location::Folder(folder) => {
            let path = folder.join(file);
            std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
        }
        Location::Archive(archive) => read_archive(&archive).and_then(|files| {
            files
                .get(file)
                .cloned()
                .ok_or_else(|| format!("{}: no {file}", archive.display()))
        }),
    })
}

pub fn read(name: &str, file: &str) -> Option<Result<Vec<u8>, String>> {
    read_in(&crate::storage::templates_dir()?, name, file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rawz-templates-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, bytes) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn finds_folders_and_archives() {
        let dir = temp_dir("names");
        std::fs::create_dir_all(dir.join("mine")).unwrap();
        std::fs::write(dir.join("mine/tracker.json"), "[]").unwrap();
        std::fs::create_dir_all(dir.join("empty")).unwrap();
        write_zip(&dir.join("other.template"), &[("tracker.json", b"[]")]);
        std::fs::write(dir.join("README.txt"), "").unwrap();
        assert_eq!(names_in(&dir), ["mine", "other"]);
    }

    #[test]
    fn reads_files_of_an_archive_even_one_folder_down() {
        let dir = temp_dir("read");
        write_zip(
            &dir.join("flat.template"),
            &[("tracker.json", b"flat"), ("icon.png", b"png")],
        );
        write_zip(
            &dir.join("nested.template"),
            &[("nested/tracker.json", b"nested")],
        );
        assert_eq!(read_in(&dir, "flat", "icon.png"), Some(Ok(b"png".to_vec())));
        assert_eq!(
            read_in(&dir, "nested", "tracker.json"),
            Some(Ok(b"nested".to_vec()))
        );
        assert!(matches!(read_in(&dir, "flat", "missing.png"), Some(Err(_))));
        assert_eq!(read_in(&dir, "absent", "tracker.json"), None);
    }
}
