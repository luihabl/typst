use std::collections::hash_map::DefaultHasher;
use std::sync::Arc;
use std::{fs, io};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

use native_tls::{Certificate, TlsConnector};
use ecow::{EcoString, eco_format};


use crate::diag::StrResult;
use crate::foundations::Bytes;

pub fn is_remote_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

pub fn load_or_fetch(url: &str) -> StrResult<Bytes> {
    let cache_dir = cache_dir();
    let cache_path = cache_path_for_url(&cache_dir, url);

    // Cache hit.
    if let Ok(data) = fs::read(&cache_path) {
        eprintln!("Using cached image: {url}");
        return Ok(Bytes::new(data));
    }

    // Cache miss.
    let data = download(url)?;

    // Best-effort write. If it fails, still return the downloaded bytes.
    let _ = write_cache_file(&cache_dir, &cache_path, &data);

    Ok(Bytes::new(data))
}

fn cache_dir() -> PathBuf {
    PathBuf::from(".typst-url-cache")
}

fn cache_path_for_url(cache_dir: &Path, url: &str) -> PathBuf {
    let hash = typst_utils::hash128(&url);
    let ext = extension_from_url(url).unwrap_or("");
    let filename = format!("{hash:032x}{ext}");
    cache_dir.join(filename)
}

// fn cache_path_for_url(cache_dir: &Path, url: &str) -> PathBuf {
//     let hash = hash_url(url);
//     let ext = extension_from_url(url).unwrap_or("");
//     let filename = format!("{hash:016x}{ext}");
//     cache_dir.join(filename)
// }

fn hash_url(url: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}

fn extension_from_url(url: &str) -> Option<&str> {
    let path = url.split('?').next()?;
    let segment = path.rsplit('/').next()?;
    let dot = segment.rfind('.')?;
    let ext = &segment[dot..];

    if ext.len() <= 10
        && ext.starts_with('.')
        && ext[1..].chars().all(|c| c.is_ascii_alphanumeric())
    {
        Some(ext)
    } else {
        None
    }
}

fn build_agent(url: &str) -> StrResult<ureq::Agent> {
    let mut builder = ureq::AgentBuilder::new();
    let tls = TlsConnector::builder();

    builder = builder.timeout(std::time::Duration::from_secs(5));

    // Set user agent.
    // builder = builder.user_agent(&self.user_agent);

    // Get the network proxy config from the environment and apply it.
    if let Some(proxy) = env_proxy::for_url_str(url)
        .to_url()
        .and_then(|url| ureq::Proxy::new(url).ok())
    {
        builder = builder.proxy(proxy);
    }

    // Apply a custom CA certificate if present.
    // if let Some(cert) = self.cert() {
    //     tls.add_root_certificate(cert?.clone());
    // }

    // Configure native TLS.
    let Ok(connector) = tls.build().map_err(io::Error::other) else {
        return Err(eco_format!("Unable to create TLS connection"));
    };
    builder = builder.tls_connector(Arc::new(connector));

    Ok(builder.build())
}

fn download(url: &str) -> StrResult<Vec<u8>> {
    let agent = build_agent(url)?;

    eprintln!("File not cached, downloading: {url}");

    let response = agent
        .get(url)
        .call()
        .map_err(|err| format!("failed to download URL ({err})"))?;

    let mut reader = response.into_reader();
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|err| format!("failed to read downloaded data ({err})"))?;

    Ok(buf)
}
// fn download(url: &str) -> StrResult<Vec<u8>> {
//     let response = ureq::get(url)
//         .call()
//         .map_err(|err| format!("failed to download URL ({err})"))?;

//     let mut reader = response.into_reader();
//     let mut buf = Vec::new();
//     reader
//         .read_to_end(&mut buf)
//         .map_err(|err| format!("failed to read downloaded data ({err})"))?;

//     Ok(buf)
// }

fn write_cache_file(cache_dir: &Path, cache_path: &Path, data: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(cache_dir)?;

    let tmp_path = tmp_path(cache_path);
    fs::write(&tmp_path, data)?;
    fs::rename(&tmp_path, cache_path)?;

    Ok(())
}

fn tmp_path(cache_path: &Path) -> PathBuf {
    let mut os = cache_path.as_os_str().to_os_string();
    os.push(".tmp");
    PathBuf::from(os)
}
