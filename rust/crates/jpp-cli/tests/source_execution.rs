//! End-to-end source execution, not host-language implementations of the examples.
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn run(args: &[&str]) -> Value {
    let result = Command::new(env!("CARGO_BIN_EXE_jpp"))
        .current_dir(root())
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

#[test]
fn composed_methods_execute_without_a_fixture() {
    let report = run(&["run", "examples/composition.jpp"]);
    assert_eq!(report["value"]["result"], 43);
    assert_eq!(report["cost"]["calls"], 0);
}

#[test]
fn adaptive_source_constructs_ten_questions() {
    let report = run(&[
        "run",
        "examples/adaptive.jpp",
        "--fixtures",
        "examples/fixtures/adaptive.json",
    ]);
    let value = &report["value"];
    assert_eq!(value["target"], 731);
    assert_eq!(value["candidates_left"], 1);
    assert_eq!(value["questions"], 10);
    assert_eq!(value["pending"], json!([]));
    assert_eq!(report["cost"]["calls"], 10);
    let evidence = value["evidence"].as_array().unwrap();
    let answers: Vec<_> = evidence.iter().map(|o| o["value"].clone()).collect();
    assert_eq!(
        answers,
        vec![
            json!(false),
            json!(true),
            json!(false),
            json!(false),
            json!(false),
            json!(true),
            json!(false),
            json!(false),
            json!(true),
            json!(false)
        ]
    );
}

#[test]
fn partial_continuations_preserve_prior_evidence_and_checks() {
    let report = run(&[
        "run",
        "examples/partial.jpp",
        "--fixtures",
        "examples/fixtures/partial.json",
    ]);
    let value = &report["value"];
    assert_eq!(
        value["used_before_complete"],
        json!({"members":["A","B"],"cost":9})
    );
    assert_eq!(value["initial"]["pending"], json!(["C", "D"]));
    assert_eq!(value["improved"]["best"], json!({"members":["C"],"cost":2}));
    assert_eq!(value["improved"]["pending"], json!(["D"]));
    assert_eq!(value["final"]["best"], value["improved"]["best"]);
    assert_eq!(value["final"]["pending"], json!([]));
    assert_eq!(value["terminal"], true);
    assert_eq!(report["cost"]["calls"], 6);
    let names: Vec<_> = report["local_checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].clone())
        .collect();
    assert_eq!(names, vec![json!("A"), json!("B"), json!("C")]);
    let initial = value["initial"]["evidence"].as_array().unwrap();
    let improved = value["improved"]["evidence"].as_array().unwrap();
    let final_evidence = value["final"]["evidence"].as_array().unwrap();
    assert_eq!(initial.len(), 4);
    assert_eq!(improved.len(), 5);
    assert_eq!(final_evidence.len(), 6);
    assert_eq!(&improved[..4], initial);
    assert_eq!(&final_evidence[..5], improved);
}

#[test]
fn ledger_replay_reconstructs_source_methods_without_fresh_effects() {
    let ledger =
        std::env::temp_dir().join(format!("jpp-source-replay-{}.json", std::process::id()));
    let ledger_name = ledger.to_str().unwrap();
    let original = run(&[
        "run",
        "examples/partial.jpp",
        "--fixtures",
        "examples/fixtures/partial.json",
        "--ledger-out",
        ledger_name,
    ]);
    let replay = run(&[
        "run",
        "examples/partial.jpp",
        "--fixtures",
        "examples/fixtures/partial.json",
        "--replay",
        ledger_name,
    ]);
    assert_eq!(replay["value"], original["value"]);
    assert_eq!(replay["cost"]["calls"], 0);
    assert_eq!(replay["local_checks"], json!([]));
    std::fs::remove_file(ledger).unwrap();
}

#[test]
fn check_rejects_source_type_errors_before_run() {
    for file in [
        "examples/errors/type-mismatch.jpp",
        "examples/errors/missing-budget.jpp",
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_jpp"))
            .current_dir(root())
            .args(["check", file])
            .output()
            .unwrap();
        assert!(!result.status.success());
        let error = String::from_utf8_lossy(&result.stderr);
        assert!(error.contains(file), "{error}");
        assert!(!error.contains("panicked"), "{error}");
    }
}

#[test]
fn zero_budget_returns_program_pending_without_requesting_an_observation() {
    let source = std::fs::read_to_string(root().join("examples/adaptive.jpp"))
        .unwrap()
        .replacen("calls: 10", "calls: 0", 1);
    let path = std::env::temp_dir().join(format!("jpp-zero-budget-{}.jpp", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let report = run(&[
        "run",
        path.to_str().unwrap(),
        "--fixtures",
        "examples/fixtures/adaptive.json",
    ]);
    assert_eq!(report["status"], "pending");
    assert_eq!(report["cost"]["calls"], 0);
    assert!(!report["pending"].as_array().unwrap().is_empty());
    std::fs::remove_file(path).unwrap();
}
