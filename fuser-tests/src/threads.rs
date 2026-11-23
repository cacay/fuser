//! Checks that a multi-threaded session spreads requests across its threads.

use std::convert::Infallible;
use std::io;
use std::path::Path;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::bail;
use tokio::task::JoinError;
use tokio::task::JoinSet;

/// Reads `hello.txt` from many tasks until each of the hello example's `n_threads` threads has
/// served a request, and returns the number of reads each thread served.
pub(crate) async fn assert_all_threads_serve(
    mount_path: &Path,
    n_threads: usize,
) -> anyhow::Result<Vec<u64>> {
    let hello_path = mount_path.join("hello.txt");
    let mut tasks = JoinSet::new();
    for _ in 0..20 {
        let hello_path = hello_path.clone();
        tasks.spawn(async move {
            loop {
                tokio::fs::read_to_string(&hello_path).await?;
            }
        });
    }

    let stats_per_thread = mount_path.join("stats-per-thread");

    let start = Instant::now();

    loop {
        let stats_per_thread = tokio::fs::read_to_string(&stats_per_thread).await?;
        let stats_per_thread: Vec<u64> = stats_per_thread
            .lines()
            .map(|l| l.parse().context("Failed to parse stats line"))
            .collect::<anyhow::Result<_>>()?;
        anyhow::ensure!(stats_per_thread.len() == n_threads);
        if stats_per_thread.iter().all(|t| *t > 0) {
            return Ok(stats_per_thread);
        }

        if start.elapsed() > Duration::from_secs(5) {
            bail!("Not all threads handled requests in 5s; current stats: {stats_per_thread:?}")
        }

        match tasks.try_join_next() {
            None => continue,
            Some(Err::<_, JoinError>(e)) => return Err(e).context("Failed to join task"),
            Some(Ok(Err::<_, io::Error>(e))) => return Err(e).context("Reader thread failed"),
            Some(Ok(Ok::<Infallible, _>(x))) => match x {},
        }
    }
}
