use std::convert::Infallible;

use super::{CountOptions, CountResult, CrawlersDb, Domain, ListOptions};
use crate::{crawler::Crawler, db::Db};
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::{info, log::warn};
use warp::http::StatusCode;

#[derive(Debug, Serialize)]
struct Error {
    error: String,
}

/// Handle a crawl request. Spawn a new crawler if one doesn't already exist for the given domain.
/// If a crawl request is already in progress, just return 200 OK.
/// We should probably respond with a Location: /domains?domain=<domain> header as well, but leave that
/// for the future.
pub(super) async fn crawl(
    domain: Domain,
    shutdown: broadcast::Sender<()>,
    db: Db,
    spawned_crawlers: CrawlersDb,
) -> Result<impl warp::Reply, Infallible> {
    let mut cdb = spawned_crawlers.lock().await;
    if cdb.contains(&domain.domain) {
        return Ok(warp::reply::with_status(
            warp::reply::json(&"{}".to_string()),
            StatusCode::OK,
        ));
    }

    let mut crawler = match Crawler::new(domain.domain.clone()) {
        Ok(crawler) => crawler,
        Err(e) => {
            warn!("Crawler error: {}", e);
            let error = Error {
                error: e.to_string(),
            };
            return Ok(warp::reply::with_status(
                warp::reply::json(&error),
                StatusCode::INTERNAL_SERVER_ERROR,
            ));
        }
    };

    cdb.insert(domain.domain);

    let cdb = spawned_crawlers.clone();
    tokio::spawn(async move {
        crawler.crawl(db, shutdown).await;

        // Remove ourselves from crawler db
        let mut cdb = cdb.lock().await;
        cdb.remove(crawler.domain());
        info!("Crawler done");
    });

    Ok(warp::reply::with_status(
        warp::reply::json(&"{}".to_string()),
        StatusCode::OK,
    ))
}

/// Handle a list request.
/// Retrieve the currently crawled unique URLs from the database.
/// Respond with `404 Not Found` if the domain in query has not been crawled.
pub(super) async fn list(options: ListOptions, db: Db) -> Result<impl warp::Reply, Infallible> {
    let urls = match db.unique_urls_for_domain(&options.domain) {
        Ok(urls) => urls,
        Err(e) => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&Error {
                    error: e.to_string(),
                }),
                StatusCode::NOT_FOUND,
            ));
        }
    };

    Ok(warp::reply::with_status(
        warp::reply::json(&urls),
        StatusCode::OK,
    ))
}

/// Count the occurences for the URL in query.
/// Respond with 404 Not Found if the domain part of the URL has not been crawled.
pub(super) async fn count(options: CountOptions, db: Db) -> Result<impl warp::Reply, Infallible> {
    let count = match db.url_count_for_domain(&options.url) {
        Ok(count) => count,
        Err(e) => {
            return Ok(warp::reply::with_status(
                warp::reply::json(&Error {
                    error: e.to_string(),
                }),
                StatusCode::NOT_FOUND,
            ));
        }
    };

    let count_result = CountResult {
        url: options.url,
        count,
    };

    Ok(warp::reply::with_status(
        warp::reply::json(&count_result),
        StatusCode::OK,
    ))
}
