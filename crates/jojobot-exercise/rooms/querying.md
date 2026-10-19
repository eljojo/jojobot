# The querying room

**Server day: 2026-10-13.**
The room's server acts out this day on every run, paid or not, so a stamp it writes
never meets a needle. A sitting's own day is the `**Day:**` marker, a different thing.

**One project, twelve pieces of work, and one cold sitting that is asked the
eight questions a board is asked.**

`asking.md` asks what a year of notes can answer. This room asks the question a
person puts to a board of work: *who is busy, what can I hand out, what is
waiting to go out, what is on this line's plate, who else is touching this
file, what stage is everything in.* The work is already on the board when the
sitting arrives. Nothing is remembered and nothing needs to be: **every answer is
a question put to the store**, and the room measures whether a session that was
told nothing finds the way to put it.

**Each answerable question is one recall expression on this build.** The room
carries a case for each (`solvable_…` in `tests/querying_room.rs`) that asks it
that way and checks the answer is the one the room is built around, so a refusal
in a paid run is a finding about what the surface teaches and never about a
question that could not be answered. **The run prints how many recall calls the
room refused and the arguments each was made with** (the section on refused
recall calls at the end of its output): the number is the measurement, and the
arguments say which question each refusal was.

## The questions, and what reads each one

The numbers are the operator's, and each is the slot its answer is kept in:
`answer_1` for the first, `answer_2` for the second, and so on. **Two of the
eight have no machine check, and the room says so rather than counting them as
passed** (5 and 8): the board expected both to stay outside jojobot, so they are
asked as the board asks them and a person reads them with the transcript beside
them.

| Slot | The question | What reads it |
| --- | --- | --- |
| 1 | who is busy right now, and on what | the slot |
| 2 | what could I hand out right now | the slot |
| 3 | what is accepted and not yet carried, in the order it will be carried | the slot, the set |
| 4 | what is on beta's plate, in the order it comes | the slot, the set |
| 5 | which commit landed the store change, and is it on the main branch yet | a person |
| 6 | what is still open that touches `src/ui.rs` | the slot |
| 7 | which stages the migration's work is in right now | the slot |
| 8 | what is blocked, and what does each one say today | a person |

**The order in 3 and 4 is held by the solvability cases and not by a lock.** A
lock is a substring of the answer and says that a handle is there, not where, so
the slot holds the set and the order is read from the transcript.

## The fiction

A team moves a small system to a new store. The operator is unnamed and says
"I". The work sits on one board, `project:migration`, in the board's own
stages: **queued, working, accepted, carried** and **done**, beside the five a
project always has (someday, next, now, waiting, done), of which the board uses
only done. Every piece of work belongs to one of three lines, alpha, beta and
gamma, and holds the order it comes in on its line. A piece that has been
accepted holds the day it is due to be carried. A piece names the files it
touches. Some wait on a colleague, and some wait on another piece.

**The register is a person running a board.** The operator's words carry the
WORK and never the METHOD: no verb, no key, no place anything is kept, and no
hint that anybody is measuring. The only words in the room that are not the
operator's ordinary speech are the names of the slots, which the operator gives
where they will look.

## What the room holds before the occupant arrives

The board's vocabulary, **the stages and what the board means by them**: the
project lists its own stages as columns, and the instance reads *carried* as a
word for finished, which is what makes a piece that depends on a carried one
startable. The keys a question compares or orders by are declared, because a
comparison needs a key declared a number or a date and a `contains` one declared
a list. And twelve pieces of work, whose first claims set the keys the questions
read.

