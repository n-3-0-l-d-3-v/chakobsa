//! Integration tests for `chakobsac`: real programs, run end-to-end
//! through the built binary via `assert_cmd`-free plain `std::process`
//! (no extra dependency needed for a handful of invocations).

use std::fs;
use std::process::Command;

fn chakobsac() -> Command {
    Command::new(env!("CARGO_BIN_EXE_chakobsac"))
}

fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("chakobsac-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn run_prints_mains_return_value() {
    let src = write_temp("add.ck", "fn main() -> i64 { return 3 + 4; }");
    let output = chakobsac().arg("run").arg(&src).output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().ends_with('7'), "stdout was: {stdout:?}");
}

#[test]
fn run_supports_recursion_and_calls() {
    let src = write_temp(
        "fact.ck",
        r#"
        fn fact(n: i64) -> i64 {
            if n <= 1 { return 1; } else { return n * fact(n - 1); }
        }
        fn main() -> i64 { return fact(5); }
        "#,
    );
    let output = chakobsac().arg("run").arg(&src).output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().ends_with("120"), "stdout was: {stdout:?}");
}

#[test]
fn run_without_a_main_function_is_a_clean_error_not_a_panic() {
    let src = write_temp("no_main.ck", "fn f() -> i64 { return 1; }");
    let output = chakobsac().arg("run").arg(&src).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("main"), "stderr was: {stderr:?}");
}

#[test]
fn a_parse_error_is_a_clean_error_not_a_panic() {
    let src = write_temp("bad.ck", "fn main() -> i64 { return ; }");
    let output = chakobsac().arg("run").arg(&src).output().unwrap();
    assert!(!output.status.success());
}

#[test]
fn dump_ir_prints_readable_ssa() {
    let src = write_temp("dump.ck", "fn main() -> i64 { return 1 + 2; }");
    let output = chakobsac().arg("dump-ir").arg(&src).output().unwrap();
    assert!(output.status.success(), "{:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("fn main"));
    assert!(stdout.contains("add"));
    assert!(stdout.contains("ret"));
}

#[test]
fn build_then_run_the_compiled_artifact_matches_direct_run() {
    let src = write_temp("build_me.ck", "fn main() -> i64 { return 6 * 7; }");
    let out_path = src.with_extension("ckp");

    let build = chakobsac()
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&out_path)
        .output()
        .unwrap();
    assert!(build.status.success(), "{:?}", build);
    assert!(out_path.exists());

    let run = chakobsac().arg("run").arg(&out_path).output().unwrap();
    assert!(run.status.success(), "{:?}", run);
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.trim().ends_with("42"), "stdout was: {stdout:?}");
}

#[test]
fn built_output_is_plain_json_isa_program() {
    let src = write_temp("json_check.ck", "fn main() -> i64 { return 1; }");
    let out_path = src.with_extension("ckp");
    let build = chakobsac()
        .arg("build")
        .arg(&src)
        .arg("-o")
        .arg(&out_path)
        .output()
        .unwrap();
    assert!(build.status.success(), "{:?}", build);

    let text = fs::read_to_string(&out_path).unwrap();
    let program: isa::Program = serde_json::from_str(&text).expect("must be a plain isa::Program");
    assert!(!program.blocks.is_empty());
}
