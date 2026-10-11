# Documents — long markdown, held by section, edited with surgical precision

> **jojobot** · kind: feature-brief · boot: on-demand

**A document is a long piece of markdown that jojobot hosts natively: read whole by a person, read in parts by an agent, every edit a version, every untouched byte kept.**

## What it is for

**There is a lot of markdown in a life**: a plan, a spec, a set of notes, a policy somebody keeps. A claim is one sentence and a page of prose is one blob, so neither holds it well. Without a home in jojobot, a long document ends up in a second store that jojobot cannot see into, and anything that points at it can only point at the whole page and cannot tell when it changed.

**Four requirements make a home worth having.** Host it well: whole as a page, in parts for an agent. Version control: every edit is a version, and any earlier version can be read and restored. Safety around editing: an edit made against a stale version, or aimed at text that matches nowhere or in several places, is refused rather than applied to a guess, and every receipt says exactly what changed. Surgical precision: changing one part touches nothing else.

## The section is the unit

**A document is a thing of kind `document`, and each of its sections is one record on it**: a heading and what sits under it. **That makes every section addressable**: a claim cites a section by its address, an edit reaches one section, and the section's own history is the version history of that part.

**Why the section and not the paragraph.** A paragraph per record multiplies the records tenfold and makes every whole read pay for reassembling them. A section is the unit people already cite (*see the second part*), and it keeps a whole read cheap. **A section too long for one record is split near the size a record may hold.** The cap comes from that measurement, not from taste.

**Order is jojobot's.** Each section carries an order key that the server mints. A caller says *insert after this section*; it never picks a number. Moving a section is a write to that section.

## Writing and reading

**A whole document goes in with one call that splits it at its headings on the server.** Writing a long file section by section, each write running every guard, costs many times more for the same result.

**A whole read returns one markdown page**, the sections in order and joined exactly as they were written. **A partial read pages through the sections under the answer ceiling**, and says what it left out and which call returns it.

**The round trip is byte-exact.** The whitespace and separators at a section's edges are kept with the section, so a document read back is the document that was put in, character for character, wherever nobody edited it. **Text inside code stays literal**: a handle in a code span or a fenced block is never resolved, linked or refused.

**Search lands on a section and arrives grouped by its document**, so a hit shows which part of which document matched.

## Editing safely

**An edit names the version it expects.** If the section has changed since that version, the edit is refused and says so. Two sessions editing one section never overwrite each other blind.

**An edit inside a section is a find and replace on an anchor.** An anchor that matches nowhere, or in more than one place, is refused; the edit is never applied to the nearest guess. **The receipt says exactly what changed**, so a lost sentence is visible the moment it is lost.

**Every edit is a version.** A section's history holds every wording it ever had. **A document can be read as it stood on a named day**, and restored by writing an old version back as a new one. Restoring never rewrites history.

## Who backs a section

**A section is a claim, and it carries provenance like any other.** A document a bot wrote from a source is an observation that names the source; one it worked out is inference. **The operator's own words are testimony.** Another session rewrites testimony only by saying how it knows the new words, which keeps the provenance honest across sessions.

## The document's own fields

**The document thing carries what a reader needs before opening it, as ordinary fields:** its `name` is the title; `one_liner`, the one line saying what it is, the same key a bot carries for the same purpose; `stale_after` when the document promises nothing past a day; `resource` when it mirrors a page that still lives elsewhere. Archiving the document retires it whole.

**A document's frontmatter is these fields, derived on the way out, never stored as text.** A file put in with a frontmatter block keeps that block as its first section, verbatim, until something parses it into fields. That way the fields and the frontmatter can never disagree, because only one of them exists.

## It ships after a room proves it

**Documents ship only after a paid room shows a cold session hosting one, citing a section and editing it** through the surface as shipped. A document kind that an agent cannot use well is a second store that only looks native.

## What it is not

⛔️ **Not a file format.** Exporting a subtree as markdown files with frontmatter (for example the Open Knowledge Format) is a read of the store, and importing one is ordinary guarded writes. Neither is how a document is stored, and neither is a restore.

⛔️ **Not a paragraph-per-record store.**

⛔️ **Not an editor.** jojobot holds, versions and guards the text; a person reads it on the web page and an agent edits it through the surface.

⛔️ **Not media.** Images and attachments are not stored here.

⛔️ **Not a move of anybody's existing pages.** Whether a page that lives elsewhere comes in is its owner's decision, taken one document at a time.
