//! **A bot's rule seats: how many starred rules ride its boot, and the star
//! that would take one too many (rule 591).**
//!
//! Marking a rule to ride the boot is an admission decision, the same as a
//! thought entering a full room. When the seats are full, the star is refused:
//! the refusal names the rules that hold the seats, and the writer unstars one
//! before the new rule can take its place. There is no borrow past full seats.
//! The admission is [`capacity::fits`](super::capacity::fits), the one every
//! bounded container asks.
//!
//! **A bot is never over its seats once this holds**, so its boot carries every
//! starred rule. A store written before it may hold a bot over its seats; that
//! bot's boot carries the newest of them by age, which is when jojobot took each
//! record in, ties broken by the number in its local id. It is never the order a
//! store serves records in, because a store may serve them by local id as text,
//! and as text `f9` comes after `f118`.

use super::capacity::{Fits, fits};
use super::{EntityId, Fact, FactAddress, MemoryError, STAR_ROUTE};

/// **How many of a bot's in-force rules are marked to ride the boot, and the
/// one sentence about it**, shown by the boot once the seats are full.
pub struct CarriedSeats {
    /// How many in-force rules carry `fields.starred == "true"`.
    pub starred: usize,
    /// The sentence to show — present once `starred` reaches the bot's seats.
    pub sentence: String,
}

/// Whether a rule is marked to ride the boot.
pub fn is_starred(rule: &Fact) -> bool {
    rule.fields.get("starred").is_some_and(|v| v == "true")
}

/// **A record's age, oldest first.** A record kept before jojobot stamped one
/// is older than every stamped one. The number in the local id breaks a tie,
/// and the id as text only orders two ids that hold no number.
pub fn by_age(a: &&Fact, b: &&Fact) -> std::cmp::Ordering {
    (a.inserted_at, a.id.ordinal(), a.id.as_str()).cmp(&(
        b.inserted_at,
        b.id.ordinal(),
        b.id.as_str(),
    ))
}

/// **Every starred rule**, oldest first. `in_force` is
/// [`super::rules_in_force`]'s answer, in any order.
pub fn starred_by_age(in_force: &[Fact]) -> Vec<&Fact> {
    let mut starred: Vec<&Fact> = in_force.iter().filter(|rule| is_starred(rule)).collect();
    starred.sort_by(by_age);
    starred
}

/// **The starred rules that hold a seat**, oldest first: every one of them when
/// they fit, the newest `seats` of them by age when a bot is over.
pub fn seated(in_force: &[Fact], seats: usize) -> Vec<&Fact> {
    let mut starred = starred_by_age(in_force);
    let from = starred.len().saturating_sub(seats);
    starred.split_off(from)
}

/// **How many rules a bot over its seats has to unstar to fit**, 0 for a bot
/// within them.
pub fn unstar_to_fit(in_force: &[Fact], seats: usize) -> usize {
    in_force
        .iter()
        .filter(|rule| is_starred(rule))
        .count()
        .saturating_sub(seats)
}

/// **The refusal for a write that stars one more of `bot`'s rules**, or
/// `None` when the seats have room. `in_force` is the bot's rules in force
/// before the write, and `written` the rule the write stars, when it already
/// exists: a rule that is already starred takes no new seat, so editing it is
/// never refused here.
pub fn refuses_a_star(
    bot: &EntityId,
    in_force: &[Fact],
    written: Option<&FactAddress>,
    seats: usize,
) -> Option<MemoryError> {
    if written.is_some_and(|address| {
        in_force
            .iter()
            .any(|rule| &rule.address() == address && is_starred(rule))
    }) {
        return None;
    }
    let starred = in_force.iter().filter(|rule| is_starred(rule)).count();
    match fits(seats, starred, 1, false) {
        Fits::Yes | Fits::OnTheReserve => None,
        Fits::No => Some(MemoryError::SeatsFull {
            subject: bot.to_string(),
            seats,
            starred,
            // **Every starred rule, not only the ones that hold a seat.** A bot over its
            // seats must unstar `starred + 1 - seats` rules, so the refusal shows all of
            // them and never asks for more choices than it lists.
            seated: starred_by_age(in_force).into_iter().cloned().collect(),
        }),
    }
}

/// **What a refused star says to do**, in one place so the skill that shows a
/// bot how to fit more rules than seats can be named here too (rule 593).
pub fn seats_full_how_to_proceed(
    subject: &str,
    seats: usize,
    starred: usize,
    verb: &str,
) -> String {
    let unstar = starred + 1 - seats;
    let which = match unstar {
        1 => "one of the starred rules above".to_string(),
        n => format!("{n} of the starred rules above, each"),
    };
    format!(
        "Nothing was written: '{subject}' has {seats} seats and {starred} starred rules, and a \
         star never pushes a rule off by itself. Unstar {which} with update_fact on its \
         address and fields {{starred: \"false\"}}, then re-call {verb}. A seat is for a rule \
         that binds on every turn; a rule that binds at one moment belongs where that moment \
         is."
    )
}

/// `in_force` is [`super::rules_in_force`]'s own answer, in any order.
///
/// `None` short of `seats` starred rules: there is
/// nothing to say yet, and a caller under the cap gets no sentence at all.
pub fn carried_seats_status(in_force: &[Fact], seats: usize) -> Option<CarriedSeats> {
    let count = in_force.iter().filter(|rule| is_starred(rule)).count();
    if count < seats {
        return None;
    }
    Some(CarriedSeats {
        starred: count,
        sentence: format!(
            "This identity has {count} rules starred to ride the boot, and only {} seats \
             exist. The seats are for rules that bind on every turn; a rule that binds at one \
             moment is better carried by what is read at that moment, such as a skill fetched \
             for that job. {}",
            seats, STAR_ROUTE,
        ),
    })
}
