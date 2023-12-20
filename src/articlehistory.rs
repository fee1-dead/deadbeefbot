//! Merge `{{On this day}}` templates into `{{article history}}` if exists.

use std::io::stdin;
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::process;

use chrono::{DateTime, TimeZone, Utc};
use color_eyre::eyre::bail;
use colored_diff::PrettyDifference;
use parsoid::map::IndexMap;
use parsoid::{Template, WikiMultinode, WikinodeIterator};
use serde::Deserialize;
use tracing::{debug, info, warn};
use wiki::api::RequestBuilderExt;
use wiki::req::parse::{Parse, ParseProp};
use wiki::req::{self, PageSpec};

use crate::extractors::ExtractContext;
use crate::{check_nobots, enwiki_bot, enwiki_parsoid, Result};
#[allow(unused_imports)]
use crate::{parsoid_from_url, site_from_url};

mod builder;

use builder::{AddToParams, ParamBuilder};

/// taken from [here](https://en.wikipedia.org/wiki/Special:WhatLinksHere?target=Template%3AArticle+history&namespace=&hidetrans=1&hidelinks=1).
///
/// This is case insensitive. Let's hope that people don't use the other capitalizations for a different thing on article talk pages.
const AH: &[&str] = &[
    "article history",
    "article milestones",
    "articlemilestones",
    "articlehistory",
];

#[derive(Clone, Debug)]
pub struct PreserveDate {
    pub date: DateTime<Utc>,
    pub orig: String,
}

impl PreserveDate {
    pub fn try_from_string(x: String) -> Result<Self, String> {
        let date = timelib::strtotime(x.clone(), None, None)?;
        Ok(PreserveDate {
            date: Utc.timestamp_opt(date, 0).unwrap(),
            orig: x,
        })
    }
}

impl<'de> Deserialize<'de> for PreserveDate {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::try_from_string(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl PartialEq for PreserveDate {
    fn eq(&self, other: &Self) -> bool {
        self.date == other.date
    }
}

impl Eq for PreserveDate {}

impl PartialOrd for PreserveDate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.date.cmp(&other.date))
    }
}

impl Ord for PreserveDate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.date.cmp(&other.date)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ActionKind {
    Fac,
    Far,
    Rbp,
    Bp,
    Flc,
    Flr,
    Ftc,
    Ftr,
    Fproc,
    Fpor,
    Gan,
    Gar,
    Gtc,
    Pr,
    Wpr,
    War,
    Afd,
    Mfd,
    Tfd,
    Csd,
    Prod,
    Drv,
}

impl ActionKind {
    pub fn as_str(&self) -> &'static str {
        use ActionKind::*;
        match self {
            Fac => "FAC",
            Far => "FAR",
            Rbp => "RBP",
            Bp => "BP",
            Flc => "FLC",
            Flr => "FLR",
            Ftc => "FTC",
            Ftr => "FTR",
            Fproc => "FPROC",
            Fpor => "FPOR",
            Gan => "GAN",
            Gar => "GAR",
            Gtc => "GTC",
            Pr => "PR",
            Wpr => "WPR",
            War => "WAR",
            Afd => "AFD",
            Mfd => "MFD",
            Tfd => "TFD",
            Csd => "CSD",
            Prod => "PROD",
            Drv => "DRV",
        }
    }
}

impl<'de> Deserialize<'de> for ActionKind {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?.to_lowercase();
        match &*s {
            "fac" => Ok(ActionKind::Fac),
            "far" => Ok(ActionKind::Far),
            "rbp" => Ok(ActionKind::Rbp),
            "bp" => Ok(ActionKind::Bp),
            "flc" => Ok(ActionKind::Flc),
            "flr" => Ok(ActionKind::Flr),
            "ftc" => Ok(ActionKind::Ftc),
            "ftr" => Ok(ActionKind::Ftr),
            "fproc" => Ok(ActionKind::Fproc),
            "fpor" => Ok(ActionKind::Fpor),
            "gan" => Ok(ActionKind::Gan),
            "gar" => Ok(ActionKind::Gar),
            "gtc" => Ok(ActionKind::Gtc),
            "pr" => Ok(ActionKind::Pr),
            "wpr" => Ok(ActionKind::Wpr),
            "war" => Ok(ActionKind::War),
            "afd" => Ok(ActionKind::Afd),
            "mfd" => Ok(ActionKind::Mfd),
            "tfd" => Ok(ActionKind::Tfd),
            "csd" => Ok(ActionKind::Csd),
            "prod" => Ok(ActionKind::Prod),
            "drv" => Ok(ActionKind::Drv),
            _ => Err(serde::de::Error::custom(format!(
                "unknown action kind: {}",
                s
            ))),
        }
    }
}

