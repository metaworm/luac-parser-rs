use nom::number::complete::le_u8;

use super::{
    lua51::{lua_local, lua_string},
    *,
};

pub fn load_upvalue(input: &[u8]) -> IResult<&[u8], UpVal> {
    map(tuple((le_u8, le_u8)), |(on_stack, id)| UpVal {
        on_stack: on_stack != 0,
        id,
        kind: 0,
    })(input)
}

pub fn lua_chunk<'h, 'a: 'h>(
    header: &'h LuaHeader,
) -> impl Parser<&'a [u8], LuaChunk, ErrorTree<&'a [u8]>> + 'h {
    |input| {
        let (input, (line_defined, last_line_defined, num_params, is_vararg, max_stack)) =
            tuple((lua_int(header), lua_int(header), be_u8, be_u8, be_u8))(input)?;
        log::trace!("chunk: \"\", line: {line_defined}-{last_line_defined}",);

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
                length_count(lua_int(header).map(|x| x as usize), load_upvalue)
                    .context("count upvalues"),
                lua_string(header),
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
            move |(
                instructions,
                constants,
                prototypes,
                upvalue_infos,
                name,
                source_lines,
                locals,
                upvalue_names,
            )| {
                LuaChunk {
                    name: name.to_vec(),
                    line_defined,
                    last_line_defined,
                    num_upvalues: upvalue_infos.len() as _,
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
                    upvalue_infos,
                }
            },
        )
        .context("chunk")
        .parse(input)
    }
}
