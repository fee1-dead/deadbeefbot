use color_eyre::eyre::eyre;
use parsoid::Template;
use serde::Deserialize;
use tracing::warn;

use super::{ExtractContext, Extractor};
use crate::articlehistory::{self as ah, ArticleHistory, PreserveDate};
use crate::Result;

#[derive(Deserialize)]
pub struct Otd {
    date: PreserveDate,
    oldid: String,
}

#[derive(Deserialize)]
pub struct Otds {
    otds: Vec<Otd>,
}

pub struct OtdExtractor;

impl Extractor for OtdExtractor {
    type Value = Otds;

    /// https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AOn+this+day&namespace=&hidetrans=1&hidelinks=1
    const ALIAS: &'static [&'static str] = &[
        "on this day",
        "selected anniversary",
        "otdtalk",
        "satalk",
        "onthisday",
    ];

    fn extract(&self, t: &Template) -> Result<Otds> {
        let mut params = t.params();
        let mut otds = Vec::new();
        for n in 1.. {
            let Some(date) = params.swap_remove(&format!("date{n}")) else {
                break;
            };
            let Some(oldid) = params.swap_remove(&format!("oldid{n}")) else {
                break;
            };
            otds.push(Otd {
                date: PreserveDate::try_from_string(date).map_err(|x| eyre!("{x}"))?,
                oldid,
            });
        }
        if !params.is_empty() {
            warn!(?params, "unrecognized parameters");
            return Err(eyre!("unrecognized parameters"));
        }

        Ok(Otds { otds })
    }

    async fn merge_value_into<'cx>(
        &self,
        _cx: ExtractContext<'cx>,
        value: Otds,
        into: &mut ArticleHistory,
    ) -> crate::Result<()> {
        for Otd { date, oldid } in value.otds {
            into.otds.push(ah::Otd {
                date,
                oldid: Some(oldid),
                link: None,
            });
        }
        Ok(())
    }
}
