use std::sync::Arc;

use color_eyre::eyre::eyre;
use deadbeefbot::{enwiki_bot, enwiki_parsoid};
use futures_util::TryStreamExt;
use parsoid::WikinodeIterator;
use serde_json::{from_value, Value};
use wiki::api::{BasicSearchResult, QueryResponse, Search};
use wiki::req::search::{ListSearch, SearchInfo, SearchProp};
use wiki::req::{Limit, Query, QueryList};

/// taken from [here](https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AArticle+history&namespace=&hidetrans=1&hidelinks=1).
///
/// This is case insensitive. Let's hope that people don't use the other capitalizations for a different thing on article talk pages.
const AH: &[&str] = &[
    "article history",
    "article milestones",
    "articlemilestones",
    "articlehistory",
];

pub fn main() -> color_eyre::Result<()> {
    deadbeefbot::setup(real_main)
}

pub async fn real_main() -> color_eyre::Result<()> {
    let client = enwiki_bot().await?;
    let parsoid = enwiki_parsoid()?;

    let q = Query {
        list: Some(
            QueryList::Search(ListSearch {
                search: "hastemplate:\"Article history\"".into(),
                limit: Limit::Max,
                prop: SearchProp::empty(),
                info: SearchInfo::empty(),
                namespace: Some("1".into()),
            })
            .into(),
        ),
        ..Default::default()
    };

    let res = client.query_all(q);

    let map = Arc::new(dashmap::DashSet::<String>::new());

    res.map_err(|x| eyre!("searching: {x}"))
        .try_for_each(|x: Value| async {
            let x: QueryResponse<Search<BasicSearchResult>> = from_value(x)?;
            let tasks = x.query.search.into_iter().map(|page| {
                let parsoid = parsoid.clone();
                let map = map.clone();
                tokio::spawn(async move {
                    let Ok(page) = parsoid.get(&page.title).await
                else {
                    return;
                };
                    let Ok(templates) = page.into_mutable().filter_templates() else {
                    return;
                };
                    for template in templates {
                        if AH.contains(&&*template.name().to_ascii_lowercase()) {
                            for (name, _) in template.params() {
                                map.insert(name);
                            }
                        }
                    }
                })
            });
            for task in tasks {
                task.await?;
            }
            Ok(())
        })
        .await?;

    Ok(())
}
