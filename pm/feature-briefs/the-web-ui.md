# The web UI — a window on your own instance

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**A directory listing over the graph, the way a web server indexes a directory.** An entity path is a URL path. A page lists what is at that node and links onward.

## One index page per node

**The surface is read-only, and there is one page per node.** You walk the graph by walking the URLs. The path grammar and the entity tree are the same thing. This is what makes the whole surface navigable without anybody designing screens.

**There is no application.** Client-side state, a framework or a component tree is the tell that somebody left the brief. A page renders what is at a node and links to its neighbours. There is nothing else to build.

## What a page shows

**A thing's page shows the fold first.** One value per key: what the thing is now.

**A key opens to the events behind it**, oldest first. The projection is what the operator reads. The history is one click away.

**A page states how the thing stands against its kind.** It names which declared keys the thing holds and which it lacks.

🚨 **A key held badly is not a key held.** When a value does not hold what its key declared, the page says so and names the key. A thing that holds every declared key with one wrong value **must not read as complete**. The operator cannot work this out from anywhere else on the page, so the page is the only place it can go wrong unseen.

**A page says whether strictness is on for that thing.** Otherwise the operator cannot tell a thing jojobot guards from one it does not.

## A view's page answers its own question

**A view is a question somebody already worked out, so its page RUNS it and lists the answer** — in the same directory shape every other page uses, each result linking onward. **Its definition stays above the answer**, because a reader should see the question they just asked.

⭐ **This is the one kind whose page would otherwise be correct and useless.** Every other kind's page answers *what is this thing* by showing its fields. **For a view, those same fields are the query rather than the content, so showing them alone answers a question nobody asked and hides the one they did.**

**A view that cannot be run says which key it is short of**, rather than rendering as an empty list — the two are indistinguishable to a reader otherwise, and only one of them means *there is nothing here*.

⚠️ **Running a stored query is not summarising.** **It computes nothing the query does not compute**, takes no delivery, opens no session and writes nothing.

## It is the operator's window, not a bot's

**No bot reads this surface.** The rules that shape what an agent may be told do not govern what it shows. It shows every record the operator has: entities, events, mailboxes, messages, sessions and journals. It is their instance and their data.

**The constraint that replaces those rules is stronger: the window must not affect bot behaviour.** Looking takes no delivery. It marks nothing read. It opens no session. It writes nothing. A window that changed what it looked at would be worse than no window, because everything seen through it would then be suspect.

## Login

**The operator logs in through the issuer jojobot already verifies against.** Authorization code with PKCE, then a session of its own.

**One client, not two.** The browser and the agent surface share an audience. They are revoked together, knowingly. A second registration is not worth it on a personal instance.

**No second door, and no development bypass that could ship.** One module checks both audiences. That is what keeps them from becoming two ways in.

## What it is not

**Not an editor.** Reading is the whole of it. A change goes through the surface, where the guards are.

**Not a dashboard.** jojobot summarises nothing here, scores nothing and ranks nothing. The page shows what is there. ⚠️ **A view's page is not the exception it looks like:** a view's content IS its answer, so running the query is showing what is there rather than summarising it.