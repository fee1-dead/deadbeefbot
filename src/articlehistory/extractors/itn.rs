use color_eyre::eyre::eyre;
use parsoid::Template;
use serde::Deserialize;
use tracing::warn;

use super::Extractor;
use crate::articlehistory::{ArticleHistory, Itn as AhItn, PreserveDate};

pub struct ItnExtractor;

#[derive(Deserialize)]
pub struct Itn {
    date: PreserveDate,
    oldid: Option<String>,
    alt: bool,
}

impl Itn {
    fn link(&self) -> Option<String> {
        if self.alt {
            Some(format!(
                "Portal:Current events/{}",
                self.date.date.format("%Y %B %d")
            ))
        } else if let Some(oldid) = &self.oldid {
            // TODO use let-chains
            if !oldid.trim().is_empty() {
                Some(format!("Special:PermanentLink/{oldid}"))
            } else {
                None
            }
        } else {
            None
        }
    }
}

impl Extractor for ItnExtractor {
    type Value = Vec<Itn>;
    /// https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AITN+talk&namespace=&hidetrans=1&hidelinks=1
    const ALIAS: &'static [&'static str] = &["itn talk", "itntalk"];

    async fn merge_value_into<'cx>(
        &self,
        _cx: super::ExtractContext<'cx>,
        value: Vec<Itn>,
        into: &mut ArticleHistory,
    ) -> crate::Result<()> {
        into.itns.extend(value.into_iter().map(|x| {
            let link = x.link();
            AhItn { date: x.date, link }
        }));
        Ok(())
    }

    fn extract(&self, t: &Template) -> crate::Result<Self::Value> {
        let mut params = t.params();
        let mut itns = Vec::new();
        let alt = params
            .swap_remove("alt")
            .map_or(false, |f| !f.trim().is_empty());
        for n in 1.. {
            let date = if n == 1 {
                params
                    .swap_remove("1")
                    .map(|month| {
                        if let Some(day) = params.swap_remove("2") {
                            format!("{month} {day}")
                        } else {
                            month
                        }
                    })
                    .or_else(|| params.swap_remove("date"))
                    .or_else(|| params.swap_remove("date1"))
            } else {
                params.swap_remove(&format!("date{n}"))
            };
            let Some(date) = date else { break };
            let oldid = if n == 1 {
                params.swap_remove("oldid")
            } else {
                None
            };
            let oldid = oldid.or_else(|| params.swap_remove(&format!("oldid{n}")));
            let alt = alt
                || params
                    .swap_remove(&format!("alt{n}"))
                    .map_or(false, |f| !f.trim().is_empty());
            itns.push(Itn {
                date: PreserveDate::try_from_string(date).map_err(|x| eyre!("{x}"))?,
                oldid,
                alt,
            });
        }
        if !params.is_empty() {
            warn!(?params, "unrecognized parameters");
            return Err(eyre!("unrecognized parameters"));
        }

        Ok(itns)
    }
}
