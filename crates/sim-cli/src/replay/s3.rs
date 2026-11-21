//! The S3 backend (`MinIO` or any S3-compatible store). Endpoint, bucket and keys come from the
//! environment only (never defaults in the repository):
//!
//! | Variable | Meaning |
//! |---|---|
//! | `OPENTYCOON_S3_ENDPOINT` | `http(s)://host[:port]` |
//! | `OPENTYCOON_S3_BUCKET` | bucket name |
//! | `OPENTYCOON_S3_ACCESS_KEY` | access key id |
//! | `OPENTYCOON_S3_SECRET_KEY` | secret key |
//! | `OPENTYCOON_S3_REGION` | optional; default `us-east-1` |
//!
//! Requests are signed with AWS Signature V4 (presigned, path-style URLs) and sent with a
//! blocking HTTP client: no async runtime.

use std::time::Duration;

use rusty_s3::actions::{DeleteObject, GetObject, HeadObject, ListObjectsV2, PutObject};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use url::Url;

use super::store::{ObjectStore, StoreError, check_key};

/// The environment variables, in the order above.
pub const ENV: [&str; 4] = [
    "OPENTYCOON_S3_ENDPOINT",
    "OPENTYCOON_S3_BUCKET",
    "OPENTYCOON_S3_ACCESS_KEY",
    "OPENTYCOON_S3_SECRET_KEY",
];

/// Optional region variable.
pub const ENV_REGION: &str = "OPENTYCOON_S3_REGION";

/// How long a presigned request stays valid.
const SIGN_FOR: Duration = Duration::from_secs(300);

/// An S3 bucket.
pub struct S3Store {
    bucket: Bucket,
    credentials: Credentials,
    agent: ureq::Agent,
}

impl std::fmt::Debug for S3Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3Store")
            .field("bucket", &self.bucket.name())
            .finish_non_exhaustive()
    }
}

fn backend(what: &str, key: &str, e: impl std::fmt::Display) -> StoreError {
    StoreError::Backend(format!("s3 {what} {key}: {e}"))
}

impl S3Store {
    /// A store for `bucket` at `endpoint` with these keys.
    pub fn new(
        endpoint: &str,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self, StoreError> {
        Self::with_region(endpoint, bucket, access_key, secret_key, "us-east-1")
    }

    fn with_region(
        endpoint: &str,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
        region: &str,
    ) -> Result<Self, StoreError> {
        let url =
            Url::parse(endpoint).map_err(|e| StoreError::Config(format!("{}: {e}", ENV[0])))?;
        let bucket = Bucket::new(url, UrlStyle::Path, bucket.to_owned(), region.to_owned())
            .map_err(|e| StoreError::Config(format!("{}: {e}", ENV[0])))?;
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .build();
        Ok(S3Store {
            bucket,
            credentials: Credentials::new(access_key, secret_key),
            agent: ureq::Agent::new_with_config(config),
        })
    }

    /// The store the environment configures; names any missing variable.
    pub fn from_env() -> Result<Self, StoreError> {
        let mut values = Vec::new();
        let mut missing = Vec::new();
        for name in ENV {
            match std::env::var(name) {
                Ok(v) if !v.is_empty() => values.push(v),
                _ => missing.push(name),
            }
        }
        if !missing.is_empty() {
            return Err(StoreError::Config(format!(
                "the S3 store needs {} in the environment",
                missing.join(", ")
            )));
        }
        let region = std::env::var(ENV_REGION).unwrap_or_else(|_| "us-east-1".into());
        Self::with_region(&values[0], &values[1], &values[2], &values[3], &region)
    }

    fn read(resp: ureq::http::Response<ureq::Body>, key: &str) -> Result<Vec<u8>, StoreError> {
        let mut body = resp.into_body();
        body.with_config()
            .limit(u64::MAX)
            .read_to_vec()
            .map_err(|e| backend("read", key, e))
    }

    fn status(
        what: &str,
        key: &str,
        resp: ureq::http::Response<ureq::Body>,
    ) -> Result<ureq::http::Response<ureq::Body>, StoreError> {
        match resp.status().as_u16() {
            200..=299 => Ok(resp),
            404 => Err(StoreError::NotFound(key.to_owned())),
            code => {
                let text = Self::read(resp, key).unwrap_or_default();
                let text = String::from_utf8_lossy(&text);
                Err(backend(what, key, format!("HTTP {code}: {}", text.trim())))
            }
        }
    }

    fn get_with(&self, key: &str, range: Option<(u64, u64)>) -> Result<Vec<u8>, StoreError> {
        check_key(key)?;
        let url = GetObject::new(&self.bucket, Some(&self.credentials), key).sign(SIGN_FOR);
        let mut req = self.agent.get(url.as_str());
        if let Some((offset, len)) = range {
            if len == 0 {
                return Ok(Vec::new());
            }
            req = req.header("Range", format!("bytes={offset}-{}", offset + len - 1));
        }
        let resp = req.call().map_err(|e| backend("get", key, e))?;
        let bytes = Self::read(Self::status("get", key, resp)?, key)?;
        if let Some((offset, len)) = range {
            if bytes.len() as u64 != len {
                return Err(backend(
                    "get",
                    key,
                    format!("range {offset}+{len} returned {} bytes", bytes.len()),
                ));
            }
        }
        Ok(bytes)
    }
}

impl ObjectStore for S3Store {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), StoreError> {
        check_key(key)?;
        let url = PutObject::new(&self.bucket, Some(&self.credentials), key).sign(SIGN_FOR);
        let resp = self
            .agent
            .put(url.as_str())
            .send(bytes)
            .map_err(|e| backend("put", key, e))?;
        Self::status("put", key, resp).map(drop)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        self.get_with(key, None)
    }

