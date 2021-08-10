Bytecode interpreter for Lox (https://craftinginterpreters.com)

This is a rewrite of the `clox` interpreter from the second half of Robert Nystrom's book _Crafting Interpreters_.
It is intended to be idiomatic (mostly) Safe Rust, but I wrote it while learning the language, so the design has a lot of wrinkles which I wouldn't repeat.

It currently matches all the features of `clox` up until the end of Chapter 26 (i.e. everything except classes and objects) and passes all of the relevant tests from the official Lox test suite.

Points of note:

- We aim for using Rust idioms where possible, but the bytecode format is the same as the one used by `clox`, i.e. a `Vec<u8>` rather than an enum of instructions (Rust forces all elements of an enum to have the same size in memory, so this would bloat the bytecode).
- The only unsafe code is where we wrap the global allocator to track the number of bytes in use (as far as I'm aware, there is no way to do this in Safe Rust)
- We check whether to run the GC after each instruction, rather than when allocating. This is probably inefficient but does mean we avoid many of the subtle GC timing bugs mentioned in the book. Following the way `clox` manages memory more closely would require us to take much tighter control of allocations, which would be hard to do without more unsafe code.
- The error handling is an ugly mishmash of `clox`'s `error()` approach and Rust's `Result` type. This is one of the areas that would be most improved by starting from a proper Rust-focused design rather than adapting from the book chapter-by-chapter.
- If I did this again, I would choose a representation for weak pointers into the VM's heap that doesn't require writing `.upgrade().unwrap().content` in so many places (probably, we should implement `Deref` so that this becomes transparent - the `unwrap()` cannot panic unless there is a bug in our GC code).
