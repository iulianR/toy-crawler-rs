use scraper::{Html, Selector};

/// HTML parser
#[derive(Debug)]
pub(crate) struct Parser {
    selector: Selector,
    html: Html,
}

impl Parser {
    /// Create a new parser for `html`.
    pub(crate) fn new(html: &str) -> Self {
        Self {
            selector: Selector::parse("a").unwrap(),
            html: Html::parse_document(html),
        }
    }

    /// Returns an iterator over the URLs in the parsed HTML.
    pub(crate) fn extract_urls(&self) -> impl Iterator<Item = &str> {
        self.html
            .select(&self.selector)
            .filter_map(|el| el.value().attr("href"))
    }
}

#[cfg(test)]
mod tests {
    use super::Parser;

    #[test]
    fn test_basic() {
        let html = r#"
<html>
    <head>
        <title>HTML!</title>
    </head>
    <body>
        <h1>HTML</h1>
        <a href="/foo">Go</a>
        <a href="https://example.com/bar>Go absolute</a>
    </body>
</html>
"#;

        let parser = Parser::new(html);
        let mut expected = vec!["/foo", "https://example.com/bar"];
        let mut urls: Vec<&str> = parser.extract_urls().collect();
        assert_eq!(urls.sort(), expected.sort());
    }
}
