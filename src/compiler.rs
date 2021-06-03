use crate::scanner::{Scanner, TokenType};
use crate::{Chunk, CompilerResult};

pub fn compile(source: &str) -> CompilerResult {
    let mut line: usize = 0;
    let mut scanner = Scanner::new(source);
    loop {
        let token = scanner.scan_token();
        if token.line != line {
            line = token.line;
            print!("{:>4} ", line);
        } else {
            print!("   | ");
        }
        println!("{:?}", token);
        if let TokenType::EOF = token.ttype {
            break;
        }
    }
    Ok(Chunk::new())
}
