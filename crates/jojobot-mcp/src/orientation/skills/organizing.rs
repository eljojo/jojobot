//! **The organizing skill** (rule 593): how a bot carries more than its rule
//! seats, its room of thoughts or a thread hold. It lives in its own file so the
//! index of skills stays an index.

/// **The name of the skill that shows a bot how to fit more than its seats,
/// its room or a thread holds** (rule 593). Every full container's refusal
/// names it through [`ORGANIZING_POINTER`].
pub(crate) const ORGANIZING_SKILL: &str = "organizing";

/// **The sentence a full container's refusal ends with**: the skill, as the
/// call that reads it.
pub(crate) const ORGANIZING_POINTER: &str =
    "To fit more than this holds, read start_here skill: organizing.";

pub(crate) const ORGANIZING: &str = r#"# organizing

**What you carry into a session is bounded: your rule seats, your thought
room, the records a thread holds. The bound does not limit what you can know.
It limits what rides with you. Everything else sits one step away, filed
where you will reach for it at the moment it applies.** Organized that way,
five seats reach fifteen rules.

## The one question

Before you write into anything bounded, ask: **is this needed every time, or
at a moment I can name?** Every time: it earns a slot. At a moment: it goes
where that moment happens, and a slot points there.

**Name a moment you will see.** "Before you write to the operator" is a
moment; you know when you are about to do it. "When it is relevant" is not:
nothing tells you it has arrived, so the pointer is never followed.

## Rules: more rules than seats

A starred rule rides every boot, and the seats are few. Keep them for two
things: the rules that bind on every turn, and one index rule that says where
the rest are and when to load them.

Give every other rule a `purpose`, a short name for the area it covers, and
start its words with the moment it applies. Your boot shows the start of each
rule it did not carry, so the moment is what you see:

    capture  subject: "bot:gamma"  content: "Before you write to the operator, give every clock in the operator's own zone"  fields: {"purpose": "operator"}

Then star one index rule that names each moment, its purpose, and the call
that loads it. Write the call into the rule: your boot does not always name
it.

    capture  subject: "bot:gamma"  fields: {"starred": "true"}
             content: "Before you write to the operator, load my rules with purpose operator. Before you file or move work, load purpose work. Load a purpose with recall bot:gamma, facts: true, status: active, fields: [{key: purpose, value: the purpose, scope: record}]."

That call, written out:

    recall  subject: "bot:gamma"  facts: true  status: "active"  fields: [{"key": "purpose", "value": "operator", "scope": "record"}]

When one purpose holds more rules than you can read at once, split it into
narrower purposes, and give it one rule of its own that names each narrower
purpose and when to load it. A rule that applies only in one kind of job
belongs on what that job reads, such as its thread or its project, and not on
your bot.

## Thoughts: a full room

A thought is one line that says what is live, with the address of the thing
that holds the substance:

    capture  subject: "bot:gamma"  content: "Milhouse is moving to Shelbyville in the spring"  shape: "connection"  object: "thread:milhouse-moves"

When the room is full, choose the thought that has stopped being live, and
drop it in the same call that writes the new one:

    capture  subject: "bot:gamma"  content: "Lisa starts at the new school in March"  shape: "connection"  object: "thread:lisa-school"  drop: "bot:gamma#f12"  drop_because: "the move happened; the thread holds it"

A room that is already over its capacity takes no new thought until you
archive one:

    update_fact  address: "bot:gamma#f12"  status: "archived"  details: "the move happened; the thread holds it"

Never squeeze two thoughts into one: the room counts ideas, and a thought that
names two things points well at neither.

## Threads: a full thread

A thread that cannot hold its subject splits into child threads, one per part
of it. Name each child for what it holds, so a reader at the parent can tell
which one to open:

    add_entity  kind: "thread"  handle: "milhouse-moves-packing"  name: "Packing"  source: "split from thread:milhouse-moves"  parent: "thread:milhouse-moves"

Move a record by writing it again as a claim about the child, with
`derived_from` its old address. Then archive the old one, with the new address
in `details`. Never rewrite records denser to make them fit: a record
compressed loses what made it worth keeping. The parent keeps what is about
the whole. A reader at the parent lists its children with
`recall  parent: "thread:milhouse-moves"`.

## When a write is refused

Full seats, a full room and a full thread each refuse the write and name what
they hold. Choose what gives, then write again:

- seats: unstar a rule that applies at a moment, and file it in the same call:
  `update_fact` with `fields: {"starred": "false", "purpose": "operator"}`.
  Check that your index rule names that purpose. If every seated rule applies
  on every turn, unstar none: ask the bot you report to, or the operator, to
  raise your `rule_seats`. You cannot raise your own;
- a room: drop the thought that has stopped being live, in the call that
  writes the new one;
- a thread: move a part of it to a child thread, or archive a record that has
  stopped being true.

**The check that you are done:** could a session that reads only what you
carry reach everything else at the moment it is needed?
"#;
