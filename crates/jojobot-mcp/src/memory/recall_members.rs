//! **A `where` that names a member of a numbered collection by its number reads
//! as the member**, in a live read and in a view's own line alike. A field serves
//! the handle of the thing it names, so the address in a value is written as the
//! handle of the member before the line is parsed and compared.

use jiff::civil::Date;
use jojobot_domain::memory::filter;
use jojobot_domain::memory::mention::roster::{expression_member_mentions, members_named};

use super::*;

impl Jojobot {
    /// The filter a `where` line parses to, with each member's address in a value
    /// read as the member. A line naming no address parses as it was written.
    pub(super) async fn parse_naming_members(
        &self,
        text: &str,
        today: Date,
    ) -> Result<filter::Filter, MemoryError> {
        let mentions = expression_member_mentions(text);
        if mentions.is_empty() {
            return filter::expression::parse(text, today);
        }
        let texts: Vec<&str> = mentions.iter().map(String::as_str).collect();
        let known = jojobot_domain::memory::mention::roster_for(&*self.memory, &texts).await?;
        filter::expression::parse(&members_named(text, &known), today)
    }
}
