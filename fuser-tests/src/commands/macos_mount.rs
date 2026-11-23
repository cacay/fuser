//! macOS mount tests

use std::path::Path;

use anyhow::Context;
use anyhow::bail;
use tokio::process::Command;

use crate::ansi::green;
use crate::canonical_temp_dir::CanonicalTempDir;
use crate::cargo::cargo_build_example;
use crate::command_utils::command_success;
use crate::mount_util::wait_for_fuse_mount;
use crate::threads::assert_all_threads_serve;

pub(crate) async fn run_macos_mount_tests() -> anyhow::Result<()> {
    let hello_exe = cargo_build_example("hello", &[]).await?;

    for n_threads in [1, 4] {
        run_test(&hello_exe, n_threads).await?;
    }

    green!("All macOS mount tests passed!");
    Ok(())
}

async fn run_test(hello_exe: &Path, n_threads: usize) -> anyhow::Result<()> {
    let description = format!("n_threads={n_threads}");
    eprintln!("\n=== Running test: {description} ===");
    run_test_inner(hello_exe, n_threads)
        .await
        .with_context(|| format!("Tests failed: {description}"))?;
    green!("OK {description}");
    Ok(())
}

async fn run_test_inner(hello_exe: &Path, n_threads: usize) -> anyhow::Result<()> {
    let mount_dir = CanonicalTempDir::new()?;
    let mount_path = mount_dir.path();
    let mount_path_str = mount_path.to_str().unwrap();

    eprintln!("Starting hello filesystem...");
    let n_threads_str = n_threads.to_string();
    // macFUSE keeps file pages cached across opens; `noubc` sends every read to the filesystem, so
    // the per-thread read counts reflect each read.
    let mut fuse_process = Command::new(hello_exe)
        .args([
            mount_path_str,
            "--n-threads",
            &n_threads_str,
            "--mount-option",
            "noubc",
        ])
        .kill_on_drop(true)
        .spawn()
        .context("Failed to start hello example")?;

    wait_for_fuse_mount(mount_path).await?;

    let hello_path = mount_path.join("hello.txt");
    let content = tokio::fs::read_to_string(&hello_path)
        .await
        .context("Failed to read hello.txt")?;

    if content != "Hello World!\n" {
        bail!(
            "hello.txt content mismatch: expected 'Hello World!', got '{}'",
            content
        );
    }

    if n_threads != 1 {
        let stats_per_thread = assert_all_threads_serve(mount_path, n_threads).await?;
        green!("OK multi-threaded tests passed: {stats_per_thread:?}");
    }

    command_success(["umount", mount_path_str]).await?;

    fuse_process
        .kill()
        .await
        .context("Failed to kill/wait FUSE process")?;

    Ok(())
}
