//! **Who made one write of a key, and in which run.**
//!
//! A write of a key is carried by one write of a claim, and that claim write
//! keeps the bot, the run and the app mark it was made with. This is the one
//! place that turns those columns into what a key's history serves, so the
//! writer, the run and the app mark cannot disagree about which write they
//! describe (rule 598).

use super::{EntityId, authorship};
use crate::actor::Actor;
use crate::app::{AppId, AppStamp};

/// **What the claim write that carried a key write kept**, as a store reads it
/// off that one row.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriteOrigin {
    /// The bot the write recorded, if its build recorded one.
    pub bot: Option<EntityId>,
    /// The run the write was made in.
    pub run: Option<String>,
    /// The bot behind that run, when a session row answers to it.
    pub run_bot: Option<EntityId>,
    /// The app mark the write carries.
    pub app_stamp: Option<AppStamp>,
}

/// **The three answers about one write**, built together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WrittenBy {
    /// Who made the write: the app that marked it, else the bot it recorded,
    /// else the bot behind its run. `None` when none of those is known.
    pub writer: Option<Actor>,
    /// The run of THE WRITE that set the value, never of the claim's first write.
    pub run: Option<String>,
    /// The app's receipt, when an app made the write.
    pub recorded_by: Option<AppStamp>,
}

/// **Read the three answers off one claim write.**
///
/// `origin` is `None` for a key write that points at no claim write: an older
/// build appended it, and nothing says which claim write it was. It reads no
/// writer and no run, and never falls back to the claim's first or newest write,
/// which is a different run once the claim was edited. `claim_mark` is the mark
/// the claim itself carries, which such a write keeps serving as before.
pub fn written_by(origin: Option<WriteOrigin>, claim_mark: Option<&AppStamp>) -> WrittenBy {
    let Some(origin) = origin else {
        return WrittenBy {
            recorded_by: claim_mark.cloned(),
            ..WrittenBy::default()
        };
    };
    let writer = match &origin.app_stamp {
        Some(stamp) => AppId::parse(&stamp.app).ok().map(Actor::App),
        None => authorship::first_writer(origin.bot, || origin.run_bot).map(Actor::Bot),
    };
    WrittenBy {
        writer,
        run: origin.run,
        recorded_by: origin.app_stamp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp() -> AppStamp {
        AppStamp {
            app: "springfield-ci".into(),
            grant: "record_deploy_event".into(),
            received_at: "2026-10-18T12:00:00Z".into(),
        }
    }

    fn bot(id: &str) -> EntityId {
        EntityId(id.into())
    }

    #[test]
    fn a_write_with_no_claim_write_names_no_writer_and_no_run() {
        let read = written_by(None, Some(&stamp()));
        assert_eq!(read.writer, None);
        assert_eq!(read.run, None);
        assert_eq!(
            read.recorded_by,
            Some(stamp()),
            "the claim's mark still serves"
        );
    }

    #[test]
    fn a_bot_write_names_the_recorded_bot_before_the_bot_behind_the_run() {
        let origin = WriteOrigin {
            bot: Some(bot("bot:alpha")),
            run: Some("run-a".into()),
            run_bot: Some(bot("bot:beta")),
            app_stamp: None,
        };
        let read = written_by(Some(origin), None);
        assert_eq!(read.writer, Some(Actor::Bot(bot("bot:alpha"))));
        assert_eq!(read.run.as_deref(), Some("run-a"));
        assert_eq!(read.recorded_by, None);
    }

    #[test]
    fn a_write_with_no_recorded_bot_names_the_bot_behind_its_run() {
        let origin = WriteOrigin {
            run: Some("run-a".into()),
            run_bot: Some(bot("bot:beta")),
            ..WriteOrigin::default()
        };
        assert_eq!(
            written_by(Some(origin), None).writer,
            Some(Actor::Bot(bot("bot:beta")))
        );
    }

    #[test]
    fn an_app_write_names_the_app_and_keeps_its_mark() {
        let origin = WriteOrigin {
            app_stamp: Some(stamp()),
            ..WriteOrigin::default()
        };
        let read = written_by(Some(origin), None);
        assert_eq!(
            read.writer,
            Some(Actor::App(AppId::parse("springfield-ci").expect("a name")))
        );
        assert_eq!(read.recorded_by, Some(stamp()));
        assert_eq!(read.run, None);
    }
}
