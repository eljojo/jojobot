//! The complete-batch contract no longer runs on a log with a connection of its
//! own. The writer owns the log: the plain cases run in `writer_log_contract.rs`
//! and the fault cases in `src/writer_log/fault_tests.rs`. This file holds no
//! case and is left for the EM to remove with its line in
//! `crates/jojobot-bar/build-metrics-binaries.txt`.
