/// **What to do about a role somebody else holds** — said once, for the
/// refusal a write gets and for the `refused` outcome of a claim at the boot
/// door, so the two cannot come to say different things.
pub(crate) fn role_taken_way_forward(role: &str, holder: &str, until: &str) -> String {
    format!("Wait until {until}, or ask '{holder}' to release '{role}' by wrapping its session.")
}

/// **What to do about a role that belongs to another bot.** The refusal a claim
/// at the boot door gets when the role is carried by, or sits under, a bot
/// other than the one booting.
pub(crate) fn role_owned_way_forward(role: &str, owner: &str) -> String {
    let slug = owner.split_once(':').map_or(owner, |(_, slug)| slug);
    format!(
        "'{role}' belongs to {owner}. To claim it, call start_here with bot set to '{slug}'. \
         Or claim a name no bot owns. Nothing was written."
    )
}

/// **What to do about a role whose object is archived.** The refusal a claim at
/// the boot door gets when the object `role:<role>` stands archived: the way
/// forward is to restore the object, which only the bot it belongs to or a bot
/// heading the chart may do, and then claim again.
pub(crate) fn role_archived_way_forward(
    role: &str,
    object: &str,
    may_restore: &[String],
) -> String {
    format!(
        "'{role}' was not claimed: its role object {object} is archived. Restore it with \
         archive_entity, handle '{object}' and restore true; only {} may. Then claim '{role}' \
         again. Nothing was written.",
        may_restore.join(" or "),
    )
}
