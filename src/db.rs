use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{Arc, RwLock},
};
use thiserror::Error;
use url::{Position, Url};

type UniqueUrlsMap = HashMap<String, usize>;
type DomainsMap = HashMap<String, UniqueUrlsMap>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DbError {
    #[error("URL does not contain domain")]
    DoesNotContainDomain,
    #[error("Domain does not exist")]
    DomainDoesNotExist,
}

/// Thread-safe in-memory database. For each domain, it stores a `HashMap` of unique URLs and the number of occurences.
/// To reduce use of system resources, story only the part after the domain URL for each unique URL and build
/// it on the spot when the list is required.
/// Future work: this database can also be split into an in-memory cache part and a database stored on disk (PostgreSQL?).
#[derive(Debug, Default, Clone)]
pub struct Db(Arc<RwLock<DomainsMap>>);

impl Db {
    /// Returns `true` if the `url` does not exist yet in the database.
    pub(crate) fn is_first_visit(&self, url: &Url) -> Result<bool, DbError> {
        let db = self.0.read().unwrap();
        let after_domain = &url[Position::BeforePath..];

        Ok(db
            .get(parse_domain(url)?.as_ref())
            .and_then(|urls| urls.get(after_domain))
            .is_none())
    }

    /// Increase the number of occurences of `url` for its domain.
    pub(crate) fn visit(&self, url: Url) -> Result<(), DbError> {
        let mut db = self.0.write().unwrap();
        let after_domain = &url[Position::BeforePath..];

        db.entry(parse_domain(&url)?.into_owned())
            .or_default()
            .entry(after_domain.to_string())
            .and_modify(|count| *count += 1)
            .or_insert(1);

        Ok(())
    }

    /// Create a list of unique URLs for a `domain`.
    /// This function will combine the domain part with the relative URLs for the domain to build a
    /// list of valid and complete URLs.
    pub(crate) fn unique_urls_for_domain(&self, domain: &Url) -> Result<Vec<Url>, DbError> {
        let db = self.0.read().unwrap();

        Ok(db
            .get(parse_domain(&domain)?.as_ref())
            .ok_or(DbError::DomainDoesNotExist)?
            .keys()
            .map(|url| domain.join(&url))
            .filter_map(|r| r.ok())
            .collect())
    }

    /// Get the count of occurences for the given `url`.
    pub(crate) fn url_count_for_domain(&self, url: &Url) -> Result<usize, DbError> {
        let db = self.0.read().unwrap();

        Ok(db
            .get(parse_domain(&url)?.as_ref())
            .ok_or(DbError::DomainDoesNotExist)?
            .get(&url[Position::BeforePath..])
            .copied()
            .unwrap_or(0usize))
    }
}

/// Mockito uses https://127.0.0.1 as URL for its paths. Compute the domain using this function,
/// so that we parse the host part instead of the domain part when testing.
fn parse_domain(url: &Url) -> Result<Cow<str>, DbError> {
    #[cfg(not(test))]
    let url = url.domain().ok_or(DbError::DoesNotContainDomain)?;

    #[cfg(test)]
    let url = {
        let url = url.host().ok_or(DbError::DoesNotContainDomain)?;
        url.to_string()
    };

    Ok(url.into())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr;
    use url::Url;

    use super::{Db, DbError};
    use crate::tests::compare_sorted;

    #[test]
    fn test_unique_urls_list_for_domain() -> anyhow::Result<()> {
        let db = Db::default();
        let domain_one = Url::from_str("https://example.com")?;
        let domain_two = Url::from_str("https://foobar.com")?;

        assert!(db.is_first_visit(&domain_one.join("/foo/test/1")?)?);

        db.visit(domain_one.join("/foo/test/1")?)?;

        assert_eq!(db.is_first_visit(&domain_one.join("/foo/test/1")?)?, false);

        db.visit(domain_one.join("/foo/test/1")?)?;
        db.visit(domain_one.join("/bar/test/1")?)?;
        db.visit(domain_one.join("/bar/test/1")?)?;
        db.visit(domain_one.join("/bar/test/1")?)?;

        db.visit(domain_two.join("/foo/test/2")?)?;
        db.visit(domain_two.join("/foo/test/2")?)?;
        db.visit(domain_two.join("/bar/test/2")?)?;
        db.visit(domain_two.join("/bar/test/2")?)?;
        db.visit(domain_two.join("/bar/test/2")?)?;

        let expected_one = vec![
            domain_one.join("/foo/test/1")?,
            domain_one.join("/bar/test/1")?,
        ];
        let unique_urls = db.unique_urls_for_domain(&domain_one)?;

        compare_sorted(expected_one, unique_urls);

        let expected_two = vec![
            domain_two.join("/foo/test/2")?,
            domain_two.join("/bar/test/2")?,
        ];
        let unique_urls = db.unique_urls_for_domain(&domain_two)?;

        compare_sorted(expected_two, unique_urls);

        Ok(())
    }

    #[test]
    fn test_url_count_for_domain() -> anyhow::Result<()> {
        let db = Db::default();
        let domain = Url::from_str("https://example.com")?;

        db.visit(domain.join("/foo")?)?;
        db.visit(domain.join("/foo")?)?;
        db.visit(domain.join("/bar")?)?;
        db.visit(domain.join("/bar")?)?;
        db.visit(domain.join("/bar")?)?;

        assert_eq!(db.url_count_for_domain(&domain.join("/foo")?)?, 2);
        assert_eq!(db.url_count_for_domain(&domain.join("/bar")?)?, 3);

        assert_eq!(db.url_count_for_domain(&domain.join("/baz")?)?, 0);

        let non_existant_domain = Url::from_str("https://who.com")?;
        assert_eq!(
            db.url_count_for_domain(&non_existant_domain.join("/foo")?),
            Err(DbError::DomainDoesNotExist)
        );

        Ok(())
    }
}
