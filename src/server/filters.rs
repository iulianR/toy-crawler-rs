use tokio::sync::broadcast;
use warp::Filter;

use super::{handlers, CountOptions, CrawlersDb, ListOptions};
use crate::db::Db;

fn with_db(db: Db) -> impl Filter<Extract = (Db,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

/// POST /domains with JSON body
pub(super) fn crawl(
    shutdown: broadcast::Sender<()>,
    db: Db,
    spawned_crawlers: CrawlersDb,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("domains")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::content_length_limit(1024))
        .and(warp::body::json())
        .and(warp::any().map(move || shutdown.clone()))
        .and(with_db(db))
        .and(warp::any().map(move || spawned_crawlers.clone()))
        .and_then(handlers::crawl)
}

/// GET /domains?domain=<url>
pub(super) fn list(
    db: Db,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("domains")
        .and(warp::get())
        .and(warp::query::<ListOptions>())
        .and(with_db(db))
        .and_then(handlers::list)
}

/// GET /domains/urls?url=<url>
pub(super) fn count(
    db: Db,
) -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    warp::path!("domains" / "urls")
        .and(warp::get())
        .and(warp::query::<CountOptions>())
        .and(with_db(db))
        .and_then(handlers::count)
}

#[cfg(test)]
mod tests {
    use crate::db::Db;

    use crate::server::{CountResult, CrawlersDb};
    use tokio::sync::broadcast;
    use url::Url;
    use warp::http::StatusCode;

    fn filled_db(domain: &Url) -> Db {
        let db = Db::default();
        db.visit(domain.join("/foo").unwrap()).unwrap();
        db.visit(domain.join("/foo").unwrap()).unwrap();
        db.visit(domain.join("/foo").unwrap()).unwrap();
        db.visit(domain.join("/foo").unwrap()).unwrap();
        db.visit(domain.join("/bar").unwrap()).unwrap();
        db.visit(domain.join("/bar").unwrap()).unwrap();

        db
    }

    #[tokio::test]
    async fn test_crawl() {
        let db = Db::default();
        let cdb = CrawlersDb::default();

        let (tx, _rx) = broadcast::channel(1);
        let filter = super::crawl(tx, db, cdb.clone());

        let response = warp::test::request()
            .method("POST")
            .body(r#"{"domain":"https://example.com"}"#)
            .path("/domains")
            .reply(&filter)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = warp::test::request()
            .method("POST")
            .body(r#"{"domain":"https://example.com"}"#)
            .path("/domains")
            .reply(&filter)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let response = warp::test::request()
            .method("POST")
            .body(r#"{"domain":"abc"}"#)
            .path("/domains")
            .reply(&filter)
            .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_empty_db() {
        let db = Db::default();
        let filter = super::list(db);
        let response = warp::test::request().path("/domains").reply(&filter).await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let response = warp::test::request()
            .path("/domains?domain=https://example.com")
            .reply(&filter)
            .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_with_db() {
        let domain = Url::parse("https://example.com").unwrap();

        let db = filled_db(&domain);
        let filter = super::list(db.clone());

        let response = warp::test::request()
            .path(&format!("/domains?domain={}", domain))
            .reply(&filter)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let urls: Vec<Url> = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(db.unique_urls_for_domain(&domain).unwrap(), urls);
    }

    #[tokio::test]
    async fn test_count() {
        let domain = Url::parse("https://example.com").unwrap();
        let url = Url::parse("https://example.com/foo").unwrap();

        let db = filled_db(&domain);
        let filter = super::count(db.clone());

        let response = warp::test::request()
            .path(&format!("/domains/urls?url={}", url))
            .reply(&filter)
            .await;

        assert_eq!(response.status(), StatusCode::OK);

        let count_result: CountResult = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(db.url_count_for_domain(&url).unwrap(), count_result.count);

        let response = warp::test::request()
            .path(r#"/domains/urls?url=https://who.com"#)
            .reply(&filter)
            .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
