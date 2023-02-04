//! Merge `{{On this day}}` templates into `{{article history}}` if exists.

use std::collections::HashMap;
use std::ops::ControlFlow;

use futures_util::TryStreamExt;
use parsoid::{Template, WikiMultinode, WikinodeIterator};
use tracing::{info, warn};
use wiki::req::search::{SearchGenerator, SearchInfo, SearchProp};
use wiki::req::{Limit, PageSpec};

use crate::{
    check_nobots, enwiki_bot, enwiki_parsoid, search_with_rev_ids, Result, SearchResponseBody,
};

/// taken from [here](https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AArticle+history&namespace=&hidetrans=1&hidelinks=1).
///
/// This is case insensitive. Let's hope that people don't use the other capitalizations for a different thing on article talk pages.
const AH: &[&str] = &[
    "article history",
    "article milestones",
    "articlemilestones",
    "articlehistory",
];

/// https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AOn+this+day&namespace=&hidetrans=1&hidelinks=1
const OTD: &[&str] = &[
    "on this day",
    "selected anniversary",
    "otdtalk",
    "satalk",
    "onthisday",
];

/// https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3ADYK+talk&namespace=&hidetrans=1&hidelinks=1
const DYK: &[&str] = &["dyktalk", "dyk talk"];

/// https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AITN+talk&namespace=&hidetrans=1&hidelinks=1
const ITN: &[&str] = &["itn talk", "itntalk"];

fn has_any_params(t: &Template, p: &[&str]) -> bool {
    p.iter().any(|x| t.param(x).is_some())
}

fn treat_otd(t: &Template, article_history: &Template) -> Result<ControlFlow<()>> {
    #[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Otd {
        date: Option<String>,
        oldid: Option<String>,
    }

    if has_any_params(article_history, &["otddate", "otdoldid", "otdlink"]) {
        warn!("has OTD already");
        return Ok(ControlFlow::Break(()));
    }

    let mut map: HashMap<u32, Otd> = HashMap::new();
    for (param, value) in t.params() {
        if let Some(num) = param.strip_prefix("date") {
            map.entry(num.parse()?).or_default().date = Some(value);
        } else if let Some(num) = param.strip_prefix("oldid") {
            map.entry(num.parse()?).or_default().oldid = Some(value);
        } else {
            warn!(?param, "unrecognized parameter");
            return Ok(ControlFlow::Break(()));
        }
    }
    let mut v: Vec<(u32, Otd)> = map.into_iter().collect();
    v.sort_unstable();

    for (n, Otd { date, oldid }) in v {
        let (Some(date), Some(oldid)) = (date, oldid) else {
            warn!(?n, "does not have both date and oldid, refusing to edit");
            return Ok(ControlFlow::Break(()));
        };

        let datename = format!("otd{n}date");
        let oldidname = format!("otd{n}oldid");

        if has_any_params(article_history, &[&datename, &oldidname]) {
            warn!("article history already has OTD param, not overwriting");
            return Ok(ControlFlow::Break(()));
        }

        article_history.set_param(&datename, &date)?;
        article_history.set_param(&oldidname, &oldid)?;
    }

    // remove the OTD template.
    t.detach();

    Ok(ControlFlow::Continue(()))
}

fn treat_dyk(t: &Template, article_history: &Template) -> Result<ControlFlow<()>> {
    let mut date = None;
    let mut hook = None;
    let mut nompage = None;
    for (name, val) in t.params() {
        match &*name {
            "1" => date = Some(val),
            "2" | "entry" => hook = Some(val),
            "nompage" => nompage = Some(val),
            // ignored parameters
            "views" | "article" | "small" | "3" | "image" => {}
            _ => {
                warn!(?name, "unrecognized parameter");
                return Ok(ControlFlow::Break(()));
            }
        }
    }

    let Some(date) = date else {
        warn!("no date provided");
        return Ok(ControlFlow::Continue(()))
    };

    if has_any_params(article_history, &["dykdate", "dykentry", "dyknom"]) {
        warn!("already has dyk template");
        return Ok(ControlFlow::Continue(()));
    }

    article_history.set_param("dykdate", &date)?;

    if let Some(entry) = hook {
        article_history.set_param("dykentry", &entry)?;
    }

    if let Some(nom) = nompage {
        article_history.set_param("dyknom", &nom)?;
    }

    t.detach();
    Ok(ControlFlow::Continue(()))
}

