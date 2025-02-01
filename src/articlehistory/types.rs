use std::num::NonZeroUsize;

use chrono::{DateTime, TimeZone, Utc};
use color_eyre::eyre::bail;
use parsoid::map::IndexMap;
use parsoid::Template;
use serde::Deserialize;
use timelib::Timezone;
use tracing::info;

use super::builder::{AddToParams, ParamBuilder};
use super::Result;

#[derive(Clone, Debug)]
pub struct PreserveDate {
    pub date: DateTime<Utc>,
    pub orig: String,
}

impl PreserveDate {
    pub fn try_from_string(x: String) -> Result<Self, String> {
        let date = timelib::strtotime(&x, None, &Timezone::parse("UTC").unwrap())?;
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
    ///
    /// See also https://en.wikipedia.org/wiki/Module:Article_history/config#L-1112
    pub fn opt_to_current_status(&self) -> Result<Option<&'static str>> {
        use ActionKind::*;
        let res = self.result.as_deref().map(str::to_ascii_lowercase);
        match (self.kind, res.as_deref()) {
            (Fac, Some("promoted" | "pass" | "passed")) => Ok(Some("FA")),
            (Fac, Some("not promoted" | "fail" | "failed")) => Ok(Some("FFAC")),
            (Fac, _) => bail!("unknown fac"),

            (Far, Some("kept" | "pass" | "passed" | "keep")) => Ok(Some("FA")),
            (Far, Some("demoted" | "removed" | "remove" | "fail" | "failed")) => Ok(Some("FFA")),
            (Far, _) => bail!("unknown far"),

            (Rbp, _) => bail!("idk how to deal with rbp"),
            (Bp, _) => Ok(None),

            (Flc, Some("promoted" | "pass" | "passed")) => Ok(Some("FL")),
            (Flc, Some("not promoted" | "fail" | "failed")) => Ok(Some("FFLC")),
            (Flc, _) => bail!("unknown flc"),

            (Flr, Some("kept" | "pass" | "passed" | "keep")) => Ok(Some("FL")),
            (Flr, Some("demoted" | "removed" | "remove" | "fail" | "failed")) => Ok(Some("FFL")),
            (Flr, _) => bail!("unknown flr"),

            (Ftc, _) => Ok(None),
            (Ftr, _) => Ok(None),

            (Fproc, Some("promoted" | "pass" | "passed")) => Ok(Some("FPO")),
            (Fproc, Some("not promoted" | "fail" | "failed")) => Ok(Some("FFPOC")),
            (Fproc, _) => bail!("unknown fproc"),

            (Fpor, Some("kept" | "pass" | "passed" | "keep")) => Ok(Some("FPO")),
            (Fpor, Some("demoted" | "removed" | "remove" | "fail" | "failed")) => Ok(Some("FFPO")),
            (Fpor, _) => bail!("unknown fpor"),

            (Gan, Some("listed" | "promoted" | "pass" | "passed")) => Ok(Some("GA")),
            (Gan, Some("not listed" | "not promoted" | "fail" | "failed")) => Ok(Some("FGAN")),
            (Gan, _) => bail!("unknown gan"),

            (Gar, Some("kept" | "pass" | "passed" | "keep")) => Ok(Some("GA")),
            (Gar, Some("delisted" | "fail" | "failed")) => Ok(Some("DGA")),
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
        let status = self
            .actions
            .iter()
            .filter_map(|action| action.opt_to_current_status().transpose())
            .collect::<Result<Vec<_>>>()?;
        let status = {
            let mut s = String::new();
            let mut status = status;
            status.reverse();
            info!(?status, "status before");
            {
                // take most recent GA-related status
                let mut found_ga = false;
                status.retain(|x| match *x {
                    "GA" | "FGAN" | "DGA" if found_ga => false,
                    // note that a former FA trumps GA.
                    "FFA" | "FA" | "GA" | "FGAN" | "DGA" => {
                        found_ga = true;
                        true
                    }
                    _ => true,
                });
            }
            {
                // take most recent "featured"-related status
                let mut found_fa = false;
                status.retain(|x| match *x {
                    "FGAN" => true,
                    s if s.starts_with('F') && found_fa => false,
                    s if s.starts_with('F') => {
                        found_fa = true;
                        true
                    }
                    _ => true,
                });
            }
            info!(?status, "status after");

            // most recent action first
            for status in status {
                if !s.is_empty() {
                    s.push('/');
                }
                s.push_str(status);
            }
            s
        };
        if status.contains('/') && !["FFA/GA", "FFAC/GA"].contains(&&*status) {
            bail!("multi-status is invalid: {status}");
        }
        if self.currentstatus.as_ref().is_some_and(|orig_status| {
            // either they have to completely match, or our status is more specific
            // than the previous status
            orig_status != &status
                && !status.contains(&format!("/{orig_status}"))
                && !status.contains(&format!("{orig_status}/"))
        }) {
            bail!(
                "current status mismatch: {:?} vs {:?}",
                self.currentstatus,
                status
            )
        }

        self.currentstatus = Some(status);
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
