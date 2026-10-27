//! Write-once store of every response fetched from an external source, written before parsing
//! (C# `FileSystemRawCache`, format-compatible).
//!
//! Layout: `raw/{source}/{endpoint}/{yyyy}/{MM}/{dd}/{HHmmssfff}-{paramsHash}-{node}.json.gz` plus a
//! `.json.meta.json` sidecar, and `latest/{source}/{endpoint}/{paramsHash}.json` pointing at the newest successful
//! entry. Bodies and sidecars are never overwritten; only latest-pointers are.

mod bundle;
mod stj;

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, SecondsFormat, Timelike, Utc};
use flate2::Compression;
use flate2::read::MultiGzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub use bundle::{export_bundle, export_bundle_to, import_bundle, import_bundle_from};

use crate::{Error, Result};

/// Identifies one logical request. `params_key` is a canonical, human-readable parameter string
/// (e.g. "insCode=46348559193224090"); with source and endpoint it forms the cache key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRequest {
    pub source: String,
    pub endpoint: String,
    pub url: String,
    pub params_key: String,
    /// JSON body for POST endpoints; `None` means GET. Must be reflected in `params_key`.
    pub json_body: Option<String>,
    /// Extra request headers some sources require (e.g. fipiran's Referer).
    pub headers: Vec<(String, String)>,
}

impl RawRequest {
    pub fn new(source: impl Into<String>, endpoint: impl Into<String>, url: impl Into<String>, params_key: impl Into<String>) -> Self {
        Self { source: source.into(), endpoint: endpoint.into(), url: url.into(), params_key: params_key.into(), json_body: None, headers: Vec::new() }
    }

    /// First 16 lowercase hex characters of SHA-256(UTF-8 params key).
    pub fn params_hash(&self) -> String {
        params_hash(&self.params_key)
    }
}

pub fn params_hash(params_key: &str) -> String {
    let digest = Sha256::digest(params_key.as_bytes());
    hex::encode(digest)[..16].to_string()
}

/// Metadata of one stored response (the `.meta.json` sidecar and the latest-pointer content).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RawEntry {
    pub source: String,
    pub endpoint: String,
    pub url: String,
    pub params_key: String,
    #[serde(with = "stj::date_time_offset")]
    pub fetched_at_utc: DateTime<Utc>,
    pub node: String,
    pub status_code: i32,
    #[serde(default)]
    pub content_type: Option<String>,
    pub length: i64,
    pub sha256: String,
    /// Path of the compressed body, relative to the cache root, with forward slashes.
    pub body_path: String,
}

impl RawEntry {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status_code)
    }

    fn to_stj_json(&self) -> String {
        let fields: Vec<(&str, stj::JsonValue)> = vec![
            ("Source", stj::JsonValue::Str(&self.source)),
            ("Endpoint", stj::JsonValue::Str(&self.endpoint)),
            ("Url", stj::JsonValue::Str(&self.url)),
            ("ParamsKey", stj::JsonValue::Str(&self.params_key)),
            ("FetchedAtUtc", stj::JsonValue::Raw(stj::format_date_time_offset(self.fetched_at_utc))),
            ("Node", stj::JsonValue::Str(&self.node)),
            ("StatusCode", stj::JsonValue::Raw(self.status_code.to_string())),
            ("ContentType", self.content_type.as_deref().map_or(stj::JsonValue::Null, stj::JsonValue::Str)),
            ("Length", stj::JsonValue::Raw(self.length.to_string())),
            ("Sha256", stj::JsonValue::Str(&self.sha256)),
            ("BodyPath", stj::JsonValue::Str(&self.body_path)),
        ];
        stj::write_indented_object(&fields)
    }
}

/// The file-system raw cache.
#[derive(Debug, Clone)]
pub struct RawCache {
    root: PathBuf,
    node: String,
}

