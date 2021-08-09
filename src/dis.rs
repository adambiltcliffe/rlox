use crate::{value::Value, Chunk, OpCode, TracingIP};
use std::convert::TryFrom;

#[allow(dead_code)]
pub(crate) fn disassemble_instruction(ip: &mut TracingIP) {
    if ip.is_line_start {
        print!("{:5} {:04} ", ip.line.unwrap(), ip.offset)
    } else {
        print!("    | {:04} ", ip.offset)
    }
    let byte = ip.read();
    match OpCode::try_from(byte) {
        Ok(instruction) => match instruction {
            OpCode::Constant => constant_instruction("CONSTANT", ip),
            OpCode::Nil => simple_instruction("NIL"),
            OpCode::True => simple_instruction("TRUE"),
            OpCode::False => simple_instruction("FALSE"),
            OpCode::Equal => simple_instruction("EQUAL"),
            OpCode::Greater => simple_instruction("GREATER"),
            OpCode::Less => simple_instruction("LESS"),
            OpCode::Negate => simple_instruction("NEGATE"),
            OpCode::Add => simple_instruction("ADD"),
            OpCode::Subtract => simple_instruction("SUBTRACT"),
            OpCode::Multiply => simple_instruction("MULTIPLY"),
            OpCode::Divide => simple_instruction("DIVIDE"),
            OpCode::Not => simple_instruction("NOT"),
            OpCode::Print => simple_instruction("PRINT"),
            OpCode::Jump => jump_instruction("JUMP", ip, 1),
            OpCode::JumpIfFalse => jump_instruction("JUMP_IF_FALSE", ip, 1),
            OpCode::Loop => jump_instruction("LOOP", ip, -1),
            OpCode::Call => byte_instruction("CALL", ip),
            OpCode::Closure => {
                let constant_index = ip.read();
                let constant = &ip.chunk.constants[constant_index as usize];
                println!("{:<16} {:<4} {}", "CLOSURE", constant_index, constant);
                match constant {
                    Value::FunctionProto(f) => {
                        for _ in 0..(f.upgrade().unwrap().content.upvalue_count) {
                            print!("    | {:04} ", ip.offset);
                            let is_local = ip.read();
                            let index = ip.read();
                            let text = match is_local {
                                0 => "upvalue",
                                _ => "local",
                            };
                            println!("|                {} {}", text, index);
                        }
                    }
                    _ => {
                        unreachable!();
                    }
                };
            }
            OpCode::CloseUpvalue => simple_instruction("CLOSE_UPVALUE"),
            OpCode::Pop => simple_instruction("POP"),
            OpCode::GetLocal => byte_instruction("GET_LOCAL", ip),
            OpCode::SetLocal => byte_instruction("SET_LOCAL", ip),
            OpCode::GetGlobal => constant_instruction("GET_GLOBAL", ip),
            OpCode::DefineGlobal => constant_instruction("DEFINE_GLOBAL", ip),
            OpCode::SetGlobal => constant_instruction("SET_GLOBAL", ip),
            OpCode::GetUpvalue => byte_instruction("GET_UPVALUE", ip),
            OpCode::SetUpvalue => byte_instruction("SET_UPVALUE", ip),
            OpCode::Return => simple_instruction("RETURN"),
        },
        Err(_) => {
            println!("Unknown opcode {}", byte);
        }
    }
}

fn simple_instruction(name: &str) {
    println!("{}", name);
}

fn byte_instruction(name: &str, ip: &mut TracingIP) {
    let byte = ip.read();
    println!("{:<16} {:<4}", name, byte);
}

fn jump_instruction(name: &str, ip: &mut TracingIP, sign: isize) {
    let offset = ip.read_short() as isize;
    println!(
        "{:<16} {:<4} -> {:<4}",
        name,
        offset,
        ip.offset as isize + offset * sign
    );
}

fn constant_instruction(name: &str, ip: &mut TracingIP) {
    let constant_index = ip.read();
    print!("{:<16} {:<4} ", name, constant_index);
    println!("{}", ip.chunk.constants[constant_index as usize]);
}

#[allow(dead_code)]
pub(crate) fn disassemble_chunk(chunk: &Chunk, name: &str) {
    println!("== {} ==", name);
    let mut ip = TracingIP::new(chunk, 0);
    while ip.valid() {
        disassemble_instruction(&mut ip);
    }
}