#[derive(Deserialize, Debug)]
pub struct Action {
    #[serde(rename = "")]
    pub kind: ActionKind,
    pub date: PreserveDate,
    pub link: Option<String>,
    pub result: Option<String>,
    pub oldid: Option<String>,
}

impl Action {
    /// Extract current status based on this table: https://en.wikipedia.org/wiki/Template:Article_history#How_to_use_in_practice
    ///
    /// If this returns Err then we've got our assumptions wrong and this page is untreatable.
    pub fn opt_to_current_status(&self) -> Result<Option<&'static str>> {
        use ActionKind::*;
        let res = self.result.as_deref().map(str::to_ascii_lowercase);
        match (self.kind, res.as_deref()) {
            (Fac, Some("promoted")) => Ok(Some("FA")),
            (Fac, Some("failed")) => Ok(Some("FFAC")),
            (Fac, _) => bail!("unknown fac"),

            (Far, Some("kept")) => Ok(Some("FA")),
            (Far, Some("removed")) => Ok(Some("FFA")),
            (Far, _) => bail!("unknown far"),

            (Rbp, _) => bail!("idk how to deal with rbp"),
            (Bp, _) => Ok(None),

            (Flc, Some("promoted")) => Ok(Some("FL")),
            (Flc, Some("failed")) => Ok(Some("FFLC")),
            (Flc, _) => bail!("unknown flc"),

            (Flr, Some("kept")) => Ok(Some("FL")),
            (Flr, Some("removed")) => Ok(Some("FFL")),
            (Flr, _) => bail!("unknown flr"),

            (Ftc, _) => Ok(None),
            (Ftr, _) => Ok(None),

            (Fproc, Some("promoted")) => Ok(Some("FPO")),
            (Fproc, Some("failed")) => Ok(Some("FFPOC")),
            (Fproc, _) => bail!("unknown fproc"),

            (Fpor, Some("kept")) => Ok(Some("FPO")),
            (Fpor, Some("removed")) => Ok(Some("FFPO")),
            (Fpor, _) => bail!("unknown fpor"),

            (Gan, Some("listed")) => Ok(Some("GA")),
            (Gan, Some("failed")) => Ok(Some("FGAN")),
            (Gan, _) => bail!("unknown gan"),

            (Gar, Some("kept" | "listed")) => Ok(Some("GA")),
            (Gar, Some("delisted")) => Ok(Some("DGA")),
            (Gar, _) => bail!("unknown gar"),

            (Gtc | Pr | Wpr | War | Afd | Mfd | Tfd | Csd | Prod | Drv, _) => Ok(None),
        }
    }
}

impl AddToParams for Action {
    fn add_to_params(self, i: NonZeroUsize, params: &mut ParamBuilder<'_>) {
        params.addnl(format!("action{i}"), self.kind.as_str());
        params.addnl(format!("action{i}date"), self.date.orig);
        params.addnl_opt(format!("action{i}link"), self.link);
        params.addnl_opt(format!("action{i}result"), self.result);
        params.addnl_opt(format!("action{i}oldid"), self.oldid);
        params.newline()
    }
}

#[derive(Deserialize, Debug)]
pub struct Dyk {
    pub date: PreserveDate,
    pub entry: Option<String>,
    pub nom: Option<String>,
    #[serde(default)]
    pub ignoreerror: bool,
}

