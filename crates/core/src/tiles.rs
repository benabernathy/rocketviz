//! Map tile proxy: the app's 3D globe and 2D map load imagery through this instead of from the providers directly.
//!
//! One place to switch providers, keep API keys out of the web UI, and cache tiles according to each provider's
//! terms: public-domain imagery (USGS) is kept indefinitely (and serves offline), while commercial imagery (Esri)
//! is cached only as long as the provider's own Cache-Control allows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// How long a provider's tiles may be kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CachePolicy {
    /// Public-domain imagery: keep indefinitely; serve from cache when offline.
    Persistent,
    /// Respect the provider's Cache-Control max-age (default 1 day); never serve expired tiles.
    HttpHeaders,
}

/// An imagery source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// Tile URL template with `{z}`, `{x}`, `{y}`, used when no key is configured.
    pub url: String,
    /// Template used when the user has a key for this provider; `{key}` is substituted.
    #[serde(default)]
    pub keyed_url: Option<String>,
    pub attribution: String,
    pub max_zoom: u8,
    pub cache: CachePolicy,
    /// Human description of coverage, e.g. "United States".
    #[serde(default)]
    pub coverage: Option<String>,
}

/// The providers rocketviz ships with.
pub fn builtin_providers() -> Vec<Provider> {
    vec![
        Provider {
            id: "esri".into(),
            name: "Esri World Imagery".into(),
            // Until an ArcGIS Location Platform key is configured, the long-standing public endpoint.
            url: "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}".into(),
            keyed_url: Some(
                "https://ibasemaps-api.arcgis.com/arcgis/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}?token={key}".into(),
            ),
            attribution: "Imagery © Esri, Maxar, Earthstar Geographics".into(),
            max_zoom: 19,
            cache: CachePolicy::HttpHeaders,
            coverage: None,
        },
        Provider {
            id: "usgs".into(),
            name: "USGS Imagery (public domain)".into(),
            url: "https://basemap.nationalmap.gov/arcgis/rest/services/USGSImageryOnly/MapServer/tile/{z}/{y}/{x}".into(),
            keyed_url: None,
            attribution: "USDA, USGS The National Map: Orthoimagery".into(),
            max_zoom: 19,
            cache: CachePolicy::Persistent,
            coverage: Some("United States".into()),
        },
    ]
}

#[derive(Debug, thiserror::Error)]
pub enum TileError {
    #[error("unknown imagery provider {0:?}")]
    UnknownProvider(String),
    #[error("tile {z}/{x}/{y} is outside the map")]
    OutOfRange { z: u8, x: u32, y: u32 },
    #[error("{provider} returned {status}: {message}")]
    Upstream { provider: String, status: u16, message: String },
    #[error("couldn't reach {provider}: {message}")]
    Network { provider: String, message: String },
}

/// One tile image.
#[derive(Debug, Clone)]
pub struct Tile {
    pub bytes: Vec<u8>,
    pub content_type: String,
    /// Seconds the web view may cache it in memory.
    pub max_age: u64,
    pub from_cache: bool,
}

pub struct TileProxy {
    client: reqwest::Client,
    cache_dir: PathBuf,
    providers: HashMap<String, Provider>,
    keys: RwLock<HashMap<String, String>>,
}

const DEFAULT_MAX_AGE: u64 = 86_400;

