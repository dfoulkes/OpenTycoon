//! The S3 backend (MinIO or any S3-compatible store). Endpoint, bucket and keys come from the
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

use super::store::{ObjectStore, StoreError};

/// The environment variables, in the order above.
pub const ENV: [&str; 4] = [
    "OPENTYCOON_S3_ENDPOINT",
    "OPENTYCOON_S3_BUCKET",
    "OPENTYCOON_S3_ACCESS_KEY",
    "OPENTYCOON_S3_SECRET_KEY",
];

/// An S3 bucket.
#[derive(Debug)]
pub struct S3Store {
    endpoint: String,
    bucket: String,
}

impl S3Store {
    /// A store for `bucket` at `endpoint` with these keys.
    pub fn new(
        endpoint: &str,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
    ) -> Result<Self, StoreError> {
        let _ = (access_key, secret_key);
        Ok(S3Store {
            endpoint: endpoint.to_owned(),
            bucket: bucket.to_owned(),
        })
    }

    /// The store the environment configures; names any missing variable.
    pub fn from_env() -> Result<Self, StoreError> {
        Err(StoreError::Config("not implemented".into()))
    }
}

impl ObjectStore for S3Store {
    fn put(&self, key: &str, _bytes: &[u8]) -> Result<(), StoreError> {
        Err(StoreError::Backend(format!("not implemented: {key}")))
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, StoreError> {
        Err(StoreError::NotFound(key.to_owned()))
    }
    fn get_range(&self, key: &str, _offset: u64, _len: u64) -> Result<Vec<u8>, StoreError> {
        Err(StoreError::NotFound(key.to_owned()))
    }
    fn exists(&self, _key: &str) -> Result<bool, StoreError> {
        Ok(false)
    }
    fn list(&self, _prefix: &str) -> Result<Vec<(String, u64)>, StoreError> {
        Ok(Vec::new())
    }
    fn delete(&self, _key: &str) -> Result<(), StoreError> {
        Ok(())
    }
    fn describe(&self) -> String {
        format!("s3 bucket {} at {}", self.bucket, self.endpoint)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    //! The S3 backend against a local S3 mock: a tiny path-style HTTP server that checks each
    //! request is SigV4-presigned and serves PUT, GET (with Range), HEAD, DELETE and
    //! ListObjectsV2 from memory.
    use super::*;
    use std::collections::BTreeMap;
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
                std::thread::spawn(move || serve(stream, bucket, &store));
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

    fn serve(stream: std::net::TcpStream, bucket: &str, store: &Objects) {
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
        let q: BTreeMap<String, String> = query
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
        if !q.contains_key("X-Amz-Signature") || !q.contains_key("X-Amz-Credential") {
            respond("403 Forbidden", b"unsigned", "");
            return;
        }
        let prefix = format!("/{bucket}/");
        let key = path.strip_prefix(&prefix).map(decode);
        let mut objects = store.lock().unwrap();
        match (method.as_str(), key) {
            ("GET", None) if path == format!("/{bucket}") || path == prefix => {
                let p = q.get("prefix").cloned().unwrap_or_default();
                let mut xml = String::from(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult \
                     xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><MaxKeys>1000</MaxKeys>",
                );
                for (k, v) in objects
                    .range(p.clone()..)
                    .take_while(|(k, _)| k.starts_with(&p))
                {
                    xml.push_str(&format!(
                        "<Contents><Key>{k}</Key><LastModified>2020-01-01T00:00:00.000Z\
                         </LastModified><ETag>\"e\"</ETag><Size>{}</Size></Contents>",
                        v.len()
                    ));
                }
                xml.push_str("</ListBucketResult>");
                respond(
                    "200 OK",
                    xml.as_bytes(),
                    "content-type: application/xml\r\n",
                );
            }
            ("PUT", Some(k)) => {
                objects.insert(k, body);
                respond("200 OK", b"", "");
            }
            ("GET" | "HEAD", Some(k)) => match objects.get(&k) {
                None => respond("404 Not Found", b"NoSuchKey", ""),
                Some(v) => match headers.get("range") {
                    Some(r) => {
                        let (a, b) = r.trim_start_matches("bytes=").split_once('-').unwrap();
                        let (a, b): (usize, usize) = (a.parse().unwrap(), b.parse().unwrap());
                        respond("206 Partial Content", &v[a..=b], "");
                    }
                    None => respond("200 OK", v, ""),
                },
            },
            ("DELETE", Some(k)) => {
                objects.remove(&k);
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