impl AddToParams for Dyk {
    fn add_to_params(self, i: NonZeroUsize, params: &mut ParamBuilder<'_>) {
        let i = if i.get() == 1 {
            String::new()
        } else {
            format!("{i}")
        };
        params.add(format!("dyk{i}date"), self.date.orig);
        params.add_opt(format!("dyk{i}entry"), self.entry);
        params.add_opt(format!("dyk{i}nom"), self.nom);
        params.add_flag(format!("dyk{i}ignoreerror"), self.ignoreerror);
        params.newline();
    }
}

#[derive(Deserialize, Debug)]
pub struct Itn {
    pub date: PreserveDate,
    pub link: Option<String>,
}

impl AddToParams for Itn {
    fn add_to_params(self, i: NonZeroUsize, params: &mut ParamBuilder<'_>) {
        params.add(format!("itn{i}date"), self.date.orig);
        params.add_opt(format!("itn{i}link"), self.link);
        params.newline();
    }
}

#[derive(Deserialize, Debug)]
pub struct Otd {
    pub date: PreserveDate,
    pub oldid: Option<String>,
    pub link: Option<String>,
}

impl AddToParams for Otd {
    fn add_to_params(self, i: NonZeroUsize, params: &mut ParamBuilder<'_>) {
        params.add(format!("otd{i}date"), self.date.orig);
        params.add_opt(format!("otd{i}oldid"), self.oldid);
        params.add_opt(format!("otd{i}link"), self.link);
        params.newline();
    }
}

#[derive(Debug, Deserialize)]
pub struct FeaturedTopic {
    pub name: String,
    #[serde(default)]
    pub main: bool,
}

impl AddToParams for FeaturedTopic {
    fn add_to_params(self, i: NonZeroUsize, params: &mut ParamBuilder<'_>) {
        let i = if i.get() == 1 {
            String::new()
        } else {
            format!("{i}")
        };
        params.addnl(format!("ft{i}name"), self.name);
        params.addnl_flag(format!("ft{i}main"), self.main);
    }
}

/// Rules:
///  * It should reorder existing actions based on their dates.
///  * it should compute the current status based on the latest action, and error if there is mismatch.
///  * It should fold over actions from other templates, and make changes to current status if necessary.
///
#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct ArticleHistory {
    pub actions: Vec<Action>,

    pub currentstatus: Option<String>,
    pub maindate: Option<PreserveDate>,
    pub maindate2: Option<PreserveDate>,
    pub itns: Vec<Itn>,
    pub dyks: Vec<Dyk>,
    pub otds: Vec<Otd>,
    #[serde(default)]
    pub four: bool,
    pub featured_topics: Vec<FeaturedTopic>,
    pub topic: Option<String>,

    #[serde(default)]
    pub collapse: bool,
    #[serde(default)]
    pub small: bool,
}

impl ArticleHistory {
    pub fn sort_and_update_status(&mut self) -> Result<()> {
        self.actions.sort_by_key(|action| action.date.date);
        let status =
            self.actions
                .iter()
                .try_rfold(ControlFlow::Continue(()), |x, action| -> Result<_> {
                    if let ControlFlow::Break(br) = x {
                        return Ok(ControlFlow::Break(br));
                    }
                    if let Some(stat) = action.opt_to_current_status()? {
                        Ok(ControlFlow::Break(stat))
                    } else {
                        Ok(ControlFlow::Continue(()))
                    }
                })?;
        match status {
            ControlFlow::Break(status) => {
                if self.currentstatus.as_ref().is_some_and(|x| x != status) {
                    bail!(
                        "current status mismatch: {:?} vs {:?}",
                        self.currentstatus,
                        status
                    )
                }

                self.currentstatus = Some(status.into())
            }
            ControlFlow::Continue(()) => {}
        }
        Ok(())
    }

