//! Merge `{{On this day}}` templates into `{{article history}}` if exists.

use std::collections::HashMap;

use futures_util::TryStreamExt;
use parsoid::{WikiMultinode, WikinodeIterator};
use tracing::{info, warn};
use wiki::req::search::{SearchGenerator, SearchInfo, SearchProp};
use wiki::req::{Limit, PageSpec};

use crate::{enwiki_bot, enwiki_parsoid, search_with_rev_ids, Result, SearchResponseBody, check_nobots};

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

                let mut found = false;
                for template in &templates {
                    if check_nobots(template) {
                        continue 'treat;
                    }

                    if OTD.contains(&&*template.name().trim_start_matches("Template:").to_ascii_lowercase()) {
                        if found {
                            warn!("Got TWO on this day templates on the page, confused bot will refuse to edit");
                            continue 'treat;
                        }
                        found = true;

                        #[derive(Default, PartialEq, Eq, PartialOrd, Ord)]
                        pub struct Otd { date: Option<String>, oldid: Option<String> }

                        let mut map: HashMap<u32, Otd> = HashMap::new();
                        for (param, value) in template.params() {
                            if let Some(num) = param.strip_prefix("date") {
                                map.entry(num.parse()?).or_default().date = Some(value);
                            } else if let Some(num) = param.strip_prefix("oldid") {
                                map.entry(num.parse()?).or_default().oldid = Some(value);
                            } else {
                                warn!(?param, "unrecognized parameter");
                                continue 'treat;
                            }
                        }
                        let mut v: Vec<(u32, Otd)> = map.into_iter().collect();
                        v.sort_unstable();

                        for (n, Otd { date, oldid }) in v {
                            let (Some(date), Some(oldid)) = (date, oldid) else {
                                warn!(?n, "does not have both date and oldid, refusing to edit");
                                continue 'treat;
                            };

                            let datename = format!("otd{n}date");
                            let oldidname = format!("otd{n}oldid");

                            if article_history.param(&datename).is_some() || article_history.param(&oldidname).is_some() {
                                warn!("article history already has OTD param, not overwriting");
                                continue 'treat;
                            }

                            article_history.set_param(&datename, &date)?;
                            article_history.set_param(&oldidname, &oldid)?;
                        }

                        // remove the OTD template.
                        template.detach();
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
