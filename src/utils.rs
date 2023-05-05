use super::{LuaConstant, LuaNumber};

use std::collections::HashSet;
use std::sync::LazyLock;

impl LuaConstant {
    pub fn as_literal_str(&self) -> Option<&str> {
        if let Self::String(s) = self {
            std::str::from_utf8(s).ok()
        } else {
            None
        }
    }

    pub fn as_ident_str(&self) -> Option<&str> {
        const KEYWORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
            [
                "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if",
                "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until",
                "while",
            ]
            .into_iter()
            .collect()
        });

        self.as_literal_str().filter(|&s| {
            let mut chars = s.chars();
            !KEYWORDS.contains(s)
                && chars
                    .next()
                    .filter(|&c| c.is_alphabetic() || c == '_')
                    .is_some()
                && chars.all(|c| c.is_alphanumeric() || c == '_')
        })
    }

    pub fn to_literal(&self) -> String {
        match self {
            Self::String(s) => {
                format!(
                    "\"{}\"",
                    if let Ok(s) = std::str::from_utf8(s) {
                        s.into()
                    } else {
                        // TODO:
                        unsafe { std::str::from_utf8_unchecked(s) }
                    }
                    .escape_debug()
                )
            }
            Self::Bool(b) => b.to_string(),
            Self::Number(LuaNumber::Float(f)) => f.to_string(),
            Self::Number(LuaNumber::Integer(i)) => i.to_string(),
            Self::Null => "nil".into(),
            Self::Proto(i) => format!("function<{i}>"),
            Self::Table { .. } => "{}".into(),
            // Self::Imp(imp) => format!("imp<{imp}>"),
        }
    }
}
