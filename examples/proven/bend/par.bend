# tree splits 0 .. 2^d - 1 in half at every level; seq walks it.
# The law is induction on the depth. Rust calls both sides.

import Base

def add(a: Nat, b: Nat) -> Nat:
  match a:
    case 0n:
      b
    case 1n++p:
      1n+add(p, b)

def add_zero(a: Nat) -> {add(a, 0n) == a : Nat}:
  match a:
    case 0n:
      {==}
    case 1n++p:
      %add_zero(p) : {1n+add(p, 0n) == 1n+_ : Nat}
      {==}

def add_assoc(a: Nat, -b: Nat, -c: Nat) -> {add(a, add(b, c)) == add(add(a, b), c) : Nat}:
  match a:
    case 0n:
      {==}
    case 1n++p:
      %add_assoc(p, b, c) : {1n+add(p, add(b, c)) == 1n+_ : Nat}
      {==}

def pow2(d: Nat) -> Nat:
  match d:
    case 0n:
      1n
    case 1n++p:
      +h = pow2(p)
      add(h, h)

# Sum i .. i+2^d-1 by splitting the interval in half.
def tree(d: Nat, +i: Nat) -> Nat:
  match d:
    case 0n:
      i
    case 1n++p:
      +step = pow2(p)
      a b = tree(p, i) tree(p, add(step, i))
      add(a, b)

# Sum i .. i+n-1 from the left.
def seq(n: Nat, +i: Nat) -> Nat:
  match n:
    case 0n:
      0n
    case 1n++m:
      add(seq(m, i), add(m, i))

# seq(m) then seq(n) shifted by m is seq over n+m.
def seq_join(n: Nat, +m: Nat, +i: Nat) -> {add(seq(m, i), seq(n, add(m, i))) == seq(add(n, m), i) : Nat}:
  match n:
    case 0n:
      add_zero(seq(m, i))
    case 1n++q:
      %seq_join(q, m, i) : {add(seq(m, i), add(seq(q, add(m, i)), add(q, add(m, i)))) == add(_, add(add(q, m), i)) : Nat}
      %add_assoc(q, m, i) : {add(seq(m, i), add(seq(q, add(m, i)), add(q, add(m, i)))) == add(add(seq(m, i), seq(q, add(m, i))), _) : Nat}
      add_assoc(seq(m, i), seq(q, add(m, i)), add(q, add(m, i)))

law tree_is_seq:
  for +d: Nat
  for +i: Nat
  {tree(d, i) == seq(pow2(d), i) : Nat}

def tree_is_seq(d, i):
  match d:
    case 0n:
      {==}
    case 1n++p:
      %seq_join(pow2(p), pow2(p), i) : {add(tree(p, i), tree(p, add(pow2(p), i))) == _ : Nat}
      %tree_is_seq(p, i) : {add(tree(p, i), tree(p, add(pow2(p), i))) == add(_, seq(pow2(p), add(pow2(p), i))) : Nat}
      %tree_is_seq(p, add(pow2(p), i)) : {add(tree(p, i), tree(p, add(pow2(p), i))) == add(tree(p, i), _) : Nat}
      {==}

def main() -> Nat:
  tree(4n, 0n)
