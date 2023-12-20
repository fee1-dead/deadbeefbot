use std::collections::HashMap;

use color_eyre::eyre::bail;
use parsoid::Template;
use serde_json::{Map, Value};

use super::{ArticleHistory, Extractor, Result};

pub struct ArticleHistoryExtractor;

impl Extractor for ArticleHistoryExtractor {
    type Value = ArticleHistory;

    /// taken from [here](https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AArticle+history&namespace=&hidetrans=1&hidelinks=1).
    ///
    /// This is case insensitive. Let's hope that people don't use the other capitalizations for a different thing on article talk pages.
    const ALIAS: &'static [&'static str] = &[
        "article history",
        "article milestones",
        "articlemilestones",
        "articlehistory",
    ];

    fn extract(&self, article_history: &Template) -> Result<Self::Value> {
        let all_params = article_history.params();

        let mut map = Map::new();
        let mut actions: HashMap<usize, HashMap<_, _>> = HashMap::new();
        let mut featured_topics: HashMap<usize, HashMap<_, _>> = HashMap::new();
        let mut dyk: HashMap<usize, HashMap<_, _>> = HashMap::new();
        let mut otd: HashMap<usize, HashMap<_, _>> = HashMap::new();
        let mut itn: HashMap<usize, HashMap<_, _>> = HashMap::new();

        for (name, param) in all_params.iter() {
            macro_rules! maybe_number_and_key {
                ($name:literal, $map: ident) => {{
                    let name = &name[$name.len()..];
                    let num_end = name
                        .chars()
                        .position(|x| !x.is_ascii_digit())
                        .unwrap_or(name.len());

                    let num = if num_end == 0 {
                        0
                    } else {
                        let Ok(num) = name[..num_end].parse::<usize>() else {
                            bail!("failed to parse {} number: {name}", $name);
                        };
                        num
                    };

                    let key = &name[num_end..];

                    if $map.entry(num).or_default().insert(key, param).is_some() {
                        bail!("duplicate {}: {num} {key}", $name);
                    }
                }};
            }
            if name.starts_with("action") {
                maybe_number_and_key!("action", actions);
            } else if name.starts_with("ft") {
                maybe_number_and_key!("ft", featured_topics);
            } else if name.starts_with("dyk") {
                maybe_number_and_key!("dyk", dyk);
            } else if name.starts_with("otd") {
                maybe_number_and_key!("otd", otd);
            } else if name.starts_with("itn") {
                maybe_number_and_key!("itn", itn);
            } else {
                map.insert(name.clone(), param.clone().into());
            }
        }

        for (mut values, key, start) in [
            (actions, "actions", 1),
            (featured_topics, "featured_topics", 0),
            (dyk, "dyks", 0),
            (otd, "otds", 0),
            (itn, "itns", 0),
        ] {
            let values = (1..).map_while(|mut idx| {
                if idx == 1 && start == 0 {
                    idx = 0;
                }
                if let Some(x) = values.remove(&idx) {
                    let value: Map<_, _> = x
                        .into_iter()
                        .map(|(key, value)| (key.to_owned(), Value::String(value.clone())))
                        .collect();
                    Some(value)
                } else {
                    None
                }
            });

            map.insert(
                key.to_owned(),
                Value::Array(values.map(Value::Object).collect()),
            );
        }

        Ok(serde_json::from_value(Value::Object(map))?)
    }

    async fn merge_value_into<'cx>(
        &self,
        _: super::ExtractContext<'cx>,
        _: Self::Value,
        _: &mut ArticleHistory,
    ) -> Result<()> {
        unreachable!()
    }
}
