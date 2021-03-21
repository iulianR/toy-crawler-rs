mod filters;
mod handlers;

use std::{collections::HashSet, sync::Arc};

use serde::{Deserialize, Serialize};

use tokio::{signal::{self, unix::SignalKind}, sync::{broadcast, Mutex}};
use tracing::info;
use url::Url;

use warp::Filter;

use crate::db::Db;

/// Database of running crawlers.
type CrawlersDb = Arc<Mutex<HashSet<Url>>>;

/// GET query options for list request.
#[derive(Debug, Deserialize)]
struct ListOptions {
    domain: Url,
}

/// GET query options for count request.
/// Similar to ListOptions, but it has a different key name.
#[derive(Debug, Deserialize)]
struct CountOptions {
    url: Url,
}

/// Used to parse JSON body of the POST /domains request
#[derive(Debug, Deserialize)]
struct Domain {
    domain: Url,
}

/// Result returned for the count GET request.
#[derive(Debug, Serialize, Deserialize)]
pub struct CountResult {
    url: Url,
    count: usize,
}

/// Create the webserver and start serving the routes.
pub(crate) async fn server(db: Db) {
    let spawned_crawlers = CrawlersDb::default();
    let (shutdown_tx, mut shutdown_rx) = broadcast::channel(1);

    let routes = filters::crawl(
        shutdown_tx.clone(),
        db.clone(),
        Arc::clone(&spawned_crawlers),
    )
    .or(filters::list(db.clone()))
    .or(filters::count(db));

    tokio::spawn(async move {
        let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();
        let mut sigquit = tokio::signal::unix::signal(SignalKind::quit()).unwrap();
        let kill = signal::ctrl_c();

        let send_kill = move || {
            info!("Received shutdown signal. Sending shutdown command.");
            shutdown_tx.send(()).unwrap();
        };
        tokio::select! {
            _ = sigterm.recv() => send_kill(),
            _ = sigquit.recv() => send_kill(),
            _ = kill => send_kill(),
        }
    });

    let (_addr, server) = warp::serve(routes).bind_with_graceful_shutdown(([0, 0, 0, 0], 3030), async move {
        shutdown_rx.recv().await.ok();
    });

    server.await
}
