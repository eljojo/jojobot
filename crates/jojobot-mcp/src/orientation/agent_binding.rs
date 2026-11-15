//! The run's runtime agent, kept before a boot can claim a role.

use super::*;

impl Jojobot {
    pub(crate) async fn finished_boot_refusal(
        &self,
        args: &OrientArgs,
    ) -> Result<Option<CallToolResult>, McpError> {
        // A wrap code is the existing correction route, not another boot.
        if args
            .resume
            .as_deref()
            .map(str::trim)
            .is_some_and(jojobot_domain::session::is_wrap_code)
        {
            return Ok(None);
        }
        if let Some(key) = args.sid.as_deref() {
            if let Some(retired) = self.registry.retired(key)
                && retired.by == sid::RetiredBy::Wrapped
            {
                return Ok(Some(crate::caller::retired_key_declined(&retired)));
            }
            if let Some(handle) = self.registry.lookup_key(key)
                && let Some(id) = handle.card
                // As in standing, an unreadable live card supplies no new
                // state. A probe must not turn a recoverable boot into an error.
                && let Ok(run) = self.sessions.read_session(&id).await
                && run.state == jojobot_domain::session::SessionState::Wrapped
                && (!run.takes_writes() || run.window_lapsed_at(self.clock().now()))
            {
                return Ok(Some(crate::caller::finished_session_declined(
                    handle.name.as_str(),
                    run.wrapped_at(),
                )));
            }
        }
        if let Some(agent) = args
            .agent
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            && !self.compaction_by_the_live_run_of(args, agent).await?
            && let Some(run) = self
                .sessions
                .wrapped_by_agent(agent)
                .await
                .map_err(session_error)?
        {
            let name = run
                .sid
                .as_ref()
                .map(|sid| sid.as_str())
                .unwrap_or(run.id.as_str());
            return Ok(Some(crate::caller::finished_session_declined(
                name,
                run.wrapped_at(),
            )));
        }
        Ok(None)
    }

    /// **Whether this is a compaction report that the agent's live run answers**, so a run
    /// the agent hosted that wrapped does not make it finished. It holds only for a call with
    /// no key that names a bot whose one live run, the run the report would select, is the
    /// run the agent hosts. Any other call keeps the refusal, so a report is never made about
    /// a run the agent does not host.
    async fn compaction_by_the_live_run_of(
        &self,
        args: &OrientArgs,
        agent: &str,
    ) -> Result<bool, McpError> {
        let keyless = args.sid.as_deref().is_none_or(|sid| sid.trim().is_empty());
        if args.compacted != Some(true) || !keyless {
            return Ok(false);
        }
        let Some(named) = named_bot(args.bot.as_deref())? else {
            return Ok(false);
        };
        let Some(live) = self
            .sessions
            .live_by_agent(agent)
            .await
            .map_err(session_error)?
        else {
            return Ok(false);
        };
        Ok(self
            .sole_live_run(&named)
            .await
            .is_ok_and(|selected| selected.card.as_ref() == Some(&live.id)))
    }

    pub(crate) async fn record_boot_agent(
        &self,
        session: &mut serde_json::Value,
        agent: Option<&str>,
    ) -> Result<Option<CallToolResult>, McpError> {
        let Some(agent) = agent else {
            return Ok(None);
        };
        let Some(sid) = session
            .get("sid")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
        else {
            return Ok(None);
        };
        let gate = self.registry.gate(&self.gate_key(Some(&sid)));
        let held = gate.lock().await;
        let caller = match self.identified(Some(&sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(Some(refused)),
        };
        let card = self.session_for(&held, &caller, None, None).await?;
        let recorded = self
            .sessions
            .set_agent(&card, agent)
            .await
            .map_err(session_error)?;
        if session
            .get("session")
            .is_some_and(serde_json::Value::is_null)
        {
            session["session"] = crate::session::session_json(&recorded);
            session["note"] = "A fresh session. Its runtime agent is recorded on the run. \
                `sid` is its key: send it on every call and tell it to nobody. \
                `run` is its public name and opens nothing."
                .into();
        }
        Ok(None)
    }
}
