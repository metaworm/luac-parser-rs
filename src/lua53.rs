use complete::le_u8;

use super::{lua52::load_upvalue, *};

pub fn load_string(input: &[u8]) -> IResult<&[u8], &[u8]> {
    let (mut input, n) = be_u8(input)?;
    let mut n = n as u64;
    if n == 0xFF {
        // TODO: usize
        let parse = |s| complete::le_u64(s);
        (input, n) = parse(input)?;
    }
    if n == 0 {
        return Ok((input, &[]));
    }
    take(n as usize - 1)(input)
}

pub fn lua_local<'a>(header: &LuaHeader) -> impl Parser<&'a [u8], LuaLocal, ErrorTree<&'a [u8]>> {
    tuple((load_string, lua_int(header), lua_int(header)))
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
        let (input, (name, line_defined, last_line_defined, num_params, is_vararg, max_stack)) =
            tuple((
                load_string,
                lua_int(header),
                lua_int(header),
                be_u8,
                be_u8,
                be_u8,
            ))(input)?;
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
                length_count(
                    lua_int(header).map(|x| x as usize),
                    alt((
                        take_lv_nil,
                        take_lv_bool,
                        take_lv_float,
                        take_lv_str,
                        take_lv_u64,
                    )),
                )
                .context("count constants"),
                length_count(lua_int(header).map(|x| x as usize), load_upvalue)
                    .context("count upvalues"),
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
                    load_string.map(|v| v.to_vec()),
                )
                .context("count upval names"),
            )),
            move |(
                instructions,
                constants,
                upvalue_infos,
                prototypes,
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
                    prototypes,
                    source_lines,
                    locals,
                    upvalue_names,
                    upvalue_infos,
                    num_constants: vec![],
                }
            },
        )
        .context("chunk")
        .parse(input)
    }
}

fn take_lv_nil(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, _) = tag(b"\0")(input)?;
    Ok((input, LuaConstant::Null))
}

fn take_lv_bool(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, (_, b)) = tuple((tag(b"\x01"), le_u8))(input)?;
    Ok((input, LuaConstant::Bool(b != 0)))
}

fn take_lv_float(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, (_, f)) = tuple((tag(b"\x03"), complete::le_f64))(input)?;
    Ok((input, LuaConstant::Number(LuaNumber::Float(f as _))))
}

fn le_u8_minus_one(input: &[u8]) -> IResult<&[u8], u8> {
    let (input, out) = le_u8(input)?;
    Ok((input, out - 1))
}

fn take_lv_str(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, (_, data)) = tuple((
        alt((tag(b"\x04"), tag("\x14"))),
        length_data(le_u8_minus_one),
    ))(input)?;

    Ok((input, LuaConstant::from(data.to_vec())))
}

fn take_lv_u64(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, (_, val)) = tuple((tag(b"\x13"), complete::le_u64))(input)?;
    Ok((input, LuaConstant::Number(LuaNumber::Integer(val as _))))
}
