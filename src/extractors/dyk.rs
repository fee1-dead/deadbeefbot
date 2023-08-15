use serde::Deserialize;

use crate::articlehistory::{self as ah, ArticleHistory};
use crate::articlehistory::PreserveDate;

use super::{Extractor, ExtractContext};


pub struct DykExtractor;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Dyk {
    pub date: PreserveDate,
    pub entry: Option<String>,
    pub nom: Option<String>,
}

impl Extractor for DykExtractor {
    type Value = Dyk;

    /// https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3ADYK+talk&namespace=&hidetrans=1&hidelinks=1
    const ALIAS: &'static [&'static str] = &["dyktalk", "dyk talk"];

    fn merge_value_into<'cx>(&self, _cx: ExtractContext<'cx>, value: Dyk, into: &mut ArticleHistory) {
        into.dyks.push(ah::Dyk { date: value.date, entry: value.entry, nom: value.nom, ignoreerror: false });
    }
}
