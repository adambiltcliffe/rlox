use crate::{Chunk, OpCode, TracingIP};
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
