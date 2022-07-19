use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::iter::Peekable;
use std::str::{Chars, FromStr};

#[derive(Clone, Debug)]
pub enum ParseKeyValueDescriptorError {
    MissingClosingQuote,
    MissingDelimiter(char),
    InvalidDelimiter {
        expected: char,
        found: char
    },
    UnexpectedContinuation
}

impl Display for ParseKeyValueDescriptorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ParseKeyValueDescriptorError::MissingClosingQuote =>
                write!(f, "Missing closing quote in string."),
            ParseKeyValueDescriptorError::MissingDelimiter(d) =>
                write!(f, "Missing delimiter: \'{}\'.", d),
            ParseKeyValueDescriptorError::InvalidDelimiter {
                expected,
                found
            } => write!(f, "Expected delimiter \'{}\', but found: \'{}\'.",
                expected, found),
            ParseKeyValueDescriptorError::UnexpectedContinuation =>
                write!(f, "Expected end, but effect descriptor continued.")
        }
    }
}

impl Error for ParseKeyValueDescriptorError { }

#[derive(Clone, Deserialize, Serialize)]
pub struct KeyValueDescriptor {
    pub name: String,
    pub key_values: HashMap<String, String>
}

fn fmt_string(f: &mut Formatter<'_>, s: &str) -> fmt::Result {
    if s.chars().any(is_delimiter) || s.starts_with('\"') {
        write!(f, "\"")?;

        for c in s.chars() {
            if is_delimiter(c) {
                write!(f, "\\{}", c)?;
            }
            else {
                write!(f, "{}", c)?;
            }
        }

        write!(f, "\"")
    }
    else {
        write!(f, "{}", s)
    }
}

impl Display for KeyValueDescriptor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.name)?;

        if self.key_values.len() > 0 {
            write!(f, "(")?;

            for (i, (k, v)) in self.key_values.iter().enumerate() {
                if i > 0 {
                    write!(f, ",")?;
                }

                fmt_string(f, k)?;
                write!(f, "=")?;
                fmt_string(f, v)?;
            }

            write!(f, ")")?;
        }

        Ok(())
    }
}

impl FromStr for KeyValueDescriptor {
    type Err = ParseKeyValueDescriptorError;

    fn from_str(code: &str)
            -> Result<KeyValueDescriptor, ParseKeyValueDescriptorError> {
        let mut chars = code.chars().peekable();
        let name = parse_string(&mut chars)?;
        let mut parenthesis = false;
        
        let key_values = match chars.next() {
            Some('(') => {
                parenthesis = true;
                parse_key_value(&mut chars)?
            },
            Some('=') => {
                let value = parse_string(&mut chars)?;
                let mut map = HashMap::new();
                map.insert(name.clone(), value);
                map
            },
            Some(c) => return Err(ParseKeyValueDescriptorError::InvalidDelimiter {
                expected: '(',
                found: c
            }),
            None => HashMap::new()
        };
        
        if parenthesis {
            consume_delimiter(&mut chars, ')')?;
        }
        
        if chars.next().is_some() {
            return Err(ParseKeyValueDescriptorError::UnexpectedContinuation);
        }

        Ok(KeyValueDescriptor {
            name,
            key_values
        })
    }
}

fn is_delimiter(c: char) -> bool {
    c == '(' || c == ')' || c == ',' || c == '='
}

fn parse_string<'a>(chars: &mut Peekable<Chars<'a>>)
        -> Result<String, ParseKeyValueDescriptorError> {
    let mut s = String::new();
    let mut quote_mode = false;

    if let Some(&first) = chars.peek() {
        if first == '\"' {
            chars.next();
            quote_mode = true;
        }
    }

    let mut escaped = false;

    while let Some(&c) = chars.peek() {
        let mut new_escaped = false;

        if is_delimiter(c) {
            if !quote_mode {
                return Ok(s);
            }
        }
        else if c == '\\' {
            if quote_mode && !escaped {
                new_escaped = true;
            }
        }
        else if c == '\"' {
            if quote_mode && !escaped {
                chars.next();
                return Ok(s);
            }
        }

        escaped = new_escaped;
        chars.next();
        s.push(c);
    }

    if quote_mode {
        Err(ParseKeyValueDescriptorError::MissingClosingQuote)
    }
    else {
        Ok(s)
    }
}

fn consume_delimiter<'a>(chars: &mut Peekable<Chars<'a>>, delimiter: char)
        -> Result<(), ParseKeyValueDescriptorError> {
    match chars.next() {
        Some(c) => {
            if c == delimiter {
                Ok(())
            }
            else {
                Err(ParseKeyValueDescriptorError::InvalidDelimiter {
                    expected: '=',
                    found: c
                })
            }
        },
        None => Err(ParseKeyValueDescriptorError::MissingDelimiter(delimiter))
    }
}

fn parse_key_value<'a>(chars: &mut Peekable<Chars<'a>>)
        -> Result<HashMap<String, String>, ParseKeyValueDescriptorError> {
    let mut first = true;
    let mut result = HashMap::new();

    while chars.peek().is_some() {
        if first {
            first = false;
        }
        else {
            consume_delimiter(chars, ',')?;
        }

        let key = parse_string(chars)?;
        consume_delimiter(chars, '=')?;
        let value = parse_string(chars)?;
        result.insert(key, value);

        if chars.peek() == Some(&')') {
            break;
        }
    }

    Ok(result)
}
