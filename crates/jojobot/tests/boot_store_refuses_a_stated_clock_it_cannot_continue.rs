//! **A server acting out a day refuses to boot when it cannot read where the
//! clock must continue from.**
//!
//! A stated clock continues after the newest stamp the store holds. When that
//! read fails, a clock that started at its day's midnight anyway would stamp new
//! records before older ones, and nothing afterwards could tell. The real clock
//! reads nothing at boot, so the same failure leaves it alone.
//!
//! **A test binary of its own.** `boot_store` seeds the process-wide kind set on
//! success, and a case running beside this one would see it already loaded.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::free_port;
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::kinds;

use jojobot_adapters::testing::Scratch;

fn day() -> jiff::civil::Date {
    jiff::civil::date(2026, 6, 1)
}

/// **A migrated store in which the newest-stamp read fails and nothing else
/// does.** The read finds the columns that keep an instant in the schema and
/// then reads each by name. A column whose name holds a backtick is found, and
/// the read built from its name is not valid SQL.
async fn store_whose_newest_stamp_read_fails(path: &std::path::Path, port: u16) {
    let mut server = Dolt::start(path, port).await.expect("the store comes up");
    migrate::run(server.pool())
        .await
        .expect("the schema applies");
    sqlx::raw_sql("CREATE TABLE stamp_probe (id INT PRIMARY KEY, `bad``stamp` VARCHAR(40) NULL)")
        .execute(server.pool())
        .await
        .expect("a table with an awkward column");
    let read = jojobot_adapters::dolt::newest_stamp_from(server.pool(), day()).await;
    server.stop().await;
    assert!(
        read.is_err(),
        "the store this case builds must fail the newest-stamp read, or it proves nothing"
    );
}

#[tokio::test]
async fn a_stated_clock_refuses_the_boot_when_the_newest_stamp_cannot_be_read() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-stated-refuses");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let _scratch = Scratch(path.clone());
    let port = free_port();
    store_whose_newest_stamp_read_fails(&path, port).await;

    let refused = jojobot::wiring::boot_store(&path, port, Clock::stating(day())).await;

    let Err(error) = refused else {
        panic!("a stated clock that could not continue must refuse the boot");
    };
    let said = format!("{error:#}");
    assert!(
        said.contains("2026-06-01"),
        "the refusal names the stated day: {said}"
    );
}

#[tokio::test]
async fn the_real_clock_boots_over_a_store_whose_newest_stamp_read_fails() {
    let path = jojobot_adapters::testing::scratch_path("jojobot-real-clock-boots");
    std::fs::create_dir_all(&path).expect("a scratch directory");
    let scratch = Scratch(path.clone());
    let port = free_port();
    store_whose_newest_stamp_read_fails(&path, port).await;

    let booted = jojobot::wiring::boot_store(&path, port, Clock::Real)
        .await
        .expect("the real clock reads no stamp at boot, so the same store boots it");

    let mut store = booted.store;
    store.stop().await;
    drop(scratch);
    kinds::load_shipped();
}
