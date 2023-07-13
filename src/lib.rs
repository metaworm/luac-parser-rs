#![feature(ptr_sub_ptr, lazy_cell, box_patterns)]

use std::{borrow::Cow, rc::Rc};

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
use serde_bytes::ByteBuf;

pub mod lua51;
pub mod lua52;
pub mod lua53;
pub mod lua54;
pub mod luajit;
pub mod luau;
pub mod utils;

pub type IResult<I, O, E = ErrorTree<I>> = Result<(I, O), nom::Err<E>>;

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LuaHeader {
    pub lua_version: u8,
    pub format_version: u8,
    pub big_endian: bool,
    pub int_size: u8,
    pub size_t_size: u8,
    pub instruction_size: u8,
    pub number_size: u8,
    pub number_integral: bool,
    // for luajit
    pub stripped: bool,
    pub has_ffi: bool,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LuaNumber {
    Integer(i64),
    Float(f64),
}

impl std::fmt::Display for LuaNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Float(n) => write!(f, "{n}"),
            Self::Integer(i) => write!(f, "{i}"),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ConstTable {
    pub array: Vec<LuaConstant>,
    pub hash: Vec<(LuaConstant, LuaConstant)>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LuaConstant {
    #[default]
    Null,
    Bool(bool),
    Number(LuaNumber),
    String(Rc<ByteBuf>),
    // for luajit
    Proto(usize),
    Table(Box<ConstTable>),
    // // for luau
    // Imp(u32),
}

impl<T: Into<Vec<u8>>> From<T> for LuaConstant {
    fn from(value: T) -> Self {
        Self::String(Rc::new(ByteBuf::from(value)))
    }
}

impl std::fmt::Debug for LuaConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "Null"),
            Self::Bool(arg0) => f.debug_tuple("Bool").field(arg0).finish(),
            Self::Number(arg0) => match arg0 {
                LuaNumber::Float(n) => f.debug_tuple("Number").field(n).finish(),
                LuaNumber::Integer(n) => f.debug_tuple("Integer").field(n).finish(),
            },
            Self::String(arg0) => f
                .debug_tuple("String")
                .field(&String::from_utf8_lossy(arg0))
                .finish(),
            Self::Proto(i) => f.debug_tuple("Proto").field(i).finish(),
            Self::Table(box ConstTable { array, hash }) => f
                .debug_struct("Table")
                .field("array", array)
                .field("hash", hash)
                .finish(),
            // Self::Imp(imp) => f.debug_tuple("Imp").field(imp).finish(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LuaLocal {
    pub name: String,
    pub start_pc: u64,
    pub end_pc: u64,
    pub reg: u8, // for luau
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LuaVarArgInfo {
    pub has_arg: bool,
    pub needs_arg: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UpVal {
    pub on_stack: bool,
    pub id: u8,
    pub kind: u8,
}

#[derive(Default, Serialize, Deserialize)]
pub struct LuaChunk {
    pub name: Vec<u8>,
    pub line_defined: u64,
    pub last_line_defined: u64,
    pub num_upvalues: u8,
    pub num_params: u8,
    /// Equivalent to framesize for luajit
    pub max_stack: u8,
    /// for luajit
    pub flags: u8,
    pub is_vararg: Option<LuaVarArgInfo>,
    pub instructions: Vec<u32>,
    pub constants: Vec<LuaConstant>,
    /// for luajit
    pub num_constants: Vec<LuaNumber>,
    pub prototypes: Vec<Self>,
    pub source_lines: Vec<(u32, u32)>,
    pub locals: Vec<LuaLocal>,
    /// for lua53
    pub upvalue_infos: Vec<UpVal>,
    pub upvalue_names: Vec<Vec<u8>>,
}

impl std::fmt::Debug for LuaChunk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LuaChunk")
            .field("name", &String::from_utf8_lossy(&self.name))
            .field("line_defined", &self.line_defined)
            .field("last_line_defined", &self.last_line_defined)
            .field("is_vararg", &self.is_vararg.is_some())
            .field("num_params", &self.num_params)
            .field("num_upvalues", &self.num_upvalues)
            .field("locals", &self.locals)
            .field("constants", &self.constants)
            .field("prototypes", &self.prototypes)
            .finish()
    }
}

impl LuaChunk {
    pub fn name(&self) -> Cow<str> {
        String::from_utf8_lossy(&self.name)
    }

    pub fn flags(&self) -> luajit::ProtoFlags {
        luajit::ProtoFlags::from_bits(self.flags).unwrap()
    }

    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
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
                    lua_version: LUA51,
                    format_version,
                    big_endian: big_endian != 1,
                    int_size,
                    size_t_size,
                    instruction_size,
                    number_size,
                    number_integral: number_integral != 0,
                    ..Default::default()
                },
            ),
            map(
                tuple((
                    tag(b"\x52"),
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    be_u8,
                    take(6usize), // LUAC_DATA
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
                    _,
                )| LuaHeader {
                    lua_version: LUA52,
                    format_version,
                    big_endian: big_endian != 1,
                    int_size,
                    size_t_size,
                    instruction_size,
                    number_size,
                    number_integral: number_integral != 0,
                    ..Default::default()
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
                    lua_version: LUA53,
                    format_version,
                    big_endian: cfg!(target_endian = "big"),
                    int_size,
                    size_t_size,
                    instruction_size,
                    number_size,
                    number_integral: false,
                    ..Default::default()
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
                    lua_version: LUA54,
                    format_version,
                    big_endian: cfg!(target_endian = "big"),
                    int_size: 4,
                    size_t_size: 8,
                    instruction_size,
                    number_size,
                    number_integral: false,
                    ..Default::default()
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
    let (input, header) = alt((lua_header, luajit::lj_header))(input)?;
    log::trace!("header: {header:?}");
    let (input, main_chunk) = match header.lua_version {
        LUA51 => lua51::lua_chunk(&header).parse(input)?,
        LUA52 => lua52::lua_chunk(&header).parse(input)?,
        LUA53 => lua53::lua_chunk(&header).parse(input)?,
        LUA54 => lua54::lua_chunk(&header).parse(input)?,
        LUAJ1 | LUAJ2 => luajit::lj_chunk(&header).parse(input)?,
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

#[cfg(feature = "rmp-serde")]
impl LuaBytecode {
    pub fn from_msgpack(mp: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(mp)
    }

    pub fn to_msgpack(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec(self)
    }
}

pub const LUA51: u8 = 0x51;
pub const LUA52: u8 = 0x52;
pub const LUA53: u8 = 0x53;
pub const LUA54: u8 = 0x54;
pub const LUAJ1: u8 = 0x11;
pub const LUAJ2: u8 = 0x12;
