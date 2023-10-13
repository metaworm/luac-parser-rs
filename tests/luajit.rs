use luac_parser::{LuaConstant, LuaNumber};

#[test]
fn test() {
    let float_bc = luac_parser::parse(&std::fs::read("tests/luajit/float.luac").unwrap()).unwrap();
    println!("{:#?}", float_bc.main_chunk.num_constants);

    assert_eq!(float_bc.main_chunk.num_constants[0], LuaNumber::Integer(36100));
    assert_eq!(float_bc.main_chunk.num_constants[1], LuaNumber::Integer(40000));
    assert_eq!(
        float_bc.main_chunk.num_constants[2],
        LuaNumber::Float(111122223333.0)
    );
    assert_eq!(
        float_bc.main_chunk.num_constants[3],
        LuaNumber::Float(111122223333.4444)
    );
    assert_eq!(float_bc.main_chunk.num_constants[4], LuaNumber::Integer(-150));
    assert_eq!(
        float_bc.main_chunk.num_constants[5],
        LuaNumber::Float(0xFFFFFFFF123 as i64 as f64)
    );

    let string_bc =
        luac_parser::parse(&std::fs::read("tests/luajit/string.luac").unwrap()).unwrap();
    match &string_bc.main_chunk.constants[1] {
        LuaConstant::String(rc_bytebuf) => {
            if let Ok(string) = String::from_utf8(rc_bytebuf.to_vec()) {
                assert_eq!(string, "A".repeat(766));
            } else {
                panic!()
            }
        }
        _ => {
            panic!()
        }
    }
}
