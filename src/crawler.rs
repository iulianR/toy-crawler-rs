use std::borrow::Cow;

use futures::{stream::SelectAll, StreamExt};
use robotstxt::DefaultMatcher;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{error, info, trace};

use crate::{db::Db, downloader::Downloader, task::Task};
use url::Url;

/// Whether an URL should further visited or not.
#[derive(Debug, PartialEq, Eq)]
enum ProcessResult {
    ShouldVisit,
    ShouldNotVisit,
}

/// A crawler that only works for the given domain.
/// It tries to respect `robots.txt` if one exists.
#[derive(Debug)]
pub(crate) struct Crawler {
    domain: Url,
    downloader: Downloader,
    robots_txt: String,
}

impl Crawler {
    /// Create a new crawler for the given `domain`.
    pub(crate) fn new(domain: Url) -> anyhow::Result<Self> {
        let downloader = Downloader::new()?;

        Ok(Self {
            domain,
            downloader,
            robots_txt: String::from(""),
        })
    }

    /// A reference to the crawler's domain.
    pub(crate) fn domain(&self) -> &Url {
        &self.domain
    }

    /// Start crawling the domain associated with this crawler and populate the `db` with found URLs.
    pub(crate) async fn crawl(&mut self, db: Db, shutdown: broadcast::Sender<()>) {
        // Try to download the `robots.txt` if it exists.
        let robots_url = self.domain.join("robots.txt").unwrap();
        let page = self.downloader.download(&robots_url).await.ok();
        if let Some(page) = page {
            self.robots_txt = page;
        }

        // Give each async task a `Sender`. When all tasks end, the senders are dropped,
        // and the crawler has finished work.
        let mut urls = SelectAll::new();
        let (tx, rx) = mpsc::unbounded_channel();

        // Seed the crawler with the initial domain URL.
        tx.send(self.domain.clone()).unwrap();
        drop(tx);
        let rx = UnboundedReceiverStream::new(rx);
        urls.push(rx);

        let (shutdown_complete_tx, mut shutdown_complete_rx) = broadcast::channel(1);

        let mut shutdown_receiver = shutdown.subscribe();

        // Process incoming URLs as long as there are still spawned async tasks that are sending data.
        loop {
            tokio::select! {
                url = urls.next() => {
                    if let Some(url) = url {
                        // Further spawn a task for each URL we are supposed to visit.
                        if self.process_url(&url, &db) == ProcessResult::ShouldVisit {
                            // Send the Sender to the task, register the receiver stream.
                            let (tx, rx) = mpsc::unbounded_channel();
                            let rx = UnboundedReceiverStream::new(rx);
                            urls.push(rx);

                            let shutdown_complete = shutdown_complete_tx.clone();

                            // Create a download + parse task
                            let mut task = Task {
                                downloader: self.downloader.clone(),
                                domain: self.domain.clone(),
                                url,
                                tx,
                                notify_shutdown: shutdown.subscribe(),
                                _shutdown_complete: shutdown_complete
                            };

                            tokio::spawn(async move {
                                task.run().await
                            });
                        }
                    } else {
                        break;
                    }
                }
                _ = shutdown_receiver.recv() => {
                    info!("Shutting down");
                    break;
                }
            }
        }

        drop(shutdown_complete_tx);

        let _ = shutdown_complete_rx.recv().await;
    }

    /// Processes the URL by registering it to the database and checking wether it should be
    /// visited or it was already visited by a previous crawler/from a diferent path.
    fn process_url(&mut self, url: &Url, db: &Db) -> ProcessResult {
        info!("Processing url {}", url);

        // Restrict to current domain.
        if url.domain() != self.domain.domain() {
            trace!("Different domain");
            return ProcessResult::ShouldNotVisit;
        }

        // Respect robots.txt
        let mut matcher = DefaultMatcher::default();
        if !matcher.allowed_by_robots(&self.robots_txt, vec!["*"], &url.as_str()) {
            trace!("Not allowed by robots");
            return ProcessResult::ShouldNotVisit;
        }

        let is_first_visit = match db.is_first_visit(&url) {
            Ok(o) => o,
            Err(e) => {
                error!("Skipping {}, DB Error: {}", url, e);
                return ProcessResult::ShouldNotVisit;
            }
        };

        // Register visit to database
        match db.visit(Cow::Borrowed(url)) {
            Ok(_) => {}
            Err(e) => {
                error!("Skipping {}, DB Error: {}", url, e);
                return ProcessResult::ShouldNotVisit;
            }
        }

        // Do not visit a second time
        if is_first_visit {
            ProcessResult::ShouldVisit
        } else {
            ProcessResult::ShouldNotVisit
        }
    }
}

#[cfg(test)]
mod tests {
    use mockito::mock;
    use tokio::sync::broadcast;

    use crate::db::Db;

    use super::Crawler;
    use crate::tests::compare_sorted;

    #[tokio::test]
    async fn crawl_domain() {
        let _m = mock("GET", "/")
            .with_status(200)
            .with_header("content-type", "text/html")
            .with_body(
                r#"
                <a href="/foo">foo</a>
                <a href="/bar">bar</a>
            "#,
            )
            .create();

        let _m = mock("GET", "/foo")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("body")
            .create();

        let db = Db::default();
        let domain = url::Url::parse(&mockito::server_url()).unwrap();
        let mut crawler = Crawler::new(domain.clone()).unwrap();

        let (tx, _rx) = broadcast::channel(1);
        crawler.crawl(db.clone(), tx).await;

        let expected = vec![
            domain.clone(),
            domain.join("/foo").unwrap(),
            domain.join("/bar").unwrap(),
        ];
        let unique_urls = db.unique_urls_for_domain(&domain).unwrap();

        compare_sorted(unique_urls, expected);
    }
}
