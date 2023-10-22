use serde::Deserialize;
use tracing::warn;

use super::Extractor;
use crate::articlehistory::{Action, ActionKind, ArticleHistory, PreserveDate};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailedGa {
    #[serde(alias = "1")]
    pub date: Option<PreserveDate>,
    pub oldid: Option<String>,
    #[serde(alias = "subtopic")]
    pub topic: Option<String>,
    pub small: Option<String>,
    pub page: Option<String>,
}

pub struct FailedGaExtractor;

impl Extractor for FailedGaExtractor {
    type Value = FailedGa;
    const ALIAS: &'static [&'static str] = &["failedga", "failed ga"];
    fn merge_value_into<'cx>(
        &self,
        _cx: super::ExtractContext<'cx>,
        value: FailedGa,
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
        into.actions.push(Action {
            kind: ActionKind::Gan,
            date: value.date.unwrap(),
            link: Some(format!("/GA{page}")),
            result: Some("failed".into()),
            oldid: value.oldid,
        })
    }
}
