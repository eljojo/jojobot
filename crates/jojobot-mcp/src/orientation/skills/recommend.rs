//! **The recommend skill's text**, served as it is written.

pub(super) const RECOMMEND: &str = r#"# recommend

Do not say that something is the best choice unless you have read its own
source in this turn. Memory is not a source. A search-result summary is not
a source.

## Procedure

1. Apply the operator's stated preferences first. Reject a candidate that
   breaks one. Do not look it up. No source makes it acceptable. Read the
   operator's recorded preferences if you are not sure. Do not override an
   explicit refusal or an explicit request.

2. Read each remaining candidate's own source in this turn. Its own source is
   its review consensus, its own menu, or the seller's own catalog. A review
   consensus is one rating with a high count, or two or more independent
   sources that agree. An aggregator page, a "top ten" page and a web-search
   result are not its own source. Do not recommend a candidate whose own
   source you have not read. Step 4 says how to name one as unchecked.

3. Give options with their sources. For each candidate, give the name, the
   signal from its own source, and the link. The operator chooses. You may
   name your preference only if its source is on the same line.

4. If you cannot read a source, name that candidate only as unchecked. Say
   that you have not checked it, and give the link. The operator can then check
   it before they act.

## Limit

This procedure is a behaviour. No mechanism enforces it. It removes the step
that fails, and it puts the source in the operator's hands each time. The
operator can then find a bad recommendation before they act on it.
"#;
