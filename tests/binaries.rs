//! The still-frame binaries, which are how patterns get reviewed.

use std::path::PathBuf;
use std::process::Command;

fn temp_directory(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("psychedelic-bin-{name}"));
    std::fs::create_dir_all(&path).expect("temp directory");
    path
}

#[test]
fn the_evaluator_binary_reports_the_files_it_wrote() {
    let directory = temp_directory("stills");
    let output = Command::new(env!("CARGO_BIN_EXE_stills"))
        .args(["5", "2", "32", directory.to_str().expect("path")])
        .output()
        .expect("run stills");
    assert!(output.status.success());
    let listed = String::from_utf8(output.stdout).expect("utf8");
    assert_eq!(listed.lines().count(), 2);
    assert!(listed.contains("still-001.png"));
}

#[test]
fn the_shader_binary_reports_the_files_it_wrote() {
    let directory = temp_directory("gpu-stills");
    let output = Command::new(env!("CARGO_BIN_EXE_gpu-stills"))
        .args(["5", "1", "32", directory.to_str().expect("path")])
        .output()
        .expect("run gpu-stills");
    assert!(output.status.success());
    let listed = String::from_utf8(output.stdout).expect("utf8");
    assert!(listed.contains("gpu-000.png"));
}

#[test]
fn the_shader_binary_prints_a_shader_for_a_seed() {
    let output = Command::new(env!("CARGO_BIN_EXE_shader"))
        .args(["7", "2.5"])
        .output()
        .expect("run shader");
    assert!(output.status.success());
    let source = String::from_utf8(output.stdout).expect("utf8");
    assert!(source.contains("fragment float4 psy_fragment"));
    assert!(source.contains("moverDistance0"));
}

#[test]
fn an_argument_that_is_not_a_number_fails_the_run() {
    let output = Command::new(env!("CARGO_BIN_EXE_stills"))
        .args(["wat"])
        .output()
        .expect("run stills");
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .expect("utf8")
            .contains("seed must be a number")
    );
}

#[test]
fn a_directory_that_does_not_exist_fails_the_run() {
    let output = Command::new(env!("CARGO_BIN_EXE_gpu-stills"))
        .args(["1", "1", "16", "/nonexistent-psychedelic-directory"])
        .output()
        .expect("run gpu-stills");
    assert!(!output.status.success());
}
