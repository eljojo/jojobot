# Containers — a bounded thing refuses what would overfill it

> **jojobot** · kind: feature-brief · boot: on-demand

**Any thing can carry a ceiling on how much is live under it. A write that would go over is refused, and the writer has to choose what gives. A thread is the container built for an ongoing line in a life.**

## Why a ceiling at all

**Context is always limited, so prioritizing happens somewhere.** Without a bound it happens nowhere: a page, a thread or a bot's state that can always be appended to grows until it is too expensive to read whole and too undifferentiated to act on. Then it can only be searched, and a search finds only what the reader already has words for.

⭐ **The ceiling is not a storage limit. It exists to force the decision at the moment of writing,** when the writer still knows why the new thing matters and what it would displace. **So only content takes a slot:** defrag's mark that it has read a thing forces no such decision, and counts against nothing.

## One mechanism at every scale

**The ceiling is a property of the thing, not of a feature.** A bot's room of thoughts, a thread of rulings and what stays live about Milhouse's bike are bounded by the same mechanism, set the same way.

**Carrying the keys is what makes a thing a container.** Set them on any ordinary thing and the behaviour attaches; nothing is declared and no special kind is needed. The behaviour is written once, for the container, rather than once per kind that can carry it.

## A thing cannot raise its own ceiling

**A capacity a thing raises when it feels crowded is not a capacity.** So the ceiling is set by somebody else: a bot cannot write the ceiling on itself, and a session cannot widen the container it is filling.

**There is one escape valve, and it is used once.** A write may borrow past a full container, landing over capacity. While the debt stands, a second borrow is refused, and bringing the container back under is the first priority. ⛔️ **Swapping one thing in for one out does not repay it** — it leaves the debt where it was.

## Getting in is a decision

**A refusal says what is held and asks the writer to name what gives, and why.** The writer chooses; the software only refuses to pretend the container is bigger than it is. **And it names the skill that shows how to fit more than the container holds:** what applies every time stays inside, and everything else is filed where a pointer inside says it is and when to go and read it.

⛔️ **An always-admitting container lets a busy week evict a year.** Thirty things touched once each displace the slow, important concerns first, because those are the least recently touched.

## A full container shrinks by relocation, never compression

**What makes room is moving detail to the thing it is really about** — a new claim there, and the old one archived with a pointer at its replacement. Nothing is carried across as-is: what a record is about is part of what it says, so moving it to another subject changes it. ⛔️ **Rewriting the same content denser is how a record goes generic and loses what made it useful.**

**Relocation is how a container becomes a tree.** A thread that cannot hold its subject splits into threads nested under it, each bounded the same way. ⭐ **No one draws the taxonomy up front; it is what the bound leaves behind.**

## Threads

**A thread is an ongoing line — a situation rather than a task.** Marge's kitchen renovation is a thread; ordering the tiles is a task. ⭐ **A task ends when it is done; a thread ends when it stops being true.**

**A thought points at a thread rather than restating it** (see *Carried state — the few things a bot holds between runs, and why it is always full*). The thread holds the substance, bounded too — **a bounded room pointing into an unbounded pile is the same failure one level down**, harder to see because the room above still reads clean.

**What was decided is held apart from what was gathered around it.** A decision is its own child of the thread, of the `decision` kind, and its words are an ordinary record on it. Reading a thread puts its standing decisions first, inside the thread's ceiling and the answer's, and names what it left out. A session that reads the research first comes away well informed and confidently wrong, because good research rarely says what was chosen. **A decision that changes is never reworded:** a new decision names the one it replaces, and the old one is archived with its reason, so what was decided when stays readable.

## Numbered collections

**A container can number what it holds.** A thing that carries `numbers_by` names the key its members are numbered by, and every descendant carrying that key holds a whole number, unique across the whole subtree, archived members included. ⭐ **So a member is named by its number:** `topic:springfield-ordinances/32` resolves wherever a handle is taken, and a bot finds ordinance 32 without searching for the words.

**jojobot keeps the count, never the writer.** The next number is a read, the largest held plus one, and a write that would repeat a number is refused, naming the member that holds it. There is no allocator to drift: two writers racing for one number get one success and one refusal that names the winner.

**A number is never reused.** A retired member is archived, not deleted, so its number still resolves, to it and to the reason it was retired. **So a number stays with its member through every verb:** a change of number, a move out of the collection, a merge, a declaration that changes how the key folds, and re-keying or dropping `numbers_by` itself are refused while they would leave a number unheld or held by two, naming the member — and `numbers_by` is taken only by a subtree that already keeps the promise. **No number is ever issued twice, a retraction's included:** it takes the claim back and not the fact that the collection issued the number, so the next number counts it and a write asking a member for it is refused. A member that carries such a number stays in its collection or is archived, which keeps the member and the record; a merge into a thing outside the collection and a move to another parent are refused. Nothing here is specific to rules; a run of episodes or a series of issues numbers the same way.

## Reading a container whole

**A container can be read whole because the ceiling already capped it.** That is the point of the bound: the reader loads everything relevant and nothing more, without guessing where to stop.

**The content is a graph, not a tree.** Something that bears on several containers draws edges at each. **So reading a container is reading it whole plus what points at it** — and an answer that leaves anything out names what it left and the call that returns it.

## What it is not

**Not a folder hierarchy designed in advance.** The shape comes from the bound, not from anybody's taste.

**Not deletion or a retention policy.** What leaves a container is archived and stays reachable.

**Not compression or summarizing on the way in.**

**Not configurable into being unlimited**, by the thing it bounds or by a caller who finds it inconvenient.
