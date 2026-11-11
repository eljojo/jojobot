/// **What to do about a role somebody else holds** — said once, for the
/// refusal a write gets and for the `refused` outcome of a claim at the boot
/// door, so the two cannot come to say different things.
// TEMPORARY(9.3.8): off the composer, start_here/memory role conflicts.
pub(crate) fn role_taken_way_forward(
    role: &str,
    holder: &str,
    until: &str,
    renewed_at: &str,
) -> String {
    format!(
        "Run '{holder}' holds '{role}'; its lease was last renewed at {renewed_at} (a write \
         renews it, at most once every 4 minutes 30 seconds) and runs until {until}. The role \
         changes hands when '{holder}' wraps its session, or at {until} if it writes nothing \
         more, or when the operator's code is presented: ask for one with start_here's \
         ask_for_code, then boot again with the challenge, the code and this claim."
    )
}

/// **What to do about a claim whose name builds no role handle.** The refusal a
/// claim at the boot door gets when its name is not a bare role name, such as a
/// name that already carries `role:`: the claim argument is the caller's to
/// change, and nothing was held.
// TEMPORARY(9.3.7): off the composer, start_here.
pub(crate) fn role_name_way_forward(role: &str) -> String {
    let bare = role.strip_prefix("role:").unwrap_or(role);
    format!(
        "'{role}' is not a role name: name the role by its bare name, lowercase letters, \
         digits and hyphens only, with no kind in front of it, for example '{bare}'. Call \
         start_here again with that claim. Nothing was held."
    )
}

/// **What to do about a role that belongs to another bot.** The refusal a claim
/// at the boot door gets when the role is carried by, or sits under, a bot
/// other than the one booting.
// TEMPORARY(9.3.7): off the composer, start_here.
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
// TEMPORARY(9.3.7): off the composer, start_here.
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
