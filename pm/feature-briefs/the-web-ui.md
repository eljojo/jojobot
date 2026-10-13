# The web UI — a window on your own instance

> **jojobot** · kind: feature-brief · boot: on-demand

**The operator's window on their own instance: server-rendered pages over the graph, one per node, browsed like a filesystem from its root.** It is how a person checks that jojobot is right, sees what the bots did, and looks things up.

## Pages, not an application

**Every page is rendered on the server and is plain HTML with links.** An entity path is a URL path, so walking the graph is walking the URLs. **There is no application**: client-side state, a framework or a component tree is the tell that somebody left the brief. A new page is a new rendering of something the graph already answers, never a new place where state lives.

**The operator reads it on a laptop, and a page must never overwhelm them.** Few words, room to breathe, one thing at a time, and detail behind a click that expands. A page leads with what a person came for and keeps bookkeeping out of sight. A page that reads as crammed or wordy is a defect, however correct it is. Breadcrumbs say where a page sits; a search box calls the search the bots already use.

**jojobot views are the model, and the HTML is the view.** A page may have its own HTML whenever that serves the operator better: a board drawn as a board, mail as mail, a list as a list. It always reads through a jojobot view, the same model the bots ask, and never the events or the store directly. A view a bot builds for itself that proves useful can be shipped with the software and given HTML of its own. The model takes who is asking from the start, so a page and a bot asking the same view each get what is theirs to see.

## The homepage is the root

**The homepage is the root of the filesystem: what jojobot holds, listed and linked, the way a directory index is.** It is jojobot's own front door, not a dashboard built for one reader. **What the bots are doing right now is a page reached from it**: each live run, its bot, the bot's own focus line in its own words, and its last activity with how long ago. jojobot never composes a summary of what a bot is doing; the bot's focus is the summary.

## A thing's page

**A thing's page shows the fold first: one value per key, what the thing is now.** Beside each value: who wrote it, when, and whether it was said, read or worked out, because a value without its provenance cannot be checked. **A key opens to the writes behind it**, oldest first.

**A page states how the thing stands against its kind.** It names the declared keys the thing holds and the ones it lacks. 🚨 **A key held badly is not a key held:** when a value does not hold what its key declared, the page says so and names the key, and a thing with one wrong value never reads as complete. The operator cannot work this out from anywhere else. **A page says whether strictness is on for that thing**, so the operator can tell a thing jojobot guards from one it does not.

**Around the thing**: its parent, its children, what points at it and what it points at, within the answer budget.

**Who is working on it.** A thing a bot has said it is working on shows that bot's live session at the top, and the sessions that worked on it before as a dated list, each linking to the session's page. Work on a task shows on every level above it, up to the project. It comes from the graph, as an inbound walk from the sessions that named the thing; nothing keeps a list by hand, and a write alone never counts as working on something.

## Built-in kinds look like what they are

**A shipped view of a built-in kind renders by what it means, and is placed where it applies.** A piece of work reads as a task. A project reads as a board, its work in its own status columns. A rhythm reads as a loop with its next due day. **A view the operator declares renders by the shape of its answer**: writes over time as a timeline, objects as a table, a tree as a tree. So a view declared tomorrow renders well with no change to the UI, and no list of view names lives in the UI.

**A view's page runs it and lists the answer**, its question shown above. A view that cannot run says which key it is short of, rather than rendering as an empty list. **A view can be bound to the thing being looked at**, so one question serves every page of its kind.

**One executor answers a view for the page and for recall alike.** A bot asking by name and the operator looking get the same answer, which is what makes the page a check on what the bots see.

## Sessions and bots

**A session's page tells what that run did, in time order, and stays readable.** Its journal leads, tallies stay tallies, and a long entry is folded; the page shows the story of the run, not every row behind it. What it wrote, sent and took is reachable from there, one level down.

**A bot's page lists its past sessions**, newest first, each linking to its page. It shows the bot's charter and rules, and **where the bot sits in the org chart**: the chain of whom it reports to, up to the head, and who reports to it.

## Mail as threads

**Mail reads as conversations, not as boxes.** A thread follows replies across the boxes they landed in, so the operator reads what bots tell each other in the order it was said. The noise a person does not read (ids, state words, machine timestamps, per-row notes) stays out of the way. **The operator's own box is theirs to read and finish**, with Open and Mark processed: the one place the window acts.

**A numbered collection reads by its numbers.** A rule is reached as its collection and its number, in a URL as much as in a call.

## It is the operator's window, not a bot's

**No bot reads this surface.** The rules that shape what an agent may be told do not govern what it shows. It shows every record the operator has: entities, events, mailboxes, messages, sessions and journals.

**The constraint that replaces those rules is stronger: the window must not affect bot behaviour.** Looking takes no delivery. It marks nothing read. It opens no session. It writes nothing, except the operator's own Open and Mark processed on their mail, which are their act and not a look. A window that changed what it looked at would make everything seen through it suspect.

## Login

**The operator logs in through the issuer jojobot already verifies against.** Authorization code with PKCE, then a session of its own.

**Two audiences, one check.** The agent surface accepts a token minted for jojobot itself. The page accepts an ID token minted for its own client, because an ID token is addressed to the client that asked for it. Both come from the same issuer, are verified with the same keys, and pass the same allowlist of who may enter. **No second door, and no development bypass that could ship.**

## What it is not

**Not an editor.** Beyond the operator's own mail, it changes nothing. A change goes through the surface, where the guards are.

**Not a dashboard.** jojobot summarises nothing here, scores nothing and ranks nothing; a "possible duplicates" page or a health score is an AI-built summary, and those are out. ⚠️ **A view's page is not the exception it looks like:** a view's content is its answer, so running it is showing what is there.

**Not a second store of state.** Every page is a reading of the graph at the moment it is asked.
