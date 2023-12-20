//! Removes twitter.com trackers in URLs.

use std::time::Duration;

use chrono::NaiveDateTime;
use color_eyre::eyre::{eyre, ContextCompat};
use fancy_regex::Regex;
use futures_util::StreamExt;
use kuchiki::traits::TendrilSink;
use parsoid::WikinodeIterator;
use reqwest::redirect::Policy;
use tracing::{debug, info};
use url::Url;
use wiki::api::QueryResponse;
use wiki::req::search::{SearchGenerator, SearchInfo, SearchProp};
use wiki::req::{Limit, PageSpec};

use crate::{
    check_nobots, parsoid_from_url, search_with_rev_ids, site_from_url, SearchResponseBody,
};

pub async fn main() -> color_eyre::Result<()> {
    for site in SUPPORTED_SITES.into_iter().rev() {
        run(site).await?;
    }
    Ok(())
}

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

const SUPPORTED_SITES: &[SiteCfg] = &[
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
            let wayback = if wayback_links_fixed > 0 {
                format!(", {wayback_links_fixed} archive link{wpl} fixed",)
            } else {
                "".to_string()
            };

            format!(
                "Removing Twitter tracker params \
                ([[Wikipedia:Bots/Requests for approval/DeadbeefBot 1|BRFA]]) \
                ({links_fixed} link{lpl} fixed{wayback})"
            )
        },
    },
    SiteCfg {
        name: "Chinese Wikipedia",
        api_url: "https://zh.wikipedia.org/w/api.php",
        parsoid_url: "https://zh.wikipedia.org/api/rest_v1",
        format: |EditMessage {
                     links_fixed,
                     wayback_links_fixed,
                 }| {
            let wayback = if wayback_links_fixed != 0 {
                format!("，同时修改{wayback_links_fixed}个存档链接")
            } else {
                String::new()
            };
            format!(
                "BOT：已从{links_fixed}个Twitter外链删除追踪参数{wayback} \
            ([[Wikipedia:机器人/申请/DeadbeefBot|BRFA]])"
            )
        },
    },
];

async fn run(site: &SiteCfg) -> color_eyre::Result<()> {
    info!("Running on {}", site.name);

    let client = site_from_url(site.api_url).await?;
    let parsoid = parsoid_from_url(site.parsoid_url)?;

    let c = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(5))
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

    let mut stream = search_with_rev_ids(
        &client,
        SearchGenerator {
            search: SEARCH.into(),
            namespace: Some("0".into()),
            limit: Limit::Value(20), // content too big
            offset: None,
            info: SearchInfo::empty(),
            prop: SearchProp::empty(),
            // search: "7YzahhfuteRXHs5EtZcP".into(),
            // namespace: "2".into(),
        },
    )
    .boxed();

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
        'treat: for mut page in res.query.pages {
            let rev = page.revisions.pop().unwrap();
            let page_id = page.pageid;
            let rev_id = rev.revid;

            let mut edit_msg = EditMessage::default();

            debug!(?page);

            let code = parsoid
                .get_revision(&page.title, rev_id as u64)
                .await?
                .into_mutable();
            for template in code.filter_templates()? {
                if check_nobots(&template) {
                    continue 'treat;
                }

                let wre = &wre;
                let c = &c;
                let edit_msg = &mut edit_msg;
                let re: color_eyre::Result<()> = (|| async move {
                    let name = template.name().to_lowercase();
                    if name != "template:cite web" && name != "template:cite tweet" {
                        return Ok(());
                    }
                    let Some(param) = template.param("archive-url") else {
                        return Ok(());
                    };
                    let Some(captures) = wre.captures(&param)? else {
                        return Ok(());
                    };
                    let timestamp = &captures[1];
                    let url = &captures[2];
                    let new_url = treat(url)?;
                    debug!(?url, ?new_url);
                    if new_url == url {
                        return Ok(());
                    }

                    // https://web.archive.org/web/timemap/?url=https://twitter.com/MariahCarey/status/1314585670644641794&collapse=timestamp&fl=timestamp
                    let url = Url::parse_with_params(
                        "https://web.archive.org/web/timemap/",
                        [
                            ("url", &*new_url),
                            ("collapse", "timestamp"),
                            ("fl", "timestamp"),
                        ],
                    )?;
                    let resp = c.get(url).timeout(Duration::from_secs(3)).send().await?;
                    debug!(?resp);
                    let resp = resp.error_for_status()?;
                    let timestamps = resp.text().await?;

                    for new_timestamp in timestamps.lines() {
                        // https://web.archive.org/web/20220624234724/https://twitter.com/MariahCarey/status/1314585670644641794
                        let actual_url =
                            format!("https://web.archive.org/web/{new_timestamp}/{new_url}");
                        debug!(?timestamp, ?actual_url);

                        let res = (|| async {
                            // prevent spamming archive.org
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            let text = c
                                .get(&actual_url)
                                .timeout(Duration::from_secs(3))
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

                            let _ = html
                                .select_first("[aria-label=\"Timeline: Conversation\"]")
                                .or_else(|_| {
                                    html.select_first(
                                        ".tweet[data-tweet-stat-initialized=\"true\"]",
                                    )
                                })
                                .map_err(|_| eyre!("main content"))?;

                            let time = wre
                                .captures(&actual_url)?
                                .and_then(|c| c.get(1))
                                .context("url should match regex")?
                                .as_str();

                            let time = NaiveDateTime::parse_from_str(time, "%Y%m%d%H%M%S")?;

                            let date = time.format("%Y-%m-%d");
                            template.set_param("archive-url", &actual_url).unwrap();
                            template
                                .set_param("archive-date", &date.to_string())
                                .unwrap();
                            color_eyre::Result::<()>::Ok(())
                        })()
                        .await;

                        match res {
                            Ok(()) => {
                                edit_msg.wayback_links_fixed += 1;
                                break;
                            }
                            Err(e) => {
                                debug!("did not fix: {}", e.to_string());
                            }
                        }
                    }

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
                    .minor()
                    .bot()
                    .send()
                    .await?;

                // TODO remove this
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }

    Ok(())
}
