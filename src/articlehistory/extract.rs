use std::collections::HashMap;

use parsoid::Template;
use serde_json::{Map, Value};
use tracing::warn;

use super::{ArticleHistory, Result};

/// first extract useful information from article history.
pub fn extract_info(article_history: &Template) -> Result<Option<ArticleHistory>> {
    let all_params = article_history.params();

    println!("{all_params:?}");

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
                        warn!("failed to parse {} number: {name}", $name);
                        return Ok(None);
                    };
                    num
                };

                let key = &name[num_end..];

                if $map.entry(num).or_default().insert(key, param).is_some() {
                    warn!("duplicate {}: {num} {key}", $name);
                    return Ok(None);
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

    println!("{map:#?}");

    let res = serde_json::from_value(Value::Object(map));

    match res {
        Ok(x) => Ok(Some(x)),
        Err(e) => {
            warn!("error when parsing article history template: {e}");
            Ok(None)
        }
    }
}
