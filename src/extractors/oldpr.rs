use color_eyre::eyre::bail;
use serde::Deserialize;
use tracing::debug;

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
    pub date: PreserveDate,
}

pub struct OldPrExtractor;

#[derive(Deserialize)]
pub struct ApiResponse {
    pub count: u64,
    pub limit: bool,
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
        let url = format!(
            "https://en.wikipedia.org/w/rest.php/v1/page/{}/history/counts/edits",
            urlencoding::encode(&link.replace(' ', "_")),
        );
        debug!(?url);
        let res = cx
            .client
            .client
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .json::<ApiResponse>()
            .await?;
        if res.count < 10 {
            bail!("can't determine if it was reviewed");
        }
        into.actions.push(Action {
            kind: ActionKind::Pr,
            link: Some(link),
            date: value.date,
            result: Some("Reviewed".into()),
            oldid: value.id,
        });
        Ok(())
    }
}
