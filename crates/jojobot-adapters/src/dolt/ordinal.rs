//! **The next ordinal of an append-only table**, decided in one place.
//!
//! An append-only table keeps a write as a row numbered after the rows its key
//! already holds, so every append asks the store for the highest number first.
//! That question was spelled at eleven sites, in two ways, and the way that read
//! the answer with `fetch_one` fails on a store that answers an ungrouped
//! aggregate over no rows with no row at all, where the stores before it answered
//! with one row holding NULL. **Both answers mean the same thing here: none yet,
//! so 1.**
//!
//! The caller names the table and the columns that make up the key. This does not
//! guess them, because the tables key differently: one by an entity and a key,
//! one by a message, one by nothing at all.

use sqlx::MySqlConnection;

/// **What the store answered for the highest ordinal**: `None` when it sent no
/// row, `Some(None)` when it sent one row holding NULL, `Some(Some(n))` when the
/// key holds rows. Kept apart from the read so the three answers can each be put
/// to the rule below without a store that sends the first.
type Highest = Option<Option<i64>>;

/// The ordinal after `highest`: 1 when the key holds none, whichever way the
/// store said so.
fn after(highest: Highest) -> i64 {
    highest.flatten().unwrap_or(0) + 1
}

/// **The ordinal the next row of `table` takes for `key`.**
///
/// `key` pairs a column with the value the rows are held under; an empty `key`
/// numbers the whole table. The names are the caller's own constants and are
/// quoted, never taken from input. The statement is one ungrouped `MAX`, cast to
/// a signed integer so an `INT` column and a `BIGINT` column read alike.
///
/// Call it inside the transaction that inserts the row, so the answer and the
/// insert see the same rows.
pub(super) async fn next(
    conn: &mut MySqlConnection,
    table: &'static str,
    key: &[(&'static str, &str)],
) -> Result<i64, sqlx::Error> {
    let mut sql = format!("SELECT CAST(MAX(ordinal) AS SIGNED) FROM `{table}`");
    for (at, (column, _)) in key.iter().enumerate() {
        sql.push_str(if at == 0 { " WHERE " } else { " AND " });
        sql.push_str(&format!("`{column}` = ?"));
    }
    let mut query = sqlx::query_scalar::<_, Option<i64>>(&sql);
    for (_, value) in key {
        query = query.bind(*value);
    }
    let highest: Highest = query.fetch_optional(&mut *conn).await?;
    Ok(after(highest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::Dolt;
    use crate::dolt::tests::Scratch;
    use crate::testing::free_port;

    /// **No row and a NULL row both read as none yet.** A real store of today
    /// sends the NULL row; the store a newer Dolt brings sends no row, and a read
    /// that cannot tell them apart is the only read that holds on both.
    #[test]
    fn no_row_and_a_null_row_both_mean_none_yet_and_a_count_is_followed() {
        assert_eq!(after(None), 1, "the store sent no row");
        assert_eq!(after(Some(None)), 1, "the store sent a row holding NULL");
        assert_eq!(after(Some(Some(7))), 8, "the key holds seven rows");
    }

    /// **1 on an empty key, N + 1 after N appends**, scoped to the key it was
    /// asked about, over a key of two columns, of one and of none, on the real
    /// store.
    #[tokio::test]
    async fn the_next_ordinal_is_one_on_an_empty_key_and_n_plus_one_after_n_appends() {
        let scratch = Scratch::new("next-ordinal");
        let mut store = Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let mut conn = store.pool().acquire().await.expect("a connection");
        sqlx::query("CREATE DATABASE ordinal_probe")
            .execute(&mut *conn)
            .await
            .expect("a database of this case's own");
        sqlx::query("USE ordinal_probe")
            .execute(&mut *conn)
            .await
            .expect("it is used");
        sqlx::query(
            "CREATE TABLE probe_log (owner VARCHAR(20) NOT NULL, `key` VARCHAR(20) NOT NULL, \
             ordinal INT NOT NULL, PRIMARY KEY (owner, `key`, ordinal))",
        )
        .execute(&mut *conn)
        .await
        .expect("the table");
        sqlx::query(
            "CREATE TABLE probe_big (owner VARCHAR(20) NOT NULL, ordinal BIGINT NOT NULL, \
             PRIMARY KEY (owner, ordinal))",
        )
        .execute(&mut *conn)
        .await
        .expect("the table");

        // A table holding no row at all: the whole-table question too.
        assert_eq!(next(&mut conn, "probe_log", &[]).await.expect("read"), 1);
        assert_eq!(
            next(
                &mut conn,
                "probe_log",
                &[("owner", "marge"), ("key", "mood")]
            )
            .await
            .expect("read"),
            1,
            "an empty key"
        );

        for n in 1..=3 {
            let ordinal = next(
                &mut conn,
                "probe_log",
                &[("owner", "marge"), ("key", "mood")],
            )
            .await
            .expect("read");
            assert_eq!(ordinal, n, "after {} appends", n - 1);
            sqlx::query("INSERT INTO probe_log (owner, `key`, ordinal) VALUES (?, ?, ?)")
                .bind("marge")
                .bind("mood")
                .bind(ordinal)
                .execute(&mut *conn)
                .await
                .expect("an append");
        }
        assert_eq!(
            next(
                &mut conn,
                "probe_log",
                &[("owner", "marge"), ("key", "mood")]
            )
            .await
            .expect("read"),
            4,
            "after three appends"
        );
        assert_eq!(
            next(
                &mut conn,
                "probe_log",
                &[("owner", "marge"), ("key", "hair")]
            )
            .await
            .expect("read"),
            1,
            "another key of the same owner is still empty"
        );
        assert_eq!(
            next(
                &mut conn,
                "probe_log",
                &[("owner", "homer"), ("key", "mood")]
            )
            .await
            .expect("read"),
            1,
            "another owner is still empty"
        );
        assert_eq!(
            next(&mut conn, "probe_log", &[]).await.expect("read"),
            4,
            "no key numbers the whole table"
        );

        sqlx::query("INSERT INTO probe_big (owner, ordinal) VALUES ('lisa', 9)")
            .execute(&mut *conn)
            .await
            .expect("an append");
        assert_eq!(
            next(&mut conn, "probe_big", &[("owner", "lisa")])
                .await
                .expect("read"),
            10,
            "a BIGINT column reads like an INT one"
        );
        drop(conn);
        store.stop().await;
    }
}
