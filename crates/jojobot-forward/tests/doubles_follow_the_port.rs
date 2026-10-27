//! **A double that states the methods it changes follows the port when the port
//! grows.** One scenario is written once and stamped twice: a port, a store that
//! implements it, and the doubles that wrap or replace that store. The second
//! stamp gives the port one more method after the doubles were written, and the
//! doubles' text is the same in both. A case that adds a port method edits the
//! trait and the real implementation, and no double.

use std::sync::Arc;

use async_trait::async_trait;

/// The macro the port's attribute writes calls `$crate::__forward_finish!`.
pub use jojobot_forward::__finish as __forward_finish;

/// The module the port's names are read in; the cases use no names from it.
pub mod names {}

macro_rules! scenario {
    ($module:ident, $callback:ident, $($extra:ident)?) => {
        mod $module {
            use super::*;

            #[jojobot_forward::port(callback = $callback, scope = "names")]
            pub trait Port: Send + Sync {
                async fn name(&self) -> String;
                async fn count(&self, by: u32) -> Result<u32, String>;
                fn label(&self) -> &'static str {
                    "the default"
                }
                $(async fn $extra(&self) -> Result<String, String>;)?
            }

            pub struct Real;

            #[async_trait]
            impl Port for Real {
                async fn name(&self) -> String {
                    "real".into()
                }
                async fn count(&self, by: u32) -> Result<u32, String> {
                    Ok(by * 2)
                }
                fn label(&self) -> &'static str {
                    "the real label"
                }
                $(async fn $extra(&self) -> Result<String, String> {
                    Ok("the real extra".into())
                })?
            }

            /// Changes `name` and forwards the rest.
            pub struct Loud {
                pub inner: Arc<dyn Port>,
            }

            #[jojobot_forward::forward($callback, via = inner)]
            impl Port for Loud {
                async fn name(&self) -> String {
                    self.inner.name().await.to_uppercase()
                }
            }

            /// Forwards only what the port requires, and the method it names.
            pub struct Strict(pub Arc<dyn Port>);

            #[jojobot_forward::forward($callback, via = 0, required_only)]
            impl Port for Strict {}

            /// Answers nothing: each method that can refuse does, with one error.
            pub struct Down;

            #[jojobot_forward::forward($callback, down = String::from("down"))]
            impl Port for Down {
                async fn name(&self) -> String {
                    "the down double's own".into()
                }
            }

            #[tokio::test]
            async fn a_double_changes_what_it_states_and_forwards_the_rest() {
                let loud = Loud { inner: Arc::new(Real) };
                assert_eq!(loud.name().await, "REAL", "the changed method");
                assert_eq!(loud.count(4).await, Ok(8), "a forwarded method");
                assert_eq!(
                    loud.label(),
                    "the real label",
                    "a defaulted method is forwarded, not answered by its default"
                );
                $(assert_eq!(
                    loud.$extra().await,
                    Ok("the real extra".to_string()),
                    "a method the port gained is forwarded with no edit to the double"
                );)?
            }

            #[tokio::test]
            async fn a_double_that_forwards_only_the_required_leaves_a_default_alone() {
                let strict = Strict(Arc::new(Real));
                assert_eq!(strict.name().await, "real");
                assert_eq!(strict.count(1).await, Ok(2));
                assert_eq!(strict.label(), "the default");
            }

            #[tokio::test]
            async fn a_down_double_refuses_each_method_that_can_refuse() {
                assert_eq!(Down.name().await, "the down double's own");
                assert_eq!(Down.count(1).await, Err("down".to_string()));
                assert_eq!(Down.label(), "the default");
                $(assert_eq!(
                    Down.$extra().await,
                    Err("down".to_string()),
                    "a method the port gained is refused with no edit to the double"
                );)?
            }
        }
    };
}

scenario!(before_the_port_grows, forward_port_before,);
scenario!(after_the_port_grows, forward_port_after, extra_method);
