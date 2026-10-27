//! Moves raw-cache entries between nodes as a single zip file (C# `RawCacheBundle`, same layout).
//!
//! Entries are write-once and uniquely named by time, request hash and node, so importing is a union: existing
//! files are skipped and latest-pointers only move forward.

use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::{Component, Path};

use chrono::NaiveDate;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::{RawCache, RawEntry, collect_files, ordinal_cmp, relative_to, strip_bom};
use crate::{Error, Result};

/// Writes every file under `raw/` (optionally only days on or after `since`) into a zip at `bundle_path`.
pub fn export_bundle(cache: &RawCache, bundle_path: &Path, since: Option<NaiveDate>) -> Result<usize> {
    if let Some(parent) = std::path::absolute(bundle_path)?.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(bundle_path)?;
    export_bundle_to(cache, std::io::BufWriter::new(file), since)
}

/// [`export_bundle`] into any seekable writer (the HTTP sync route streams into memory).
pub fn export_bundle_to<W: Write + Seek>(cache: &RawCache, writer: W, since: Option<NaiveDate>) -> Result<usize> {
    let mut zip = ZipWriter::new(writer);
    let raw_root = cache.root().join("raw");
    let mut count = 0;

    if raw_root.is_dir() {
        let mut files = Vec::new();
        collect_files(&raw_root, &mut files)?;
        let mut relatives: Vec<(String, std::path::PathBuf)> = files.into_iter().filter_map(|f| relative_to(cache.root(), &f).map(|r| (r, f))).collect();
        relatives.sort_by(|a, b| ordinal_cmp(&a.1.to_string_lossy(), &b.1.to_string_lossy()));

        for (relative, file) in relatives {
            if relative.ends_with(".tmp") || since.is_some_and(|s| date_from_path(&relative).is_some_and(|d| d < s)) {
                continue;
            }

            // Bodies are already gzip-compressed; only the small JSON sidecars benefit from compression.
            let method = if relative.ends_with(".gz") { CompressionMethod::Stored } else { CompressionMethod::Deflated };
            let options = SimpleFileOptions::default().compression_method(method);
            zip.start_file(relative.as_str(), options)?;
            let mut source = File::open(&file)?;
            std::io::copy(&mut source, &mut zip)?;
            count += 1;
        }
    }

    zip.finish()?;
    Ok(count)
}

/// Imports a bundle: a union of files, then latest-pointers refreshed in fetch order. Returns (imported, skipped).
pub fn import_bundle(cache: &RawCache, bundle_path: &Path) -> Result<(usize, usize)> {
    import_bundle_from(cache, File::open(bundle_path)?)
}

/// [`import_bundle`] from any seekable reader.
pub fn import_bundle_from<R: Read + Seek>(cache: &RawCache, reader: R) -> Result<(usize, usize)> {
    let mut archive = ZipArchive::new(reader)?;
    let (mut imported, mut skipped) = (0, 0);
    let mut new_entries: Vec<RawEntry> = Vec::new();
    fs::create_dir_all(cache.root())?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        if !name.starts_with("raw/") || name.ends_with('/') {
            continue;
        }

        let relative = Path::new(&name);
        if relative.components().any(|c| !matches!(c, Component::Normal(_))) {
            return Err(Error::Format(format!("Bundle entry escapes the cache root: {name}")));
        }
        let target = cache.root().join(relative);
        if target.exists() {
            skipped += 1;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
        entry.read_to_end(&mut bytes)?;
        let temp = target.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
        fs::write(&temp, &bytes)?;
        if target.exists() {
            let _ = fs::remove_file(&temp);
            skipped += 1;
            continue;
        }
        fs::rename(&temp, &target)?;
        imported += 1;

        if name.ends_with(".meta.json")
            && let Ok(raw_entry) = serde_json::from_slice::<RawEntry>(strip_bom(&bytes))
        {
            new_entries.push(raw_entry);
        }
    }

    new_entries.retain(RawEntry::is_success);
    new_entries.sort_by_key(|e| e.fetched_at_utc);
    for raw_entry in &new_entries {
        cache.refresh_latest_pointer(raw_entry)?;
    }

    Ok((imported, skipped))
}

/// `raw/{source}/{endpoint}/{yyyy}/{MM}/{dd}/{file}` → the date.
fn date_from_path(relative: &str) -> Option<NaiveDate> {
    let parts: Vec<&str> = relative.split('/').collect();
    if parts.len() < 7 {
        return None;
    }
    let y = parts[3].parse().ok()?;
    let m = parts[4].parse().ok()?;
    let d = parts[5].parse().ok()?;
    NaiveDate::from_ymd_opt(y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw_cache::RawRequest;
    use chrono::{TimeZone, Utc};

    #[test]
    fn import_is_a_union_and_moves_latest_forward_only() {
        let dirs = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let request = RawRequest::new("tsetmc", "X", "https://example/x", "k=1");
        let iran = RawCache::new(dirs.0.path(), "iran-1");
        let abroad = RawCache::new(dirs.1.path(), "eu-1");
        iran.store(&request, b"old", 200, None, Utc.with_ymd_and_hms(2026, 10, 1, 9, 0, 0).unwrap()).unwrap();
        abroad.store(&request, b"new", 200, None, Utc.with_ymd_and_hms(2026, 10, 2, 9, 0, 0).unwrap()).unwrap();

        let bundle = dirs.2.path().join("bundle.zip");
        export_bundle(&abroad, &bundle, None).unwrap();
        let (imported, skipped) = import_bundle(&iran, &bundle).unwrap();
        let again = import_bundle(&iran, &bundle).unwrap();

        assert_eq!(2, imported);
        assert_eq!(0, skipped);
        assert_eq!(2, again.1);
        let latest = iran.get_latest(&request).unwrap().unwrap();
        assert_eq!(b"new".to_vec(), iran.read_body(&latest).unwrap());
    }

    #[test]
    fn export_since_skips_older_days() {
        let dirs = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let cache = RawCache::new(dirs.0.path(), "n");
        let request = RawRequest::new("s", "e", "u", "p");
        cache.store(&request, b"a", 200, None, Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap()).unwrap();
        cache.store(&request, b"b", 200, None, Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()).unwrap();
        let count = export_bundle(&cache, &dirs.1.path().join("b.zip"), NaiveDate::from_ymd_opt(2026, 9, 15)).unwrap();
        assert_eq!(2, count);
    }
}
