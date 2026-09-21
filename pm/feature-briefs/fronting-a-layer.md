# Fronting a layer jojobot does not own

> **jojobot** · kind: feature-brief · boot: on-demand · verified: 2026-09-03

**jojobot stands in front of the services that hold a life, without becoming their owner.** The task board still owns tasks. The calendar still owns time. The wiki still owns its pages. The person whose life it is keeps editing all of them by hand. What jojobot adds is a single doorway with the discipline built into it.

## Why a doorway rather than a copy

**An agent given raw access to five services has to hold five sets of rules**, and it holds them in prose it may or may not consult. Every layer has a way of losing your work that its own API reports as success, and every one of those is a thing somebody has to remember.

**A doorway makes the rule structural.** The read-back happens because the adapter does it, not because a session recalled that it should. That is the same reason the discipline of where things get stored lives in the software: an agent told to be careful is not a guarantee, and an agent that cannot be careless is.

**Fronting is not ingesting.** jojobot reads the outer service live and answers from it. The outer service stays the authority, so there is no sync to reconcile and no second store competing to be believed.

**A copy kept to answer faster is allowed, and it declares how old it is.** The danger was never the copy; it is a copy that cannot be told apart from the truth. So a cached answer says when it was taken and what it was taken from, anything that decides something reads through to the source, and nothing is served from a copy whose age is unknown. **Two stores that both look authoritative is the failure — one authority and a dated projection of it is not.**

## The anti-corruption adapter

**Each layer gets one adapter, and the adapter is where that layer's failure modes go to be tested.** A service that silently drops part of a write is not fixed by the adapter — it is *caught* by it, every time, in a place where the cost of checking does not fall on anybody's context.

**That is what makes the expensive check cheap.** Reading a whole document back to compare it is unaffordable when the comparison lands in an agent's working memory and pointless when nobody does it. Inside the server it costs nothing anyone is counting, and the caller gets the answer rather than the evidence: *the section changed, three blocks in, none lost.*

**A write that did not land as asked says exactly what did land.** Not a bare failure — a failure throws away the one thing the caller most needs, which is which part got through. The receipt names what was written and what was lost, so a caller can tell a half-success from nothing having happened and act on the difference.

## What a caller sees

**One connection, and jojobot's own verbs are the same for every bot.** A bot works out what is appropriate from its charter; jojobot aligns and does not police. **Which foreign surfaces a connection can reach is a property of that connection, not a variation in jojobot's own verb set** — the uniform surface and a narrowed set of reachable layers are different questions and only the first is settled.

**The doorway can originate outward.** Some things are worth saying when nobody asked — a deadline that is about to pass, a price that is about to change. Reaching the person then is a property of the doorway, not of whichever layer happened to hold the date, because a doorway that can only answer is a doorway that goes quiet exactly when it matters most.

*(Whether an outside system can push INWARD — a house or a service writing into a mailbox unprompted — is deliberately not settled here. It wants concrete cases before it wants a design.)*

## What it is not

**Not ownership.** No collection, no calendar and no board is jojobot's. It never creates one in the operator's space and never treats one as shared. The person keeps editing every one of them by hand, and a doorway that quietly starts deciding what belongs in them has stopped being a doorway.

**Not a migration.** What lives in an outer service stays there. Where a record LIVES and what FRONTS it are different questions, and answering the first does not answer the second.

**Not a general proxy.** The point is the discipline, not the reach. An adapter that passes calls through without adding a guard, a receipt or a domain verb has added a hop and nothing else.

**Not a second source of truth.** A copy may be kept and must be dated. What is refused is a copy that answers as though it were the service — undated, written back to, or trusted for a decision — because an invisible staleness in a system whose named failure is confidently wrong answers is the worst possible trade.