use std::fs;

use chrono::TimeZone;
use color_eyre::eyre::{eyre, Context, ContextCompat};
use fancy_regex::Regex;
use futures_util::StreamExt;
use kuchiki::traits::TendrilSink;
use parsoid::WikinodeIterator;
use reqwest::redirect::Policy;
use serde::Deserialize;
use tracing::{info, debug};
use url::Url;
use wiki::api::QueryResponse;
use wiki::builder::SiteBuilder;
use wiki::req::search::{SearchGenerator, SearchInfo, SearchProp};
use wiki::req::{self, Limit, PageSpec, Query, QueryGenerator};

const UA: &str = concat!(
    "DeadbeefBot/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/fee1-dead/deadbeefbot; ent3rm4n@gmail.com) mwapi/0.4.3 parsoid/0.7.4"
);

#[derive(Default, Debug)]
pub struct EditMessage {
    pub links_fixed: usize,
    pub wayback_links_fixed: usize,
}

pub struct SiteCfg {
    pub name: &'static str,
    pub format: fn(EditMessage) -> String,
    pub api_url: &'static str,
    pub parsoid_url: &'static str,
}

macro_rules! pluralize {
    ($x: expr) => {
        if $x == 1 {
            ""
        } else {
            "s"
        }
    };
}

const SUPPORTED_SITES: &'static [SiteCfg] = &[
    SiteCfg {
        name: "English Wikipedia",
        api_url: "https://en.wikipedia.org/w/api.php",
        parsoid_url: "https://en.wikipedia.org/api/rest_v1",
        format: |EditMessage {
                     links_fixed,
                     wayback_links_fixed,
                 }| {
            let lpl = pluralize!(links_fixed);
            let wpl = pluralize!(wayback_links_fixed);

            format!(
                "Removing Twitter tracker params \
                ([[Wikipedia:Bots/Requests_for_approval/ScannerBot|BRFA]]) \
                ({links_fixed} link{lpl} fixed, \
                    {wayback_links_fixed} archive link{wpl} fixed)"
            )
        },
    },
    /*SiteCfg {
        name: "Chinese Wikipedia",
        api_url: "https://zh.wikipedia.org/w/api.php",
        parsoid_url: "https://zh.wikipedia.org/api/rest_v1",
        format: |EditMessage {
                     links_fixed,
                     wayback_links_fixed,
                 }| {
            format!("BOT：已从{links_fixed}个Twitter外链删除追踪参数，同时修改{wayback_links_fixed}个存档链接 \
            ([[Wikipedia:机器人/申请/DeadbeefBot|BRFA]])")
        },
    },*/
];

