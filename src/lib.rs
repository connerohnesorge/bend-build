//! Compile [Bend](https://github.com/bendlang/bend) into a Rust program from `build.rs`.
//!
//! ```ignore
//! // build.rs
//! fn main() -> Result<(), bend_build::Error> {
//!     bend_build::compile(&["src/math.bend"])?;
//!     Ok(())
//! }
//! ```
//!
//! ```ignore
//! // src/main.rs
//! include!(concat!(env!("OUT_DIR"), "/bend_build/math.rs"));
//!
//! fn main() {
//!     assert_eq!(math::add(20, 22), 42);
//! }
//! ```
//!
//! The Bend file is type-checked, including its `law`s, before any C is
//! emitted. A failed proof fails the Cargo build. See the crate README for
//! the calling convention and for what [issue 813](https://github.com/bendlang/bend/issues/813)
//! will replace.

mod emit;
mod parse;

use parse::{scan, select, Signature};
use std::fmt::Write;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Failure while reading Bend, running `bend` / `clang`, or generating bindings.
#[derive(Debug)]
pub struct Error {
    message: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::new(err.to_string())
    }
}

/// One Bend file and the defs to export from it.
///
/// An empty export list means "every monomorphic `U32` / `F32` / `Nat` def
/// except `main`".
#[derive(Clone, Debug)]
struct Input {
    path: PathBuf,
    exports: Vec<String>,
    explicit: bool,
}

/// Configuration for a `build.rs` invocation.
#[derive(Clone, Debug)]
pub struct Builder {
    inputs: Vec<Input>,
    bend: Option<PathBuf>,
    clang: Option<PathBuf>,
    verdict: bool,
    threads: u32,
    early_error: Option<String>,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            bend: None,
            clang: None,
            verdict: false,
            threads: 1,
            early_error: None,
        }
    }
}

impl Builder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a Bend file. Later [`Self::export`] calls attach to this file
    /// until the next `file`.
    pub fn file(mut self, path: impl AsRef<Path>) -> Self {
        self.inputs.push(Input {
            path: path.as_ref().to_path_buf(),
            exports: Vec::new(),
            explicit: false,
        });
        self
    }

    /// Export `name` from the file most recently added with [`Self::file`].
    ///
    /// The first `export` for a file switches that file from automatic
    /// selection to an explicit list.
    pub fn export(mut self, name: impl Into<String>) -> Self {
        let name = name.into();
        match self.inputs.last_mut() {
            Some(input) => {
                input.explicit = true;
                input.exports.push(name);
            }
            None => {
                self.early_error = Some(format!(
                    "bend_build::Builder::export(\"{name}\") needs a .file() first"
                ));
            }
        }
        self
    }

    /// Use this `bend` binary instead of `$BEND` or `bend` on `PATH`.
    pub fn bend_bin(mut self, path: impl AsRef<Path>) -> Self {
        self.bend = Some(path.as_ref().to_path_buf());
        self
    }

    /// Use this clang instead of `$BEND_CC`, a clang `$CC`, or `clang` on `PATH`.
    ///
    /// The Bend C backend needs clang 14 or newer. gcc cannot compile it.
    pub fn clang(mut self, path: impl AsRef<Path>) -> Self {
        self.clang = Some(path.as_ref().to_path_buf());
        self
    }

    /// After the ordinary check, re-check the Bend file with `bend --verdict`.
    ///
    /// That path needs Lean v4.34.0, or `$BENDTT` pointing at a kernel Bend
    /// already built. It checks the Bend source, not the generated harness:
    /// the harness is a calling convention, and its host defs are not proofs.
    pub fn verdict(mut self, on: bool) -> Self {
        self.verdict = on;
        self
    }

    /// CPU threads for the Bend runtime inside each loaded module.
    ///
    /// `1` (the default) runs fork/join defs on the sequential path, which is
    /// the path the examples check. A larger count lets a parallel `let`
    /// (`a b = f() g()`) use more cores. `!` GPU calls are not enabled.
    pub fn threads(mut self, threads: u32) -> Self {
        self.threads = threads.max(1);
        self
    }

    /// Check the Bend files, compile them, and write Rust bindings under
    /// `$OUT_DIR/bend_build/`.
    pub fn compile(self) -> Result<(), Error> {
        if let Some(err) = self.early_error {
            return Err(Error::new(err));
        }
        if self.inputs.is_empty() {
            return Err(Error::new(
                "bend_build::compile needs at least one .bend file",
            ));
        }
        let out_dir = std::env::var_os("OUT_DIR").ok_or_else(|| {
            Error::new("OUT_DIR is not set; call bend_build from a Cargo build.rs")
        })?;
        let out_dir = PathBuf::from(out_dir).join("bend_build");
        fs::create_dir_all(&out_dir)?;
        println!("cargo:rerun-if-env-changed=BEND");
        println!("cargo:rerun-if-env-changed=BEND_CC");
        println!("cargo:rerun-if-env-changed=BENDTT");

        let bend = find_bend(self.bend.as_deref())?;
        println!("cargo:rerun-if-changed={}", bend.display());
        let clang = find_clang(self.clang.as_deref())?;
        let mut stems = Vec::new();
        for input in &self.inputs {
            let stem = file_stem(&input.path)?;
            let module = emit::module_name(&stem);
            if stems.iter().any(|prev| prev == &module) {
                return Err(Error::new(format!(
                    "two Bend files sanitize to the same module `{module}`"
                )));
            }
            stems.push(module.clone());
            compile_one(
                input,
                &module,
                &bend,
                &clang,
                &out_dir,
                self.verdict,
                self.threads,
            )?;
        }
        println!("cargo:rustc-link-lib=pthread");
        println!("cargo:rustc-link-lib=m");
        Ok(())
    }
}

