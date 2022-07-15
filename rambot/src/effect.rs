use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use std::iter::Peekable;
use std::str::{Chars, FromStr};

pub enum ParseEffectDescriptorError {
    MissingClosingQuote,
    MissingDelimiter(char),
    InvalidDelimiter {
        expected: char,
        found: char
    },
    UnexpectedContinuation
}

impl Display for ParseEffectDescriptorError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ParseEffectDescriptorError::MissingClosingQuote =>
                write!(f, "Missing closing quote in string."),
            ParseEffectDescriptorError::MissingDelimiter(d) =>
                write!(f, "Missing delimiter: \'{}\'.", d),
            ParseEffectDescriptorError::InvalidDelimiter {
                expected,
                found
            } => write!(f, "Expected delimiter \'{}\', but found: \'{}\'.",
                expected, found),
            ParseEffectDescriptorError::UnexpectedContinuation =>
                write!(f, "Expected end, but effect descriptor continued.")
        }
    }
}

pub struct EffectDescriptor {
    pub name: String,
    pub key_values: HashMap<String, String>
}

impl FromStr for EffectDescriptor {
    type Err = ParseEffectDescriptorError;

    fn from_str(code: &str)
            -> Result<EffectDescriptor, ParseEffectDescriptorError> {
        let mut chars = code.chars().peekable();
        let name = parse_string(&mut chars)?;
        
        let key_values = match chars.next() {
            Some('(') => Some(parse_key_value(&mut chars)?),
            Some(c) => return Err(ParseEffectDescriptorError::InvalidDelimiter {
                expected: '(',
                found: c
            }),
            None => None
        };
        
        if key_values.is_some() {
            consume_delimiter(&mut chars, ')')?;
        }
        
        if chars.next().is_some() {
            return Err(ParseEffectDescriptorError::UnexpectedContinuation);
        }
        
        let key_values = key_values.unwrap_or_else(HashMap::new);
        
        Ok(EffectDescriptor {
            name,
            key_values
        })
    }
}

fn is_delimiter(c: char) -> bool {
    c == '(' || c == ')' || c == ',' || c == '='
}

fn parse_string<'a>(chars: &mut Peekable<Chars<'a>>)
        -> Result<String, ParseEffectDescriptorError> {
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
        Err(ParseEffectDescriptorError::MissingClosingQuote)
    }
    else {
        Ok(s)
    }
}

fn consume_delimiter<'a>(chars: &mut Peekable<Chars<'a>>, delimiter: char)
        -> Result<(), ParseEffectDescriptorError> {
    match chars.next() {
        Some(c) => {
            if c == delimiter {
                Ok(())
            }
            else {
                Err(ParseEffectDescriptorError::InvalidDelimiter {
                    expected: '=',
                    found: c
                })
            }
        },
        None => Err(ParseEffectDescriptorError::MissingDelimiter(delimiter))
    }
}

fn parse_key_value<'a>(chars: &mut Peekable<Chars<'a>>)
        -> Result<HashMap<String, String>, ParseEffectDescriptorError> {
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