```world
entity  person:kyle | Kyle

# The board, with its own stages. The five a project always has stay in the list.
entity  project:migration | The Migration
record  project:migration | {"columns": "someday, next, now, waiting, done, queued, working, accepted, carried"} | the board's stages

# What the board means by a stage: a carried piece is a finished one.
record  topic:instance | {"status_word/carried": "done"} | the board reads carried as finished

# The keys the questions compare and order by.
type    board | position=number, carry=date, touches=list:text, line=text

child   project:migration | work:delta | Delta
record  work:delta | {"status": "working", "line": "alpha", "position": "1", "touches": "src/store.rs, src/ui.rs"} | in hand on alpha

child   project:migration | work:epsilon | Epsilon
record  work:epsilon | {"status": "queued", "line": "alpha", "position": "2", "touches": "src/ui.rs"} | next on alpha

child   project:migration | work:sigma | Sigma
record  work:sigma | {"status": "queued", "line": "alpha", "position": "3", "touches": "docs/brief.md", "depends_on": "work:epsilon"} | after epsilon

child   project:migration | work:tau | Tau
record  work:tau | {"status": "working", "line": "beta", "position": "1", "touches": "src/store.rs"} | in hand on beta

child   project:migration | work:theta | Theta
record  work:theta | {"status": "queued", "line": "beta", "position": "2", "touches": "src/mail.rs", "waiting_on": "person:kyle"} | held for Kyle

child   project:migration | work:iota | Iota
record  work:iota | {"status": "accepted", "line": "beta", "position": "3", "carry": "2026-10-07", "touches": "src/mail.rs"} | accepted on beta

child   project:migration | work:kappa | Kappa
record  work:kappa | {"status": "accepted", "line": "gamma", "position": "1", "carry": "2026-10-05", "touches": "docs/brief.md"} | accepted on gamma

child   project:migration | work:lambda | Lambda
record  work:lambda | {"status": "accepted", "line": "gamma", "position": "2", "carry": "2026-10-09", "touches": "src/ui.rs"} | accepted on gamma

child   project:migration | work:omega | Omega
record  work:omega | {"status": "carried", "line": "gamma", "position": "3", "carry": "2026-10-02", "touches": "src/store.rs, src/ui.rs"} | carried on gamma

child   project:migration | work:rho | Rho
record  work:rho | {"status": "queued", "line": "gamma", "position": "4", "touches": "src/mail.rs", "depends_on": "work:omega"} | after omega

child   project:migration | work:xi | Xi
record  work:xi | {"status": "done", "line": "alpha", "position": "5", "touches": "src/ui.rs"} | finished on alpha

child   project:migration | work:omicron | Omicron
record  work:omicron | {"status": "queued", "line": "beta", "position": "10", "touches": "src/lib.rs"} | plain and waiting on beta, tenth in line
```

## Phase 1 — the board, asked cold

**Session: fresh.** **Day: 2026-10-12.**

⚠️ **The arithmetic, so a reader can check the room is solvable.**

* **Busy** (1): the pieces in the *working* stage are delta on alpha and tau on
  beta, and nothing else.
* **Hand out** (2): the pieces in the *queued* stage that wait on nobody and on
  nothing unfinished are epsilon, rho and omicron. Sigma waits on epsilon, which is
  not finished. Theta waits on Kyle. Rho waits on omega, which is *carried*, and the
  instance reads carried as finished, so rho is startable.
* **Accepted, not carried** (3): iota, kappa and lambda. In the order they will be
  carried: kappa (5 October), iota (7 October), lambda (9 October). Omega is carried
  already and is not among them.
* **Beta's plate** (4): the pieces of beta that are queued or working, in the
  order they come: tau (1), theta (2), omicron (10). Iota is on beta and is
  accepted, so it is not. **Omicron is tenth, so the order is a number's:** read
  as text, 10 sorts before 2 and omicron would come second.
* **Touching `src/ui.rs`, still open** (6): delta, epsilon and lambda. Xi is done
  and omega is carried, and both touch it.
* **Stages in use** (7): working, queued, accepted, carried and done. Someday,
  next, now and waiting are in the board's list and no piece is in them.

> This is a role-play: play the day below as if it is really today.
>
> Nobody will answer you, but your answer is being read. You must answer.
>
> start jojobot as assistant — it is 12 October 2026 and I want to ask you some things about the migration board. Number them as I do, write every file as it is written, and keep each answer where a later sitting of yours can find it, under the name I give it, on whatever it is about: answer_1 for the first, answer_2 for the second, and so on. 1. Who is busy right now, and on what? 2. What could I hand out right now? 3. Which pieces of work have I accepted but not carried yet, in the order they will be carried? 4. What is on beta's plate, in the order it comes? 5. Which commit landed the store change, and is it on the main branch yet? 6. What open work has its hands in src/ui.rs? 7. Which stages is the migration's work in right now? 8. What is blocked, and what does each one say today?

