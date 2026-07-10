use std::{fs, process::Command};

use assert_cmd::cargo::cargo_bin;
use tempfile::tempdir;

#[test]
fn memory_command_honors_root_argument() {
    let target = tempdir().unwrap();
    let other = tempdir().unwrap();
    let binary = cargo_bin("research-harness");

    let output = Command::new(binary)
        .arg("--root")
        .arg(target.path())
        .args(["memory", "add-business", "目标：测试 root"])
        .current_dir(other.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read_to_string(target.path().join(".research-harness/memory/business.md"))
            .unwrap()
            .contains("目标：测试 root")
    );
    assert!(!other.path().join(".research-harness").exists());
}
