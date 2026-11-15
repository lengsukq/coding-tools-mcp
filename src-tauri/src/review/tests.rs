use std::fs;
use std::path::Path;

use super::diff::{sensitive_path, snapshot_walk_count, unified_patch};
use super::{
    attach_session, create_patch_review, create_session_review, create_snapshot_review,
    create_workspace_review, direct_git_repositories, issue_token, load_review,
    snapshot_text_files,
};

#[test]
fn snapshot_prunes_ignored_and_sensitive_trees_without_losing_changes() {
    let temp = tempfile::tempdir().expect("temp workspace");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("node_modules/package/deep")).unwrap();
    fs::create_dir_all(root.join("target/debug")).unwrap();
    fs::create_dir_all(root.join("credentials/cache")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("node_modules/package/deep/a.txt"), "ignore").unwrap();
    fs::write(root.join("target/debug/a.txt"), "ignore").unwrap();
    fs::write(root.join("credentials/cache/token.txt"), "ignore").unwrap();
    for i in 0..256 {
        fs::write(
            root.join(format!("node_modules/package/deep/{i}.js")),
            "ignored",
        )
        .unwrap();
    }

    let old_walk_count = snapshot_walk_count(root, false);
    let pruned_walk_count = snapshot_walk_count(root, true);
    eprintln!("review snapshot fixture entries: old={old_walk_count} pruned={pruned_walk_count}");
    assert!(old_walk_count > pruned_walk_count + 256);
    let before = snapshot_text_files(root);
    assert_eq!(before.len(), 1);
    assert_eq!(before["src/main.rs"], "fn main() {}\n");

    fs::write(
        root.join("src/main.rs"),
        "fn main() { println!(\"hi\"); }\n",
    )
    .unwrap();
    let after = snapshot_text_files(root);
    let link = create_snapshot_review(root, "snapshot-prune-test", None, "test", &before, &after)
        .unwrap()
        .expect("changed source must generate review");
    let review = load_review(root, &link.change_id, &link.token).unwrap();
    assert_eq!(review.stats.files, 1);
}

#[test]
fn sensitive_files_are_detected() {
    assert!(sensitive_path(".env"));
    assert!(sensitive_path("config/client.key"));
    assert!(!sensitive_path("src/main.rs"));
}

#[test]
fn review_id_rejects_traversal() {
    let dir = tempfile::tempdir().unwrap();
    assert!(load_review(dir.path(), "../secret", "x").is_err());
}

#[test]
fn patch_contains_only_changed_hunks_with_context() {
    let before = "a\nb\nc\nd\ne\nf\ng\nh\ni\n";
    let after = "a\nb\nc\nd\nE\nf\ng\nh\ni\n";
    let patch = unified_patch("x.txt", before, after, false);
    assert!(patch.contains("-e\n+E\n"));
    assert!(!patch.contains("-a\n"));
}

#[test]
fn reissuing_token_does_not_invalidate_existing_review_url() {
    let dir = tempfile::tempdir().unwrap();
    let original = create_patch_review(
        dir.path(),
        "change-1",
        None,
        "test",
        &[(
            "src/a.txt".into(),
            "update".into(),
            "before\n".into(),
            "after\n".into(),
        )],
    )
    .unwrap();
    assert!(load_review(dir.path(), "change-1", &original.token).is_ok());

    let second = issue_token(dir.path(), "change-1").unwrap();
    assert!(load_review(dir.path(), "change-1", &original.token).is_ok());
    assert!(load_review(dir.path(), "change-1", &second.token).is_ok());
}

#[test]
fn workspace_review_freezes_tracked_and_untracked_changes_vs_head() {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init"]);
    git(&["config", "user.email", "review@example.test"]);
    git(&["config", "user.name", "Review Test"]);
    fs::write(dir.path().join("tracked.txt"), "before\n").unwrap();
    git(&["add", "tracked.txt"]);
    git(&["commit", "-m", "baseline"]);
    fs::write(dir.path().join("tracked.txt"), "after\n").unwrap();
    fs::write(dir.path().join("new.txt"), "new\n").unwrap();
    fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
    fs::write(
        dir.path().join("node_modules/pkg/generated.js"),
        "ignored\n",
    )
    .unwrap();

    let link = create_workspace_review(dir.path(), "workspace")
        .unwrap()
        .unwrap();
    let review = load_review(dir.path(), &link.change_id, &link.token).unwrap();
    assert_eq!(review.scope, "workspace");
    assert_eq!(review.stats.files, 2);
    assert!(review.files.iter().any(|file| file.path == "tracked.txt"
        && file.patch.contains("-before")
        && file.patch.contains("+after")));
    assert!(review
        .files
        .iter()
        .any(|file| file.path == "new.txt" && file.status == "add"));
    assert!(!review
        .files
        .iter()
        .any(|file| file.path.contains("node_modules")));
}

#[test]
fn session_review_aggregates_operation_reviews_by_file() {
    let dir = tempfile::tempdir().unwrap();
    let first = create_patch_review(
        dir.path(),
        "one",
        Some("op-1"),
        "one",
        &[(
            "src/a.txt".into(),
            "update".into(),
            "a\n".into(),
            "b\n".into(),
        )],
    )
    .unwrap();
    attach_session(dir.path(), &first.change_id, "chat-1").unwrap();
    let second = create_patch_review(
        dir.path(),
        "two",
        Some("op-2"),
        "two",
        &[(
            "src/a.txt".into(),
            "update".into(),
            "b\n".into(),
            "c\n".into(),
        )],
    )
    .unwrap();
    attach_session(dir.path(), &second.change_id, "chat-1").unwrap();

    let aggregate = create_session_review(dir.path(), "chat-1", "session")
        .unwrap()
        .unwrap();
    let review = load_review(dir.path(), &aggregate.change_id, &aggregate.token).unwrap();
    assert_eq!(review.scope, "session");
    assert_eq!(review.session_id.as_deref(), Some("chat-1"));
    assert_eq!(review.stats.files, 1);
    assert_eq!(review.operation_ids.len(), 2);
    assert!(review.files[0].patch.contains("-a"));
    assert!(review.files[0].patch.contains("+c"));
}

#[test]
fn workspace_review_detects_first_level_git_repositories() {
    let dir = tempfile::tempdir().unwrap();
    let child = dir.path().join("service-a");
    fs::create_dir_all(&child).unwrap();
    for i in 0..20 {
        fs::create_dir(dir.path().join(format!("ordinary-{i}"))).unwrap();
    }
    let git = |cwd: &Path, args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&child, &["init"]);
    git(&child, &["config", "user.email", "review@example.test"]);
    git(&child, &["config", "user.name", "Review Test"]);
    fs::write(child.join("index.txt"), "before\n").unwrap();
    git(&child, &["add", "index.txt"]);
    git(&child, &["commit", "-m", "baseline"]);
    fs::write(child.join("index.txt"), "after\n").unwrap();

    let repositories = direct_git_repositories(dir.path());
    assert_eq!(repositories.len(), 1);
    assert_eq!(repositories[0].0, "service-a");

    let link = create_workspace_review(dir.path(), "workspace")
        .unwrap()
        .unwrap();
    let review = load_review(dir.path(), &link.change_id, &link.token).unwrap();
    assert_eq!(review.stats.files, 1);
    assert_eq!(review.files[0].path, "service-a/index.txt");
    assert!(review.files[0].patch.contains("-before"));
    assert!(review.files[0].patch.contains("+after"));
}