    /// Does the final job of re-serializing this into the template.
    pub fn into_template(mut self, t: &mut Template) -> Result<()> {
        self.sort_and_update_status()?;
        //        t.set_name("Article history{{subst:User:0xDeadbeef/newline}}".into())?;

        let mut params = IndexMap::new();

        let mut builder = ParamBuilder::new(&mut params);

        builder.add_all(self.actions);
        builder.addnl_opt("currentstatus", self.currentstatus);
        builder.addnl_opt("maindate", self.maindate.map(|x| x.orig));
        builder.addnl_opt("maindate2", self.maindate2.map(|x| x.orig));
        builder.add_all(self.itns);
        builder.add_all(self.dyks);
        builder.add_all(self.otds);
        builder.addnl_flag("four", self.four);
        builder.add_all(self.featured_topics);
        builder.addnl_opt("topic", self.topic);
        builder.addnl_flag("collapse", self.collapse);
        builder.addnl_flag("small", self.small);

        t.set_params(params)?;

        Ok(())
    }
}

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
                    // Parsoid doesn't change the parameter position if the parameter name isn't
                    // changed. We insert {{subst:null}} at the end to trick the parser.
                    v.push((
                        format!("{prefix}{}{{{{subst:null}}}}", stringify!($value)),
                        x,
                    ));
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
    }, /*
       FailedGa {
           date: Option<String>,
           oldid: Option<String>,
           page: Option<String>,
           topic: Option<String>,
       },*/
}

impl ParameterType {
    pub fn is_empty(&self) -> bool {
        fn check(x: &Option<String>) -> bool {
            x.as_deref().map(str::trim).map_or(true, str::is_empty)
        }
        match self {
            Self::Itn { date, link } => check(date) && check(link),
            Self::Dyk {
                date: a,
                entry: b,
                nom: c,
            }
            | Self::Otd {
                date: a,
                oldid: b,
                link: c,
            } => check(a) && check(b) && check(c),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ty {
    Itn,
    Dyk,
    Otd,
}

mod extract;

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

    let article_history = match templates.iter().find(|x| {
        AH.contains(
            &&*x.name()
                .trim_start_matches("Template:")
                .to_ascii_lowercase(),
        )
    }) {
        Some(article_history) => {
            article_history
                .set_name("Article history{{subst:User:0xDeadbeef/newline}}".to_owned())?;
            article_history
        }
        None => {
            // mount an article history template.
            let Some(banner) = templates.iter().find(|x| {
                x.name()
                    .trim_start_matches("Template:")
                    .to_ascii_lowercase()
                    == "wikiproject banner shell"
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

    let Some(mut ah) = crate::articlehistory::extract::extract_info(article_history)? else {
        return Ok(());
    };

    let cx = ExtractContext {
        client,
        parsoid,
        title,
    };

    info!("Extracting [[{title}]], rev: {rev}, AH: {ah:#?}");

    for template in &templates {
        if check_nobots(template) {
            return Ok(());
        }

        crate::extractors::extract_all(cx, template, &mut ah)?;
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
                    .summary("merged OTD/ITN/DYK templates to {{article history}} ([[Wikipedia:Bots/Requests for approval/DeadbeefBot 2|BRFA]])")
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
            .summary("merged OTD/ITN/DYK templates to {{article history}} ([[Wikipedia:Bots/Requests for approval/DeadbeefBot 2|BRFA]])")
            .baserevid(rev as u32)
            .minor()
            .bot()
            .send()
            .await?;
    }

    Ok(())
}

pub async fn main() -> Result<()> {
    let pages = reqwest::get("https://petscan.wmflabs.org/?psid=24768643&format=plain")
        .await?
        .text()
        .await?;
    let pages: Vec<_> = pages.lines().collect();
    // let pages = vec!["Talk:Ebla"];

    debug!("got {} pages from petscan", pages.len());

    // let client = site_from_url("https://test.wikipedia.org/w/api.php").await?;
    let client = enwiki_bot().await?;

    // let parsoid = parsoid_from_url("https://test.wikipedia.org/api/rest_v1")?;
    let parsoid = enwiki_parsoid()?;

    for page in pages {
        treat(&client, &parsoid, page, true).await?;
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }

    Ok(())
}
