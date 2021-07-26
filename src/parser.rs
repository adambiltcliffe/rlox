use crate::compiler::Compiler;
use crate::scanner::{Token, TokenType};
use crate::value::HeapEntry;
use crate::OpCode;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;

#[derive(PartialOrd, PartialEq, Ord, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(usize)]
pub enum Precedence {
    None = 0,
    Assignment = 1,
    Or = 2,
    And = 3,
    Equality = 4,
    Comparison = 5,
    Term = 6,
    Factor = 7,
    Unary = 8,
    Call = 9,
    Primary = 10,
}

type ParseFn = fn(&mut Compiler<'_, '_>);

pub struct ParseRule {
    pub prefix: Option<ParseFn>,
    pub infix: Option<ParseFn>,
    pub precedence: Precedence,
}

impl Default for ParseRule {
    fn default() -> Self {
        ParseRule {
            prefix: None,
            infix: None,
            precedence: Precedence::None,
        }
    }
}

pub fn get_rule(ttype: TokenType) -> ParseRule {
    match ttype {
        TokenType::LeftParen => ParseRule {
            prefix: Some(grouping),
            ..ParseRule::default()
        },
        TokenType::Minus => ParseRule {
            prefix: Some(unary),
            infix: Some(binary),
            precedence: Precedence::Term,
        },
        TokenType::Plus => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Term,
        },
        TokenType::Slash => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Factor,
        },
        TokenType::Star => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Factor,
        },
        TokenType::Bang => ParseRule {
            prefix: Some(unary),
            ..ParseRule::default()
        },
        TokenType::BangEqual => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Equality,
        },
        TokenType::EqualEqual => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Equality,
        },
        TokenType::Greater => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Comparison,
        },
        TokenType::GreaterEqual => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Comparison,
        },
        TokenType::Less => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Comparison,
        },
        TokenType::LessEqual => ParseRule {
            prefix: None,
            infix: Some(binary),
            precedence: Precedence::Comparison,
        },
        TokenType::Identifier => ParseRule {
            prefix: Some(variable),
            ..ParseRule::default()
        },
        TokenType::StringLiteral => ParseRule {
            prefix: Some(string),
            ..ParseRule::default()
        },
        TokenType::NumberLiteral => ParseRule {
            prefix: Some(number),
            ..ParseRule::default()
        },
        TokenType::False => ParseRule {
            prefix: Some(literal),
            ..ParseRule::default()
        },
        TokenType::Nil => ParseRule {
            prefix: Some(literal),
            ..ParseRule::default()
        },
        TokenType::True => ParseRule {
            prefix: Some(literal),
            ..ParseRule::default()
        },
        _ => ParseRule::default(),
    }
}

fn grouping(c: &mut Compiler) {
    c.expression();
    c.consume(TokenType::RightParen, "Expect ')' after expression.")
}

fn unary(c: &mut Compiler) {
    let token = c.unwrap_previous();
    let op_type = token.ttype;
    let line = token.line;
    c.parse_precedence(Precedence::Unary);
    match op_type {
        TokenType::Minus => c.emit_byte_with_line(OpCode::Negate.into(), line),
        TokenType::Bang => c.emit_byte_with_line(OpCode::Not.into(), line),
        _ => unreachable!(),
    }
}

fn binary(c: &mut Compiler) {
    let ttype = c.unwrap_previous().ttype;
    let precedence: usize = get_rule(ttype).precedence.into();
    c.parse_precedence(Precedence::try_from(precedence + 1).unwrap());
    match ttype {
        TokenType::BangEqual => c.emit_bytes(OpCode::Equal.into(), OpCode::Not.into()),
        TokenType::EqualEqual => c.emit_byte(OpCode::Equal.into()),
        TokenType::Greater => c.emit_byte(OpCode::Greater.into()),
        TokenType::GreaterEqual => c.emit_bytes(OpCode::Less.into(), OpCode::Not.into()),
        TokenType::Less => c.emit_byte(OpCode::Less.into()),
        TokenType::LessEqual => c.emit_bytes(OpCode::Greater.into(), OpCode::Not.into()),
        TokenType::Plus => c.emit_byte(OpCode::Add.into()),
        TokenType::Minus => c.emit_byte(OpCode::Subtract.into()),
        TokenType::Star => c.emit_byte(OpCode::Multiply.into()),
        TokenType::Slash => c.emit_byte(OpCode::Divide.into()),
        _ => unreachable!(),
    }
}

fn number(c: &mut Compiler) {
    let n: f64 = c.unwrap_previous().content.unwrap().parse().unwrap();
    c.emit_constant(n.into());
}

fn string(c: &mut Compiler) {
    let vm = &mut c.vm;
    let prev = &c.previous;
    let content = prev.as_ref().unwrap().content.unwrap();
    let w = HeapEntry::create_string(vm, &content[1..content.len() - 1]);
    c.emit_constant(w.into());
}

fn variable(c: &mut Compiler) {
    // unlike the book, this doesn't yet forward to named_variable() because
    // doing so introduces a double-borrow problem we don't want to solve yet
    let name = c.previous_identifier();
    match c.identifier_constant(name) {
        Err(e) => c.error(&format!("{}", e), e),
        Ok(global) => c.emit_bytes(OpCode::GetGlobal.into(), global),
    }
}

fn literal(c: &mut Compiler) {
    match c.unwrap_previous().ttype {
        TokenType::False => c.emit_byte(OpCode::False.into()),
        TokenType::Nil => c.emit_byte(OpCode::Nil.into()),
        TokenType::True => c.emit_byte(OpCode::True.into()),
        _ => unreachable!(),
    }
}
