#![feature(ptr_sub_ptr, once_cell)]

use std::borrow::Cow;

#[allow(unused_imports)]
use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_till, take_until, take_while, take_while_m_n},
    character::{
        complete::{alphanumeric1, char as cchar, multispace0, multispace1, none_of, one_of},
        is_alphabetic, is_newline, is_space,
        streaming::space1,
    },
    combinator::{fail, map, map_res, opt, value},
    number::complete::be_u8,
    sequence::{delimited, tuple},
};
use nom::{
    bytes::complete::take,
    combinator::success,
    error::{context, ErrorKind, ParseError},
    multi::{length_count, length_data},
    number::{complete, Endianness},
    Parser,
};
use nom_supreme::{error::*, ParserExt};
use serde::{Deserialize, Serialize};

mod lua51;
mod lua53;
mod lua54;
pub mod utils;

pub type IResult<I, O, E = ErrorTree<I>> = Result<(I, O), nom::Err<E>>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LuaHeader {
    pub lua_version: u8,
    pub format_version: u8,
    pub big_endian: bool,
    pub int_size: u8,
    pub size_t_size: u8,
    pub instruction_size: u8,
    pub number_size: u8,
    pub number_integral: bool,
}

impl LuaHeader {
    pub fn endian(&self) -> Endianness {
        if self.big_endian {
            Endianness::Big
        } else {
            Endianness::Little
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LuaNumber {
    Integer(i64),
    Float(f64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LuaConstant {
    Null,
    Bool(bool),
    Number(LuaNumber),
    String(Vec<u8>),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LuaLocal {
    pub name: String,
    pub start_pc: u64,
    pub end_pc: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LuaVarArgInfo {
    pub has_arg: bool,
    pub needs_arg: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpVal {
    pub on_stack: bool,
    pub id: u8,
    pub kind: u8,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LuaChunk {
    pub name: Vec<u8>,
    pub line_defined: u64,
    pub last_line_defined: u64,
    pub num_upvalues: u8,
    pub num_params: u8,
    pub is_vararg: Option<LuaVarArgInfo>,
    pub max_stack: u8,
    pub instructions: Vec<u32>,
    pub constants: Vec<LuaConstant>,
    pub prototypes: Vec<LuaChunk>,
    pub source_lines: Vec<(u32, u32)>,
    pub locals: Vec<LuaLocal>,
    pub upvalue_infos: Vec<UpVal>, // for lua53
    pub upvalue_names: Vec<Vec<u8>>,
}

impl LuaChunk {
    pub fn name(&self) -> Cow<str> {
        String::from_utf8_lossy(&self.name)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LuaBytecode {
    pub header: LuaHeader,
    pub main_chunk: LuaChunk,
}

fn lua_header(input: &[u8]) -> IResult<&[u8], LuaHeader, ErrorTree<&[u8]>> {
    let (rest, (_, result)) = tuple((
        tag(b"\x1BLua"),
        alt((
            map(
                tuple((
                    tag(b"\x51"),
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                )),
                |(
                    _,
                    format_version,
                    big_endian,
                    int_size,
                    size_t_size,
                    instruction_size,
                    number_size,
                    number_integral,
                )| LuaHeader {
                    lua_version: 0x51,
                    format_version,
                    big_endian: big_endian != 1,
                    int_size,
                    size_t_size,
                    instruction_size,
                    number_size,
                    number_integral: number_integral != 0,
                },
            ),
            map(
                tuple((
                    tag(b"\x53"),
                    be_u8,
                    take(6usize), // LUAC_DATA
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    complete::le_i64,
                    complete::le_f64,
                    be_u8,
                )),
                |(
                    _,
                    format_version,
                    _luac_data,
                    int_size,
                    size_t_size,
                    instruction_size,
                    _integer_size, // lua_Integer
                    number_size,
                    _,
                    _,
                    _,
                )| LuaHeader {
                    lua_version: 0x53,
                    format_version,
                    big_endian: cfg!(target_endian = "big"),
                    int_size,
                    size_t_size,
                    instruction_size,
                    number_size,
                    number_integral: false,
                },
            ),
            map(
                tuple((
                    tag(b"\x54"),
                    be_u8,
                    take(6usize), // LUAC_DATA
                    be_u8,
                    be_u8,
                    be_u8,
                    complete::le_i64,
                    complete::le_f64,
                    be_u8,
                )),
                |(
                    _,
                    format_version,
                    _luac_data,
                    instruction_size,
                    _integer_size, // lua_Integer
                    number_size,
                    _,
                    _,
                    _,
                )| LuaHeader {
                    lua_version: 0x54,
                    format_version,
                    big_endian: cfg!(target_endian = "big"),
                    int_size: 4,
                    size_t_size: 8,
                    instruction_size,
                    number_size,
                    number_integral: false,
                },
            ),
        )),
    ))(input)?;
    Ok((rest, result))
}

fn must<I, O, E: ParseError<I>, P: Parser<I, O, E>>(
    cond: bool,
    mut parser: P,
) -> impl FnMut(I) -> IResult<I, O, E> {
    move |input| {
        if cond {
            parser.parse(input)
        } else {
            Err(nom::Err::Error(E::from_error_kind(
                input,
                ErrorKind::Switch,
            )))
        }
    }
}

fn lua_int<'a>(header: &LuaHeader) -> impl Parser<&'a [u8], u64, ErrorTree<&'a [u8]>> {
    let intsize = header.int_size;
    alt((
        must(
            intsize == 8,
            map(complete::u64(header.endian()), |v| v as u64),
        ),
        must(
            intsize == 4,
            map(complete::u32(header.endian()), |v| v as u64),
        ),
        must(
            intsize == 2,
            map(complete::u16(header.endian()), |v| v as u64),
        ),
        must(intsize == 1, map(be_u8, |v| v as u64)),
    ))
    .context("integer")
}

fn lua_size_t<'a>(header: &LuaHeader) -> impl Parser<&'a [u8], u64, ErrorTree<&'a [u8]>> {
    let sizesize = header.size_t_size;
    alt((
        must(
            sizesize == 8,
            map(complete::u64(header.endian()), |v| v as u64),
        ),
        must(
            sizesize == 4,
            map(complete::u32(header.endian()), |v| v as u64),
        ),
        must(
            sizesize == 2,
            map(complete::u16(header.endian()), |v| v as u64),
        ),
        must(sizesize == 1, map(be_u8, |v| v as u64)),
    ))
    .context("size_t")
}

fn lua_number<'a>(header: &LuaHeader) -> impl Parser<&'a [u8], LuaNumber, ErrorTree<&'a [u8]>> {
    let int = header.number_integral;
    let size = header.number_size;
    alt((
        must(
            int == true,
            map(
                alt((
                    must(size == 8, map(complete::be_i8, |v| v as i64)),
                    must(size == 4, map(complete::i16(header.endian()), |v| v as i64)),
                    must(size == 2, map(complete::i32(header.endian()), |v| v as i64)),
                    must(size == 1, map(complete::i64(header.endian()), |v| v as i64)),
                )),
                |v| LuaNumber::Integer(v),
            ),
        ),
        must(
            int == false,
            map(
                alt((
                    must(size == 8, map(complete::f64(header.endian()), |v| v as f64)),
                    must(size == 4, map(complete::f32(header.endian()), |v| v as f64)),
                )),
                |v| LuaNumber::Float(v),
            ),
        ),
    ))
    .context("number")
}

pub fn lua_bytecode(input: &[u8]) -> IResult<&[u8], LuaBytecode, ErrorTree<&[u8]>> {
    let (input, header) = lua_header(input)?;
    log::trace!("header: {header:?}");
    let (input, main_chunk) = match header.lua_version {
        0x51 => lua51::lua_chunk(&header).parse(input)?,
        0x53 => lua53::lua_chunk(&header).parse(input)?,
        0x54 => lua54::lua_chunk(&header).parse(input)?,
        _ => context("unsupported lua version", fail)(input)?,
    };
    Ok((input, LuaBytecode { header, main_chunk }))
}

pub fn parse(input: &[u8]) -> Result<LuaBytecode, String> {
    lua_bytecode(input).map(|x| x.1).map_err(|e| {
        format!(
            "{:#?}",
            e.map(|e| e.map_locations(|p| unsafe { p.as_ptr().sub_ptr(input.as_ptr()) }))
        )
    })
}
