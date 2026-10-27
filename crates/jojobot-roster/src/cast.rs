/// **The cast.** The people, pets, places and organisations of the four shows,
/// and the Greek letters. Keep it fictional.
///
/// **The bar prints every entry added here since `origin/main`** in its verdict,
/// so a name that could be mistaken for somebody's life is read by a person on
/// the way in. That is the whole reason it is a list of its own: a name that
/// belongs here is one whose being fictional rests on the show it comes from,
/// and a reviewer has to check that, once, in the diff that adds it. A borderline
/// name (a brand, a street that sounds real) stays here for that reason.
///
/// The gate accepts a handle on this list or on [`crate::ORDINARY`]; the two lists
/// together are the allowlist, and no handle is on both.
pub const ROSTER: &[&str] = &[
    "document:bart",
    "document:homer",
    "document:krusty",
    "document:lisa",
    "document:marge",
    "document:skinner",
    "bot:bart",
    "bot:lisa",
    "bot:marge",
    "bot:moe",
    "bot:nelson",
    "bot:patty",
    "bot:ralph",
    "bot:smithers",
    "bot:milhouse",
    "bot:otto",
    "org:costingtons",
    "org:globex",
    "org:krusty-burger",
    "org:moes-tavern",
    "org:moes-taproom", // made up: a second near-name of the tavern, for two overrides on one thing
    // Homer's snowplow business, the Simpsons, repurposed here as a
    // furnace-servicing company.
    "org:mr-plow",
    // Quahog is the fictional town Family Guy is set in.
    // A family around the Daredevil Gorge (a gorge, a town council, a stunt show) that
    // stands for no real place, organisation or event. The sibling and distance shapes
    // the tests measure come from the handles' spelling alone.
    "org:springfield-town-council", // made up: the council of Springfield, a canon town
    "org:quahog-community-college",
    "org:springfield-cyclery",
    "org:springfield-movers",
    "person:butters",
    "person:kenny",
    "person:kyle",
    "person:marjorine",
    "person:apu",
    "person:barney-gumble",
    "person:bart",
    "place:springfield-nuclear-plant",
    "place:kwik-e-mart",
    "person:bart-simpson",
    "person:bartholomew",
    "person:bartholomew-simpson",
    "person:burns",
    // Bob's Burgers.
    "person:gayle",
    "person:gene",
    "person:homer",
    "person:homer-simpson",
    "person:hugo",
    "person:krusty",
    "person:randy",
    "person:sharon",
    "person:stan",
    "person:token",
    "person:wendy",
    // Bob's Burgers.
    "person:linda",
    "person:lisa",
    "person:marge",
    "person:louise",
    "person:martin",
    "person:maude",
    // Homer's alias in The Simpsons.
    "person:max-power",
    "person:milhouse",
    "person:ralph",
    "person:ned-flanders",
    "person:nelson",
    "person:otto",
    "person:skinner",
    // Bob's Burgers.
    "person:teddy",
    "person:tina",
    "pet:santas-little-helper",
    "pet:snowball",
    "place:capital-city",
    "place:leftorium",
    "place:moes-tavern",
    "place:moes-taproom", // made up: the place beside it
    "place:moes",
    "place:drunken-clam", // Family Guy's bar, the second name for a place in the repair room
    "place:daredevil-gorge", // made up: named for the gorge Homer tries to jump in "Bart the Daredevil"
    "place:north-haverbrook",
    // The Belcher family's street, Bob's Burgers.
    "place:ocean-avenue",
    "place:quahog",
    "place:shelbyville",
    "place:springfield",
    "trip:springfield-1999",   // made up: a trip between two roster places
    "trip:shelbyville-2001",   // made up: the half-planned trip beside it
    "topic:rules-springfield", // made up: a numbered topic of decisions
    "project:donut-fund",      // made up: a project of donuts, for a page's how-known note
    "work:donut-budget",       // made up: a piece of donut work, for the same note
    "topic:rules-shelbyville", // made up: a numbered topic with sixty decisions
    "decision:springfield-12",
    "decision:springfield-13",
    "decision:springfield-14",
    "decision:shelbyville-40", // made up: a decision under another town's topic that replaces a springfield one
    "place:springfield-mall",
    // Bob's Burgers.
    "place:wagstaff-pool",
    // Wonder Wharf is the amusement park in Bob's Burgers; the road is a plain road name.
    "place:wharf-road",
    // The amusement park in Bob's Burgers.
    "place:wonder-wharf",
];

/// By PM ruling, these 24 slugs are valid under every known kind. Characters
/// remain typed entries in ROSTER; the Greek list is deliberately universal.
pub const GREEK: &[&str] = &[
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
    "lambda", "mu", "nu", "xi", "omicron", "pi", "rho", "sigma", "tau", "upsilon", "phi", "chi",
    "psi", "omega",
];
