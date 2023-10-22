// TODO use this

use serde::Deserialize;

use super::{ExtractContext, Extractor};
use crate::articlehistory::{ArticleHistory, PreserveDate};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OldPeerReview {
    // Default: 1
    pub archive: Option<u64>,
    pub reviewedname: Option<String>,
    pub archivelink: Option<String>,
    pub id: Option<String>,
    pub date: Option<PreserveDate>,
}

pub struct OldPrExtractor;

impl Extractor for OldPrExtractor {
    type Value = OldPeerReview;

    const ALIAS: &'static [&'static str] = &["old peer review", "oldpeerreview"];

    fn merge_value_into<'cx>(
        &self,
        _cx: ExtractContext<'cx>,
        _value: OldPeerReview,
        _into: &mut ArticleHistory,
    ) {
    }
}