/// Check and compile each file, exporting every supported numeric def.
pub fn compile<P: AsRef<Path>>(files: &[P]) -> Result<(), Error> {
    let mut builder = Builder::new();
    for file in files {
        builder = builder.file(file);
    }
    builder.compile()
}

fn compile_one(
    input: &Input,
    module: &str,
    bend: &Path,
    clang: &Path,
    out_dir: &Path,
    verdict: bool,
    threads: u32,
) -> Result<(), Error> {
    let source = input
        .path
        .canonicalize()
        .map_err(|err| Error::new(format!("cannot read {}: {err}", input.path.display())))?;
    if source.extension().and_then(|ext| ext.to_str()) != Some("bend") {
        return Err(Error::new(format!(
            "{} is not a .bend file",
            source.display()
        )));
    }
    println!("cargo:rerun-if-changed={}", source.display());
    rerun_imports(&source, &mut Vec::new())?;

    let text = fs::read_to_string(&source)?;
    let items = scan(&text);
    let explicit = input.explicit.then_some(input.exports.as_slice());
    let selected = select(&items, explicit)
        .map_err(|err| Error::new(format!("{}: {err}", source.display())))?;

    check_proofs(bend, &source, false)?;
    if verdict {
        check_proofs(bend, &source, true)?;
    }

    let work = out_dir.join(module);
    fs::create_dir_all(&work)?;
    let import_path = relative_import(&work, &source)?;
    let harness = emit::harness(&import_path, &selected);
    let harness_path = work.join("harness.bend");
    fs::write(&harness_path, harness)?;
    let c_path = work.join("bend.c");
    run(
        bend,
        &[harness_path.as_os_str(), "-o".as_ref(), c_path.as_os_str()],
        &format!("bend failed while compiling {}", source.display()),
    )?;
    let generated = fs::read_to_string(&c_path)?;
    let refs: Vec<&Signature> = selected;
    let shimmed = emit::shim(&generated, &refs, module, threads).map_err(Error::new)?;
    let lib_c = work.join("lib.c");
    fs::write(&lib_c, shimmed)?;

    cc::Build::new()
        .compiler(clang)
        .file(&lib_c)
        .opt_level(2)
        .flag("-std=c11")
        .flag("-pthread")
        .flag("-Wno-unused-function")
        .warnings(false)
        .compile(&format!("bend_build_{module}"));

    let rust = emit::rust_module(module, &source.display().to_string(), &refs);
    let rust_path = out_dir.join(format!("{module}.rs"));
    fs::write(&rust_path, rust)?;
    eprintln!(
        "bend-build: linked {} ({})",
        source.display(),
        refs.iter()
            .map(|sig| sig.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

fn check_proofs(bend: &Path, source: &Path, verdict: bool) -> Result<(), Error> {
    let mut args: Vec<&std::ffi::OsStr> = vec![source.as_os_str()];
    if verdict {
        args.push("--verdict".as_ref());
    } else {
        args.push("--check-only".as_ref());
    }
    let output = bend_command(bend)
        .args(&args)
        .output()
        .map_err(|err| Error::new(format!("failed to run {}: {err}", bend.display())))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    if !output.status.success() || !combined.contains("ALL PROOFS CHECK") {
        let mode = if verdict { "--verdict" } else { "--check-only" };
        return Err(Error::new(format!(
            "bend {mode} {} did not report ALL PROOFS CHECK (exit {:?})\n{combined}",
            source.display(),
            output.status.code()
        )));
    }
    let mode = if verdict { "--verdict" } else { "--check-only" };
    eprintln!("bend-build: {mode} {} — ALL PROOFS CHECK", source.display());
    Ok(())
}

fn rerun_imports(file: &Path, seen: &mut Vec<PathBuf>) -> Result<(), Error> {
    if seen.iter().any(|prev| prev == file) {
        return Ok(());
    }
    seen.push(file.to_path_buf());
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(_) => return Ok(()),
    };
    let dir = file.parent().unwrap_or_else(|| Path::new("."));
    for line in text.lines() {
        let code = line.split('#').next().unwrap_or("").trim();
        let Some(rest) = code.strip_prefix("import ") else {
            continue;
        };
        let path = rest.split_whitespace().next().unwrap_or("");
        if !path.ends_with(".bend") {
            continue;
        }
        let next = dir.join(path);
        println!("cargo:rerun-if-changed={}", next.display());
        if next.is_file() {
            rerun_imports(&next, seen)?;
        }
    }
    Ok(())
}

fn relative_import(from_dir: &Path, target: &Path) -> Result<String, Error> {
    let from = from_dir.canonicalize()?;
    let target = target.canonicalize()?;
    let mut from_parts: Vec<_> = from.components().collect();
    let to_parts: Vec<_> = target.components().collect();
    let mut shared = 0;
    while shared < from_parts.len()
        && shared < to_parts.len()
        && from_parts[shared] == to_parts[shared]
    {
        shared += 1;
    }
    from_parts.truncate(shared);
    let mut parts = Vec::new();
    for _ in shared..from.components().count() {
        parts.push("..".to_string());
    }
    for (index, component) in to_parts.iter().skip(shared).enumerate() {
        let text = component.as_os_str().to_string_lossy();
        let last = index + 1 == to_parts.len() - shared;
        let ok = if last {
            is_bend_file_name(&text)
        } else {
            is_bend_component(&text)
        };
        if !ok {
            return Err(Error::new(format!(
                "Bend module path `{text}` in {} is not a plain name (letters, digits, `_`, `-`)",
                target.display()
            )));
        }
        parts.push(text.into_owned());
    }
    if parts.is_empty() {
        return Err(Error::new(
            "cannot import a Bend file from its own directory as the harness",
        ));
    }
    Ok(parts.join("/"))
}

fn is_bend_component(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_bend_file_name(text: &str) -> bool {
    text.strip_suffix(".bend").is_some_and(is_bend_component)
}

fn file_stem(path: &Path) -> Result<String, Error> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| Error::new(format!("bad Bend path {}", path.display())))?;
    if !is_bend_component(stem) {
        return Err(Error::new(format!(
            "Bend file stem `{stem}` must be letters, digits, `_`, or `-`"
        )));
    }
    Ok(stem.to_string())
}

fn find_bend(explicit: Option<&Path>) -> Result<PathBuf, Error> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = std::env::var_os("BEND") {
        return Ok(PathBuf::from(path));
    }
    which("bend").ok_or_else(|| {
        Error::new(
            "cannot find `bend`. Install it (`curl -fsSL https://bend-lang.com/install.sh | sh`) \
             and put it on PATH, or set $BEND",
        )
    })
}

