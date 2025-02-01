//! Merge `{{On this day}}` templates into `{{article history}}` if exists.

use std::fs::{File, OpenOptions};
use std::io::stdin;
use std::process;

use color_eyre::eyre::bail;
use colored_diff::PrettyDifference;
use extractors::ExtractContext;
use parsoid::{Template, WikiMultinode, WikinodeIterator};
use rand::seq::SliceRandom;
use rand::thread_rng;
use tracing::{debug, info, trace, warn};
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

pub async fn treat_inner(
    client: &wiki::Bot,
    parsoid: &parsoid::Client,
    title: &str,
    prompt: bool,
) -> Result<()> {
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
                bail!("skipping, article doesn't have wp banner shell");
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
        allow_interactive: false,
    };

    info!("Extracting [[{title}]], rev: {rev}");
    trace!("AH: {ah:#?}");

    for template in &templates {
        if check_nobots(template) {
            return Ok(());
        }

        extractors::extract_all(cx, template, &mut ah).await?;
    }

    trace!("extraction complete, AH: {ah:#?}");

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
                    .summary("implementing {{article history}} ([[Wikipedia:Bots/Requests for approval/DeadbeefBot 3|BRFA]])")
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
            .summary("implementing {{article history}} ([[Wikipedia:Bots/Requests for approval/DeadbeefBot 3|BRFA]])")
            .baserevid(rev as u32)
            .minor()
            .bot()
            .send()
            .await?;
    }

    Ok(())
}

pub async fn treat(
    client: &wiki::Bot,
    parsoid: &parsoid::Client,
    title: &str,
    prompt: bool,
    cnt: &mut u64,
    f: &mut File,
) -> Result<()> {
    use std::io::Write;
    info!("Treating [[{title}]]");

    if let Err(e) = treat_inner(client, parsoid, title, prompt).await {
        warn!(?e);
        writeln!(f, "Error while treating [[{title}]]: {e}")?;
    } else {
        *cnt += 1;
    }

    Ok(())
}

pub async fn main(petscan: &str) -> Result<()> {
    let pages = reqwest::get(petscan)
        .await?
        .error_for_status()?
        .text()
        .await?;
    // let pages: Vec<_> = pages.lines().collect();
    // let pages = std::fs::read_to_string("ptemp3.txt")?;
    let mut pages: Vec<_> = pages.lines().collect();
    debug!("got {} pages from petscan", pages.len());

    pages.shuffle(&mut thread_rng());
    // let pages = pages.choose_multiple(&mut thread_rng(), 10);
    // let pages = vec!["Talk:Warsaw Uprising (1794)"];

    // let client = site_from_url("https://test.wikipedia.org/w/api.php").await?;
    let client = enwiki_bot().await?;

    // let parsoid = parsoid_from_url("https://test.wikipedia.org/api/rest_v1")?;
    let parsoid = enwiki_parsoid()?;

    let mut count = 0;
    let mut f = OpenOptions::new()
        .append(true)
        .create(true)
        .open("./logs.txt")?;
    for page in pages {
        treat(&client, &parsoid, page, false, &mut count, &mut f).await?;
        /* if count >= 1 {
            return Ok(())
        } */
        tokio::time::sleep(tokio::time::Duration::from_secs(6)).await;
    }

    Ok(())
}
