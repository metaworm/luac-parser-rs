use super::*;

pub fn lua_string<'a>(header: &LuaHeader) -> impl Parser<&'a [u8], &'a [u8], ErrorTree<&'a [u8]>> {
    length_data(lua_size_t(header).map(|x| x as usize))
        .map(|v| if v.is_empty() { v } else { &v[..v.len() - 1] })
        .context("string")
}

pub fn lua_local<'a>(header: &LuaHeader) -> impl Parser<&'a [u8], LuaLocal, ErrorTree<&'a [u8]>> {
    tuple((lua_string(header), lua_int(header), lua_int(header)))
        .map(|(name, start_pc, end_pc)| LuaLocal {
            name: String::from_utf8_lossy(name).into(),
            start_pc,
            end_pc,
            ..Default::default()
        })
        .context("local")
}

pub fn lua_chunk<'h, 'a: 'h>(
    header: &'h LuaHeader,
) -> impl Parser<&'a [u8], LuaChunk, ErrorTree<&'a [u8]>> + 'h {
    |input| {
        let (input, name) = lua_string(header).parse(input)?;
        let (
            input,
            (line_defined, last_line_defined, num_upvalues, num_params, is_vararg, max_stack),
        ) = tuple((lua_int(header), lua_int(header), be_u8, be_u8, be_u8, be_u8))(input)?;
        log::trace!(
            "chunk: {}, line: {line_defined}-{last_line_defined}",
            String::from_utf8_lossy(name)
        );

        map(
            tuple((
                length_count(lua_int(header).map(|x| x as usize), |input| {
                    alt((must(
                        header.instruction_size == 4,
                        complete::u32(header.endian()),
                    ),))(input)
                })
                .context("count instruction"),
                length_count(lua_int(header).map(|x| x as usize), |input| {
                    let (input, b) = be_u8(input)?;
                    let result = match b {
                        0 => success(LuaConstant::Null)(input),
                        1 => map(be_u8, |v| LuaConstant::Bool(v != 0))(input),
                        3 => map(lua_number(header), |v| LuaConstant::Number(v))(input),
                        4 => map(lua_string(header), |v| LuaConstant::from(v.to_vec()))(input),
                        _ => Err(nom::Err::Error(ErrorTree::from_char(
                            input,
                            char::from_digit(b as _, 10).unwrap_or('x'),
                        ))),
                    };
                    result
                })
                .context("count constants"),
                |i| {
                    length_count(lua_int(header).map(|x| x as usize), lua_chunk(header))
                        .context("count prototypes")
                        .parse(i)
                },
                length_count(
                    lua_int(header).map(|x| x as usize),
                    lua_int(header).map(|n| (n as u32, 0u32)),
                )
                .context("count source lines"),
                length_count(lua_int(header).map(|x| x as usize), lua_local(header))
                    .context("count locals"),
                length_count(
                    lua_int(header).map(|x| x as usize),
                    lua_string(header).map(|v| v.to_vec()),
                )
                .context("count upval names"),
            )),
            move |(instructions, constants, prototypes, source_lines, locals, upvalue_names)| {
                LuaChunk {
                    name: name.to_vec(),
                    line_defined,
                    last_line_defined,
                    num_upvalues,
                    num_params,
                    flags: 0,
                    is_vararg: if (is_vararg & 2) != 0 {
                        Some(LuaVarArgInfo {
                            has_arg: (is_vararg & 1) != 0,
                            needs_arg: (is_vararg & 4) != 0,
                        })
                    } else {
                        None
                    },
                    max_stack,
                    instructions,
                    constants,
                    num_constants: vec![],
                    prototypes,
                    source_lines,
                    locals,
                    upvalue_names,
                    upvalue_infos: vec![],
                }
            },
        )
        .context("chunk")
        .parse(input)
    }
}