fn treat_itn(t: &Template, article_history: &Template) -> Result<ControlFlow<()>> {
    #[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Itn {
        date: Option<String>,
        oldid: Option<String>,
    }

    if has_any_params(article_history, &["itndate", "itnlink"]) {
        warn!("has ITN already");
        return Ok(ControlFlow::Break(()));
    }

    let mut map: HashMap<u32, Itn> = HashMap::new();
    for (param, value) in t.params() {
        if let Some(num) = param.strip_prefix("date") {
            map.entry(if num.is_empty() { 1 } else { num.parse()? })
                .or_default()
                .date = Some(value);
        } else if let Some(num) = param.strip_prefix("oldid") {
            map.entry(if num.is_empty() { 1 } else { num.parse()? })
                .or_default()
                .oldid = Some(value);
        } else {
            warn!(?param, "unrecognized parameter");
            return Ok(ControlFlow::Break(()));
        }
    }
    let mut v: Vec<(u32, Itn)> = map.into_iter().collect();
    v.sort_unstable();

    for (n, Itn { date, oldid }) in v {
        let Some(date) = date else {
            warn!(?n, "does not have date, refusing to edit");
            return Ok(ControlFlow::Break(()));
        };

        let datename = format!("itn{n}date");
        let linkname = format!("itn{n}link");

        if has_any_params(article_history, &[&datename, &linkname]) {
            warn!("article history already has ITN param, not overwriting");
            return Ok(ControlFlow::Break(()));
        }

        article_history.set_param(&datename, &date)?;

        if let Some(oldid) = oldid {
            article_history.set_param(&linkname, &format!("https://en.wikipedia.org/w/index.php?title=Template:In_the_news&oldid={oldid}"))?;
        }
    }

    // remove the itn template.
    t.detach();

    Ok(ControlFlow::Continue(()))
}

pub async fn main() -> Result<()> {
    let client = enwiki_bot().await?;
    let parsoid = enwiki_parsoid()?;
    let pages = search_with_rev_ids(
        &client,
        SearchGenerator {
            search: r#"hastemplate:"On this day" hastemplate:"Article history""#.into(),
            limit: Limit::Value(100),
            offset: None,
            prop: SearchProp::empty(),
            info: SearchInfo::empty(),
            namespace: Some("1".into()),
        },
    );

    pages
        .try_for_each(|x: SearchResponseBody| async {
            'treat: for mut page in x.pages {
                info!("Treating [[{}]]", page.title);
                let rev = page.revisions.pop().unwrap();
                let wikicode = parsoid
                    .get_revision(&page.title, rev.revid.into())
                    .await?
                    .into_mutable();
                let templates = wikicode.filter_templates()?;
                let Some(article_history) = templates
                    .iter()
                    .find(|x| AH.contains(&&*x.name().trim_start_matches("Template:").to_ascii_lowercase()))
                else {
                    continue 'treat;
                };

                let mut foundotd = false;
                let mut founddyk = false;
                let mut founditn = false;
                for template in &templates {
                    if check_nobots(template) {
                        continue 'treat;
                    }

                    let template_name = template.name().trim_start_matches("Template:").to_ascii_lowercase();

                    if OTD.contains(&&*template_name) {
                        if foundotd {
                            warn!("Got TWO on this day templates on the page, confused bot will refuse to edit");
                            continue 'treat;
                        }
                        foundotd = true;

                        if treat_otd(template, article_history)?.is_break() {
                            continue 'treat;
                        }
                    }

                    if DYK.contains(&&*template_name) {
                        if founddyk {
                            warn!("Got TWO DYK templates on the page, confused bot will refuse to edit");
                            continue 'treat;
                        }
                        founddyk = true;

                        if treat_dyk(template, article_history)?.is_break() {
                            continue 'treat;
                        }
                    }

                    if ITN.contains(&&*template_name) {
                        if founditn {
                            warn!("Got TWO ITN templates on the page, confused bot will refuse to edit");
                            continue 'treat;
                        }
                        founditn = true;

                        if treat_itn(template, article_history)?.is_break() {
                            continue 'treat;
                        }
                    }
                }

                // we are done with modifying wikicode.
                let text = parsoid.transform_to_wikitext(&wikicode).await?;
                client
                    .build_edit(PageSpec::PageId(page.pageid))
                    .text(text)
                    .summary("merged {{on this day}} template to {{article history}}.")
                    .baserevid(rev.revid)
                    .minor()
                    .bot()
                    .send()
                    .await?;

                // TODO remove this
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }

            Ok(())
        })
        .await?;

    Ok(())
}
