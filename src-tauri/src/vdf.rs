//! Minimal parser for Valve's text KeyValues (VDF) format, as used by
//! `libraryfolders.vdf`, `appmanifest_*.acf` and `localconfig.vdf`.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Vdf {
    Str(String),
    Map(BTreeMap<String, Vdf>),
}

impl Vdf {
    /// Case-insensitive child lookup (Valve files are inconsistent with casing).
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Map(m) => m.get(key).or_else(|| {
                m.iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(key))
                    .map(|(_, v)| v)
            }),
            Vdf::Str(_) => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Vdf> {
        keys.iter().try_fold(self, |node, k| node.get(k))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Map(_) => None,
        }
    }

    pub fn str_at(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Vdf::as_str)
    }

    pub fn as_map(&self) -> Option<&BTreeMap<String, Vdf>> {
        match self {
            Vdf::Map(m) => Some(m),
            Vdf::Str(_) => None,
        }
    }
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

#[derive(Debug, PartialEq)]
enum Tok {
    Str(String),
    Open,
    Close,
}

impl<'a> Lexer<'a> {
    fn next_tok(&mut self) -> Result<Option<Tok>, String> {
        loop {
            match self.chars.peek().copied() {
                None => return Ok(None),
                Some(c) if c.is_whitespace() => {
                    self.chars.next();
                }
                Some('/') => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'/') {
                        for c in self.chars.by_ref() {
                            if c == '\n' {
                                break;
                            }
                        }
                    } else {
                        return Err("unexpected '/'".into());
                    }
                }
                Some('{') => {
                    self.chars.next();
                    return Ok(Some(Tok::Open));
                }
                Some('}') => {
                    self.chars.next();
                    return Ok(Some(Tok::Close));
                }
                Some('"') => {
                    self.chars.next();
                    let mut s = String::new();
                    loop {
                        match self.chars.next() {
                            None => return Err("unterminated string".into()),
                            Some('"') => break,
                            Some('\\') => match self.chars.next() {
                                Some('n') => s.push('\n'),
                                Some('t') => s.push('\t'),
                                Some('\\') => s.push('\\'),
                                Some('"') => s.push('"'),
                                Some(o) => {
                                    s.push('\\');
                                    s.push(o);
                                }
                                None => return Err("unterminated escape".into()),
                            },
                            Some(c) => s.push(c),
                        }
                    }
                    return Ok(Some(Tok::Str(s)));
                }
                Some(_) => {
                    // Unquoted token (rare, but allowed by the format).
                    let mut s = String::new();
                    while let Some(&c) = self.chars.peek() {
                        if c.is_whitespace() || c == '{' || c == '}' || c == '"' {
                            break;
                        }
                        s.push(c);
                        self.chars.next();
                    }
                    return Ok(Some(Tok::Str(s)));
                }
            }
        }
    }
}

fn parse_map(lx: &mut Lexer, nested: bool) -> Result<BTreeMap<String, Vdf>, String> {
    let mut map = BTreeMap::new();
    loop {
        let key = match lx.next_tok()? {
            None if !nested => return Ok(map),
            None => return Err("unexpected end of input".into()),
            Some(Tok::Close) if nested => return Ok(map),
            Some(Tok::Close) => return Err("unexpected '}'".into()),
            Some(Tok::Open) => return Err("unexpected '{'".into()),
            Some(Tok::Str(k)) => k,
        };
        match lx.next_tok()? {
            Some(Tok::Str(v)) => {
                map.insert(key, Vdf::Str(v));
            }
            Some(Tok::Open) => {
                let child = parse_map(lx, true)?;
                map.insert(key, Vdf::Map(child));
            }
            _ => return Err(format!("missing value for key '{key}'")),
        }
    }
}

/// Parse a VDF document. The result is always a map of the top-level keys.
pub fn parse(input: &str) -> Result<Vdf, String> {
    let mut lx = Lexer {
        chars: input.chars().peekable(),
    };
    parse_map(&mut lx, false).map(Vdf::Map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_appmanifest() {
        let src = r#"
"AppState"
{
	"appid"		"730"
	"name"		"Counter-Strike 2"
	"StateFlags"		"4"
	"installdir"		"Counter-Strike Global Offensive"
	"UserConfig"
	{
		"language"		"english"
	}
}"#;
        let v = parse(src).unwrap();
        let app = v.get("AppState").unwrap();
        assert_eq!(app.str_at("appid"), Some("730"));
        assert_eq!(app.str_at("NAME"), Some("Counter-Strike 2"));
        assert_eq!(
            app.path(&["userconfig", "language"]).and_then(Vdf::as_str),
            Some("english")
        );
    }

    #[test]
    fn handles_escapes_and_comments() {
        let v =
            parse("// comment\n\"a\" \"C:\\\\Games\\\\x\"\n\"b\" { \"c\" \"q\\\"q\" }").unwrap();
        assert_eq!(v.str_at("a"), Some("C:\\Games\\x"));
        assert_eq!(v.path(&["b", "c"]).and_then(Vdf::as_str), Some("q\"q"));
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse("\"a\" {").is_err());
        assert!(parse("\"a\"").is_err());
        assert!(parse("}").is_err());
    }
}
