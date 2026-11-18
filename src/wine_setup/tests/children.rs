//! The registry of the child processes of an install (curl, tar, xattr): what closing the app
//! and cancelling do to them.

use super::support::{process_state, wait_for};
use crate::wine_setup::*;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

/// The unpacking commands end when the app closes, and nothing new starts.
#[test]
fn closing_the_app_ends_a_running_command_and_refuses_new_ones() {
    let children = Arc::new(Children::new());
    let runner = {
        let children = children.clone();
        std::thread::spawn(move || children.run(Command::new("/bin/sleep").arg("30")))
    };
    wait_for("the command to start", || !children.pids().is_empty());
    let sleeper = children.pids()[0];
    assert_eq!(children.stop(), 1);
    let ended = runner.join().unwrap();
    assert!(ended.is_err(), "the command was interrupted: {ended:?}");
    assert_eq!(process_state(sleeper), "");
    let late = children.run(Command::new("/bin/sh").args(["-c", "exit 0"]));
    assert_eq!(late.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
    assert!(children.pids().is_empty());
}

/// Ending one child (an install the person cancelled) can take a while; the other install
/// commands must not wait for it: the registry is unlocked while the child is being ended.
#[test]
fn ending_one_child_does_not_hold_up_the_registry() {
    let children = Arc::new(Children::new());
    let slow = children
        .start(Command::new("/bin/sleep").arg("30"))
        .unwrap();
    let other = children
        .start(Command::new("/bin/sleep").arg("30"))
        .unwrap();
    let (begun_tx, begun_rx) = std::sync::mpsc::channel::<()>();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let killer = {
        let children = children.clone();
        std::thread::spawn(move || {
            children.kill_then(slow, |mut child| {
                // Ending this child is slow: it is held until the test lets it go.
                begun_tx.send(()).unwrap();
                let _ = release_rx.recv_timeout(Duration::from_secs(10));
                let _ = child.kill();
                let _ = child.wait();
            });
        })
    };
    begun_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // Meanwhile the registry answers about another child and starts a new one.
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let user = {
        let children = children.clone();
        std::thread::spawn(move || {
            let running = matches!(children.poll(other), Poll::Running);
            let late = children.start(Command::new("/bin/sleep").arg("30"));
            done_tx.send((running, late.is_ok())).unwrap();
        })
    };
    // It answers at once, so this is only the bound: a registry held by the child that is being
    // ended never answers before the release below, however long it is waited for.
    let answered = done_rx.recv_timeout(Duration::from_secs(10));
    release_tx.send(()).unwrap();
    killer.join().unwrap();
    user.join().unwrap();
    children.stop();
    assert_eq!(
        answered.ok(),
        Some((true, true)),
        "the registry waited for the child that was being ended"
    );
}

/// There is no moment in which a child left the registry without having been sent the kill:
/// a `stop` that runs while `kill` is still waiting finds nothing to end, so the signal must
/// already be out when the registry is unlocked.
#[test]
fn a_child_leaving_the_registry_has_already_been_sent_the_kill() {
    let children = Children::new();
    let id = children
        .start(Command::new("/bin/sleep").arg("30"))
        .unwrap();
    let mut ended = false;
    children.kill_then(id, |mut child| {
        let end = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < end {
            if matches!(child.try_wait(), Ok(Some(_))) {
                ended = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = child.kill();
        let _ = child.wait();
    });
    children.stop();
    assert!(ended, "the child was taken out of the registry still alive");
}

/// A child that is ended is reaped before `kill` returns (no zombie), and it is no longer
/// tracked; the others go on.
#[test]
fn killing_a_child_reaps_it_and_leaves_the_others() {
    let children = Children::new();
    let doomed = children
        .start(Command::new("/bin/sleep").arg("30"))
        .unwrap();
    let kept = children
        .start(Command::new("/bin/sleep").arg("30"))
        .unwrap();
    let pids = children.pids();
    children.kill(doomed);
    assert_eq!(process_state(pids[0]), "", "the killed child is a zombie");
    assert_eq!(children.pids(), [pids[1]]);
    assert!(matches!(children.poll(doomed), Poll::Gone));
    assert!(matches!(children.poll(kept), Poll::Running));
    // Killing it again, or a child that never was, does nothing.
    children.kill(doomed);
    children.kill(9999);
    assert_eq!(children.stop(), 1);
    assert_eq!(process_state(pids[1]), "");
}

/// A command that ends by itself is reaped and gives its status.
#[test]
fn a_command_that_ends_is_reaped_with_its_status() {
    let children = Children::new();
    let ok = children
        .run(Command::new("/bin/sh").args(["-c", "exit 0"]))
        .unwrap();
    let bad = children
        .run(Command::new("/bin/sh").args(["-c", "exit 3"]))
        .unwrap();
    assert!(ok.success());
    assert_eq!(bad.code(), Some(3));
    assert!(children.pids().is_empty());
    assert!(children
        .run(&mut Command::new("/nonexistent/botella"))
        .is_err());
}
