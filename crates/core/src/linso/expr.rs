//! The map conditions, which LinSoTracker runs through Python's `eval`.
//!
//! Templates only use a small part of Python: `and`, `or`, `not`, brackets,
//! `True`, `False` and calls with string arguments, such as
//! `do('CanAccessForest') and have('Forest Keys', '>=2')`.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Const(bool),
    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Call(String, Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub source: String,
    pub reason: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.reason, self.source)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Str(String),
    Open,
    Close,
    Comma,
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => {
                chars.next();
            }
            '(' => {
                chars.next();
                tokens.push(Token::Open);
            }
            ')' => {
                chars.next();
                tokens.push(Token::Close);
            }
            ',' => {
                chars.next();
                tokens.push(Token::Comma);
            }
            '\'' | '"' => {
                let quote = c;
                chars.next();
                let mut text = String::new();
                loop {
                    match chars.next() {
                        Some(c) if c == quote => break,
                        Some('\\') => text.extend(chars.next()),
                        Some(c) => text.push(c),
                        None => return Err("unterminated string".into()),
                    }
                }
                tokens.push(Token::Str(text));
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '_' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Name(name));
            }
            other => return Err(format!("unexpected {other:?}")),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.at).cloned();
        self.at += 1;
        token
    }

    fn keyword(&mut self, word: &str) -> bool {
        if matches!(self.peek(), Some(Token::Name(name)) if name == word) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: Token) -> Result<(), String> {
        match self.next() {
            Some(found) if found == token => Ok(()),
            found => Err(format!("expected {token:?}, found {found:?}")),
        }
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.keyword("or") {
            left = Expr::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.not()?;
        while self.keyword("and") {
            left = Expr::And(Box::new(left), Box::new(self.not()?));
        }
        Ok(left)
    }

    fn not(&mut self) -> Result<Expr, String> {
        if self.keyword("not") {
            Ok(Expr::Not(Box::new(self.not()?)))
        } else {
            self.atom()
        }
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Token::Open) => {
                let inner = self.or()?;
                self.expect(Token::Close)?;
                Ok(inner)
            }
            Some(Token::Name(name)) if name == "True" => Ok(Expr::Const(true)),
            Some(Token::Name(name)) if name == "False" => Ok(Expr::Const(false)),
            Some(Token::Name(name)) => {
                self.expect(Token::Open)?;
                let mut args = Vec::new();
                if self.peek() == Some(&Token::Close) {
                    self.next();
                    return Ok(Expr::Call(name, args));
                }
                loop {
                    match self.next() {
                        Some(Token::Str(arg)) => args.push(arg),
                        other => return Err(format!("expected a string, found {other:?}")),
                    }
                    match self.next() {
                        Some(Token::Comma) => {}
                        Some(Token::Close) => break,
                        other => return Err(format!("expected , or ), found {other:?}")),
                    }
                }
                Ok(Expr::Call(name, args))
            }
            other => Err(format!("unexpected {other:?}")),
        }
    }
}

impl Expr {
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let error = |reason| ParseError {
            source: source.to_owned(),
            reason,
        };
        let tokens = tokenize(source).map_err(error)?;
        let mut parser = Parser { tokens, at: 0 };
        let expr = parser.or().map_err(error)?;
        if parser.at < parser.tokens.len() {
            return Err(error("trailing input".into()));
        }
        Ok(expr)
    }

    /// Short-circuits like Python, so `call` only sees what Python would run.
    pub fn eval(&self, call: &mut impl FnMut(&str, &[String]) -> bool) -> bool {
        match self {
            Expr::Const(value) => *value,
            Expr::Not(inner) => !inner.eval(call),
            Expr::And(left, right) => left.eval(call) && right.eval(call),
            Expr::Or(left, right) => left.eval(call) || right.eval(call),
            Expr::Call(name, args) => call(name, args),
        }
    }
}

/// The optional second argument of `have`, such as `'>=2'`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Comparison {
    op: Op,
    value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Op {
    Eq,
    Ne,
    Ge,
    Le,
    Gt,
    Lt,
}

impl Comparison {
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (op, rest) = [
            ("==", Op::Eq),
            ("!=", Op::Ne),
            (">=", Op::Ge),
            ("<=", Op::Le),
            (">", Op::Gt),
            ("<", Op::Lt),
        ]
        .into_iter()
        .find_map(|(prefix, op)| text.strip_prefix(prefix).map(|rest| (op, rest)))?;
        Some(Self {
            op,
            value: rest.trim().parse().ok()?,
        })
    }

    pub fn holds(self, left: i64) -> bool {
        let left = left as f64;
        match self.op {
            Op::Eq => left == self.value,
            Op::Ne => left != self.value,
            Op::Ge => left >= self.value,
            Op::Le => left <= self.value,
            Op::Gt => left > self.value,
            Op::Lt => left < self.value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(source: &str, truthy: &[&str]) -> bool {
        Expr::parse(source)
            .unwrap()
            .eval(&mut |_, args| truthy.contains(&args[0].as_str()))
    }

    #[test]
    fn parses_the_shapes_used_by_the_map_template() {
        let source = "do('A') and (have('B') or rules('C')) and not(have('D', '>=2'))";
        assert!(eval(source, &["A", "C"]));
        assert!(!eval(source, &["A", "C", "D"]));
        assert!(!eval(source, &["B", "C"]));
        // Python allows a call right after `or`, without a space.
        assert!(eval("have('X') or(have('Y'))", &["Y"]));
        assert!(eval("True", &[]));
        assert!(!eval("False or False", &[]));
    }

    #[test]
    fn not_binds_tighter_than_and() {
        assert!(eval("not have('A') and have('B')", &["B"]));
        assert!(!eval("not have('A') and have('B')", &["A", "B"]));
    }

    #[test]
    fn evaluation_short_circuits() {
        let expr = Expr::parse("have('A') or have('B')").unwrap();
        let mut seen = Vec::new();
        expr.eval(&mut |_, args| {
            seen.push(args[0].clone());
            true
        });
        assert_eq!(seen, ["A"]);
    }

    #[test]
    fn rejects_what_it_cannot_read() {
        assert!(Expr::parse("have('A'").is_err());
        assert!(Expr::parse("have('A') and").is_err());
        assert!(Expr::parse("1 + 2").is_err());
    }

    #[test]
    fn comparisons() {
        let at_least_two = Comparison::parse(">=2").unwrap();
        assert!(at_least_two.holds(2));
        assert!(!at_least_two.holds(1));
        assert!(Comparison::parse("==60").unwrap().holds(60));
        assert_eq!(Comparison::parse("two"), None);
    }
}
