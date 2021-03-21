use url::Url;

/// The internal HTTP client is already wrapper in `Arc`, so that means that the
/// downloader is cheap to clone.
#[derive(Debug, Clone)]
pub(crate) struct Downloader(reqwest::Client);

impl Downloader {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let client = reqwest::ClientBuilder::new().build()?;
        Ok(Self(client))
    }

    pub(crate) async fn download(&self, url: &Url) -> anyhow::Result<String> {
        Ok(self.0.get(url.as_str()).send().await?.text().await?)
    }
}
