use std::{
    io::Result,
    path::Path,
    process::{Command, Stdio},
};

fn compile(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    Ok(Command::new("luau")
        .arg("--compile=binary")
        .arg("-g2")
        .arg(path.as_ref())
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?
        .stdout)
}

#[test]
fn test() {
    for e in std::fs::read_dir("tests/luau").unwrap().flatten() {
        let p = e.path();
        println!("--------------- {p:?} ---------------");
        luac_parser::luau::bytecode(&compile(p).unwrap()).unwrap();
    }
}
