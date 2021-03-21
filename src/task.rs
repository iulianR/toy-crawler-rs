use crate::{downloader::Downloader, parser::Parser};
use tokio::sync::{broadcast, mpsc};
use tracing::{error, info, warn};
use url::Url;

/// Task representing one URL to download and parse.
#[derive(Debug)]
pub(crate) struct Task {
    pub(crate) downloader: Downloader,
    pub(crate) domain: Url,
    pub(crate) url: Url,
    // Channel where the task can send found URLs to.
    pub(crate) tx: mpsc::UnboundedSender<Url>,
    // Channel use to receive shutdown notifications.
    pub(crate) notify_shutdown: broadcast::Receiver<()>,
    // Dropped when task is done. Will notify crawler so it can gracefully shutdown.
    pub(crate) _shutdown_complete: broadcast::Sender<()>,
}

impl Task {
    pub(crate) async fn run(&mut self) {
        tokio::select! {
            response = self.downloader.download(&self.url) => {
                match response {
                    Ok(response) => {
                        for url in Parser::new(&response).extract_urls() {
                            if let Some(url) = build_absolute_url(&self.domain, url) {
                                match self.tx.send(url) {
                                    Ok(_) => {}
                                    Err(_) => {
                                        info!("Failed to send. Receiver has probably shut down");
                                    }
                                }
                            }
                        }
                    },
                    Err(_) => error!("Failed to download url: {}", self.url),
                }
            }
            _ = self.notify_shutdown.recv() => {
                info!("Shutting down");
            }
        }

        // match self.downloader.download(&self.url).await {
        //     Ok(response) => {
        //         for url in Parser::new(&response).extract_urls() {
        //             if let Some(url) = build_absolute_url(&self.domain, url) {
        //                 match self.tx.send(url) {
        //                     Ok(_) => {}
        //                     Err(_) => {
        //                         info!("Failed to send. Receiver has probably shut down");
        //                     }
        //                 }
        //             }
        //         }
        //     }
        //     Err(_) => {
        //         error!("Failed to download url: {}", self.url);
        //     }
        // }
    }
}

/// Combine the `domain` URL that we are crawling with a relative path to build
/// an absolute url.
fn build_absolute_url(domain: &Url, url: &str) -> Option<Url> {
    let url = match url::Url::parse(&url) {
        Ok(url) => url,
        Err(e) => match e {
            url::ParseError::RelativeUrlWithoutBase => domain.join(&url).unwrap(),
            _ => {
                warn!("Unknown url: {}", url);
                return None;
            }
        },
    };

    Some(url)
}