impl TileProxy {
    pub fn new(cache_dir: impl Into<PathBuf>, providers: Vec<Provider>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!(
                "rocketviz/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/benabernathy/rocketviz)"
            ))
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP client");
        Self {
            client,
            cache_dir: cache_dir.into(),
            providers: providers.into_iter().map(|p| (p.id.clone(), p)).collect(),
            keys: RwLock::new(HashMap::new()),
        }
    }

    pub fn providers(&self) -> Vec<&Provider> {
        let mut v: Vec<_> = self.providers.values().collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    /// Sets (or clears, with an empty key) the API key for a provider.
    pub fn set_key(&self, provider: &str, key: &str) {
        let mut keys = self.keys.write().unwrap();
        if key.trim().is_empty() {
            keys.remove(provider);
        } else {
            keys.insert(provider.to_string(), key.trim().to_string());
        }
    }

    pub fn has_key(&self, provider: &str) -> bool {
        self.keys.read().unwrap().contains_key(provider)
    }

    /// Tries a key by fetching the world tile from the provider's keyed endpoint (never from the cache), so the
    /// settings screen can reject a mistyped or unauthorized key before saving it.
    pub async fn check_key(&self, provider: &str, key: &str) -> Result<(), TileError> {
        let p = self.providers.get(provider).ok_or_else(|| TileError::UnknownProvider(provider.into()))?;
        let Some(template) = &p.keyed_url else { return Ok(()) };
        let url = template.replace("{z}", "0").replace("{x}", "0").replace("{y}", "0").replace("{key}", key.trim());
        self.fetch(p, &url).await.map(|_| ())
    }

    /// Fetches a tile, from the cache when its policy allows.
    pub async fn tile(&self, provider: &str, z: u8, x: u32, y: u32) -> Result<Tile, TileError> {
        let p = self.providers.get(provider).ok_or_else(|| TileError::UnknownProvider(provider.into()))?;
        if z > 24 || x >= (1u32 << z) || y >= (1u32 << z) {
            return Err(TileError::OutOfRange { z, x, y });
        }
        let key = self.keys.read().unwrap().get(provider).cloned();
        // Keyed and keyless tiles may differ; cache them separately.
        let cache_ns = if key.is_some() { format!("{provider}-keyed") } else { provider.to_string() };
        let path = self.cache_dir.join(&cache_ns).join(z.to_string()).join(x.to_string()).join(y.to_string());
        let cached = read_cached(&path);
        let now = now_secs();
        if let Some((tile, expires)) = &cached {
            if expires.is_none_or(|e| e > now) {
                let max_age = expires.map_or(DEFAULT_MAX_AGE, |e| e - now);
                return Ok(Tile { max_age, from_cache: true, ..tile.clone() });
            }
        }

        let template = match (&key, &p.keyed_url) {
            (Some(_), Some(k)) => k.as_str(),
            _ => p.url.as_str(),
        };
        let url = template
            .replace("{z}", &z.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string())
            .replace("{key}", key.as_deref().unwrap_or(""));
        match self.fetch(p, &url).await {
            Ok((tile, max_age)) => {
                let expires = match p.cache {
                    CachePolicy::Persistent => None,
                    CachePolicy::HttpHeaders => Some(now + max_age),
                };
                if p.cache == CachePolicy::Persistent || max_age > 0 {
                    if let Err(e) = write_cached(&path, &tile, expires) {
                        tracing::warn!("couldn't cache tile {}: {e}", path.display());
                    }
                }
                Ok(Tile { max_age: max_age.max(60), ..tile })
            }
            // Offline: public-domain tiles may be served past their freshness.
            Err(e @ TileError::Network { .. }) if p.cache == CachePolicy::Persistent => match cached {
                Some((tile, _)) => Ok(Tile { max_age: 60, from_cache: true, ..tile }),
                None => Err(e),
            },
            Err(e) => Err(e),
        }
    }

    async fn fetch(&self, p: &Provider, url: &str) -> Result<(Tile, u64), TileError> {
        let network = |e: reqwest::Error| TileError::Network { provider: p.name.clone(), message: e.to_string() };
        let r = self.client.get(url).send().await.map_err(network)?;
        let status = r.status().as_u16();
        let content_type = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let max_age = r
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_max_age)
            .unwrap_or(DEFAULT_MAX_AGE);
        let bytes = r.bytes().await.map_err(network)?.to_vec();
        // Providers report errors (e.g. Esri's "Token Required") as 200 with a JSON or text body.
        if status != 200 || !content_type.starts_with("image/") {
            let message = String::from_utf8_lossy(&bytes[..bytes.len().min(300)]).into_owned();
            return Err(TileError::Upstream { provider: p.name.clone(), status, message });
        }
        Ok((Tile { bytes, content_type, max_age, from_cache: false }, max_age))
    }
}

