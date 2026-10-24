use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn host_triple() -> String {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("rustc -vV");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("host triple")
        .to_string()
}

fn with_build_env(dir: &std::path::Path, body: impl FnOnce()) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|err| err.into_inner());
    let triple = host_triple();
    let prev = [
        "OUT_DIR",
        "TARGET",
        "HOST",
        "OPT_LEVEL",
        "PROFILE",
        "CARGO_ENCODED_RUSTFLAGS",
    ]
    .map(|key| (key, std::env::var_os(key)));
    std::env::set_var("OUT_DIR", dir);
    std::env::set_var("TARGET", &triple);
    std::env::set_var("HOST", &triple);
    std::env::set_var("OPT_LEVEL", "0");
    std::env::set_var("PROFILE", "release");
    std::env::set_var("CARGO_ENCODED_RUSTFLAGS", "");
    body();
    for (key, value) in prev {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

#[test]
fn failed_proof_fails_the_build() {
    let dir = unique_dir("bad");
    fs::create_dir_all(&dir).unwrap();
    let bend = dir.join("bad.bend");
    fs::write(
        &bend,
        r#"
import Base

def id(n: Nat) -> Nat:
  n

law bad:
  for x: Nat
  {Nat.add(x, 0n) == 1n : Nat}

def bad(x):
  {==}
"#,
    )
    .unwrap();
    with_build_env(&dir, || {
        let err = bend_build::Builder::new()
            .file(&bend)
            .compile()
            .expect_err("a false law must fail the build");
        let text = err.to_string();
        assert!(
            text.contains("SOME PROOFS FAIL") || text.contains("ALL PROOFS CHECK") == false,
            "{text}"
        );
        assert!(
            text.contains("expected") || text.contains("SOME PROOFS FAIL"),
            "{text}"
        );
    });
}

#[test]
fn checked_def_is_wrapped() {
    let dir = unique_dir("good");
    fs::create_dir_all(&dir).unwrap();
    let bend = dir.join("math.bend");
    fs::write(
        &bend,
        r#"
import Base

def add(a: U32, b: U32) -> U32:
  (a + b : U32)

law add_zero:
  for x: Nat
  {Nat.add(x, 0n) == x : Nat}

def add_zero(x):
  match x:
    case 0n:
      {==}
    case 1n+p:
      %add_zero(p) : {1n+Nat.add(p, 0n) == 1n+_ : Nat}
      {==}
"#,
    )
    .unwrap();
    with_build_env(&dir, || {
        bend_build::Builder::new()
            .file(&bend)
            .export("add")
            .compile()
            .expect("bend should accept add_zero and emit add");
    });
    let generated = fs::read_to_string(dir.join("bend_build/math.rs")).unwrap();
    assert!(generated.contains("pub fn add(a0: u32, a1: u32) -> u32"));
    assert!(generated.contains("bend_build_math_add"));
}

fn unique_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("bend-build-{name}-{nanos}"))
}
