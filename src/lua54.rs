use super::*;
use complete::le_u8;

pub fn load_unsigned<'a>(mut limit: usize) -> impl Parser<&'a [u8], usize, ErrorTree<&'a [u8]>> {
    move |mut input| -> IResult<&'a [u8], usize> {
        let mut x = 0;
        limit >>= 7;
        loop {
            let (rest, b) = le_u8(input)?;
            input = rest;
            if x >= limit {
                context("integer overflow", fail::<&[u8], &str, _>)(input)?;
            }
            x = (x << 7) | (b as usize & 0x7f);
            if b & 0x80 != 0 {
                break;
            }
        }
        Ok((input, x))
    }
}

pub fn load_size(input: &[u8]) -> IResult<&[u8], usize> {
    load_unsigned(!0).parse(input)
}

pub fn lua_int(input: &[u8]) -> IResult<&[u8], u64> {
    map(load_unsigned(i32::MAX as _), |x| x as u64)(input)
}

pub fn load_string(input: &[u8]) -> IResult<&[u8], &[u8]> {
    let (input, n) = load_size(input)?;
    if n == 0 {
        return Ok((input, &[]));
    }
    context("string", take(n - 1))(input)
}

pub fn load_upvalue(input: &[u8]) -> IResult<&[u8], UpVal> {
    map(tuple((le_u8, le_u8, le_u8)), |(on_stack, id, kind)| UpVal {
        on_stack: on_stack != 0,
        id,
        kind,
    })(input)
}

pub fn lua_local<'a>(_header: &LuaHeader) -> impl Parser<&'a [u8], LuaLocal, ErrorTree<&'a [u8]>> {
    tuple((load_string, lua_int, lua_int))
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
            context(
                "chunk header",
                tuple((load_string, lua_int, lua_int, be_u8, be_u8, be_u8)),
            )(input)?;
        log::trace!(
            "chunk: {}, line: {line_defined}-{last_line_defined}",
            String::from_utf8_lossy(name)
        );

        map(
            tuple((
                length_count(lua_int.map(|x| x as usize), complete::u32(header.endian()))
                    .context("count instruction"),
                length_count(
                    lua_int.map(|x| x as usize),
                    alt((
                        take_lv_nil,
                        take_lv_false,
                        take_lv_true,
                        take_lv_float,
                        take_lv_str,
                        take_lv_u64,
                    )),
                )
                .context("count constants"),
                length_count(lua_int.map(|x| x as usize), load_upvalue).context("count upvalues"),
                |i| {
                    length_count(lua_int.map(|x| x as usize), lua_chunk(header))
                        .context("count prototypes")
                        .parse(i)
                },
                length_count(lua_int.map(|x| x as usize), le_u8).context("count line info"),
                length_count(
                    lua_int.map(|x| x as usize),
                    tuple((lua_int, lua_int)).map(|(a, b)| (a as u32, b as u32)),
                )
                .context("count source lines"),
                length_count(lua_int.map(|x| x as usize), lua_local(header))
                    .context("count locals"),
                length_count(lua_int.map(|x| x as usize), load_string.map(|v| v.to_vec()))
                    .context("count upval names"),
            )),
            move |(
                instructions,
                constants,
                upvalues,
                prototypes,
                _line_info,
                source_lines,
                locals,
                upvalue_names,
            )| {
                LuaChunk {
                    name: name.to_vec(),
                    line_defined,
                    last_line_defined,
                    num_upvalues: upvalues.len() as _,
                    flags: 0,
                    num_params,
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
                    num_constants: vec![],
                    upvalue_infos: upvalues,
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

fn take_lv_false(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, _) = tag(b"\x01")(input)?;
    Ok((input, LuaConstant::Bool(false)))
}

fn take_lv_true(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, _) = tag(b"\x11")(input)?;
    Ok((input, LuaConstant::Bool(true)))
}

fn take_lv_float(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, (_, f)) = tuple((tag(b"\x13"), complete::le_f64))(input)?;
    Ok((input, LuaConstant::Number(LuaNumber::Float(f))))
}

fn take_lv_str(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, (_, data)) = tuple((alt((tag(b"\x04"), tag("\x14"))), load_string))(input)?;
    Ok((input, LuaConstant::from(data.to_vec())))
}

fn take_lv_u64(input: &[u8]) -> IResult<&[u8], LuaConstant> {
    let (input, (_, val)) = tuple((tag(b"\x03"), complete::le_u64))(input)?;
    Ok((input, LuaConstant::Number(LuaNumber::Integer(val as _))))
}
