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

/// **Whether a claim's name builds no role handle by its own spelling.** The shape
/// of the handle is judged alone: a refusal of the kind itself, on an instance that
/// does not hold it, says nothing about the name, so the refusal for the caller to
/// change is not for it.
pub(crate) fn names_no_role_handle(role: &str) -> bool {
    jojobot_domain::memory::validate_shape(&jojobot_domain::memory::EntityId(format!(
        "role:{role}"
    )))
    .is_err()
}

/// **What to do about a claim whose name builds no role handle.** The refusal a
/// claim at the boot door gets when its name is malformed: the claim argument is
/// the caller's to change, and nothing was held.
// TEMPORARY(9.3.7): off the composer, start_here.
pub(crate) fn role_name_way_forward(role: &str) -> String {
    format!(
        "'{role}' is not a role name: a role is named by lowercase letters, digits and \
         hyphens only, with or without `role:` in front of it. Call start_here again with a \
         claim spelled that way. Nothing was held."
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

#[cfg(test)]
mod tests {
    use super::names_no_role_handle;

    /// **Only a spelling that builds no role handle is the caller's to change.** A
    /// name of lowercase letters, digits and hyphens builds one, so a refusal of it
    /// is the instance's and not the name's; a capital, a space, a colon or nothing
    /// at all builds none.
    #[test]
    fn a_name_builds_no_role_handle_only_by_its_own_spelling() {
        for fine in ["gamma", "gamma-2", "delta"] {
            assert!(!names_no_role_handle(fine), "{fine} builds a role handle");
        }
        for bad in ["Role:A b", "role:gamma", "A", "g amma", ""] {
            assert!(names_no_role_handle(bad), "{bad:?} builds none");
        }
    }
}
