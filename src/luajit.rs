#![allow(dead_code)]

use std::cell::RefCell;

use nom::{multi::count, number::complete::le_u8};
use nom_leb128::{leb128_u32, leb128_u64, leb128_usize};

use super::*;

const FLAG_IS_BIG_ENDIAN: u8 = 0b00000001;
const FLAG_IS_STRIPPED: u8 = 0b00000010;
const FLAG_HAS_FFI: u8 = 0b00000100;

const BCDUMP_KGC_CHILD: u8 = 0;
const BCDUMP_KGC_TAB: u8 = 1;
const BCDUMP_KGC_I64: u8 = 2;
const BCDUMP_KGC_U64: u8 = 3;
const BCDUMP_KGC_COMPLEX: u8 = 4;
const BCDUMP_KGC_STR: u8 = 5;

const BCDUMP_KTAB_NIL: u8 = 0;
const BCDUMP_KTAB_FALSE: u8 = 1;
const BCDUMP_KTAB_TRUE: u8 = 2;
const BCDUMP_KTAB_INT: u8 = 3;
const BCDUMP_KTAB_NUM: u8 = 4;
const BCDUMP_KTAB_STR: u8 = 5;

pub fn uleb128_33(mut input: &[u8]) -> IResult<&[u8], u32, ErrorTree<&[u8]>> {
    let v;
    (input, v) = le_u8(input)?;
    let mut v = v as u32 >> 1; // uint32_t v = (*p++ >> 1);
    if v >= 0x40 {
        let mut sh = -1i32;
        v &= 0x3f;
        let mut p = le_u8(input)?.1;
        loop {
            sh += 7;
            v |= (p as u32 & 0x7f) << (sh as u32);
            (input, p) = le_u8(input)?;
            if p < 0x80 {
                break;
            } // while (*p++ >= 0x80);
        }
    }
    Ok((input, v))
}

pub fn lj_header(input: &[u8]) -> IResult<&[u8], LuaHeader, ErrorTree<&[u8]>> {
    let (rest, (_, result)) = tuple((
        tag(b"\x1bLJ"),
        alt((
            map(tuple((tag(b"\x01"), be_u8)), |(_, flags)| LuaHeader {
                lua_version: LUAJ1,
                format_version: 0,
                big_endian: flags & FLAG_IS_BIG_ENDIAN != 0,
                int_size: 4,
                size_t_size: 4,
                instruction_size: 4,
                number_size: 4,
                number_integral: false,
                stripped: flags & FLAG_IS_STRIPPED != 0,
                has_ffi: flags & FLAG_HAS_FFI != 0,
            })
            .context("luajit1"),
            map(tuple((tag(b"\x02"), be_u8)), |(_, flags)| LuaHeader {
                lua_version: LUAJ2,
                format_version: 0,
                big_endian: flags & FLAG_IS_BIG_ENDIAN != 0,
                int_size: 4,
                size_t_size: 4,
                instruction_size: 4,
                number_size: 4,
                number_integral: false,
                stripped: flags & FLAG_IS_STRIPPED != 0,
                has_ffi: flags & FLAG_HAS_FFI != 0,
            })
            .context("luajit2"),
        )),
    ))(input)?;
    Ok((rest, result))
}