```locks
# The slots. Each is the key the entry names, and the answer is the whole record
# the key sits on, so every read below is narrowed to that key. A sitting may instead
# keep an answer as a document whose handle is the slot (a handle holds no underscore,
# so answer_1 is document:answer-1), and the `also` line finds it when the key holds
# nothing. A slot with neither says so, which is not a wrong answer.

# 1. Busy: delta and tau, and nothing that is only queued or only accepted.
recall {"fields": [{"key": "answer_1"}], "keys": ["answer_1"]}
also    read_document {"document": "document:answer-1"}
unkept  no answer was kept for slot 1
carries work:delta
carries work:tau
lacks   work:epsilon
lacks   work:kappa
say     the board: the pieces in hand are not delta and tau alone, so either one was missed or a queued or an accepted piece was counted as busy

# 2. What could be handed out: epsilon, rho and omicron. Sigma waits on a piece that is
# not finished, theta waits on a colleague, and rho is startable only because the
# board reads carried as finished.
recall {"fields": [{"key": "answer_2"}], "keys": ["answer_2"]}
also    read_document {"document": "document:answer-2"}
unkept  no answer was kept for slot 2
carries work:epsilon
carries work:rho
carries work:omicron
lacks   work:sigma
lacks   work:theta
say     the board: what could be handed out is not epsilon, rho and omicron alone, so either a piece that waits was offered or rho, which waits only on a carried piece, was left out

# 3. Accepted and not yet carried: iota, kappa and lambda. Omega is carried already.
recall {"fields": [{"key": "answer_3"}], "keys": ["answer_3"]}
also    read_document {"document": "document:answer-3"}
unkept  no answer was kept for slot 3
carries work:iota
carries work:kappa
carries work:lambda
lacks   work:omega
say     the board: the pieces accepted and not yet carried are not iota, kappa and lambda alone, so one is missing or omega, which is carried already, was listed

# 4. Beta's plate: the queued and working pieces of beta. Iota is beta's and accepted.
recall {"fields": [{"key": "answer_4"}], "keys": ["answer_4"]}
also    read_document {"document": "document:answer-4"}
unkept  no answer was kept for slot 4
carries work:tau
carries work:theta
carries work:omicron
lacks   work:iota
lacks   work:delta
say     the board: beta's plate is not tau, theta and omicron alone, so one is missing, iota, which is accepted, was listed, or a piece of another line was

# 6. Open and touching src/ui.rs: delta, epsilon and lambda. Xi is done and omega is carried.
recall {"fields": [{"key": "answer_6"}], "keys": ["answer_6"]}
also    read_document {"document": "document:answer-6"}
unkept  no answer was kept for slot 6
carries work:delta
carries work:epsilon
carries work:lambda
lacks   work:xi
lacks   work:omega
say     the board: what is open and touches src/ui.rs is not delta, epsilon and lambda alone, so one is missing or xi, which is done, or omega, which is carried, was listed

# 7. The stages in use: working, queued, accepted, carried and done. The board lists nine,
# and no piece is in the other four. A sentence may name those four as empty, so nothing
# here refuses them; a person reads which way the stage words were taken.
recall {"fields": [{"key": "answer_7"}], "keys": ["answer_7"]}
also    read_document {"document": "document:answer-7"}
unkept  no answer was kept for slot 7
carries working
carries queued
carries accepted
carries carried
carries done
say     the board: the stages in use are not working, queued, accepted, carried and done, so one is missing
```

## What this room cannot measure

**Whether anything here was easy to find.** Every lock says a question is
answerable. None says the answer was near the surface.

**Two questions are a person's reading** (5 and 8). Which commit landed a change
and whether it is on a branch is a question about git, and what each blocked piece
says today is the board's text; the board expected both to stay outside jojobot,
so a lock over a model's phrasing would measure the phrasing. The recorded
answers sit in the store for whoever reads the run.

**The order in 3 and 4.** A lock is a substring of the answer and cannot say which
handle comes first. The solvability cases hold that each question is answerable in
its order with one expression; the order the sitting gave is in the transcript.

**Whether the stage words in 7 were read from the work or from the project's list
of stages, and which four the sitting says are empty.** The lock holds that the five
stages in use are there. It does not hold the four unused ones absent: a right answer
says they are empty, and a lock that refuses the sentence refuses the answer. The four
are someday, next, now and waiting, and a person reads the answer beside the
transcript.

**Pieces named by name.** A lock on a handle also holds when the answer names the
entity by its name, as a whole word in any case, because a person answering writes
Delta and not `work:delta`. A needle that is no handle is matched as written.

**What a refusal taught.** The count of refused recall calls says how many
questions the surface did not answer as they were put. It does not say whether the
next call was better for it; the arguments of each refused call are printed beside
it so a reader can follow the sitting from one to the next.
