use std::collections::HashMap;

use parsoid::Template;
use serde_json::{Map, Value};
use tracing::warn;

use super::{ArticleHistory, Result};
use crate::articlehistory::ParameterType;

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

pub type ExtractResultMulti = Result<Option<Vec<ParameterType>>>;
pub type ExtractResultSingle = Result<Option<ParameterType>>;


pub fn extract_itn(t: &Template) -> ExtractResultMulti {
    #[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Itn {
        date: Option<String>,
    }

    let mut date1 = None;
    let mut year1 = None;
    let mut map: HashMap<u32, Itn> = HashMap::new();
    for (param, value) in t.params() {
        if param == "1" {
            date1 = Some(value);
        } else if param == "2" {
            year1 = Some(value);
        } else if let Some(num) = param.strip_prefix("date") {
            map.entry(if num.is_empty() { 1 } else { num.parse()? })
                .or_default()
                .date = Some(value);
        } else if param.strip_prefix("oldid").is_some() || param.strip_prefix("alt").is_some() {
            // ignore oldid
        } else {
            warn!(?param, "unrecognized parameter");
            return Ok(None);
        }
    }

    if let Some(mut d) = date1 {
        if let Some(y) = year1 {
            d.push(' ');
            d.push_str(&y);
        }

        map.entry(1).or_default().date = Some(d);
    }

    Ok(Some(
        map.into_values()
            .map(|Itn { date }| ParameterType::Itn { date, link: None })
            .collect(),
    ))
}
