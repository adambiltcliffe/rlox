use crate::compiler::Compiler;
use crate::scanner::TokenType;
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

type ParseFn = fn(&mut Compiler<'_, '_>, bool);

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
        TokenType::And => ParseRule {
            prefix: None,
            infix: Some(and_op),
            precedence: Precedence::And,
        },
        TokenType::Or => ParseRule {
            prefix: None,
            infix: Some(or_op),
            precedence: Precedence::Or,
        },
        _ => ParseRule::default(),
    }
}

fn grouping(c: &mut Compiler, _can_assign: bool) {
    c.expression();
    c.consume(TokenType::RightParen, "Expect ')' after expression.")
}

fn unary(c: &mut Compiler, _can_assign: bool) {
    let token = c.previous.as_ref().unwrap();
    let op_type = token.ttype;
    let line = token.line;
    c.parse_precedence(Precedence::Unary);
    match op_type {
        TokenType::Minus => c.emit_byte_with_line(OpCode::Negate.into(), line),
        TokenType::Bang => c.emit_byte_with_line(OpCode::Not.into(), line),
        _ => unreachable!(),
    }
}

fn binary(c: &mut Compiler, _can_assign: bool) {
    let ttype = c.previous.as_ref().unwrap().ttype;
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

fn number(c: &mut Compiler, _can_assign: bool) {
    let n: f64 = c
        .previous
        .as_ref()
        .unwrap()
        .content
        .unwrap()
        .parse()
        .unwrap();
    c.emit_constant(n.into());
}

fn string(c: &mut Compiler, _can_assign: bool) {
    let vm = &mut c.vm;
    let prev = &c.previous;
    let content = prev.as_ref().unwrap().content.unwrap();
    let w = HeapEntry::create_string(vm, &content[1..content.len() - 1]);
    c.emit_constant(w.into());
}

fn variable(c: &mut Compiler, can_assign: bool) {
    // unlike the book, this doesn't yet forward to named_variable() because
    // doing so introduces a double-borrow problem we don't want to solve yet
    let name_str = c.previous.as_ref().unwrap().content.unwrap();
    let name_val = c.previous_identifier();
    let slot = c.resolve_local(name_str);
    let (get_op, set_op, arg) = match slot {
        Some(a) => (OpCode::GetLocal, OpCode::SetLocal, Ok(a)),
        None => (
            OpCode::GetGlobal,
            OpCode::SetGlobal,
            c.identifier_constant(name_val),
        ),
    };
    match arg {
        Err(e) => c.short_error(e),
        Ok(a) => {
            if can_assign && c.match_token(TokenType::Equal) {
                c.expression();
                c.emit_bytes(set_op.into(), a)
            } else {
                c.emit_bytes(get_op.into(), a)
            }
        }
    }
}

fn literal(c: &mut Compiler, _can_assign: bool) {
    match c.previous.as_ref().unwrap().ttype {
        TokenType::False => c.emit_byte(OpCode::False.into()),
        TokenType::Nil => c.emit_byte(OpCode::Nil.into()),
        TokenType::True => c.emit_byte(OpCode::True.into()),
        _ => unreachable!(),
    }
}

fn and_op(c: &mut Compiler, _can_assign: bool) {
    let end_jump = c.emit_jump(OpCode::JumpIfFalse);
    c.emit_byte(OpCode::Pop.into());
    c.parse_precedence(Precedence::And);
    c.patch_jump(end_jump);
}

fn or_op(c: &mut Compiler, _can_assign: bool) {
    let else_jump = c.emit_jump(OpCode::JumpIfFalse);
    let end_jump = c.emit_jump(OpCode::Jump);
    c.patch_jump(else_jump);
    c.emit_byte(OpCode::Pop.into());
    c.parse_precedence(Precedence::Or);
    c.patch_jump(end_jump);
}
