# Finding things — recall and search

> **jojobot** · kind: feature-brief · boot: on-demand

**Getting an answer out of the graph.** The whole product rests on this feature. A graph nothing can interrogate is a pile of pages with extra steps.

## Two doors, and why not one

* `**recall**` **is the precise lookup.** You know the subject. You say the shape you want, and you get that shape back.
* `**search**` **is the breadth.** You do not know where to look. It ranks free text, it takes filters, and every hit arrives with its surroundings rather than as a bare match. **Matching is deliberately loose and an answer says a miss is possible**, because a singular typed against a stored plural returns nothing, and nothing is indistinguishable from never having been told. **It reaches the earlier wordings of a claim when a caller asks for them**, so a record that has since been corrected is still findable by what it used to say.

**One reads like a declarative configuration. The other reads like a web search.** They are not two ways to do one job. They answer different questions. Collapse them, and the precise side starts to guess while the broad side turns rigid.

**Neither door grows a sibling.** To reach more, add axes to the verbs that exist. A surface that adds a verb whenever a question comes up becomes a catalogue of the questions somebody thought of.

## Every question reads the projection

**A filter, a comparison and a walk all read the fold.** They read what a thing currently holds. They never read one event's own value.

*Which friends have eaten three or more donuts* asks about the folded total. A query that read raw events would return a different set, and nothing about the answer would say so.

## Saying which — one expression over keys

**A caller says which things it wants in one short expression over keys**, the way a filter on a task board reads: `status != done && owner = bot:gamma`, `due <= today+7d`, `tags contains kitchen`. The words are `=`, `!=`, `<`, `<=`, `>`, `>=`, `in` (any of), `contains` (a list holds an item), `&&`, `||`, `!` and parentheses. There are no wildcards.

**The expression reaches keys only.** Subject, kind, parent, type and the walk stay arguments beside it, so an expression never has to say where to look.

**One scope covers the whole expression**: what a thing holds now, by default, or what one record says, when asked. A term can never pick its own, because an answer that mixed the two could not be read back as one question.

**A declaration licenses the comparisons that need a type.** Before, after, less and more need a key declared a date or a number. `contains` needs a key declared a list. Equality, any-of, exists and their negations need nothing declared. A comparison used without its licence is refused, naming the declaration it needs. A declaration named like a kind belongs to that kind, so one kind's `due` never licenses another's; a type under any other name is vocabulary and licenses any object that carries its key. Where the two disagree about a key, the kind's declaration wins. A call that names no kind asks each object's own kind, and the answer counts what it left out by kind rather than shrinking in silence.

**Each entity is judged by its own kind's declarations.** A board holds a `thing` named gamma and a `place` named daredevil-gorge, both with a `due`. If only the `thing` type declares `due` a date, `due < today+7d` with no kind keeps gamma, and the answer counts the place under `left_out_unlicensed` (kind `place`, count 1) with the `declare_type` that would keep it. A walk's `keeping` is judged the same way, per object it reaches, and counts what it drops. A call that names a kind is judged by that kind alone, before any read.

**A negation keeps only things that hold the key.** `status != done` is the things holding some other status. A thing with no status is not among them, because *has none* and *has another* are different answers. `!` is the complement within what the call selected, never the whole store.

**A date can be relative to the asker's day**: `today`, `today+3d`, `today-2w`, resolved when the question is asked, in that session's zone.

**Order is one key and a direction**, applied before the answer is cut to size. Things lacking the key come last.

**A mistake says where it is.** An expression that does not parse is refused, naming the token, its position and the words it accepts. A term on a key that nothing holds says so, rather than returning an empty answer that reads as *there is none*. A key the records hold and the thing does not is told to use `scope: record`, and a call through a view says the view. An order on a key nothing holds, and a walk's `keeping` on one, are refused the same way. A comparison without its declaration is refused with the `declare_type` to make. A `scope` beside no `where` is refused, not ignored.

## What a bot may reach

**Search is a bot's Spotlight.** It covers two populations at once. **What is the bot's own:** its own past sessions, its own journal, and its mailbox when the caller asks for mail — mail is off unless requested, because most searches are not about mail and a box would otherwise crowd out the answer. **What is shared:** the objects any bot may see, because they belong to the operator.

