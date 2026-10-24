include!(concat!(env!("OUT_DIR"), "/bend_build/par.rs"));

fn main() {
    // tree(d, i) sums i .. i+2^d-1 by forking. seq walks the same range.
    // The Bend law says they agree for every depth; these are witnesses.
    assert_eq!(par::tree(4, 0), 120);
    assert_eq!(par::tree(4, 0), par::seq(16, 0));
    assert_eq!(par::tree(8, 0), par::seq(256, 0));
    assert_eq!(par::tree(8, 0), 32_640);
    assert_eq!(par::tree(8, 1), par::seq(256, 1));
    assert_eq!(par::tree(8, 1), 32_896);
    println!("par_sum: tree(8, 0) = seq(256, 0) = {}", par::tree(8, 0));
}