    fn get_range(&self, key: &str, offset: u64, len: u64) -> Result<Vec<u8>, StoreError> {
        self.get_with(key, Some((offset, len)))
    }

    fn exists(&self, key: &str) -> Result<bool, StoreError> {
        check_key(key)?;
        let url = HeadObject::new(&self.bucket, Some(&self.credentials), key).sign(SIGN_FOR);
        let resp = self
            .agent
            .head(url.as_str())
            .call()
            .map_err(|e| backend("head", key, e))?;
        match Self::status("head", key, resp) {
            Ok(_) => Ok(true),
            Err(StoreError::NotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<(String, u64)>, StoreError> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut action = ListObjectsV2::new(&self.bucket, Some(&self.credentials));
            action.with_prefix(prefix);
            if let Some(t) = &token {
                action.with_continuation_token(t.clone());
            }
            let url = action.sign(SIGN_FOR);
            let resp = self
                .agent
                .get(url.as_str())
                .call()
                .map_err(|e| backend("list", prefix, e))?;
            let body = Self::read(Self::status("list", prefix, resp)?, prefix)?;
            let text = String::from_utf8_lossy(&body);
            let parsed =
                ListObjectsV2::parse_response(&text).map_err(|e| backend("list", prefix, e))?;
            out.extend(
                parsed
                    .contents
                    .into_iter()
                    .map(|c| (percent_decode(&c.key), c.size)),
            );
            match parsed.next_continuation_token {
                Some(t) if !t.is_empty() => token = Some(t),
                _ => break,
            }
        }
        out.sort();
        Ok(out)
    }

    fn delete(&self, key: &str) -> Result<(), StoreError> {
        check_key(key)?;
        let url = DeleteObject::new(&self.bucket, Some(&self.credentials), key).sign(SIGN_FOR);
        let resp = self
            .agent
            .delete(url.as_str())
            .call()
            .map_err(|e| backend("delete", key, e))?;
        match Self::status("delete", key, resp) {
            Ok(_) | Err(StoreError::NotFound(_)) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn describe(&self) -> String {
        format!("s3 bucket {}", self.bucket.name())
    }
}

/// Undoes the `encoding-type=url` encoding of listed keys.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                i += 1;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
pub(crate) mod tests {
    //! The S3 backend against a local S3 mock: a tiny path-style HTTP server that checks each
    //! request is SigV4-presigned and serves PUT, GET (with Range), HEAD, DELETE and
    //! `ListObjectsV2` from memory.
    use super::*;
    use std::collections::BTreeMap;
    use std::fmt::Write as _;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    type Objects = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;

    /// Starts the mock on a free port; returns its endpoint and its objects.
    pub(crate) fn mock(bucket: &'static str) -> (String, Objects) {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let objects: Objects = Arc::default();
        let store = objects.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let store = store.clone();
                std::thread::spawn(move || serve(&stream, bucket, &store));
            }
        });
        (endpoint, objects)
    }

