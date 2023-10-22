use serde::Deserialize;

use super::{ExtractContext, Extractor};
use crate::articlehistory::{self as ah, ArticleHistory, PreserveDate};

pub struct DykExtractor;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Dyk {
    #[serde(rename = "1")]
    pub date: String,
    #[serde(alias = "2")]
    pub two: Option<String>,
    pub entry: Option<String>,
    pub nompage: Option<String>,
}

impl Extractor for DykExtractor {
    type Value = Dyk;

    /// https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3ADYK+talk&namespace=&hidetrans=1&hidelinks=1
    const ALIAS: &'static [&'static str] = &["dyktalk", "dyk talk"];

    fn merge_value_into<'cx>(
        &self,
        _cx: ExtractContext<'cx>,
        value: Dyk,
        into: &mut ArticleHistory,
    ) {
        let (date, entry, nom) = match value {
            Dyk {
                two: Some(year),
                entry: Some(entry),
                date,
                nompage,
            } if year.chars().all(|c| c.is_ascii_digit()) => {
                let date = format!("{date} {year}");
                (date, Some(entry), nompage)
            }
            Dyk {
                entry: None,
                two: Some(entry),
                nompage,
                date,
            } => (date, Some(entry), nompage),
            Dyk {
                date,
                entry,
                nompage,
                ..
            } => (date, entry, nompage),
        };
        into.dyks.push(ah::Dyk {
            date: PreserveDate::try_from_string(date).unwrap(),
            entry,
            nom,
            ignoreerror: false,
        });
    }
}