fn find_clang(explicit: Option<&Path>) -> Result<PathBuf, Error> {
    let candidates: Vec<PathBuf> = if let Some(path) = explicit {
        vec![path.to_path_buf()]
    } else if let Some(path) = std::env::var_os("BEND_CC") {
        vec![PathBuf::from(path)]
    } else {
        let mut found = Vec::new();
        if let Some(cc) = std::env::var_os("CC") {
            found.push(PathBuf::from(cc));
        }
        if let Some(path) = which("clang") {
            found.push(path);
        }
        found
    };
    let mut tried = Vec::new();
    for candidate in candidates {
        match clang_major(&candidate) {
            Ok(major) if major >= 14 => return Ok(candidate),
            Ok(major) => tried.push(format!("{} is clang {major}", candidate.display())),
            Err(err) => tried.push(format!("{} ({err})", candidate.display())),
        }
    }
    Err(Error::new(format!(
        "Bend's C backend needs clang 14 or newer. Tried: {}",
        if tried.is_empty() {
            "nothing on PATH".to_string()
        } else {
            tried.join("; ")
        }
    )))
}

fn clang_major(path: &Path) -> Result<u32, String> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|err| err.to_string())?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let marker = "clang version ";
    let Some(at) = text.find(marker) else {
        return Err("not clang".to_string());
    };
    let digits: String = text[at + marker.len()..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits
        .parse()
        .map_err(|_| format!("cannot parse clang version from {text}"))
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}

fn bend_command(bend: &Path) -> Command {
    let mut cmd = Command::new(bend);
    if std::env::var_os("BEND_NO_TELEMETRY").is_none() {
        cmd.env("BEND_NO_TELEMETRY", "1");
    }
    cmd
}

fn run(bin: &Path, args: &[&std::ffi::OsStr], context: &str) -> Result<(), Error> {
    let output = bend_command(bin)
        .args(args)
        .output()
        .map_err(|err| Error::new(format!("{context}: failed to run {}: {err}", bin.display())))?;
    if output.status.success() {
        return Ok(());
    }
    let mut message = format!(
        "{context}: `{} {}` exited {:?}\n",
        bin.display(),
        args.iter()
            .map(|arg| arg.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" "),
        output.status.code()
    );
    let _ = write!(message, "{}", String::from_utf8_lossy(&output.stdout));
    let _ = write!(message, "{}", String::from_utf8_lossy(&output.stderr));
    Err(Error::new(message))
}
