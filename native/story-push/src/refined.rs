//! The refined review queue: stories that failed or skipped their readiness review and wait in
//! `refined` for the next one, oldest first.

use crate::lenient_story::Js;
use chrono::{NaiveDate, Utc};

/// Days a story may sit in `refined` before lint warns, unless `team.refinedStaleDays` says.
pub(crate) const STALE_DAYS: u64 = 7;

pub(crate) struct Waiting<'a> {
    pub(crate) story: &'a Js,
    pub(crate) since: Option<NaiveDate>,
    pub(crate) days: Option<i64>,
}

/// When a refined story entered `refined`: its last `set status=refined` changelog line. A story
/// put in `refined` without one has an unknown entry date; `updated` would not do, because any
/// later edit such as a note moves it and makes a stale story look fresh.
fn since(story: &Js) -> Option<NaiveDate> {
    crate::metrics::history(String::new(), &story.to_value())
        .log
        .status
        .into_iter()
        .rev()
        .find(|(_, status)| status == "refined")
        .map(|(date, _)| date)
}

pub(crate) fn waiting(story: &Js) -> Option<Waiting<'_>> {
    (story.get("status").string() == "refined").then(|| {
        let since = since(story);
        Waiting {
            story,
            since,
            days: since.map(|d| (Utc::now().date_naive() - d).num_days().max(0)),
        }
    })
}

/// Every refined story, oldest first. An unknown entry date sorts first: it is at least as
/// likely to be forgotten as the oldest known one.
pub(crate) fn queue(stories: &[Js]) -> Vec<Waiting<'_>> {
    let mut queue: Vec<Waiting> = stories.iter().filter_map(waiting).collect();
    queue.sort_by(|a, b| {
        a.since
            .cmp(&b.since)
            .then_with(|| a.story.get("id").string().cmp(&b.story.get("id").string()))
    });
    queue
}
