//! Merge `{{On this day}}` templates into `{{article history}}` if exists.

use std::io::stdin;
use std::process;

use colored_diff::PrettyDifference;
use extractors::ExtractContext;
use parsoid::{Template, WikiMultinode, WikinodeIterator};
use rand::seq::SliceRandom;
use rand::thread_rng;
use tracing::{debug, info, warn};
use wiki::api::RequestBuilderExt;
use wiki::req::parse::{Parse, ParseProp};
use wiki::req::{self, PageSpec};

use crate::articlehistory::extractors::{ArticleHistoryExtractor, Extractor};
use crate::{check_nobots, enwiki_bot, enwiki_parsoid, Result};
#[allow(unused_imports)]
use crate::{parsoid_from_url, site_from_url};

mod builder;
mod extract;
mod extractors;
mod types;

pub use types::*;

pub async fn treat(
    client: &wiki::Bot,
    parsoid: &parsoid::Client,
    title: &str,
    prompt: bool,
) -> Result<()> {
    info!("Treating [[{}]]", title);

    let wikicode = parsoid.get(title).await?.into_mutable();
    let rev = wikicode.revision_id().unwrap();
    let templates = wikicode.filter_templates()?;
    let ah_created;

    let article_history = match templates
        .iter()
        .find(|t| ArticleHistoryExtractor.is_extractable(t))
    {
        Some(article_history) => {
            article_history
                .set_name("Article history{{subst:User:0xDeadbeef/newline}}".to_owned())?;
            article_history
        }
        None => {
            // mount an article history template.
            let banner_shell_aliases = include_str!("banneralias.txt");
            let Some(banner) = templates.iter().find(|x| {
                banner_shell_aliases
                    .lines()
                    .any(|name| name == x.name().trim_start_matches("Template:"))
            }) else {
                warn!("skipping, article doesn't have wp banner shell");
                return Ok(());
            };

            ah_created = Template::new_simple("Article history{{subst:User:0xDeadbeef/newline}}");

            let nl = Template::new_simple("subst:User:0xDeadbeef/newline");
            let node = nl.as_nodes().pop().unwrap();
            banner
                .as_nodes()
                .first()
                .unwrap()
                .insert_before(node.clone());
            node.insert_before(ah_created.as_nodes().pop().unwrap());
            &ah_created
        }
    };

    let mut ah = ArticleHistoryExtractor.extract(article_history)?;

    let cx = ExtractContext {
        client,
        parsoid,
        title,
        // TODO fix this
        allow_interactive: true,
    };

    info!("Extracting [[{title}]], rev: {rev}, AH: {ah:#?}");

    for template in &templates {
        if check_nobots(template) {
            return Ok(());
        }

        extractors::extract_all(cx, template, &mut ah).await?;
    }

    info!("extraction complete, AH: {ah:#?}");

    ah.into_template(&mut article_history.clone())?;

    let text = parsoid.transform_to_wikitext(&wikicode).await?;
    // we sometimes get newlines leftover at the beginning. We need to clean that up
    let text = text.trim_start();

    if prompt {
        // do a pst
        let val = client
            .post(req::Action::Parse(Parse {
                text: Some(text.into()),
                title: Some(title.into()),
                onlypst: true,
                prop: ParseProp::empty(),
                ..Default::default()
            }))
            .send_and_report_err()
            .await?["parse"]["text"]
            .take();
        let val = val.as_str().unwrap();
        let prev_text = client.fetch_content(title).await?;
        let diff = PrettyDifference {
            expected: &prev_text,
            actual: val,
        };
        println!("{diff}");
        println!("Make edit? [y/N/q(uit)]");
        match &*stdin()
            .lines()
            .next()
            .expect("failed to read line")?
            .as_str()
            .to_ascii_lowercase()
        {
            "y" => {
                client
                    .build_edit(PageSpec::Title(title.to_owned()))
                    .text(text)
                    .summary("merged OTD/ITN/DYK templates to {{article history}} ([[Wikipedia:Bots/Requests for approval/DeadbeefBot 3|BRFA]]) (in trial)")
                    .baserevid(rev as u32)
                    .minor()
                    .bot()
                    .send()
                    .await?;
            }
            "q" | "quit" => {
                process::exit(0);
            }
            _ => {}
        }
    } else {
        client
            .build_edit(PageSpec::Title(title.to_owned()))
            .text(text)
            .summary("merged OTD/ITN/DYK templates to {{article history}} ([[Wikipedia:Bots/Requests for approval/DeadbeefBot 3|BRFA]]) (in trial)")
            .baserevid(rev as u32)
            .minor()
            .bot()
            .send()
            .await?;
    }

    Ok(())
}

pub async fn main() -> Result<()> {
    // TODO testing mode, we be sampling!
    let pages = reqwest::get("https://petscan.wmflabs.org/?psid=26657648&format=plain")
        .await?
        .error_for_status()?
        .text()
        .await?;
    let pages: Vec<_> = pages.lines().collect();
    debug!("got {} pages from petscan", pages.len());

    let pages = pages.choose_multiple(&mut thread_rng(), 3);
    // let pages = vec!["Talk:Reign of Cleopatra"];

    // let client = site_from_url("https://test.wikipedia.org/w/api.php").await?;
    let client = enwiki_bot().await?;

    // let parsoid = parsoid_from_url("https://test.wikipedia.org/api/rest_v1")?;
    let parsoid = enwiki_parsoid()?;

    for page in pages {
        treat(&client, &parsoid, page, false).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }

    Ok(())
}
