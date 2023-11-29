use luac_parser::{LuaConstant, LuaNumber};

#[test]
fn test_duble() {
    let parsed = luac_parser::parse(&std::fs::read("tests/lua51/concat-duble.luac").unwrap()).unwrap();
    println!("{:#?}", parsed.main_chunk.constants);

    let LuaConstant::String(str_print) = &parsed.main_chunk.constants[0] else { unreachable!() };
    let LuaConstant::String(str_print_1) = &parsed.main_chunk.constants[1] else { unreachable!() };
    let LuaConstant::Number(LuaNumber::Float(num_1)) = &parsed.main_chunk.constants[2] else { unreachable!() };
    let LuaConstant::String(str_print_2) = &parsed.main_chunk.constants[3] else { unreachable!() };
    let LuaConstant::Number(LuaNumber::Float(num_2)) = &parsed.main_chunk.constants[4] else { unreachable!() };
    let LuaConstant::String(str_print_1337) = &parsed.main_chunk.constants[5] else { unreachable!() };
    let LuaConstant::Number(LuaNumber::Float(num_1337)) = &parsed.main_chunk.constants[6] else { unreachable!() };

    assert_eq!(str_print.as_slice(), b"print");
    assert_eq!(str_print_1.as_slice(), b"print number 1: ");
    assert_eq!(*num_1, 1.);
    assert_eq!(str_print_2.as_slice(), b"print number 2: ");
    assert_eq!(*num_2, 2.);
    assert_eq!(str_print_1337.as_slice(), b"print number 1337: ");
    assert_eq!(*num_1337, 1337.);
}

#[test]
fn test_int() {
    let parsed = luac_parser::parse(&std::fs::read("tests/lua51/concat-int.luac").unwrap()).unwrap();
    println!("{:#?}", parsed.main_chunk.constants);

    let LuaConstant::String(str_print) = &parsed.main_chunk.constants[0] else { unreachable!() };
    let LuaConstant::String(str_print_1) = &parsed.main_chunk.constants[1] else { unreachable!() };
    let LuaConstant::Number(LuaNumber::Integer(num_1)) = &parsed.main_chunk.constants[2] else { unreachable!() };
    let LuaConstant::String(str_print_2) = &parsed.main_chunk.constants[3] else { unreachable!() };
    let LuaConstant::Number(LuaNumber::Integer(num_2)) = &parsed.main_chunk.constants[4] else { unreachable!() };
    let LuaConstant::String(str_print_1337) = &parsed.main_chunk.constants[5] else { unreachable!() };
    let LuaConstant::Number(LuaNumber::Integer(num_1337)) = &parsed.main_chunk.constants[6] else { unreachable!() };

    assert_eq!(str_print.as_slice(), b"print");
    assert_eq!(str_print_1.as_slice(), b"print number 1: ");
    assert_eq!(*num_1, 1);
    assert_eq!(str_print_2.as_slice(), b"print number 2: ");
    assert_eq!(*num_2, 2);
    assert_eq!(str_print_1337.as_slice(), b"print number 1337: ");
    assert_eq!(*num_1337, 1337);
}
