use std::fs;

use color_eyre::eyre::Context;
use futures_util::{Future, Stream, TryStreamExt};
use parsoid::Template;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use wiki::req::search::SearchGenerator;
use wiki::req::{self, Query, QueryGenerator};
use wiki::ClientBuilder;

const UA: &str = concat!(
    "DeadbeefBot/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/fee1-dead/deadbeefbot; ent3rm4n@gmail.com) mwapi/0.4.3 parsoid/0.7.4"
);

pub mod articlehistory;
pub mod remove_twitter_trackers;

pub type Result<T, E = color_eyre::Report> = std::result::Result<T, E>;

#[derive(Deserialize, Debug)]
struct Revision {
    revid: u32,
}
#[derive(Deserialize, Debug)]
struct SearchResult {
    pageid: u32,
    title: String,
    revisions: Vec<Revision>,
}
#[derive(Deserialize, Debug)]
struct SearchResponseBody {
    pages: Vec<SearchResult>,
}

pub fn search_with_rev_ids<T: DeserializeOwned>(
    client: &wiki::Bot,
    gen: SearchGenerator,
) -> impl Stream<Item = Result<T>> {
    let pages = client.query_all(Query {
        prop: Some(
            req::QueryProp::Revisions(req::QueryPropRevisions {
                prop: req::RvProp::IDS,
                slots: req::RvSlot::Main.into(),
                limit: req::Limit::None,
            })
            .into(),
        ),
        generator: Some(QueryGenerator::Search(gen)),
        ..Default::default()
    });

    pages
        .map_err(Into::into)
        .and_then(|x| async { Ok(serde_json::from_value(x)?) })
}

pub async fn enwiki_bot() -> Result<wiki::Bot> {
    site_from_url("https://en.wikipedia.org/w/api.php").await
}

pub async fn site_from_url(url: &str) -> Result<wiki::Bot> {
    Ok(ClientBuilder::new(url)
        .oauth(
            fs::read_to_string("./token.secret")
                .context("please put oauth2 token in token.secret")?
                .trim(),
        )
        .user_agent(UA)
        .build()
        .await?)
}

pub fn enwiki_parsoid() -> Result<parsoid::Client> {
    parsoid_from_url("https://en.wikipedia.org/api/rest_v1")
}

pub fn parsoid_from_url(url: &str) -> Result<parsoid::Client> {
    Ok(parsoid::Client::new(url, UA)?)
}

pub fn check_nobots(t: &Template) -> bool {
    let name = t.name().to_ascii_lowercase();
    name == "template:nobots"
        || (name == "template:bots"
            && (t.param("allow").as_deref() == Some("none")
                || t.param("deny").as_deref() == Some("all")
                || t.param("optout").as_deref() == Some("all")
                || t.param("deny").map_or(false, |x| x.contains("DeadbeefBot"))))
}

pub fn setup<F: Future<Output = color_eyre::Result<()>>>(
    x: impl FnOnce() -> F,
) -> color_eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(x())
}
