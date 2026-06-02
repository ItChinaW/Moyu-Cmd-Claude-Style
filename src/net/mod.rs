use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderValue};
use std::time::Duration;

pub const HOST: &str = "https://www.zhihu.com";
pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Build the header set for a signed Zhihu request.
pub fn zhihu_headers(cookie: &str, x_zse_96: &str) -> Result<HeaderMap> {
    let mut h = HeaderMap::new();
    h.insert("cookie", HeaderValue::from_str(cookie).context("cookie header")?);
    h.insert("x-zse-93", HeaderValue::from_static("101_3_3.0"));
    h.insert("x-zse-96", HeaderValue::from_str(x_zse_96).context("x-zse-96 header")?);
    h.insert("x-api-version", HeaderValue::from_static("3.0.91"));
    h.insert("user-agent", HeaderValue::from_static(USER_AGENT));
    Ok(h)
}

#[derive(Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Result<Self> {
        let inner = reqwest::Client::builder()
            .gzip(true)
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(20))
            .build()
            .context("build reqwest client")?;
        Ok(Self { inner })
    }

    /// GET `{HOST}{path_with_query}` with signed Zhihu headers. Returns body text.
    pub async fn signed_get(
        &self,
        path_with_query: &str,
        cookie: &str,
        x_zse_96: &str,
    ) -> Result<String> {
        let url = format!("{HOST}{path_with_query}");
        let headers = zhihu_headers(cookie, x_zse_96)?;
        let resp = self.inner.get(&url).headers(headers).send().await
            .context("send request")?;
        let status = resp.status();
        let body = resp.text().await.context("read body")?;
        if !status.is_success() {
            anyhow::bail!("HTTP {status}: {}", body.chars().take(200).collect::<String>());
        }
        Ok(body)
    }

    /// GET an arbitrary URL (e.g. an image) and return the raw bytes. Sends a Zhihu
    /// referer so hotlink-protected image CDNs (zhimg.com) serve the file.
    pub async fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let resp = self
            .inner
            .get(url)
            .header("referer", HOST)
            .header("user-agent", USER_AGENT)
            .send()
            .await
            .context("send image request")?;
        let status = resp.status();
        let bytes = resp.bytes().await.context("read image bytes")?;
        if !status.is_success() {
            anyhow::bail!("HTTP {status} fetching image");
        }
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_include_signature_and_cookie() {
        let h = zhihu_headers("d_c0=abc", "2.0_demo").unwrap();
        assert_eq!(h.get("x-zse-96").unwrap(), "2.0_demo");
        assert_eq!(h.get("x-zse-93").unwrap(), "101_3_3.0");
        assert_eq!(h.get("cookie").unwrap(), "d_c0=abc");
    }
}
