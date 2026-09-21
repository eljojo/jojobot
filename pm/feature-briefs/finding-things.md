# Finding things — recall and search

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**Getting an answer out of the graph.** The whole product rests on this feature. A graph nothing can interrogate is a pile of pages with extra steps.

## Two doors, and why not one

* `**recall**` **is the precise lookup.** You know the subject. You say the shape you want, and you get that shape back.
* `**search**` **is the breadth.** You do not know where to look. It ranks free text, it takes filters, and every hit arrives with its surroundings rather than as a bare match. **Matching is deliberately loose and an answer says a miss is possible**, because a singular typed against a stored plural returns nothing, and nothing is indistinguishable from never having been told. **It reaches the earlier wordings of a claim when a caller asks for them**, so a record that has since been corrected is still findable by what it used to say.

**One reads like a declarative configuration. The other reads like a web search.** They are not two ways to do one job. They answer different questions. Collapse them, and the precise side starts to guess while the broad side turns rigid.

**Neither door grows a sibling.** To reach more, add axes to the verbs that exist. A surface that adds a verb whenever a question comes up becomes a catalogue of the questions somebody thought of.

## Every question reads the projection

**A filter, a comparison and a walk all read the fold.** They read what a thing currently holds. They never read one event's own value.

*Which friends have eaten three or more donuts* asks about the folded total. A query that read raw events would return a different set, and nothing about the answer would say so.

**A key's declared value type licenses the comparison.** Dates compare as before and after. Numbers compare as less and greater. *Expired* and *overdue* are orderings, not matches.

## What a bot may reach

**Search is a bot's Spotlight.** It covers two populations at once. **What is the bot's own:** its own past sessions, its own journal, and its mailbox when the caller asks for mail — mail is off unless requested, because most searches are not about mail and a box would otherwise crowd out the answer. **What is shared:** the objects any bot may see, because they belong to the operator.

**A bot reads only its own past sessions.** Owner scoping is what makes sessions safe to index. It keeps another bot's work private without costing a bot access to its own.

**Sessions rank lower than everything else**, because a session is context rather than an answer. Ranking one low enough never to surface is the same as excluding it, and that defeats the point.

⚠️ **An owner filter and an empty result look identical to a caller.** A bot that may not see something must be able to tell that apart from nothing being there. Otherwise wrong scoping reads as an empty graph.

## What the query does

A graph gets asked all of the following. A query that stops short of any of it sends somebody back to reading pages.

* **Traversal past one hop**, so a walk reaches a thing through a thing.
* **Relations the caller names**, declared by a kind's reference keys. The caller does not assemble them. Asking for a camera's lenses is vocabulary. Describing how two records meet is a query language.
* **Filters that combine**, and ordering, on the folded values a kind's declared keys produce.
* **Values inside records.** A declared kind is what makes this possible: a key means the same thing across things only once a kind declares it.
* **Which of the two questions you are asking**, on either door. *Which of these are described like a pet* keeps the partial ones and names the keys each lacks. *Which of these ARE pets* keeps only the complete ones. **Both are true of the same thing at once.** A pet with no name belongs in one answer and not the other. **The reader chooses. It is never a property the thing carries.** A caller who names neither gets the tolerant one, because a thing that vanishes from an answer is worse than a thing that arrives with its gaps stated.
* **Absence** — what has no counterpart. A person who appears everywhere and has no card. A thing whose date is not in the future. A category nothing is filed under. **A personal graph gets asked this constantly, and it is the hardest thing to express**, because every filter says which records to keep and no record is there to keep.
* **Aggregates** — counting, summing, grouping, and comparing two results. A filter returns a list. Some questions want a number.
* **Full text over prose**, including journals and past sessions.
* **Field-level visibility on one record.** A colleague's charter is public. Their mail is not.

⚠️ **An absence answer names the population it searched.** *Nothing among these* is an answer a reader can act on. *There is none* is a claim no query can support. **An under-scoped filter that reports its empty result as a finding is the fabrication failure arriving through a read.** That is the one direction nobody watches.

## Views

**A view is a named query, exposed as a high-level object.** A caller asks for the thing, not for the query that produces it. The query is jojobot's business. The answer is the caller's.

**A view is not a shortcut beside a query builder.** The surface *is* views over one graph. That is the stronger form and the intended one.

**Somebody declares a view.** jojobot does not ship a hardcoded list of the questions anybody anticipated. The first real view is a bot consulting who its neighbours are, which is a query over the graph rather than a new capability.

## What it is not

**Not an inference engine.** jojobot performs no reasoning of its own. It filters, walks and ranks. The mind that reads the result is the only mind in the system.

**Not a place an agent learns how the answer was checked.** A caller learns what is true for it. It never learns the mechanism jojobot used to establish that.