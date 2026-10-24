include!(concat!(env!("OUT_DIR"), "/bend_build/fold.rs"));

fn main() {
    // sum_to(n) = 1 + ... + n, and the Bend file proves its list folds agree.
    assert_eq!(fold::sum_to(0), 0);
    assert_eq!(fold::sum_to(1), 1);
    assert_eq!(fold::sum_to(10), 55);
    assert_eq!(fold::sum_to(100), 5050);
    println!("fold: sum 1..=100 = {}", fold::sum_to(100));
}
