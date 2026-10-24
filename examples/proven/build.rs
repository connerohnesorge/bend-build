fn main() {
    if let Err(err) = bend_build::Builder::new()
        .file("bend/arith.bend")
        .export("add")
        .export("mul")
        .export("add_u32")
        .export("mul_u32")
        .export("add_f32")
        .file("bend/fold.bend")
        .export("sum_to")
        .file("bend/par.bend")
        .export("tree")
        .export("seq")
        .verdict(std::env::var_os("BEND_VERDICT").is_some())
        .compile()
    {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
