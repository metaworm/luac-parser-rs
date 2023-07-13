use nom::{multi::count, number::complete::le_u8};

use super::*;

pub fn varint(mut input: &[u8]) -> IResult<&[u8], usize> {
    let mut x = 0usize;
    let mut shift = 0usize;
    loop {
        let (rest, b) = le_u8(input)?;
        input = rest;

        x |= ((b & 0x7f) as usize) << shift;
        shift += 7;

        if b & 0x80 == 0 {
            break;
        }
    }

    Ok((input, x))
}

pub fn string<'a>(input: &'a [u8], stable: &[Rc<ByteBuf>]) -> IResult<&'a [u8], Rc<ByteBuf>> {
    map(varint, |i| {
        if i > 0 {
            stable[i - 1].clone()
        } else {
            Rc::new(ByteBuf::new())
        }
    })(input)
}

pub const LBC_CONSTANT_NIL: u8 = 0;
pub const LBC_CONSTANT_BOOLEAN: u8 = 1;
pub const LBC_CONSTANT_NUMBER: u8 = 2;
pub const LBC_CONSTANT_STRING: u8 = 3;
pub const LBC_CONSTANT_IMPORT: u8 = 4;
pub const LBC_CONSTANT_TABLE: u8 = 5;
pub const LBC_CONSTANT_CLOSURE: u8 = 6;

pub fn table<'a>(mut input: &'a [u8], k: &[LuaConstant]) -> IResult<&'a [u8], ConstTable> {
    let numk;
    (input, numk) = varint(input)?;
    let mut result = ConstTable {
        hash: Vec::with_capacity(numk),
        ..Default::default()
    };
    for _ in 0..numk {
        let ik;
        (input, ik) = varint(input)?;
        result
            .hash
            .push((k[ik].clone(), LuaConstant::Number(LuaNumber::Integer(0))));
    }
    Ok((input, result))
}

pub fn constants<'a>(
    mut input: &'a [u8],
    stable: &[Rc<ByteBuf>],
) -> IResult<&'a [u8], Vec<LuaConstant>> {
    let num;
    (input, num) = varint(input)?;
    let mut result = Vec::with_capacity(num);
    for _ in 0..num {
        let ty;
        let k;
        (input, ty) = le_u8(input)?;
        (input, k) = match ty {
            LBC_CONSTANT_NIL => Ok((input, LuaConstant::Null)),
            LBC_CONSTANT_BOOLEAN => map(le_u8, |b| LuaConstant::Bool(b != 0))(input),
            LBC_CONSTANT_NUMBER => map(complete::f64(Endianness::Little), |n| {
                LuaConstant::Number(LuaNumber::Float(n))
            })(input),
            LBC_CONSTANT_STRING => map(|i| string(i, stable), LuaConstant::String)(input),
            // LBC_CONSTANT_IMPORT => map(complete::be_u32, |i| LuaConstant::Imp(i as _))(input),
            LBC_CONSTANT_IMPORT => map(complete::be_u32, |_| LuaConstant::Null)(input),
            LBC_CONSTANT_TABLE => {
                let (input, t) = table(input, &result)?;
                Ok((input, LuaConstant::Table(t.into())))
            }
            LBC_CONSTANT_CLOSURE => map(varint, LuaConstant::Proto)(input),
            // _ => context("string", fail::<&u8, LuaConstant, _>).parse(input),
            _ => unreachable!("const type: {ty}"),
        }?;
        result.push(k);
    }
    Ok((input, result))
}

pub fn bytecode(input: &[u8]) -> IResult<&[u8], LuaChunk> {
    let (input, _version) = le_u8(input)?;
    // string table
    let (input, stable) = length_count(
        varint,
        map(
            |input| {
                let (input, n) = varint(input)?;
                context("string", take(n))(input)
            },
            |s| Rc::new(ByteBuf::from(s.to_vec())),
        ),
    )(input)?;

    // proto table
    let (mut input, num) = varint(input)?;
    let mut protos = Vec::with_capacity(num);

    let string = |i| string(i, &stable);

    for _ in 0..num {
        let (input1, (max_stack, num_params, num_upvalues, is_vararg)) =
            tuple((be_u8, be_u8, be_u8, be_u8))(input)?;

        let (mut input1, (instructions, constants, prototypes, line_defined, name, has_lineinfo)) =
            tuple((
                length_count(varint, complete::u32(Endianness::Little)),
                |i| constants(i, stable.as_slice()),
                length_count(varint, map(varint, |i| core::mem::take(&mut protos[i]))),
                map(varint, |n| n as u64),
                string,
                le_u8,
            ))(input1)?;

        if has_lineinfo > 0 {
            let (input2, linegaplog2) = be_u8(input1)?;
            let intervals = ((instructions.len() - 1) >> (linegaplog2 as usize)) + 1;
            let (input2, _lineinfo) = count(be_u8, instructions.len())(input2)?;
            let (input2, _abslineinfo) = count(complete::be_i32, intervals)(input2)?;
            input1 = input2;
        }

        let (mut input1, has_debuginfo) = le_u8(input1)?;
        let mut locals = vec![];
        let mut upvalue_names = vec![];
        if has_debuginfo > 0 {
            (input1, (locals, upvalue_names)) = tuple((
                length_count(
                    varint,
                    map(
                        tuple((string, varint, varint, le_u8)),
                        |(name, start, end, reg)| LuaLocal {
                            name: String::from_utf8_lossy(name.as_slice()).into(),
                            start_pc: start as _,
                            end_pc: end as _,
                            reg,
                        },
                    ),
                ),
                length_count(varint, map(string, |s| s.as_ref().clone().into_vec())),
            ))(input1)?;
        }

        input = input1;
        let proto = LuaChunk {
            name: name.to_vec(),
            line_defined,
            last_line_defined: 0,
            num_upvalues,
            num_params,
            max_stack,
            prototypes,
            is_vararg: if is_vararg > 0 {
                Some(LuaVarArgInfo {
                    has_arg: true,
                    needs_arg: true,
                })
            } else {
                None
            },
            instructions,
            constants,
            locals,
            upvalue_names,
            ..Default::default()
        };
        protos.push(proto);
    }

    let (input, mainid) = varint(input)?;
    let main = core::mem::take(&mut protos[mainid]);
    assert!(!main.is_empty());

    Ok((input, main))
}
