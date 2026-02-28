use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::cargo::cargo_bin_cmd;
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn mkrk(dir: &Path) -> Command {
    let mut cmd: Command = cargo_bin_cmd!("mkrk").into();
    cmd.current_dir(dir);
    cmd.env("NO_COLOR", "1");
    cmd
}

/// Create a project directory inside the tempdir with a valid name.
/// Returns (tempdir_guard, project_path). The tempdir guard must be kept alive.
fn project_dir() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().unwrap();
    let project = tmp.path().join("testproject");
    fs::create_dir(&project).unwrap();
    (tmp, project)
}

/// Initialize a project with a single editable category for testing.
fn init_test_project() -> (TempDir, PathBuf) {
    let (tmp, project) = project_dir();
    mkrk(&project)
        .args(["init", "--category", "test/**:editable"])
        .assert()
        .success();
    (tmp, project)
}

/// Create a file inside the test category and return its relative path.
fn create_test_file(dir: &Path, name: &str, content: &str) -> String {
    let test_dir = dir.join("test");
    fs::create_dir_all(&test_dir).unwrap();
    fs::write(test_dir.join(name), content).unwrap();
    Path::new("test").join(name).to_string_lossy().into_owned()
}

// --- Binary startup ---

#[test]
fn binary_runs() {
    let mut cmd: Command = cargo_bin_cmd!("mkrk").into();
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("mkrk"));
}

// --- Init ---

#[test]
fn init_creates_project() {
    let (tmp, project) = project_dir();
    mkrk(&project).arg("init").assert().success();
    assert!(project.join(".mkrk").exists());
    assert!(project.join("evidence").exists());
    assert!(project.join("sources").exists());
    assert!(project.join("notes").exists());
    assert!(project.join("tools").exists());
    drop(tmp);
}

#[test]
fn init_no_categories() {
    let (tmp, project) = project_dir();
    mkrk(&project).args(["init", "-n"]).assert().success();
    assert!(project.join(".mkrk").exists());
    assert!(!project.join("evidence").exists());
    assert!(!project.join("sources").exists());
    drop(tmp);
}

#[test]
fn init_refuses_double() {
    let (tmp, project) = project_dir();
    mkrk(&project).args(["init", "-n"]).assert().success();
    mkrk(&project)
        .args(["init", "-n"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
    drop(tmp);
}

// --- Status ---

#[test]
fn status_after_init() {
    let (_tmp, project) = init_test_project();
    mkrk(&project).arg("status").assert().success();
}

// --- Category ---

#[test]
fn category_add_and_list() {
    let (_tmp, project) = init_test_project();
    mkrk(&project)
        .args(["category", "add", "docs", "--pattern", "docs/**"])
        .assert()
        .success();
    mkrk(&project)
        .arg("category")
        .assert()
        .success()
        .stdout(predicate::str::contains("docs"));
}

// --- Ingest + List ---

#[test]
fn ingest_tracks_files() {
    let (_tmp, project) = init_test_project();
    create_test_file(&project, "report.txt", "classified content");

    mkrk(&project).arg("ingest").assert().success();

    mkrk(&project)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("report.txt"));
}

#[test]
fn list_by_category() {
    let (tmp, project) = project_dir();
    mkrk(&project)
        .args([
            "init",
            "--category",
            "alpha/**:editable",
            "--category",
            "beta/**:editable",
        ])
        .assert()
        .success();

    fs::create_dir_all(project.join("alpha")).unwrap();
    fs::create_dir_all(project.join("beta")).unwrap();
    fs::write(project.join("alpha/a.txt"), "alpha").unwrap();
    fs::write(project.join("beta/b.txt"), "beta").unwrap();

    mkrk(&project).arg("ingest").assert().success();

    mkrk(&project)
        .args(["list", ":alpha"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a.txt").and(predicate::str::contains("b.txt").not()));
    drop(tmp);
}

// --- Verify ---

#[test]
fn verify_passes_unmodified() {
    let (_tmp, project) = init_test_project();
    create_test_file(&project, "clean.txt", "untouched");
    mkrk(&project).arg("ingest").assert().success();
    mkrk(&project).arg("verify").assert().success();
}

#[test]
fn verify_fails_modified() {
    let (_tmp, project) = init_test_project();
    let rel = create_test_file(&project, "tampered.txt", "original");
    mkrk(&project).arg("ingest").assert().success();

    fs::write(project.join(&rel), "modified").unwrap();

    mkrk(&project)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("MODIFIED"));
}

#[test]
fn verify_fails_missing() {
    let (_tmp, project) = init_test_project();
    let rel = create_test_file(&project, "vanished.txt", "here today");
    mkrk(&project).arg("ingest").assert().success();

    fs::remove_file(project.join(&rel)).unwrap();

    mkrk(&project)
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("MISSING"));
}

// --- Tags ---

fn assert_file_tags(project: &Path, file: &str, pred: impl predicates::Predicate<str>) {
    mkrk(project)
        .args(["tags", file, "--no-hash-check"])
        .assert()
        .success()
        .stdout(pred);
}

#[test]
fn tag_and_list_tags() {
    let (_tmp, project) = init_test_project();
    let rel = create_test_file(&project, "doc.txt", "tagged content");
    mkrk(&project).arg("ingest").assert().success();

    mkrk(&project)
        .args(["tag", &rel, "important"])
        .assert()
        .success();

    assert_file_tags(&project, &rel, predicate::str::contains("important"));
}

#[test]
fn untag_removes_tag() {
    let (_tmp, project) = init_test_project();
    let rel = create_test_file(&project, "doc.txt", "tagged content");
    mkrk(&project).arg("ingest").assert().success();

    mkrk(&project)
        .args(["tag", &rel, "removeme"])
        .assert()
        .success();

    mkrk(&project)
        .args(["untag", &rel, "removeme"])
        .assert()
        .success();

    assert_file_tags(&project, &rel, predicate::str::contains("removeme").not());
}
