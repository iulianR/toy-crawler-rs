use db::Db;
use tokio;

mod crawler;
mod db;
mod downloader;
mod parser;
mod server;
mod task;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init()?;

    let db = Db::default();
    server::server(db).await;

    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    pub(crate) fn compare_sorted<T>(mut first: Vec<T>, mut second: Vec<T>)
    where
        T: Ord + std::fmt::Debug,
    {
        first.sort();
        second.sort();

        assert_eq!(first, second)
    }
}
