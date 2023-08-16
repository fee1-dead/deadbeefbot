use crate::articlehistory::ArticleHistory;
use crate::Result;
use parsoid::Template;
use parsoid::WikiMultinode;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use wiki::Bot;

mod dyk;
mod ga;
mod oldpr;

#[derive(Clone, Copy, Debug)]
pub struct ExtractContext<'cx> {
    pub client: &'cx Bot,
    pub parsoid: &'cx parsoid::Client,
}

pub fn simple_extract<T: DeserializeOwned>(t: &Template) -> Result<T> {
    let x: Map<_, _> = t
        .params()
        .into_iter()
        .map(|(a, b)| (a, Value::String(b)))
        .collect();
    Ok(serde_json::from_value(Value::Object(x))?)
}

pub fn template_name(t: &Template) -> String {
    t.name()
        .trim_start_matches("Template:")
        .to_ascii_lowercase()
}

pub trait Extractor {
    type Value: DeserializeOwned;

    const ALIAS: &'static [&'static str];

    /// A check for template name that this is extractable.
    fn is_extractable(&self, t: &Template) -> bool {
        let name = template_name(t);
        Self::ALIAS.iter().any(|x| x.eq_ignore_ascii_case(&name))
    }

    fn extract(&self, t: &Template) -> Result<Self::Value> {
        Ok(serde_json::from_value(simple_extract(t)?)?)
    }
    fn merge_value_into<'cx>(
        &self,
        cx: ExtractContext<'cx>,
        value: Self::Value,
        into: &mut ArticleHistory,
    );
}

pub fn extract_all<'cx>(
    cx: ExtractContext<'cx>,
    t: &Template,
    ah: &mut ArticleHistory,
) -> crate::Result<()> {
    macro_rules! extract {
        ($v:expr) => {
            let e = $v;
            if e.is_extractable(t) {
                let val = e.extract(t)?;
                e.merge_value_into(cx, val, ah);
                t.detach();
                return Ok(());
            }
        };
    }
    extract!(dyk::DykExtractor);
    extract!(oldpr::OldPrExtractor);
    extract!(ga::GaExtractor);
    Ok(())
}