pub fn lj_complex_constant<'a, 'h>(
    stack: &'h RefCell<Vec<LuaChunk>>,
    protos: &'h RefCell<Vec<LuaChunk>>,
    endian: Endianness,
) -> impl Parser<&'a [u8], LuaConstant, ErrorTree<&'a [u8]>> + 'h {
    move |input| {
        let (input, ty) = leb128_u64(input)?;
        Ok(match ty as u8 {
            BCDUMP_KGC_I64 => map(
                tuple((nom_leb128::leb128_u32, nom_leb128::leb128_u32)),
                |(lo, hi)| LuaConstant::Number(LuaNumber::Integer(lo as i64 | ((hi as i64) << 32))),
            )(input)?,
            BCDUMP_KGC_U64 => map(
                tuple((nom_leb128::leb128_u32, nom_leb128::leb128_u32)),
                |(lo, hi)| {
                    LuaConstant::Number(LuaNumber::Integer(
                        (lo as u64 | ((hi as u64) << 32)) as i64,
                    ))
                },
            )(input)?,
            BCDUMP_KGC_TAB => lj_tab(endian).context("read table").parse(input)?,
            BCDUMP_KGC_CHILD => match stack.borrow_mut().pop() {
                Some(proto) => {
                    let result = LuaConstant::Proto(protos.borrow().len());
                    protos.borrow_mut().push(proto);
                    (input, result)
                }
                None => context("pop proto", fail).parse(input)?,
            },
            _ if ty >= BCDUMP_KGC_STR as u64 => {
                let len = ty - BCDUMP_KGC_STR as u64;
                let (input, s) = take(len as usize)(input)?;
                (input, LuaConstant::from(s.to_vec()))
            }
            _ => unreachable!("BCDUMP_KGC: {ty}"),
        })
    }
}

pub fn lj_tab<'a>(endian: Endianness) -> impl Parser<&'a [u8], LuaConstant, ErrorTree<&'a [u8]>> {
    move |input: &'a [u8]| {
        let (input, (narray, nhash)) = tuple((leb128_u32, leb128_u32))(input)?;
        // println!("#array {narray} #hash {nhash}");
        let (input, (arr, mut hash)) = tuple((
            count(lj_tabk(endian).context("count table array"), narray as _),
            count(
                tuple((lj_tabk(endian), lj_tabk(endian))).context("count table hash"),
                nhash as _,
            ),
        ))(input)?;
        let mut aiter = arr.into_iter();
        match aiter.next() {
            Some(LuaConstant::Null) | None => {}
            Some(a0) => hash.push((LuaConstant::Number(LuaNumber::Integer(0)), a0.clone())),
        }
        Ok((
            input,
            LuaConstant::Table(
                ConstTable {
                    array: aiter.collect(),
                    hash,
                }
                .into(),
            ),
        ))
    }
}

fn combine_number(lo: u32, hi: u32, endian: Endianness) -> f64 {
    unsafe {
        core::mem::transmute(if endian == Endianness::Big {
            ((lo as u64) << 32) | hi as u64
        } else {
            ((hi as u64) << 32) | lo as u64
        })
    }
}

pub fn lj_tabk<'a>(endian: Endianness) -> impl Parser<&'a [u8], LuaConstant, ErrorTree<&'a [u8]>> {
    move |input: &'a [u8]| {
        let (input, ty) = leb128_usize(input)?;
        // println!("tabk: {ty}");
        Ok(match ty as u8 {
            BCDUMP_KTAB_NIL => (input, LuaConstant::Null),
            BCDUMP_KTAB_FALSE => (input, LuaConstant::Bool(false)),
            BCDUMP_KTAB_TRUE => (input, LuaConstant::Bool(true)),
            BCDUMP_KTAB_INT => map(leb128_u32, |n| {
                LuaConstant::Number(LuaNumber::Integer(n as _))
            })(input)?,
            BCDUMP_KTAB_NUM => map(tuple((leb128_u32, leb128_u32)), |(lo, hi)| {
                LuaConstant::Number(LuaNumber::Float(combine_number(lo, hi, endian)))
            })(input)?,
            _ if ty >= BCDUMP_KTAB_STR as usize => {
                let len = ty - BCDUMP_KTAB_STR as usize;
                let (input, s) = take(len)(input)?;
                (input, LuaConstant::from(s.to_vec()))
            }
            _ => unreachable!("BCDUMP_KTAB: {ty}"),
        })
    }
}

