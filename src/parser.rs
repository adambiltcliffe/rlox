use crate::compiler::Compiler;
use crate::scanner::TokenType;
use crate::{OpCode, Value};
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

type ParseFn = fn(&mut Compiler<'_>);

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
        TokenType::NumberLiteral => ParseRule {
            prefix: Some(number),
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
        _ => unreachable!(),
    }
}

fn binary(c: &mut Compiler) {
    let ttype = c.unwrap_previous().ttype;
    let precedence: usize = get_rule(ttype).precedence.into();
    c.parse_precedence(Precedence::try_from(precedence + 1).unwrap());
    match ttype {
        TokenType::Plus => c.emit_byte(OpCode::Add.into()),
        TokenType::Minus => c.emit_byte(OpCode::Subtract.into()),
        TokenType::Star => c.emit_byte(OpCode::Multiply.into()),
        TokenType::Slash => c.emit_byte(OpCode::Divide.into()),
        _ => unreachable!(),
    }
}

fn number(c: &mut Compiler) {
    let value: Value = c.unwrap_previous().content.unwrap().parse().unwrap();
    c.emit_constant(value);
}
