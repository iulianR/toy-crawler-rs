# Web Crawler Server

## Features

* crawler uses an asynchronous runtime for concurrency as there is a lot of Network I/O going on
* crawler respects `robots.txt`
* can create multiple `Crawlers` for multiple `domains`, and they will all run in parallel
* can obtain partial results from database while crawlers are running
* graceful shutdown
* unit tests and integration tests
    * tests for database
    * tests for endpoint filters and the handlers
    * tests for crawler with `mockito` as mock server
    * good coverage, but I didn't want to spend more time on it
* deployment as Docker image

## Architecture

### Server
As soon as the application is run, an async task is spawned that will receive and handle SIGKILL, SIGTERM, SIGQUIT and the HTTP server (using `warp`) starts serving. When a POST request is received with a new domain, a crawler is spawned.
* while that crawler is running, any other POST request for the same domain will return 200OK and will be dropped
* if the crawler finishes, the next request for the same domain will work again
* more requests can be sent in parallel to spawn crawlers for other domains.

Any other request will retrieve the **current** data from the database. Partial results can be returned if a crawler are still working on the domain.

### Crawler architecture

Each crawler is responsible for processing one URL. The crawler uses channels to communicate with other async tasks. The initial URL is sent on the initial channel and then the function asynchronously awaits URLs on the receive end of the channel.

For each URL sent, a new processing task is spawned. Each processing task receives a send end of a new channel, and the receive end is pushed into a map of receive streams. Every URL they find will be sent on the channel.

### Graceful shutdown

When a signal is received, the async tasks handling the shutdown will notify warp and all crawlers through a broadcast channel. Each crawler will notify its tasks and then the tasks will gracefully shutdown and notify the crawler back. The crawler can then safely shutdown, the server will also shutdown, and the application will stop.

## Commands

I used [cargo-make](https://crates.io/crates/cargo-make) to extend the `cargo` functionality a bit. The following commands are available:
* `cargo make docker-build` - builds the docker image
* `cargo make deploy` - runs the container (it also built if not already build) and exposes the webserver at `localhost:3030`
* `cargo make tarpaulin` - installs `cargo-tarpaulin` and builds the coverage report. Also exports the report in Lcov format, which can be used by some editor extensions to display coverage inside the editor itself. I use it with `Coverage Gutters` in `VS Code`.

# Crates

* `warp` for webserver
* `tokio` for asynchronous runtime
* `tokio-stream` for streams in `tokio`. Helps with detecting when crawling terminates.
* `serde`, `serde_json` serialization/deserialization, mostly for parsing POST JSON body and returning errors as JSON to caller.
* `futures` to be able to use `StreamAll`
* `reqwest` as HTTP client.
* `tracing`, `tracing-subscriber` for logging.
* `scraper` to scrape HTML for links.
* `anyhow` for error handling in some parts of the app.
* `thiserror` for error handling in the more lib-like parts of the app.
* `url` for its URL type.
* `robotstxt` to parse and match against `robots.txt`.

## Assumptions

The section about endpoints that need to be exposed was a bit ambiguous. From my understanding what I had to implement was:
* an endpoint that receives a domain and starts crawling the domain
    * went with: `POST /domains` with JSON body: `{"domain": "<url>"}`
    * crawling can take a while, so just start the job and return 200OK
    * multiple requests for the same domain will either start a new crawler (if one is not already working) or do nothing. It will return 200OK regardless (check further work).
* an endpoint to obtain the list of unique URLs for one domain
    * went with: `GET /domains?domain=<url>`
* an endpoint to obtain the number of appeareances of one URL for a domain
    * went with: `GET /domains/urls?url=<url>`
    * the domain is part of the `<url>`
    * I might've misunderstood this one and I was supposed to obtain the count of unique URLs for one domain (?). I thought this can be easily found by checking the size of the return unique URL list, and I went with actually counting the occurences of URLs for each domain.

## Further work

* Currently using an in-memory database. This is obviously not ideal. If I were to spend more time on it, I would go with adding a separate database solution and having a caching layer on top of it. I'm already a bit familiar with using `sqlx` and `PostgreSQL` in personal projects. I also used `Movine` for database migrations.
* The response returned by `POST` on `/domains` can be improved. I didn't think the required changes are too complicated to justify spending time on them at this moment, but I can happily discuss about alternative solutions. I think the correct way to handle long running operations is to:
    * set the `Location:` header of the response to `/domains?domain=<url>`
    * return `Accepted 202` on subsequent request and enqueue crawl tasks

## Requests (using httpie)

* Start crawl
`http POST http://localhost:3030/domains domain=https://google.com`
* List domains
`http GET http://localhost:3030/domains?domain=https://google.com`
* URL count
`http GET http://localhost:3030/domains/urls?url=https://google.com`