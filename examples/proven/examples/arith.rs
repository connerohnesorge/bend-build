include!(concat!(env!("OUT_DIR"), "/bend_build/arith.rs"));

fn main() {
    assert_eq!(arith::add_u32(20, 22), 42);
    assert_eq!(arith::mul_u32(6, 7), 42);
    assert_eq!(arith::add_u32(u32::MAX, 1), 0);
    assert_eq!(arith::add(20, 22), 42);
    assert_eq!(arith::mul(6, 7), 42);
    assert_eq!(arith::add(0, 41), 41);
    assert_eq!(arith::add_f32(1.5, 2.25), 3.75);
    println!(
        "arith: 20 + 22 = {}, 6 * 7 = {}",
        arith::add(20, 22),
        arith::mul(6, 7)
    );
}
