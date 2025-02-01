use std::io::stdin;

use color_eyre::eyre::bail;
use serde::Deserialize;

// use serde_json::Value;
use super::{ExtractContext, Extractor};
use crate::articlehistory::{Action, ActionKind, ArticleHistory, PreserveDate};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OldPeerReview {
    // Default: 1
    pub archive: Option<String>,
    pub reviewedname: Option<String>,
    pub archivelink: Option<String>,
    #[serde(rename = "ID")]
    pub id: Option<String>,
    pub date: Option<PreserveDate>,
}

pub struct OldPrExtractor;

#[derive(Deserialize)]
pub struct ApiResponse {
    pub count: u64,
    // pub limit: bool,
}

impl Extractor for OldPrExtractor {
    type Value = OldPeerReview;

    const ALIAS: &'static [&'static str] = &["old peer review", "oldpeerreview"];

    async fn merge_value_into<'cx>(
        &self,
        cx: ExtractContext<'cx>,
        value: OldPeerReview,
        into: &mut ArticleHistory,
    ) -> crate::Result<()> {
        let title = cx.title.strip_prefix("Talk:").unwrap();
        let link = if let Some(link) = value.archivelink {
            link
        } else {
            format!(
                "Wikipedia:Peer review/{}/archive{}",
                value.reviewedname.as_deref().unwrap_or(title),
                value.archive.unwrap_or_else(|| "1".into())
            )
        };
        let normalized_link = link.replace(' ', "_");
        let title = urlencoding::encode(&normalized_link);
        let url =
            format!("https://en.wikipedia.org/w/rest.php/v1/page/{title}/history/counts/edits");
        let res = cx
            .client
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse>()
            .await?;
        let result = if res.count < 7 {
            if cx.allow_interactive {
                println!(
                    "is this peer review reviewed? (https://en.wikipedia.org/wiki/{title}) [y/n/q]"
                );
                match stdin()
                    .lines()
                    .next()
                    .transpose()?
                    .as_deref()
                    .map(str::trim)
                    .map(str::to_ascii_lowercase)
                    .as_deref()
                {
                    None => bail!("stdin is piped"),
                    Some("y") => "Reviewed",
                    Some("n") => "Not reviewed",
                    Some(_) => bail!("unrecognized response"),
                }
            } else {
                bail!("can't determine if it was reviewed");
            }
        } else {
            "Reviewed"
        };
        let date = if let Some(date) = value.date {
            date
        } else {
            /*let mut res = cx.client.client.get(format!("https://en.wikipedia.org/w/api.php?action=query&titles={title}&prop=revisions&rvlimit=1&rvprop=timestamp&format=json")).send().await?.error_for_status()?.json::<Value>().await?;
            let val = res["query"]["pages"]
                .as_object_mut()
                .unwrap()
                .values_mut()
                .next()
                .unwrap()["revisions"][0]["timestamp"]
                .take();
            let Value::String(s) = val else {
                bail!("nonstr")
            };
            PreserveDate::try_from_string(s).map_err(|_| eyre!("nondate"))?*/
            bail!("automatic determination of date is disabled")
        };
        into.actions.push(Action {
            kind: ActionKind::Pr,
            link: Some(link),
            date,
            result: Some(result.into()),
            oldid: value.id,
        });
        Ok(())
    }
}
