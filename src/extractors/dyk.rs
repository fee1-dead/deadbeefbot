use crate::articlehistory::PreserveDate;

use super::{Extractor, ExtractContext};


pub struct DykExtractor;

pub struct Dyk {
    pub date: PreserveDate,
    pub entry: Option<String>,
    pub nom: Option<String>,
}

impl Extractor for DykExtractor {
    type Value = Dyk;

    fn is_extractable(&self, t: &parsoid::Template) -> bool {
        todo!()
    }

    fn extract(&self, t: &parsoid::Template) -> crate::Result<Self::Value> {
        todo!()
    }

    fn merge_value_into<'cx>(&self, cx: ExtractContext<'cx>, value: Self::Value, into: &mut crate::articlehistory::ArticleHistory) {
        todo!()
    }
}