fn lj_num_constant<'a>(
    endian: Endianness,
) -> impl Parser<&'a [u8], LuaNumber, ErrorTree<&'a [u8]>> {
    move |input: &'a [u8]| {
        let isnum = be_u8(input)?.1 & 1 != 0;
        let (input, lo) = uleb128_33(input)?;
        if isnum {
            map(leb128_u32, |hi| {
                LuaNumber::Float(combine_number(lo, hi, endian))
            })(input)
        } else {
            Ok((input, LuaNumber::Integer(lo as _)))
        }
    }
}

fn lj_proto<'a, 'h>(
    header: &'h LuaHeader,
    stack: &'h RefCell<Vec<LuaChunk>>,
) -> impl Parser<&'a [u8], Option<LuaChunk>, ErrorTree<&'a [u8]>> + 'h {
    move |input| {
        // proto header
        let (input, size) = leb128_u32.parse(input)?;
        if size == 0 {
            return Ok((input, None));
        }
        let (
            mut input,
            (
                flags,
                num_params,
                framesize,
                num_upvalues,
                complex_constants_count,
                numeric_constants_count,
                instructions_count,
            ),
        ) = tuple((
            be_u8, be_u8, be_u8, be_u8, leb128_u32, leb128_u32, leb128_u32,
        ))(input)?;

        let mut line_defined = 0;
        let mut numline = 0;
        let mut debuginfo_size = 0;
        if !header.stripped {
            (input, (debuginfo_size, line_defined, numline)) =
                tuple((leb128_u64, leb128_u64, leb128_u64))(input)?;
        }
        let last_line_defined = line_defined + numline;

        let instructions;
        let upvalue_infos;
        let mut constants;
        let num_constants;
        let protos = RefCell::new(vec![]);
        (
            input,
            (instructions, upvalue_infos, constants, num_constants),
        ) = tuple((
            count(complete::u32(header.endian()), instructions_count as usize)
                .context("count instruction"),
            count(
                map(complete::u16(header.endian()), |v| UpVal {
                    on_stack: v & 0x8000 != 0,
                    id: (v & 0x7FFF) as _,
                    kind: 0,
                }),
                num_upvalues as usize,
            )
            .context("count upvals"),
            count(
                lj_complex_constant(stack, &protos, header.endian()),
                complex_constants_count as usize,
            )
            .context("count complex_constant"),
            count(
                lj_num_constant(header.endian()),
                numeric_constants_count as usize,
            )
            .context("count numeric_constants"),
        ))(input)?;
        constants.reverse();

        if debuginfo_size > 0 {
            (input, _) = take(debuginfo_size as usize)(input)?;
        }

        Ok((
            input,
            Some(LuaChunk {
                name: vec![],
                num_upvalues,
                num_params,
                line_defined,
                last_line_defined,
                flags,
                instructions,
                upvalue_infos,
                constants,
                num_constants,
                max_stack: framesize,
                prototypes: protos.into_inner(),
                ..Default::default()
            }),
        ))
    }
}

pub fn lj_chunk<'h, 'a: 'h>(
    header: &'h LuaHeader,
) -> impl Parser<&'a [u8], LuaChunk, ErrorTree<&'a [u8]>> + 'h {
    move |mut input| {
        let mut name = &b""[..];
        if !header.stripped {
            let namelen;
            (input, namelen) = leb128_u32.parse(input)?;
            (input, name) = take(namelen as usize)(input)?;
        }
        let protos = RefCell::new(vec![]);
        while let (i, Some(proto)) = lj_proto(&header, &protos).parse(input)? {
            protos.borrow_mut().push(proto);
            input = i;
        }
        let mut protos = protos.into_inner();
        Ok((
            input,
            if let Some(mut chunk) = protos.pop().filter(|_| protos.is_empty()) {
                chunk.name = name.to_vec();
                chunk
            } else {
                context("stack unbalanced", fail).parse(input)?.1
            },
        ))
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ProtoFlags: u8 {
        const HAS_CHILD = 0b00000001;
        const IS_VARIADIC = 0b00000010;
        const HAS_FFI = 0b00000100;
        const JIT_DISABLED = 0b00001000;
        const HAS_ILOOP = 0b0001000;
    }
}
