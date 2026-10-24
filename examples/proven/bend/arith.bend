# Arithmetic with proofs the Bend checker accepts.
# add_comm is induction; mul_zero is the base of mul.
# U32 and F32 wrappers are what Rust calls. F32 is axiomatic in Bend,
# so no law talks about it.

import Base

def add(a: Nat, b: Nat) -> Nat:
  match a:
    case 0n:
      b
    case 1n+p:
      1n+add(p, b)

def mul(a: Nat, +b: Nat) -> Nat:
  match a:
    case 0n:
      0n
    case 1n+p:
      add(b, mul(p, b))

def add_u32(a: U32, b: U32) -> U32:
  (a + b : U32)

def mul_u32(a: U32, b: U32) -> U32:
  (a * b : U32)

def add_f32(a: F32, b: F32) -> F32:
  (a + b : F32)

def add_zero(a: Nat) -> {add(a, 0n) == a : Nat}:
  match a:
    case 0n:
      {==}
    case 1n+p:
      %add_zero(p) : {1n+add(p, 0n) == 1n+_ : Nat}
      {==}

def add_succ_r(-a: Nat, b: Nat) -> {add(b, 1n+a) == 1n+add(b, a) : Nat}:
  match b:
    case 0n:
      {==}
    case 1n+p:
      %add_succ_r(a, p) : {1n+add(p, 1n+a) == 1n+_ : Nat}
      {==}

law add_comm:
  for a: Nat
  for +b: Nat
  {add(a, b) == add(b, a) : Nat}

def add_comm(a, b):
  match a:
    case 0n:
      Equal.sym(Nat, add(b, 0n), b, add_zero(b))
    case 1n+p:
      %Equal.sym(Nat, add(p, b), add(b, p), add_comm(p, b)) : {1n+_ == add(b, 1n+p) : Nat}
      %Equal.sym(Nat, add(b, 1n+p), 1n+add(b, p), add_succ_r(p, b)) : {1n+add(b, p) == _ : Nat}
      {==}

law mul_zero:
  for +b: Nat
  {mul(0n, b) == 0n : Nat}

def mul_zero(b):
  {==}

def main() -> Nat:
  mul(6n, 7n)
