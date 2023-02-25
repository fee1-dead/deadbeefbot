//! Merge `{{On this day}}` templates into `{{article history}}` if exists.

use std::collections::HashMap;
use std::fs;

use parsoid::{WikiMultinode, WikinodeIterator};
use rand::seq::SliceRandom;
use tracing::{debug, info};
use wiki::req::PageSpec;

use crate::articlehistory::extract::{extract_dyk, extract_itn, extract_otd};
use crate::{
    check_nobots, enwiki_bot, enwiki_parsoid, Result
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

pub struct Parameter {
    pub index: usize,
    pub ty: ParameterType,
}

impl Parameter {
    pub fn print_into(self, v: &mut Vec<(String, String)>) {
        let prefix = match self.ty {
            ParameterType::Itn { .. } => "itn",
            ParameterType::Dyk { .. } => "dyk",
            ParameterType::Otd { .. } => "otd",
        };

        let prefix = format!("{prefix}{}", self.index);

        macro_rules! print {
            ($value:expr) => {
                if let Some(x) = $value {
                    v.push((format!("{prefix}{}", stringify!($value)), x));
                }
            };
        }

        match self.ty {
            ParameterType::Itn { date, link } => {
                print!(date);
                print!(link);
            }
            ParameterType::Dyk { date, entry, nom } => {
                print!(date);
                print!(entry);
                print!(nom);
            }
            ParameterType::Otd { date, oldid, link } => {
                print!(date);
                print!(oldid);
                print!(link);
            }
        }

        v.last_mut()
            .unwrap()
            .1
            .push_str("{{subst:User:0xDeadbeef/newline}}");
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ParameterType {
    Itn {
        date: Option<String>,
        link: Option<String>,
    },
    Dyk {
        date: Option<String>,
        entry: Option<String>,
        nom: Option<String>,
    },
    Otd {
        date: Option<String>,
        oldid: Option<String>,
        link: Option<String>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    Itn,
    Dyk,
    Otd,
}

mod extract;

pub struct Info {
    start_index: Option<usize>,
    others: Vec<(String, String)>,
    params: HashMap<(Ty, usize), Parameter>,
}

pub async fn treat(client: &wiki::Bot, parsoid: &parsoid::Client, title: &str) -> Result<()> {
    info!("Treating [[{}]]", title);

    let wikicode = parsoid.get(title).await?.into_mutable();
    let rev = wikicode.revision_id().unwrap();
    let templates = wikicode.filter_templates()?;
    let Some(article_history) = templates
        .iter()
        .find(|x| AH.contains(&&*x.name().trim_start_matches("Template:").to_ascii_lowercase()))
    else {
        return Ok(())
    };

    article_history.set_name("Article history".to_owned())?;

    let Some(Info {
        start_index, mut others, params
    }) = extract::extract_info(article_history)? else {
        return Ok(())
    };

    let mut params: Vec<_> = params.into_values().map(|p| p.ty).collect();

    for template in &templates {
        if check_nobots(template) {
            return Ok(())
        }

        let template_name = template
            .name()
            .trim_start_matches("Template:")
            .to_ascii_lowercase();

        if OTD.contains(&&*template_name) {
            template.detach();
            let Some(mut x) = extract_otd(template)? else { return Ok(()) };
            debug!(?x);
            params.append(&mut x)
        } else if DYK.contains(&&*template_name) {
            template.detach();
            let Some(x) = extract_dyk(template)? else { return Ok(()) };
            params.push(x)
        } else if ITN.contains(&&*template_name) {
            template.detach();
            let Some(mut x) = extract_itn(template)? else { return Ok(()) };
            params.append(&mut x)
        }
    }

    // sort parameters.
    params.sort_unstable();

    let mut to_insert = Vec::with_capacity(params.len());

    debug!(?params);

    let mut dykcount = 0;
    let mut otdcount = 0;
    let mut itncount = 0;

    // convert parameters to final form.
    for param in params {
        let index = match param {
            ParameterType::Dyk { .. } => {
                dykcount += 1;
                dykcount
            }
            ParameterType::Otd { .. } => {
                otdcount += 1;
                otdcount
            }
            ParameterType::Itn { .. } => {
                itncount += 1;
                itncount
            }
        };

        Parameter { index, ty: param }.print_into(&mut to_insert);
    }

    let index = start_index.unwrap_or_else(|| others.len());
    let others_last = others.split_off(index);
    /*if let Some((_, b)) = others.last_mut() {
        b.push_str("{{subst:User:0xDeadbeef/newline}}")
    }*/
    others.extend(to_insert);
    others.extend(others_last);

    let params = others;
    article_history.set_params(params.into_iter().collect())?;

    // we are done with modifying wikicode.
    let text = parsoid.transform_to_wikitext(&wikicode).await?;

    client
                    .build_edit(PageSpec::Title(title.to_owned()))
                    .text(text)
                    .summary("merged OTD/ITN/DYK templates to {{article history}} ([[Wikipedia:Bots/Requests for approval/DeadbeefBot 2|BRFA]]) (trial)")
                    .baserevid(rev as u32)
                    .minor()
                    .bot()
                    .send()
                    .await?;

    Ok(())
}

pub async fn main() -> Result<()> {
    let client = enwiki_bot().await?;
    // let client = enwiki_bot().await?;
    let parsoid = enwiki_parsoid()?;


    let x = fs::read_to_string("./scan.txt")?;
    let pages: Vec<_> = x.lines().collect();
    
    // randomly take 49 pages
    let pages: Vec<_> = pages.choose_multiple(&mut rand::thread_rng(), 1).collect();

    for page in pages {
        treat(&client, &parsoid, page).await?;
    }
    // let parsoid = enwiki_parsoid()?;
    /*let pages = search_with_rev_ids(
        &client,
        SearchGenerator {
            // TODO also ITN and DYK
            search: r#"hastemplate:"On this day" hastemplate:"Article history""#.into(),
            limit: Limit::Value(100),
            offset: None,
            prop: SearchProp::empty(),
            info: SearchInfo::empty(),
            namespace: Some("1".into()),
        },
    );

    pages
        .try_for_each(|x: QueryResponse<SearchResponseBody>| async {
            'treat: for mut page in x.query.pages {
                // TODO remove this
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }

            Ok(())
        })
        .await?;
*/
    Ok(())
}
