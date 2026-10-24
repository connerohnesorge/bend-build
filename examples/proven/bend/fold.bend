# A list fold and a right fold add the same Nats.
# sum_to is the def Rust calls; the law is about the two folds.

import Base

def add(a: Nat, b: Nat) -> Nat:
  match a:
    case 0n:
      b
    case 1n+p:
      1n+add(p, b)

def add_zero(a: Nat) -> {add(a, 0n) == a : Nat}:
  match a:
    case 0n:
      {==}
    case 1n+p:
      %add_zero(p) : {1n+add(p, 0n) == 1n+_ : Nat}
      {==}

def add_assoc(a: Nat, -b: Nat, -c: Nat) -> {add(a, add(b, c)) == add(add(a, b), c) : Nat}:
  match a:
    case 0n:
      {==}
    case 1n+p:
      %add_assoc(p, b, c) : {1n+add(p, add(b, c)) == 1n+_ : Nat}
      {==}

def fold_add(xs: +List<Nat>, acc: Nat) -> Nat:
  match xs:
    case Nil{}:
      acc
    case Con{h, t}:
      fold_add(t, add(acc, h))

def sum_r(xs: +List<Nat>) -> Nat:
  match xs:
    case Nil{}:
      0n
    case Con{h, t}:
      add(h, sum_r(t))

def fold_gen(xs: +List<Nat>, +acc: Nat) -> {fold_add(xs, acc) == add(acc, sum_r(xs)) : Nat}:
  match xs:
    case Nil{}:
      Equal.sym(Nat, add(acc, 0n), acc, add_zero(acc))
    case Con{h, t}:
      %Equal.sym(Nat, fold_add(t, add(acc, h)), add(add(acc, h), sum_r(t)), fold_gen(t, add(acc, h))) : {_ == add(acc, add(h, sum_r(t))) : Nat}
      %add_assoc(acc, h, sum_r(t)) : {_ == add(acc, add(h, sum_r(t))) : Nat}
      {==}

law fold_is_sum:
  for +xs: +List<Nat>
  {fold_add(xs, 0n) == sum_r(xs) : Nat}

def add_zero_l(b: Nat) -> {add(0n, b) == b : Nat}:
  {==}

def fold_is_sum(xs):
  %Equal.sym(Nat, fold_add(xs, 0n), add(0n, sum_r(xs)), fold_gen(xs, 0n)) : {_ == sum_r(xs) : Nat}
  add_zero_l(sum_r(xs))

def sum_to(n: Nat) -> Nat:
  match n:
    case 0n:
      0n
    case 1n++p:
      add(sum_to(p), n)

def main() -> Nat:
  sum_to(10n)
