use luac_parser::LuaNumber;

#[test]
fn test() {
    let bc = luac_parser::parse(&std::fs::read("tests/luajit/float.luac").unwrap()).unwrap();
    println!("{:#?}", bc.main_chunk.num_constants);

    assert_eq!(bc.main_chunk.num_constants[0], LuaNumber::Integer(36100));
    assert_eq!(bc.main_chunk.num_constants[1], LuaNumber::Integer(40000));
    assert_eq!(
        bc.main_chunk.num_constants[2],
        LuaNumber::Float(111122223333.0)
    );
    assert_eq!(
        bc.main_chunk.num_constants[3],
        LuaNumber::Float(111122223333.4444)
    );
    assert_eq!(bc.main_chunk.num_constants[4], LuaNumber::Integer(-150));
    assert_eq!(
        bc.main_chunk.num_constants[5],
        LuaNumber::Float(0xFFFFFFFF123 as i64 as f64)
    );
}
