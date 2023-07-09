use serde::Deserialize;

use crate::articlehistory::PreserveDate;

use super::{Extractor, template_name, ExtractContext};

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

const ALIAS: &[&str] = &["old peer review", "oldpeerreview"];

impl Extractor for OldPrExtractor {
    type Value = OldPeerReview;

    fn is_extractable(&self, t: &parsoid::Template) -> bool {
        let name = template_name(t);
        ALIAS.iter().any(|x| x.eq_ignore_ascii_case(&name))
    }

    fn extract(&self, t: &parsoid::Template) -> crate::Result<OldPeerReview> {
        Ok(serde_json::from_value(super::simple_extract(t)?)?)
    }

    fn merge_value_into<'cx>(&self, cx: ExtractContext<'cx>, value: OldPeerReview, into: &mut crate::articlehistory::ArticleHistory) {

    }
}

