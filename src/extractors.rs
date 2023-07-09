use crate::Result;
use crate::articlehistory::ArticleHistory;
use parsoid::Template;
use serde::de::DeserializeOwned;
use serde_json::{Value, Map};
use wiki::Client;

mod dyk;
mod oldpr;


pub struct ExtractContext<'cx> {
    pub client: &'cx Client,
    pub parsoid: &'cx parsoid::Client,
}


pub fn simple_extract<T: DeserializeOwned>(t: &Template) -> Result<T> {
    let x: Map<_, _>  = t.params().into_iter().map(|(a, b)| (a, Value::String(b))).collect();
    Ok(serde_json::from_value(Value::Object(x))?)
}

pub fn template_name(t: &Template) -> String {
    t.name().trim_start_matches("Template:").to_ascii_lowercase()
}

pub trait Extractor {
    type Value;

    /// A check for template name that this is extractable.
    fn is_extractable(&self, t: &Template) -> bool;

    fn extract(&self, t: &Template) -> Result<Self::Value>;
    fn merge_value_into<'cx>(&self, cx: ExtractContext<'cx>, value: Self::Value, into: &mut ArticleHistory);
}



