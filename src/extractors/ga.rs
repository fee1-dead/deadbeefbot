use serde::Deserialize;
use tracing::warn;

use super::Extractor;
use crate::articlehistory::{Action, ActionKind, ArticleHistory, PreserveDate};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ga {
    #[serde(alias = "1")]
    pub date: Option<PreserveDate>,
    pub oldid: Option<String>,
    #[serde(alias = "subtopic")]
    pub topic: Option<String>,
    pub small: Option<String>,
    pub page: Option<String>,
}

pub struct GaExtractor;

impl Extractor for GaExtractor {
    type Value = Ga;
    /// https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AGA&namespace=&hidetrans=1&hidelinks=1
    const ALIAS: &'static [&'static str] = &["ga"];
    fn merge_value_into<'cx>(
        &self,
        cx: super::ExtractContext<'cx>,
        value: Ga,
        into: &mut ArticleHistory,
    ) {
        if let Some(topic) = value.topic {
            if let Some(topic2) = &into.topic {
                if topic2 != &topic {
                    warn!("topic mismatch");
                    return;
                }
            }

            into.topic = Some(topic);
        }
        let Some(page) = value.page else {
            warn!("no page");
            return;
        };
        let title = cx.title;
        into.actions.push(Action {
            kind: ActionKind::Gan,
            date: value.date.unwrap(),
            link: Some(format!("{title}/GA{page}")),
            result: Some("listed".into()),
            oldid: value.oldid,
        })
    }
}