fn parse_max_age(cache_control: &str) -> Option<u64> {
    let lower = cache_control.to_ascii_lowercase();
    if lower.contains("no-store") {
        return Some(0);
    }
    lower.split(',').find_map(|d| d.trim().strip_prefix("max-age=")).and_then(|v| v.trim().parse().ok())
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Cache entry: `<path>` holds the image, `<path>.meta` holds "content-type\nexpires" (expires empty = never).
fn read_cached(path: &Path) -> Option<(Tile, Option<u64>)> {
    let meta = std::fs::read_to_string(path.with_extension("meta")).ok()?;
    let mut lines = meta.lines();
    let content_type = lines.next()?.to_string();
    let expires = lines.next().and_then(|e| e.trim().parse().ok());
    let bytes = std::fs::read(path).ok()?;
    Some((Tile { bytes, content_type, max_age: 0, from_cache: true }, expires))
}

fn write_cached(path: &Path, tile: &Tile, expires: Option<u64>) -> std::io::Result<()> {
    std::fs::create_dir_all(path.parent().expect("tile path has a parent"))?;
    // Write to temp files and rename, so a concurrent reader never sees a half-written tile.
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, &tile.bytes)?;
    std::fs::rename(&tmp, path)?;
    let meta = format!("{}\n{}", tile.content_type, expires.map(|e| e.to_string()).unwrap_or_default());
    std::fs::write(&tmp, meta)?;
    std::fs::rename(&tmp, path.with_extension("meta"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A tiny HTTP server: `respond(path) -> (status, content-type, cache-control, body)`. Counts requests.
    async fn mock(
        respond: impl Fn(&str) -> (u16, &'static str, &'static str, Vec<u8>) + Send + Sync + 'static,
    ) -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let (h, respond) = (hits.clone(), Arc::new(respond));
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                let (h, respond) = (h.clone(), respond.clone());
                tokio::spawn(async move {
                    let mut buf = vec![0; 4096];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                    h.fetch_add(1, Ordering::SeqCst);
                    let (status, ct, cc, body) = respond(&path);
                    let head = format!("HTTP/1.1 {status} X\r\ncontent-type: {ct}\r\ncache-control: {cc}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
                    let _ = s.write_all(head.as_bytes()).await;
                    let _ = s.write_all(&body).await;
                });
            }
        });
        (format!("http://{addr}"), hits)
    }

    fn provider(base: &str, cache: CachePolicy) -> Provider {
        Provider {
            id: "p".into(),
            name: "Test".into(),
            url: format!("{base}/tile/{{z}}/{{y}}/{{x}}"),
            keyed_url: Some(format!("{base}/keyed/{{z}}/{{y}}/{{x}}?token={{key}}")),
            attribution: "test".into(),
            max_zoom: 19,
            cache,
            coverage: None,
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rv-tiles-{name}-{}-{}", std::process::id(), now_secs()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[tokio::test]
    async fn fetches_caches_and_serves_from_cache() {
        let (base, hits) = mock(|_| (200, "image/jpeg", "max-age=3600", b"JPEG".to_vec())).await;
        let proxy = TileProxy::new(temp_dir("cache"), vec![provider(&base, CachePolicy::HttpHeaders)]);
        let a = proxy.tile("p", 16, 13706, 25056).await.unwrap();
        assert_eq!((a.bytes.as_slice(), a.from_cache), (&b"JPEG"[..], false));
        let b = proxy.tile("p", 16, 13706, 25056).await.unwrap();
        assert!(b.from_cache && b.max_age <= 3600);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn respects_no_store_and_expiry() {
        let (base, hits) = mock(|_| (200, "image/png", "no-store", b"PNG".to_vec())).await;
        let proxy = TileProxy::new(temp_dir("nostore"), vec![provider(&base, CachePolicy::HttpHeaders)]);
        proxy.tile("p", 3, 1, 1).await.unwrap();
        proxy.tile("p", 3, 1, 1).await.unwrap();
        assert_eq!(hits.load(Ordering::SeqCst), 2, "no-store tiles aren't cached");
    }

    #[tokio::test]
    async fn uses_the_keyed_url_and_rejects_error_bodies() {
        let (base, _) = mock(|path| {
            if path.contains("token=SECRET") {
                (200, "image/jpeg", "max-age=60", b"OK".to_vec())
            } else {
                (200, "text/plain", "max-age=0", br#"{"error":{"code":499,"message":"Token Required."}}"#.to_vec())
            }
        })
        .await;
        let mut p = provider(&base, CachePolicy::HttpHeaders);
        p.url = p.keyed_url.clone().unwrap().replace("{key}", "");
        let proxy = TileProxy::new(temp_dir("keyed"), vec![p]);
        let err = proxy.tile("p", 2, 1, 1).await.unwrap_err();
        assert!(matches!(&err, TileError::Upstream { message, .. } if message.contains("Token Required")), "{err}");
        proxy.set_key("p", "SECRET");
        assert_eq!(proxy.tile("p", 2, 1, 1).await.unwrap().bytes, b"OK");
        assert!(proxy.check_key("p", " SECRET ").await.is_ok());
        assert!(matches!(proxy.check_key("p", "WRONG").await, Err(TileError::Upstream { .. })));
    }

    #[tokio::test]
    async fn persistent_tiles_serve_offline() {
        let (base, _) = mock(|_| (200, "image/jpeg", "max-age=1", b"USGS".to_vec())).await;
        let dir = temp_dir("offline");
        let proxy = TileProxy::new(&dir, vec![provider(&base, CachePolicy::Persistent)]);
        proxy.tile("p", 10, 5, 5).await.unwrap();
        // Same cache, provider now unreachable.
        let offline = TileProxy::new(&dir, vec![provider("http://127.0.0.1:9", CachePolicy::Persistent)]);
        let t = offline.tile("p", 10, 5, 5).await.unwrap();
        assert!(t.from_cache && t.bytes == b"USGS");
    }

    #[tokio::test]
    async fn rejects_unknown_providers_and_bad_tiles() {
        let proxy = TileProxy::new(temp_dir("bad"), builtin_providers());
        assert!(matches!(proxy.tile("nope", 1, 0, 0).await, Err(TileError::UnknownProvider(_))));
        assert!(matches!(proxy.tile("usgs", 2, 4, 0).await, Err(TileError::OutOfRange { .. })));
    }

    #[test]
    fn parses_cache_control() {
        assert_eq!(parse_max_age("public, max-age=86400"), Some(86400));
        assert_eq!(parse_max_age("Max-Age=0, must-revalidate"), Some(0));
        assert_eq!(parse_max_age("no-store"), Some(0));
        assert_eq!(parse_max_age("public"), None);
    }
}
