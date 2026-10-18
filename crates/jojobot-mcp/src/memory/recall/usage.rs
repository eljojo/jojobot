//! **The usage records a `recall` can reach**, projected from the history of
//! every bot's runs and held in no store (see [`jojobot_domain::usage`]).
//!
//! Which of them the asker may read is decided here from the chart, and handed to
//! the walk as a [`UsageReach`]; the walk's one predicate does the withholding.

use std::collections::HashSet;

use jiff::civil::Date;
use jojobot_domain::memory::graph::UsageReach;
use jojobot_domain::memory::search::DocScan;
use jojobot_domain::memory::{EntityId, MemoryError, is_the_assistant};
use jojobot_domain::session::SessionError;
use jojobot_domain::usage::{RunUsage, weeks};

use super::DayFrame;
use crate::Jojobot;
use crate::caller::Caller;

/// Why the usage records could not be projected.
pub(super) enum Unread {
    /// The run history could not be read.
    Runs(SessionError),
    /// The chart could not be read.
    Chart(MemoryError),
}

impl Jojobot {
    /// **Every bot's weeks as documents, and whose of them `caller` may read.**
    ///
    /// The reach is the asker's own bot and every bot below it on the chart; the
    /// assistant, the head of the chart, reads all; a caller with no identity
    /// reads none. Weeks are projected for every bot whatever the reach, because
    /// the walk counts the ones it withholds and a reader told how many weeks it
    /// was not shown is not told they do not exist.
    pub(super) async fn usage_docs(
        &self,
        caller: Option<&Caller>,
        frame: DayFrame<'_>,
        today: Date,
    ) -> Result<(Vec<DocScan>, UsageReach), Unread> {
        let totals = self.sessions.run_totals().await.map_err(Unread::Runs)?;
        let runs: Vec<RunUsage> = totals
            .iter()
            .map(|run| RunUsage {
                bot: run.bot.clone(),
                began: run.started_on.unwrap_or_else(|| frame.day(run.started_at)),
                served_chars: run.served_chars,
                resets: run.resets.clone(),
                dated_from: run.dated_from,
            })
            .collect();
        let calls = self.sessions.call_counts().await.map_err(Unread::Runs)?;
        let served = self.sessions.served_days().await.map_err(Unread::Runs)?;
        let docs = weeks(&runs, &calls, &served, &|moment| frame.day(moment), today)
            .iter()
            .map(jojobot_domain::usage::Week::projected)
            .collect();
        let reach = match caller {
            None => UsageReach::Own,
            Some(caller) if is_the_assistant(&caller.bot) => UsageReach::All,
            Some(caller) => {
                let owners: HashSet<&EntityId> = runs.iter().map(|run| &run.bot).collect();
                let mut below = HashSet::new();
                for owner in owners {
                    match self.chain_above(owner).await {
                        Ok(above) if above.contains(&caller.bot) => {
                            below.insert(owner.clone());
                        }
                        Ok(_) => {}
                        // **An owner no record answers to is below nobody**, so
                        // its weeks are withheld and the read goes on. Any other
                        // failure is the store's and ends the read: a week left
                        // out because the store was down would read as one the
                        // asker may not see.
                        Err(MemoryError::UnknownEntity { .. }) => {}
                        Err(error) => return Err(Unread::Chart(error)),
                    }
                }
                UsageReach::Bots(below)
            }
        };
        Ok((docs, reach))
    }
}
