//! A small reader for the Bend definitions this crate can bind.
//!
//! Bend's checker is the authority on whether a file is well typed. This
//! scanner only decides which top-level `def`s have a monomorphic runtime
//! signature in `U32`, `F32`, and `Nat`, which is the slice the C calling
//! convention below can pass without inventing a value ABI.

/// A Bend numeric type with a one-word native representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumTy {
    U32,
    F32,
    Nat,
}

impl NumTy {
    pub fn parse(src: &str) -> Option<Self> {
        match src.trim() {
            "U32" => Some(Self::U32),
            "F32" => Some(Self::F32),
            "Nat" => Some(Self::Nat),
            _ => None,
        }
    }

    pub fn bend_name(self) -> &'static str {
        match self {
            Self::U32 => "U32",
            Self::F32 => "F32",
            Self::Nat => "Nat",
        }
    }

    pub fn rust_name(self) -> &'static str {
        match self {
            Self::U32 => "u32",
            Self::F32 => "f32",
            Self::Nat => "u64",
        }
    }

    /// Two distinct literals, so a harness can mention a def twice.
    pub fn samples(self) -> (&'static str, &'static str) {
        match self {
            Self::U32 => ("0", "1"),
            Self::F32 => ("0.0", "1.0"),
            Self::Nat => ("0n", "1n"),
        }
    }

    /// Expression of type `Nat` that uses `var`, so the harness result stays live.
    pub fn as_nat(self, var: &str) -> String {
        match self {
            Self::U32 => format!("U32.to_nat({var})"),
            Self::F32 => format!("U32.to_nat(F32.to_u32({var}))"),
            Self::Nat => var.to_string(),
        }
    }
}

/// A `def` whose every runtime parameter and whose result are [`NumTy`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub name: String,
    pub params: Vec<NumTy>,
    pub ret: NumTy,
}

/// One top-level `def`, either bindable or deliberately left out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Numeric(Signature),
    Skipped { name: String, reason: String },
}

impl Item {
    pub fn name(&self) -> &str {
        match self {
            Self::Numeric(sig) => &sig.name,
            Self::Skipped { name, .. } => name,
        }
    }
}

/// Scan top-level definitions. Proofs, `main`, and richer types come back as
/// [`Item::Skipped`] so a later `.export("name")` can explain why.
pub fn scan(src: &str) -> Vec<Item> {
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    let mut items = Vec::new();
    while i < lines.len() {
        let code = strip_comment(lines[i]);
        if !is_top_def(code) {
            i += 1;
            continue;
        }
        let (header, next) = take_header(&lines, i);
        i = next;
        if let Some(item) = parse_header(&header) {
            items.push(item);
        }
    }
    items
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(at) => &line[..at],
        None => line,
    }
}

fn is_top_def(code: &str) -> bool {
    let trimmed = code.trim_end();
    !trimmed.is_empty()
        && !code.starts_with([' ', '\t'])
        && (trimmed.starts_with("def ") || trimmed == "def")
}

/// Read a signature that may wrap, stopping at the `:` that closes it.
fn take_header(lines: &[&str], start: usize) -> (String, usize) {
    let mut buf = String::new();
    let mut depth = 0i32;
    let mut i = start;
    while i < lines.len() {
        let code = strip_comment(lines[i]);
        if i != start {
            let body = code.trim();
            if body.is_empty() {
                i += 1;
                continue;
            }
            let fresh_top = !code.starts_with([' ', '\t']);
            if fresh_top && depth == 0 {
                break;
            }
        }
        let mut closed = false;
        for c in code.chars() {
            buf.push(c);
            match c {
                '(' | '{' | '[' => depth += 1,
                ')' | '}' | ']' => depth -= 1,
                ':' if depth == 0 => {
                    closed = true;
                    break;
                }
                _ => {}
            }
        }
        i += 1;
        if closed {
            break;
        }
        buf.push(' ');
    }
    (buf, i)
}

fn parse_header(header: &str) -> Option<Item> {
    let compact = header.split_whitespace().collect::<Vec<_>>().join(" ");
    let rest = compact.strip_prefix("def ")?;
    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(rest.len());
    let name = &rest[..name_len];
    if name.is_empty() || !is_bend_name(name) {
        return None;
    }
    let after = rest[name_len..].trim_start();
    let Some(after) = after.strip_prefix('(') else {
        return Some(skipped(name, "it has no parameter list"));
    };
    let Some(end_paren) = matching_paren(after) else {
        return Some(skipped(name, "its parameter list is not closed"));
    };
    let params_src = &after[..end_paren];
    let tail = after[end_paren + 1..].trim_start();
    let Some(tail) = tail.strip_prefix("->") else {
        return Some(skipped(
            name,
            "it has no `->` result (law proofs are not callable exports)",
        ));
    };
    let mut params = Vec::new();
    if !params_src.trim().is_empty() {
        for raw in split_args(params_src) {
            match parse_param(raw) {
                Ok(ty) => params.push(ty),
                Err(reason) => return Some(skipped(name, reason)),
            }
        }
    }
    let ret_src = tail.trim().trim_end_matches(':').trim();
    let Some(ret) = NumTy::parse(ret_src) else {
        return Some(skipped(
            name,
            format!("its result `{ret_src}` is not U32, F32, or Nat"),
        ));
    };
    Some(Item::Numeric(Signature {
        name: name.to_string(),
        params,
        ret,
    }))
}