impl RawCache {
    pub fn new(root: impl Into<PathBuf>, node: impl Into<String>) -> Self {
        let root: PathBuf = root.into();
        let root = std::path::absolute(&root).unwrap_or(root);
        Self { root, node: node.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn node(&self) -> &str {
        &self.node
    }

    /// Stores a response (body, sidecar and, for a 2xx, the latest-pointer) and returns its metadata.
    pub fn store(&self, request: &RawRequest, body: &[u8], status_code: i32, content_type: Option<&str>, fetched_at: DateTime<Utc>) -> Result<RawEntry> {
        // .NET DateTimeOffset has 100 ns ticks; keep the same resolution so sidecars round-trip identically.
        let utc = truncate_to_ticks(fetched_at);
        let node = sanitize(&self.node);
        let directory = PathBuf::from("raw")
            .join(sanitize(&request.source))
            .join(sanitize(&request.endpoint))
            .join(utc.format("%Y").to_string())
            .join(utc.format("%m").to_string())
            .join(utc.format("%d").to_string());
        fs::create_dir_all(self.root.join(&directory))?;

        let base_name = format!("{}{:03}-{}-{}", utc.format("%H%M%S"), utc.nanosecond() / 1_000_000 % 1000, request.params_hash(), node);
        let body_relative = self.write_once(&directory, &base_name, body)?;
        let body_path = to_forward_slashes(&body_relative);

        let entry = RawEntry {
            source: request.source.clone(),
            endpoint: request.endpoint.clone(),
            url: request.url.clone(),
            params_key: request.params_key.clone(),
            fetched_at_utc: utc,
            node: self.node.clone(),
            status_code,
            content_type: content_type.map(str::to_string),
            length: body.len() as i64,
            sha256: hex::encode(Sha256::digest(body)),
            body_path,
        };

        // Path.ChangeExtension(".../x.json.gz", null) + ".meta.json" = ".../x.json.meta.json".
        let meta_relative = meta_path_for(&body_relative);
        let mut meta = OpenOptions::new().write(true).create_new(true).open(self.root.join(meta_relative))?;
        meta.write_all(entry.to_stj_json().as_bytes())?;
        drop(meta);

        if entry.is_success() {
            self.write_latest_pointer(&entry)?;
        }
        Ok(entry)
    }

    /// The most recent successful response for this request, or `None`.
    pub fn get_latest(&self, request: &RawRequest) -> Result<Option<RawEntry>> {
        self.get_latest_by_hash(&request.source, &request.endpoint, &request.params_hash())
    }

    fn get_latest_by_hash(&self, source: &str, endpoint: &str, hash: &str) -> Result<Option<RawEntry>> {
        let path = self.latest_pointer_path(source, endpoint, hash);
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(strip_bom(&bytes))?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// True when a latest-pointer exists for the request (no parsing).
    pub fn has_latest(&self, request: &RawRequest) -> bool {
        self.latest_pointer_path(&request.source, &request.endpoint, &request.params_hash()).is_file()
    }

    /// Decompresses a stored body.
    pub fn read_body(&self, entry: &RawEntry) -> Result<Vec<u8>> {
        let file = File::open(self.root.join(&entry.body_path))?;
        let mut decoder = MultiGzDecoder::new(std::io::BufReader::new(file));
        let mut buffer = Vec::with_capacity(usize::try_from(entry.length).unwrap_or(0).max(1024));
        decoder.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    /// The body of the latest successful response, if any.
    pub fn read_latest(&self, request: &RawRequest) -> Result<Option<Vec<u8>>> {
        match self.get_latest(request)? {
            Some(entry) => Ok(Some(self.read_body(&entry)?)),
            None => Ok(None),
        }
    }

    /// Every stored entry of one endpoint, ordered by sidecar path.
    pub fn list(&self, source: &str, endpoint: &str) -> Result<Vec<RawEntry>> {
        let directory = self.root.join("raw").join(sanitize(source)).join(sanitize(endpoint));
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let mut files = Vec::new();
        collect_files(&directory, &mut files)?;
        let mut metas: Vec<PathBuf> = files.into_iter().filter(|p| p.to_string_lossy().ends_with(".meta.json")).collect();
        metas.sort_by(|a, b| ordinal_cmp(&a.to_string_lossy(), &b.to_string_lossy()));
        metas.iter().map(|p| -> Result<RawEntry> { Ok(serde_json::from_slice(strip_bom(&fs::read(p)?))?) }).collect()
    }

    /// Points "latest" at an entry that arrived from another node, if it is newer than the current one.
    pub fn refresh_latest_pointer(&self, entry: &RawEntry) -> Result<()> {
        if entry.is_success() { self.write_latest_pointer(entry) } else { Ok(()) }
    }

    fn write_once(&self, directory: &Path, base_name: &str, body: &[u8]) -> Result<PathBuf> {
        let temp = self.root.join(directory).join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
        {
            let file = OpenOptions::new().write(true).create_new(true).open(&temp)?;
            let mut encoder = GzEncoder::new(std::io::BufWriter::new(file), Compression::new(6));
            encoder.write_all(body)?;
            encoder.finish()?.flush()?;
        }

        // Two writes for the same request in the same millisecond on the same node get a numeric suffix.
        for attempt in 0..=100 {
            let name = if attempt == 0 { base_name.to_string() } else { format!("{base_name}-{attempt}") };
            let relative = directory.join(format!("{name}.json.gz"));
            let target = self.root.join(&relative);
            if target.exists() {
                continue;
            }
            match rename_no_replace(&temp, &target) {
                Ok(()) => return Ok(relative),
                Err(_) if target.exists() => continue,
                Err(e) => {
                    let _ = fs::remove_file(&temp);
                    return Err(e.into());
                }
            }
        }
        let _ = fs::remove_file(&temp);
        Err(Error::InvalidOperation(format!("Could not find a free name for {base_name} in {}.", directory.display())))
    }

    fn write_latest_pointer(&self, entry: &RawEntry) -> Result<()> {
        let hash = params_hash(&entry.params_key);
        let path = self.latest_pointer_path(&entry.source, &entry.endpoint, &hash);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Never move the pointer backwards in time (e.g. when a node replays older responses during sync).
        if let Ok(Some(current)) = self.get_latest_by_hash(&entry.source, &entry.endpoint, &hash)
            && current.fetched_at_utc > entry.fetched_at_utc
        {
            return Ok(());
        }

        let temp = PathBuf::from(format!("{}.{}.tmp", path.display(), uuid::Uuid::new_v4().simple()));
        fs::write(&temp, entry.to_stj_json())?;
        replace_file(&temp, &path)?;
        Ok(())
    }

    fn latest_pointer_path(&self, source: &str, endpoint: &str, hash: &str) -> PathBuf {
        self.root.join("latest").join(sanitize(source)).join(sanitize(endpoint)).join(format!("{hash}.json"))
    }
}

fn meta_path_for(body_relative: &Path) -> PathBuf {
    let text = body_relative.to_string_lossy();
    let without_gz = text.strip_suffix(".gz").map(str::to_string).unwrap_or_else(|| text.to_string());
    PathBuf::from(format!("{without_gz}.meta.json"))
}

fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

fn truncate_to_ticks(value: DateTime<Utc>) -> DateTime<Utc> {
    let nanos = value.nanosecond() % 1_000_000_000;
    value.with_nanosecond(nanos / 100 * 100).unwrap_or(value)
}

/// .NET `Path.GetInvalidFileNameChars()` on Windows, plus both slashes, become '_'.
pub fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            '"' | '<' | '>' | '|' | ':' | '*' | '?' | '\\' | '/' => '_',
            c if (c as u32) < 32 => '_',
            c => c,
        })
        .collect()
}

/// Ordinal (UTF-16 code unit) string comparison, as .NET `StringComparer.Ordinal`.
pub fn ordinal_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

pub(crate) fn collect_files(directory: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Moves `from` to `to` failing if `to` exists (`File.Move(..., overwrite: false)`). Only this process writes
/// sidecars for its own node name, so the check-then-rename window is not a practical race.
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "target exists"));
    }
    fs::rename(from, to)
}