    fn decode(s: &str) -> String {
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).unwrap()
    }

    fn serve(stream: &std::net::TcpStream, bucket: &str, store: &Objects) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_owned();
        let target = parts.next().unwrap_or_default().to_owned();
        let mut headers = BTreeMap::new();
        loop {
            let mut h = String::new();
            reader.read_line(&mut h).unwrap();
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_owned());
            }
        }
        let len: usize = headers
            .get("content-length")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; len];
        reader.read_exact(&mut body).unwrap();
        let (path, query) = target.split_once('?').unwrap_or((&target, ""));
        let params: BTreeMap<String, String> = query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_owned(), decode(v)))
            .collect();
        let respond = |status: &str, body: &[u8], extra: &str| {
            let mut s = stream.try_clone().unwrap();
            let head = format!(
                "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n{extra}\r\n",
                body.len()
            );
            s.write_all(head.as_bytes()).unwrap();
            if method != "HEAD" {
                s.write_all(body).unwrap();
            }
            s.flush().unwrap();
        };
        if !params.contains_key("X-Amz-Signature") || !params.contains_key("X-Amz-Credential") {
            respond("403 Forbidden", b"unsigned", "");
            return;
        }
        let prefix = format!("/{bucket}/");
        let key = path
            .strip_prefix(&prefix)
            .map(decode)
            .filter(|k| !k.is_empty());
        let mut objects = store.lock().unwrap();
        match (method.as_str(), key) {
            ("GET", None) if path == format!("/{bucket}") || path == prefix => {
                let p = params.get("prefix").cloned().unwrap_or_default();
                let mut xml = String::from(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult \
                     xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><MaxKeys>1000</MaxKeys>",
                );
                for (k, v) in objects
                    .range(p.clone()..)
                    .take_while(|(k, _)| k.starts_with(&p))
                {
                    let _ = write!(
                        xml,
                        "<Contents><Key>{k}</Key><LastModified>2020-01-01T00:00:00.000Z\
                         </LastModified><ETag>\"e\"</ETag><Size>{}</Size></Contents>",
                        v.len()
                    );
                }
                xml.push_str("</ListBucketResult>");
                respond(
                    "200 OK",
                    xml.as_bytes(),
                    "content-type: application/xml\r\n",
                );
            }
            ("PUT", Some(key)) => {
                objects.insert(key, body);
                respond("200 OK", b"", "");
            }
            ("GET" | "HEAD", Some(key)) => match objects.get(&key) {
                None => respond("404 Not Found", b"NoSuchKey", ""),
                Some(object) => match headers.get("range") {
                    Some(range) => {
                        let (first, last) =
                            range.trim_start_matches("bytes=").split_once('-').unwrap();
                        let (first, last): (usize, usize) =
                            (first.parse().unwrap(), last.parse().unwrap());
                        respond("206 Partial Content", &object[first..=last], "");
                    }
                    None => respond("200 OK", object, ""),
                },
            },
            ("DELETE", Some(key)) => {
                objects.remove(&key);
                respond("204 No Content", b"", "");
            }
            _ => respond("400 Bad Request", b"?", ""),
        }
    }

    #[test]
    fn the_s3_store_conforms_against_a_local_mock() {
        let (endpoint, objects) = mock("bucket");
        let store = S3Store::new(&endpoint, "bucket", "test-key", "test-secret").unwrap();
        crate::replay::store::tests::conformance(&store);
        assert!(
            objects
                .lock()
                .unwrap()
                .contains_key("replays/f/r/p-s000.otr")
        );
    }

    #[test]
    fn the_description_names_the_bucket_but_no_credentials() {
        let (endpoint, _) = mock("bucket");
        let store = S3Store::new(&endpoint, "bucket", "test-key", "test-secret").unwrap();
        let d = store.describe();
        assert!(d.contains("bucket") && !d.contains("test-key") && !d.contains("test-secret"));
    }

    /// Opt-in: the real bucket the environment configures (`OPENTYCOON_S3_IT=1`). Writes,
    /// reads, lists and deletes under `runs/test-it-<pid>/`.
    #[test]
    fn integration_against_a_real_bucket_when_asked() {
        if std::env::var("OPENTYCOON_S3_IT").as_deref() != Ok("1") {
            return;
        }
        let store = S3Store::from_env().unwrap();
        let base = format!("runs/test-it-{}/", std::process::id());
        let key = format!("{base}a.bin");
        store.put(&key, b"0123456789").unwrap();
        assert_eq!(store.get_range(&key, 2, 3).unwrap(), b"234");
        assert_eq!(store.list(&base).unwrap(), vec![(key.clone(), 10)]);
        store.delete(&key).unwrap();
        assert!(!store.exists(&key).unwrap());
    }
}
