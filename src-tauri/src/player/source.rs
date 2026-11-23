use std::path::PathBuf;

/// A local MP4, a public HTTPS MP4, or a user-configured WebDAV media stream.
#[derive(Clone, Debug)]
pub enum MediaSource {
    Local(PathBuf),
    Https(String),
    WebDav {
        url: String,
        auth_header: Option<String>,
    },
}

pub fn webdav_media_url(value: &str) -> Result<String, String> {
    if value.len() > 2048
        || !(value.starts_with("http://") || value.starts_with("https://"))
        || value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
        || value.contains('#')
    {
        return Err("无效的 WebDAV 媒体链接".into());
    }
    let after_scheme = value.split_once("://").map(|(_, r)| r).unwrap_or("");
    let authority = after_scheme.split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return Err("WebDAV 链接无效，请勿在链接中内嵌账号密码".into());
    }
    let clean_path = value.split('?').next().unwrap_or("").to_ascii_lowercase();
    if ![".mp4", ".mkv", ".mov", ".m4v", ".webm"]
        .iter()
        .any(|ext| clean_path.ends_with(ext))
    {
        return Err("不支持的 WebDAV 视频格式".into());
    }
    Ok(value.to_owned())
}

pub fn public_https_url(value: &str) -> Result<String, String> {
    if value.len() > 2048
        || !value.starts_with("https://")
        || value
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
        || value.contains('#')
    {
        return Err("Enter a direct public HTTPS MP4 URL (no fragment or whitespace)".into());
    }
    let authority = value[8..].split(['/', '?']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.contains('[') {
        return Err("A public DNS hostname is required".into());
    }
    let host = if let Some((host, port)) = authority.rsplit_once(':') {
        if port != "443" {
            return Err("Only HTTPS port 443 is supported".into());
        }
        host
    } else {
        authority
    };
    let host = host.to_ascii_lowercase();
    let labels: Vec<_> = host.split('.').collect();
    if labels.len() < 2
        || labels.iter().any(|label| {
            label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
        || !labels
            .last()
            .unwrap()
            .bytes()
            .all(|c| c.is_ascii_alphabetic())
        || labels.last().unwrap().len() < 2
        || ["local", "internal", "localhost", "test", "invalid"].contains(&labels.last().unwrap())
    {
        return Err("A publicly resolvable DNS hostname is required".into());
    }
    if !value
        .split('?')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .ends_with(".mp4")
    {
        return Err("M3 supports direct HTTPS MP4 links only".into());
    }
    Ok(value.to_owned())
}

impl MediaSource {
    pub fn https(value: String) -> Result<Self, String> {
        Ok(Self::Https(public_https_url(&value)?))
    }

    pub fn webdav(url: String, auth_header: Option<String>) -> Result<Self, String> {
        Ok(Self::WebDav {
            url: webdav_media_url(&url)?,
            auth_header,
        })
    }

    pub fn label(&self) -> String {
        match self {
            Self::Local(path) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "local MP4".into()),
            Self::Https(url) => format!(
                "HTTPS · {}",
                url[8..].split(['/', '?']).next().unwrap_or("remote")
            ),
            Self::WebDav { url, .. } => {
                let host = url
                    .split_once("://")
                    .map(|(_, r)| r.split(['/', '?']).next().unwrap_or("webdav"))
                    .unwrap_or("webdav");
                format!("WebDAV · {host}")
            }
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Local(_) => "local",
            Self::Https(_) => "https",
            Self::WebDav { .. } => "webdav",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::public_https_url;

    #[test]
    fn accepts_public_https_and_rejects_invalid_or_private_urls() {
        assert!(public_https_url("https://samplelib.com/lib/preview/mp4/sample-20s.mp4").is_ok());
        for url in [
            "http://cdn.example.com/movie.mp4",
            "https://localhost/movie.mp4",
            "https://192.168.1.23/movie.mp4",
            "https://cdn.example.local/movie.mp4",
            "https://user:pass@cdn.example.com/movie.mp4",
            "https://cdn.example.com:8080/movie.mp4",
            "https://cdn.example.com/movie.m3u8",
            "https://cdn.example.com/movie.mp4#fragment",
            "https://cdn.example.com/evil\r\n.mp4",
        ] {
            assert!(public_https_url(url).is_err(), "expected rejection: {url}");
        }
    }
}