/// Atomically replaces `to` with `from` (`File.Move(..., overwrite: true)`).
fn replace_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(from);
            Err(e)
        }
    }
}

/// Relative cache paths that `export_bundle` and the HTTP sync route share.
pub(crate) fn relative_to(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root).ok().map(to_forward_slashes)
}

/// Index of latest pointers keyed by (source, endpoint, hash), used by bulk readers.
pub type LatestIndex = BTreeMap<(String, String, String), RawEntry>;

/// Formats a timestamp the way the sidecars do (`2026-10-17T13:10:31.9482894+00:00`).
pub fn format_fetched_at(value: DateTime<Utc>) -> String {
    stj::format_date_time_offset(value)
}

/// JSON string escaping exactly as System.Text.Json's default encoder writes it.
pub fn escape_json_default(value: &str, out: &mut String) {
    stj::escape_into(value, out);
}

/// RFC 3339 helper for logs.
pub fn rfc3339(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn request() -> RawRequest {
        RawRequest::new("tsetmc", "ClosingPrice/GetClosingPriceInfo", "https://cdn.tsetmc.com/api/ClosingPrice/GetClosingPriceInfo/1", "insCode=1")
    }

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 17, 9, 0, 0).unwrap()
    }

    #[test]
    fn stored_body_round_trips_and_becomes_latest() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RawCache::new(dir.path(), "test-node");
        let entry = cache.store(&request(), b"{\"a\":1}", 200, Some("application/json"), t0()).unwrap();
        let latest = cache.get_latest(&request()).unwrap().unwrap();
        assert_eq!(entry.body_path, latest.body_path);
        assert_eq!("test-node", latest.node);
        assert_eq!(b"{\"a\":1}".to_vec(), cache.read_body(&latest).unwrap());
        assert_eq!(format!("raw/tsetmc/ClosingPrice_GetClosingPriceInfo/2026/10/17/090000000-{}-test-node.json.gz", params_hash("insCode=1")), entry.body_path);
        assert!(dir.path().join(entry.body_path.replace(".json.gz", ".json.meta.json")).is_file());
    }

    #[test]
    fn every_fetch_is_kept_and_errors_never_become_latest() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RawCache::new(dir.path(), "test-node");
        cache.store(&request(), b"first", 200, None, t0()).unwrap();
        cache.store(&request(), b"second", 200, None, t0() + chrono::Duration::minutes(1)).unwrap();
        cache.store(&request(), b"blocked", 403, None, t0() + chrono::Duration::minutes(2)).unwrap();
        assert_eq!(3, cache.list("tsetmc", "ClosingPrice/GetClosingPriceInfo").unwrap().len());
        let latest = cache.get_latest(&request()).unwrap().unwrap();
        assert_eq!(b"second".to_vec(), cache.read_body(&latest).unwrap());
    }

    #[test]
    fn replaying_an_older_response_does_not_move_latest_backwards() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RawCache::new(dir.path(), "test-node");
        cache.store(&request(), b"new", 200, None, t0() + chrono::Duration::hours(1)).unwrap();
        cache.store(&request(), b"old", 200, None, t0()).unwrap();
        let latest = cache.get_latest(&request()).unwrap().unwrap();
        assert_eq!(b"new".to_vec(), cache.read_body(&latest).unwrap());
    }

    #[test]
    fn same_millisecond_writes_do_not_overwrite_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let cache = RawCache::new(dir.path(), "test-node");
        let a = cache.store(&request(), b"a", 200, None, t0()).unwrap();
        let b = cache.store(&request(), b"b", 200, None, t0()).unwrap();
        assert_ne!(a.body_path, b.body_path);
        assert_eq!(b"a".to_vec(), cache.read_body(&a).unwrap());
        assert_eq!(b"b".to_vec(), cache.read_body(&b).unwrap());
    }

    #[test]
    fn params_hash_matches_the_dotnet_cache() {
        // Pointers written by the .NET collector: latest/codal/search_v1_categories/e3b0c44298fc1c14.json (empty key)
        // and latest/tsetmc/ClosingPrice_GetClosingPriceInfo/1985e238e30ac1c4.json (insCode=25244329144808274).
        assert_eq!("e3b0c44298fc1c14", params_hash(""));
        assert_eq!("1985e238e30ac1c4", params_hash("insCode=25244329144808274"));
    }

    #[test]
    fn sidecar_json_matches_system_text_json() {
        let entry = RawEntry {
            source: "tsetmc".into(),
            endpoint: "Instrument/GetInstrumentSearch".into(),
            url: "https://x/y?a=1&b=2".into(),
            params_key: "text=فولاد".into(),
            fetched_at_utc: Utc.with_ymd_and_hms(2026, 10, 17, 13, 10, 31).unwrap() + chrono::Duration::nanoseconds(948_289_400),
            node: "dev-local".into(),
            status_code: 200,
            content_type: None,
            length: 5,
            sha256: "ab".into(),
            body_path: "raw/x.json.gz".into(),
        };
        let json = entry.to_stj_json();
        assert!(json.contains("\"ParamsKey\": \"text=\\u0641\\u0648\\u0644\\u0627\\u062F\""), "{json}");
        assert!(json.contains("\"Url\": \"https://x/y?a=1\\u0026b=2\""), "{json}");
        assert!(json.contains("\"FetchedAtUtc\": \"2026-10-17T13:10:31.9482894+00:00\""), "{json}");
        assert!(json.contains("\"ContentType\": null"), "{json}");
        let parsed: RawEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, parsed);
    }
}
