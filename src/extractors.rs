use parsoid::{Template, WikiMultinode};
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use tracing::{debug, trace};
use wiki::Bot;

use crate::articlehistory::ArticleHistory;
use crate::Result;

mod dyk;
mod failedga;
mod ga;
mod itn;
mod oldpr;
mod otd;

#[derive(Clone, Copy, Debug)]
pub struct ExtractContext<'cx> {
    pub client: &'cx Bot,
    pub parsoid: &'cx parsoid::Client,
    pub title: &'cx str,
    pub allow_interactive: bool,
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

pub fn super_extract<T: Extractor + ?Sized>(t: &Template) -> Result<T::Value> {
    Ok(serde_json::from_value(simple_extract(t)?)?)
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
        super_extract::<Self>(t)
    }

    async fn merge_value_into<'cx>(
        &self,
        cx: ExtractContext<'cx>,
        value: Self::Value,
        into: &mut ArticleHistory,
    ) -> Result<()>;
}

pub fn detach_template(t: &Template) {
    let prev = t.as_nodes().first().unwrap().previous_sibling();
    let next = t.as_nodes().last().unwrap().next_sibling();
    trace!(?prev, ?next);
    let mut wasnl = false;
    for node in prev.into_iter().chain(next) {
        // clean any leftover extra newlines
        if let Some(s) = node.as_text() {
            let (newline_count, len) = {
                let s = &*s.borrow();
                (s.chars().take_while(|&x| x == '\n').count(), s.len())
            };
            if newline_count != len {
                continue;
            }
            if wasnl {
                *s.borrow_mut() = "".into();
            } else if newline_count >= 2 {
                *s.borrow_mut() = "\n".into();
            }
            wasnl = newline_count > 0;
        }
    }
    t.detach();
}

pub async fn extract_all<'cx>(
    cx: ExtractContext<'cx>,
    t: &Template,
    ah: &mut ArticleHistory,
) -> crate::Result<()> {
    macro_rules! extract {
        ($v:expr) => {
            let e = $v;
            if e.is_extractable(t) {
                debug!("extracted through `{}`", stringify!($v));
                let val = e.extract(t)?;
                e.merge_value_into(cx, val, ah).await?;
                detach_template(t);
                return Ok(());
            }
        };
    }
    extract!(dyk::DykExtractor);
    extract!(oldpr::OldPrExtractor);
    extract!(ga::GaExtractor);
    extract!(failedga::FailedGaExtractor);
    extract!(otd::OtdExtractor);
    extract!(itn::ItnExtractor);
    Ok(())
}