fn skipped(name: &str, reason: impl Into<String>) -> Item {
    Item::Skipped {
        name: name.to_string(),
        reason: reason.into(),
    }
}

fn is_bend_name(name: &str) -> bool {
    let mut parts = name.split('.');
    parts.all(|part| {
        let mut chars = part.chars();
        match chars.next() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
            _ => return false,
        }
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    }) && !name.is_empty()
}

fn matching_paren(src: &str) -> Option<usize> {
    let mut depth = 1i32;
    for (i, c) in src.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_args(src: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, c) in src.char_indices() {
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(src[start..i].trim());
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    let last = src[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

fn parse_param(raw: &str) -> Result<NumTy, String> {
    let (mode, rest) = match raw.chars().next() {
        Some(c @ ('+' | '-' | '~')) => (c, raw[c.len_utf8()..].trim_start()),
        _ => (' ', raw),
    };
    if mode == '-' || mode == '~' {
        return Err(format!(
            "parameter `{raw}` is erased or a template; exports take runtime U32, F32, and Nat only"
        ));
    }
    let Some((binder, ty)) = rest.split_once(':') else {
        return Err(format!("parameter `{raw}` has no type"));
    };
    if binder.trim().is_empty() {
        return Err(format!("parameter `{raw}` has no name"));
    }
    NumTy::parse(ty)
        .ok_or_else(|| format!("parameter `{raw}` is not a monomorphic U32, F32, or Nat"))
}

/// Pick the defs a build should wrap.
///
/// `explicit == None` exports every numeric def except `main`. An explicit
/// list exports those names, in that order, and refuses anything else.
pub fn select<'a>(
    items: &'a [Item],
    explicit: Option<&[String]>,
) -> Result<Vec<&'a Signature>, String> {
    if let Some(names) = explicit {
        let mut chosen = Vec::with_capacity(names.len());
        for name in names {
            let item = items
                .iter()
                .find(|item| item.name() == name)
                .ok_or_else(|| {
                    let have: Vec<_> = items
                        .iter()
                        .filter_map(|item| match item {
                            Item::Numeric(sig) if sig.name != "main" => Some(sig.name.as_str()),
                            _ => None,
                        })
                        .collect();
                    format!(
                        "no top-level def `{name}` to export (numeric defs here: {})",
                        if have.is_empty() {
                            "none".to_string()
                        } else {
                            have.join(", ")
                        }
                    )
                })?;
            match item {
                Item::Numeric(sig) if sig.name == "main" => {
                    return Err(
                        "`main` is the Bend program entry, not a library export".to_string()
                    );
                }
                Item::Numeric(sig) => chosen.push(sig),
                Item::Skipped { reason, .. } => {
                    return Err(format!("cannot export `{name}`: {reason}"));
                }
            }
        }
        if chosen.is_empty() {
            return Err("export list is empty".to_string());
        }
        return Ok(chosen);
    }

    let chosen: Vec<_> = items
        .iter()
        .filter_map(|item| match item {
            Item::Numeric(sig) if sig.name != "main" => Some(sig),
            _ => None,
        })
        .collect();
    if chosen.is_empty() {
        let skipped: Vec<_> = items
            .iter()
            .filter_map(|item| match item {
                Item::Skipped { name, reason } => Some(format!("{name} ({reason})")),
                Item::Numeric(_) => None,
            })
            .collect();
        return Err(format!(
            "no U32, F32, or Nat def to export{}",
            if skipped.is_empty() {
                String::new()
            } else {
                format!("; skipped: {}", skipped.join("; "))
            }
        ));
    }
    Ok(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numeric_defs_and_skips_proofs() {
        let src = r#"
import Base

# a wrapped signature
def add(a: U32, b: U32) -> U32:
  (a + b : U32)

def scale(+n: Nat,
    k: F32) -> F32:
  k

def id(-A: Data, x: A) -> A:
  x

def add_zero(x):
  {==}

law ignored:
  for x: Nat
  {x == x : Nat}

def main() -> U32:
  add(1, 2)
"#;
        let items = scan(src);
        let names: Vec<_> = items.iter().map(Item::name).collect();
        assert!(names.contains(&"add"));
        assert!(names.contains(&"scale"));
        assert!(names.contains(&"id"));
        assert!(names.contains(&"add_zero"));
        assert!(!names.contains(&"ignored"));
        let chosen = select(&items, None).unwrap();
        assert_eq!(
            chosen
                .iter()
                .map(|sig| sig.name.as_str())
                .collect::<Vec<_>>(),
            vec!["add", "scale"]
        );
        assert_eq!(chosen[0].params, vec![NumTy::U32, NumTy::U32]);
        assert_eq!(chosen[0].ret, NumTy::U32);
        assert_eq!(chosen[1].params, vec![NumTy::Nat, NumTy::F32]);
        assert_eq!(chosen[1].ret, NumTy::F32);
        let err = select(&items, Some(&["id".into()])).unwrap_err();
        assert!(err.contains("erased"), "{err}");
        let err = select(&items, Some(&["add_zero".into()])).unwrap_err();
        assert!(err.contains("->"), "{err}");
    }
}