async fn run(site: &SiteCfg) -> color_eyre::Result<()> {
    info!("Running on {}", site.name);

    let client = SiteBuilder::new(site.api_url)
        .oauth(
            fs::read_to_string("./token.secret")
                .context("please put oauth2 token in token.secret")?
                .trim(),
        )
        .user_agent(UA)
        .build()
        .await?;

    let parsoid = parsoid::Client::new(site.parsoid_url, UA)?;
    let c = reqwest::Client::builder()
        .redirect(Policy::none())
        .build()?;
    let re = Regex::new(
        r"(?<!\?url=|/|cache:)https?://(?:mobile\.)?twitter\.com/\w+/status/\d+\?[^\s}<|]+",
    )?;
    let wre = Regex::new(
        r"https?://web\.archive\.org/web/([0-9]+)/(https?://(?:mobile\.)?twitter\.com/\w+/status/\d+(?:\?[^\s}<|]+)?)",
    )?;

    const SEARCH: &str =
        r"insource:/twitter\.com\/[a-zA-Z0-9]+\/status\/[0-9]+\/?\?([st]|cxt|ref_[a-z]+)=/";

    static BAD_PARAMS: &[&str] = &["cxt", "ref_src", "ref_url", "s", "t"];

    #[derive(Deserialize)]
    struct Revision {
        revid: u32,
    }
    #[derive(Deserialize)]
    struct SearchResult {
        pageid: u32,
        title: String,
        revisions: Vec<Revision>,
    }
    #[derive(Deserialize)]
    struct SearchResponseBody {
        pages: Vec<SearchResult>,
    }

    let mut stream = client.query_all(Query {
        prop: Some(
            req::QueryProp::Revisions(req::QueryPropRevisions {
                prop: req::RvProp::IDS,
                slots: req::RvSlot::Main.into(),
                limit: req::Limit::None,
            })
            .into(),
        ),
        generator: Some(QueryGenerator::Search(SearchGenerator {
            // search: SEARCH.into(),
            // namespace: "0".into(),
            limit: Limit::Value(20), // content too big
            offset: None,
            info: SearchInfo::empty(),
            prop: SearchProp::empty(),
            search: "7YzahhfuteRXHs5EtZcP".into(),
            namespace: "2".into(),
        })),
        ..Default::default()
    });

    fn treat(s: &str) -> color_eyre::Result<String> {
        let mut url = Url::parse(s)?;
        let mut s = form_urlencoded::Serializer::new(String::new());
        let mut some = false;
        for (key, value) in url.query_pairs() {
            if !BAD_PARAMS.contains(&&*key) {
                s.append_pair(&key, &value);
                some = true;
            }
        }
        if some {
            url.set_query(Some(&s.finish()));
        } else {
            url.set_query(None);
        }
        Ok(url.into())
    }

    while let Some(it) = stream.next().await {
        let it = it?;
        let res: QueryResponse<SearchResponseBody> = serde_json::from_value(it)?;
        for mut page in res.query.pages {
            let rev = page.revisions.pop().unwrap();
            let page_id = page.pageid;
            let rev_id = rev.revid;

            let mut edit_msg = EditMessage::default();

            let code = parsoid
                .get_revision(&page.title, rev_id as u64)
                .await?
                .into_mutable();
            for template in code.filter_templates()? {
                let wre = &wre;
                let c = &c;
                let edit_msg = &mut edit_msg;
                let re: color_eyre::Result<()> = (|| async move {
                    let name = template.name().to_lowercase();
                    debug!(?name);
                    if name != "template:cite web" && name != "template:cite tweet" {
                        return Ok(());
                    }
                    let param = template.param("archive-url").context("archive url")?;
                    let captures = wre.captures(&param)?.context("match regex")?;
                    let timestamp = &captures[1];
                    let url = &captures[2];
                    let new_url = treat(url)?;
                    debug!(?url, ?new_url);
                    if new_url == url {
                        return Ok(());
                    }

                    // https://web.archive.org/web/20220624234724/https://twitter.com/MariahCarey/status/1314585670644641794
                    let url = format!("https://web.archive.org/web/{timestamp}/{new_url}");
                    let resp = c.get(&url).send().await?;
                    debug!(?resp);
                    // x-archive-redirect-reason: found capture at 20220624234724
                    // location https://web.archive.org/web/20220624234724/https://twitter.com/MariahCarey/status/1314585670644641794
                    let mut actual_url = url;
                    if resp.status().as_u16() == 302
                        && resp
                            .headers()
                            .get("x-archive-redirect-reason")
                            .and_then(|v| v.to_str().ok())
                            .map_or(false, |s| s.starts_with("found capture at"))
                    {
                        resp.headers()
                            .get("location")
                            .and_then(|v| v.to_str().ok())
                            .map(|v| actual_url = v.to_owned())
                            .context("location")?;
                    }

                    let text = c
                        .get(&actual_url)
                        .send()
                        .await?
                        .error_for_status()?
                        .text()
                        .await?;
                    let html = kuchiki::parse_html().one(text);

                    let title = html
                        .select_first("title")
                        .map(|t| t.text_contents())
                        .map_err(|_| eyre!("title"))?;
                    if title.trim() == "Twitter" {
                        Err(eyre!("buggy url"))?;
                    }

                    let time = wre
                        .captures(&actual_url)?
                        .and_then(|c| c.get(1))
                        .context("url should match regex")?
                        .as_str();

                    let time = chrono::Utc.datetime_from_str(time, "%Y%m%d%H%M%S")?;

                    let date = time.format("%Y-%m-%d");
                    template.set_param("archive-url", &actual_url).unwrap();
                    template
                        .set_param("archive-date", &date.to_string())
                        .unwrap();
                    edit_msg.wayback_links_fixed += 1;
                    Ok(())
                })()
                .await;

                if let Err(e) = re {
                    info!("did not fix archive: {e}");
                }
            }

            let text = parsoid.transform_to_wikitext(&code).await?;
            let mut newtext = text.clone();

            let matches: Vec<_> = re.find_iter(&text).collect();
            for m in matches.into_iter().rev() {
                let m = m?;
                let new_url = treat(m.as_str())?;

                if new_url != m.as_str() {
                    // yay!
                    newtext.replace_range(m.range(), &new_url);
                    edit_msg.links_fixed += 1;
                }
            }

            debug!(?edit_msg);
            if edit_msg.links_fixed + edit_msg.wayback_links_fixed > 0 {
                client
                    .build_edit(PageSpec::PageId(page_id))
                    .text(newtext)
                    .summary((site.format)(edit_msg))
                    .baserevid(rev_id)
                    .send()
                    .await?;

                panic!();
            }
        }
    }

    Ok(())
}

async fn real_main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    for site in SUPPORTED_SITES {
        run(site).await?;
    }
    Ok(())
}

fn main() -> color_eyre::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(real_main())
}
