//! **The receipt argument `capture` and `update_fact` share.**
//!
//! A promotion to settled carries one: a check that ran, or the operator's
//! confirmation with its day and place. The two shapes are told apart on the
//! wire by which key is present, and exactly one must be.

use jojobot_domain::memory::Receipt;

use super::parse::parse_date;
use super::*;

/// What a claim was settled on.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct ReceiptArg {
    /// **A check that ran**: what was checked, written so another agent can run
    /// it again. It records that a check ran and never runs it.
    #[serde(default)]
    pub(crate) check: Option<String>,
    /// **The operator confirmed it**: the day, and where (a message id or the
    /// session).
    #[serde(default)]
    pub(crate) confirmed: Option<ConfirmedArg>,
}

/// Where and when the operator confirmed a claim.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct ConfirmedArg {
    /// The day the operator confirmed it, `YYYY-MM-DD`.
    pub(crate) on: String,
    /// Where: a message id or the session.
    pub(crate) at: String,
}

/// The domain receipt a call's `receipt` argument names. **Exactly one of
/// `check` and `confirmed`**: neither is no receipt, and both would be two
/// receipts under one name.
pub(crate) fn parse_receipt(arg: &ReceiptArg) -> Result<Receipt, McpError> {
    match (&arg.check, &arg.confirmed) {
        (Some(check), None) => Ok(Receipt::Check(check.clone())),
        (None, Some(confirmed)) => {
            let on = parse_date(Some(&confirmed.on))?.ok_or_else(|| {
                McpError::invalid_params(
                    "a confirmed receipt needs the day the operator confirmed it, `on`, as \
                     YYYY-MM-DD"
                        .to_string(),
                    None,
                )
            })?;
            Ok(Receipt::Confirmed {
                on,
                at: confirmed.at.clone(),
            })
        }
        _ => Err(McpError::invalid_params(
            "a receipt names exactly one of `check` (what was checked, written so another agent \
             can run it again) and `confirmed` (the operator confirmed it: `on` the day, `at` \
             where, a message id or the session)"
                .to_string(),
            None,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(text: &str) -> ReceiptArg {
        ReceiptArg {
            check: Some(text.into()),
            confirmed: None,
        }
    }

    fn confirmed(on: &str, at: &str) -> ReceiptArg {
        ReceiptArg {
            check: None,
            confirmed: Some(ConfirmedArg {
                on: on.into(),
                at: at.into(),
            }),
        }
    }

    /// **Exactly one shape.** Each of the two names its receipt; none, both and
    /// a day that is no day are refused as malformed calls. The positives sit
    /// beside the refusals so a parser that refused everything would fail here.
    #[test]
    fn a_receipt_names_exactly_one_shape() {
        assert_eq!(
            parse_receipt(&check("counted the locks")).expect("a check parses"),
            Receipt::Check("counted the locks".into())
        );
        assert_eq!(
            parse_receipt(&confirmed("2026-10-12", "message q7w2ke"))
                .expect("a confirmation parses"),
            Receipt::Confirmed {
                on: jiff::civil::date(2026, 10, 12),
                at: "message q7w2ke".into(),
            }
        );

        let neither = ReceiptArg {
            check: None,
            confirmed: None,
        };
        let both = ReceiptArg {
            check: Some("counted".into()),
            ..confirmed("2026-10-12", "message q7w2ke")
        };
        for refused in [neither, both] {
            let err = parse_receipt(&refused).expect_err("not exactly one shape");
            assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        }
        let err = parse_receipt(&confirmed("last tuesday", "message q7w2ke"))
            .expect_err("a day that is no day");
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        let err = parse_receipt(&confirmed("", "message q7w2ke")).expect_err("no day at all");
        assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
}
