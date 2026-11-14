//! The TOML subset the design models use, ported from `src/domain/toml.mjs`: comments, bare or
//! quoted keys, basic and literal strings, integers, booleans, arrays (multi-line), inline tables,
//! `[table]` and `[[array]]` headers. Anything else (dates, floats, dotted keys, multi-line strings)
//! is rejected with its line number, so a model never parses to something its author did not write.

use crate::js_number;
use serde_json::{Map, Value};
use std::collections::HashSet;

/// Parses `text` into a JSON object, or fails with `line N: reason` exactly as the Node parser does.
pub(crate) fn parse(text: &str) -> Result<Value, String> {
    let mut parser = Parser {
        src: text.replace("\r\n", "\n").chars().collect(),
        pos: 0,
        line: 1,
        root: Map::new(),
        headers: HashSet::new(),
        current: Current::Root,
    };
    parser.document()?;
    Ok(Value::Object(parser.root))
}

enum Current {
    Root,
    Table(String),
    ArrayItem(String),
}

struct Parser {
    src: Vec<char>,
    pos: usize,
    line: usize,
    root: Map<String, Value>,
    headers: HashSet<String>,
    current: Current,
}

type Parsed<T> = Result<T, String>;

fn is_bare(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// JavaScript's `\s`.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

fn assign(table: &mut Map<String, Value>, key: String, value: Value, line: usize) -> Parsed<()> {
    if table.contains_key(&key) {
        return Err(format!("line {line}: duplicate key \"{key}\""));
    }
    table.insert(key, value);
    Ok(())
}

impl Parser {
    fn fail<T>(&self, message: &str) -> Parsed<T> {
        self.fail_at(message, self.line)
    }
    fn fail_at<T>(&self, message: &str, at: usize) -> Parsed<T> {
        Err(format!("line {at}: {message}"))
    }
    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }
    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }
    fn starts_with(&self, text: &str) -> bool {
        text.chars()
            .enumerate()
            .all(|(i, c)| self.src.get(self.pos + i) == Some(&c))
    }
    fn advance(&mut self) {
        if self.peek() == Some('\n') {
            self.line += 1;
        }
        self.pos += 1;
    }
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.advance();
        }
    }
    fn skip_comment(&mut self) {
        if self.peek() == Some('#') {
            while !self.at_end() && self.peek() != Some('\n') {
                self.advance();
            }
        }
    }
    /// Inside arrays and inline tables: whitespace, newlines and comments.
    fn skip_blank(&mut self) {
        loop {
            self.skip_spaces();
            self.skip_comment();
            if self.peek() != Some('\n') {
                return;
            }
            self.advance();
        }
    }
    fn end_of_line(&mut self) -> Parsed<()> {
        self.skip_spaces();
        self.skip_comment();
        if !self.at_end() && self.peek() != Some('\n') {
            return self.fail("expected end of line");
        }
        Ok(())
    }

    fn key(&mut self) -> Parsed<String> {
        let key = if matches!(self.peek(), Some('"' | '\'')) {
            self.string()?
        } else {
            let start = self.pos;
            while self.peek().is_some_and(is_bare) {
                self.advance();
            }
            if start == self.pos {
                let got = self.peek().map_or("end of file".into(), String::from);
                return self.fail(&format!("expected a key, got \"{got}\""));
            }
            self.src[start..self.pos].iter().collect()
        };
        self.skip_spaces();
        if self.peek() == Some('.') {
            return self.fail("dotted keys are not supported");
        }
        Ok(key)
    }

    fn string(&mut self) -> Parsed<String> {
        let quote = self.peek().unwrap_or('"');
        if self.starts_with(&quote.to_string().repeat(3)) {
            return self.fail("multi-line strings are not supported");
        }
        let start = self.pos;
        self.advance();
        while self.peek() != Some(quote) {
            if matches!(self.peek(), None | Some('\n')) {
                return self.fail("unterminated string");
            }
            if quote == '"' && self.peek() == Some('\\') {
                self.advance();
            }
            self.advance();
        }
        self.advance();
        let raw: String = self.src[start..self.pos].iter().collect();
        if quote == '\'' {
            return Ok(raw[1..raw.len() - 1].to_owned());
        }
        serde_json::from_str(&raw).or_else(|_| self.fail(&format!("unsupported escape in {raw}")))
    }

    fn array(&mut self) -> Parsed<Value> {
        let mut out = Vec::new();
        let opened = self.line;
        self.advance();
        loop {
            self.skip_blank();
            if self.at_end() {
                return self.fail_at("unterminated array", opened);
            }
            if self.peek() == Some(']') {
                break;
            }
            out.push(self.value()?);
            self.skip_blank();
            if self.at_end() {
                return self.fail_at("unterminated array", opened);
            }
            if self.peek() == Some(',') {
                self.advance();
            } else if self.peek() != Some(']') {
                return self.fail("expected \",\" or \"]\" in array");
            }
        }
        self.advance();
        Ok(Value::Array(out))
    }

    fn inline_table(&mut self) -> Parsed<Value> {
        let mut out = Map::new();
        let opened = self.line;
        self.advance();
        loop {
            self.skip_blank();
            if self.at_end() {
                return self.fail_at("unterminated inline table", opened);
            }
            if self.peek() == Some('}') {
                break;
            }
            let key = self.key()?;
            if self.peek() != Some('=') {
                return self.fail(&format!("expected \"=\" after \"{key}\""));
            }
            self.advance();
            self.skip_spaces();
            let value = self.value()?;
            assign(&mut out, key, value, self.line)?;
            self.skip_blank();
            if self.peek() == Some(',') {
                self.advance();
            } else if self.peek() != Some('}') {
                return self.fail("expected \",\" or \"}\" in inline table");
            }
        }
        self.advance();
        Ok(Value::Object(out))
    }

    fn value(&mut self) -> Parsed<Value> {
        match self.peek() {
            Some('"' | '\'') => return self.string().map(Value::String),
            Some('[') => return self.array(),
            Some('{') => return self.inline_table(),
            _ => (),
        }
        let Some(word) = self.word() else {
            return self.fail("unsupported value");
        };
        self.pos += word.chars().count();
        Ok(match word.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            digits => js_number::number(
                digits
                    .replace('_', "")
                    .parse()
                    .expect("word() only matches a signed decimal integer"),
            ),
        })
    }

    /// `true`, `false`, or `[+-]?\d[\d_]*`, followed by whitespace, `,`, `]`, `}`, `#`, or the end.
    fn word(&self) -> Option<String> {
        let rest = &self.src[self.pos..];
        let boundary = |i: usize| {
            rest.get(i)
                .is_none_or(|&c| is_js_space(c) || matches!(c, ',' | ']' | '}' | '#'))
        };
        for literal in ["true", "false"] {
            if self.starts_with(literal) && boundary(literal.len()) {
                return Some(literal.into());
            }
        }
        let sign = usize::from(matches!(rest.first(), Some('+' | '-')));
        if !rest.get(sign).is_some_and(char::is_ascii_digit) {
            return None;
        }
        let end = sign
            + 1
            + rest[sign + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit() || **c == '_')
                .count();
        boundary(end).then(|| rest[..end].iter().collect())
    }

    fn header(&mut self) -> Parsed<()> {
        let is_array = self.starts_with("[[");
        self.advance();
        if is_array {
            self.advance();
        }
        self.skip_spaces();
        let name = self.key()?;
        let close = if is_array { "]]" } else { "]" };
        for c in close.chars() {
            if self.peek() != Some(c) {
                return self.fail(&format!("expected \"{close}\" after table name"));
            }
            self.advance();
        }
        if is_array {
            let marker = format!("[[{name}]]");
            if self.root.contains_key(&name)
                && !(self.root[&name].is_array() && self.headers.contains(&marker))
            {
                return self.fail(&format!("\"{name}\" is not an array of tables"));
            }
            self.headers.insert(marker);
            if let Value::Array(items) = self
                .root
                .entry(name.clone())
                .or_insert_with(|| Value::Array(Vec::new()))
            {
                items.push(Value::Object(Map::new()));
            }
            self.current = Current::ArrayItem(name);
        } else {
            if self.headers.contains(&name) || self.root.contains_key(&name) {
                return self.fail(&format!("table \"{name}\" is defined twice"));
            }
            self.headers.insert(name.clone());
            self.root.insert(name.clone(), Value::Object(Map::new()));
            self.current = Current::Table(name);
        }
        self.end_of_line()
    }

    fn current_table(&mut self) -> &mut Map<String, Value> {
        let table = match &self.current {
            Current::Root => return &mut self.root,
            Current::Table(name) => self.root.get_mut(name),
            Current::ArrayItem(name) => self
                .root
                .get_mut(name)
                .and_then(|items| items.as_array_mut()?.last_mut()),
        };
        table
            .and_then(Value::as_object_mut)
            .expect("the current table exists")
    }

    fn document(&mut self) -> Parsed<()> {
        while !self.at_end() {
            self.skip_spaces();
            self.skip_comment();
            if self.at_end() {
                break;
            }
            if self.peek() == Some('\n') {
                self.advance();
                continue;
            }
            if self.peek() == Some('[') {
                self.header()?;
                continue;
            }
            let key = self.key()?;
            if self.peek() != Some('=') {
                return self.fail(&format!("expected \"=\" after \"{key}\""));
            }
            self.advance();
            self.skip_spaces();
            let value = self.value()?;
            let line = self.line;
            assign(self.current_table(), key, value, line)?;
            self.end_of_line()?;
        }
        Ok(())
    }
}