**A bot reads only its own past sessions.** Owner scoping is what makes sessions safe to index. It keeps another bot's work private without costing a bot access to its own.

**Sessions rank lower than everything else**, because a session is context rather than an answer. Ranking one low enough never to surface is the same as excluding it, and that defeats the point.

⚠️ **An owner filter and an empty result look identical to a caller.** A bot that may not see something must be able to tell that apart from nothing being there. Otherwise wrong scoping reads as an empty graph.

## What the query does

A graph gets asked all of the following. A query that stops short of any of it sends somebody back to reading pages.

* **Traversal past one hop**, so a walk reaches a thing through a thing.
* **Relations the caller names**, declared by a kind's reference keys. The caller does not assemble them. Asking for a camera's lenses is vocabulary. The caller never describes how two records meet; the declared relation does.
* **Filters that combine**, and ordering, on the folded values a kind's declared keys produce.
* **Values inside records.** A declared kind is what makes this possible: a key means the same thing across things only once a kind declares it.
* **Which of the two questions you are asking**, on either door. *Which of these are described like a pet* keeps the partial ones and names the keys each lacks. *Which of these ARE pets* keeps only the complete ones. **Both are true of the same thing at once.** A pet with no name belongs in one answer and not the other. **The reader chooses. It is never a property the thing carries.** A caller who names neither gets the tolerant one, because a thing that vanishes from an answer is worse than a thing that arrives with its gaps stated.
* **Absence** — what has no counterpart. A person who appears everywhere and has no card. A thing whose date is not in the future. A category nothing is filed under. **A personal graph gets asked this constantly, and it is the hardest thing to express**, because every filter says which records to keep and no record is there to keep.
* **Aggregates** — counting, summing, grouping, and comparing two results. A filter returns a list. Some questions want a number.
* **Full text over prose**, including journals and past sessions.
* **Field-level visibility on one record.** A colleague's charter is public. Their mail is not.

⚠️ **An absence answer names the population it searched.** *Nothing among these* is an answer a reader can act on. *There is none* is a claim no query can support. **An under-scoped filter that reports its empty result as a finding is the fabrication failure arriving through a read.** That is the one direction nobody watches.

## Views

**A view is a saved recall: a question stored under a name, with typed inputs.** A caller asks for the thing, not for the query that produces it. ⭐ **A session that never learns the query language gets what it needs by naming a view and filling its inputs**: the work that can start under one project, what falls due in a window. The query is jojobot's business. The answer is the caller's.

**An input is declared with its type and checked like any write.** A view names each input, what it holds (a thing of a named kind, a date, a number, text) and whether it may be left out. A handle is bound to its permanent id, so a rename never breaks a saved question, and a date takes the same relative days the filter string does. A view asked without a required input, or with one of the wrong type, is refused, naming every input and its type: the refusal is how a session meeting the view cold learns its shape. Listing the views shows each one's question and inputs.

**A view is not a shortcut beside a query builder.** The surface *is* views over one graph. That is the stronger form and the intended one.

**A view is data, declared by somebody.** The few that ship arrive as ordinary records, the same shape a caller declares. A bot consulting who its neighbours are is a query over the graph rather than a new capability.

**The ready view answers *what can be started*.** A piece of work is ready when it is not finished, every thing it `depends_on` is finished, and nothing is held under `waiting_on`. Work already under way is ready, so a caller who wants only what has not started narrows the view with `where` to its board's not-started status. The answer counts the work it left out and names the call that says why: the view with `subject` set to one piece of work comes back with `ready: false` and `blocked_by`. A blocker is a dependency that is unfinished, archived or names nothing, the thing the work waits on, or the work itself being finished or archived. **Archived work is never ready, however it is named**: an archived thing is not finished, and nobody starts it.

## What it is not

**Not an inference engine.** jojobot performs no reasoning of its own. It filters, walks and ranks. The mind that reads the result is the only mind in the system.

**Not a place an agent learns how the answer was checked.** A caller learns what is true for it. It never learns the mechanism jojobot used to establish that.