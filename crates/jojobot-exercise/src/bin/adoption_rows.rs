//! **Print one row per kept adoption run.**
//!
//!     adoption_rows <directory of <condition>-<run>.jsonl>
//!
//! Reads the raw streams a paid run keeps beside its transcript and prints the
//! table `jojobot_exercise::adoption` builds. Costs nothing and reaches nothing.

use anyhow::{Context, Result};

fn main() -> Result<()> {
    let dir = std::env::args()
        .nth(1)
        .context("adoption_rows needs the directory the runs were kept in")?;
    let rows = jojobot_exercise::adoption::rows_in(
        std::path::Path::new(&dir),
        &jojobot_exercise::adoption::FACTS,
    )?;
    anyhow::ensure!(!rows.is_empty(), "{dir} holds no run");
    print!("{}", jojobot_exercise::adoption::table(&rows));
    Ok(())
}
