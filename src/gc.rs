use crate::value::{
    Closure, Function, Native, ObjectRef, ObjectRoot, Upvalue, UpvalueLocation, Value,
};
use crate::VM;

#[cfg(feature = "verbose_gc")]
use crate::memory::get_allocated_bytes;

pub trait Mark {
    fn can_free(&self) -> bool;
    fn unmark(&self);
}

pub trait Trace: Mark + std::fmt::Display {
    fn trace(&self, wl: &mut Worklist);
}

type Worklist = Vec<Box<dyn Trace>>;

impl VM {
    pub fn collect_garbage(&mut self) {
        #[cfg(feature = "verbose_gc")]
        println!("--gc begin, {} bytes allocated", get_allocated_bytes());

        let mut wl = Vec::new();
        self.mark_roots(&mut wl);
        loop {
            match wl.pop() {
                None => break,
                Some(oroot) => {
                    oroot.trace(&mut wl);
                }
            }
        }
        #[cfg(feature = "verbose_gc")]
        {
            print!("mark and trace completed - ");
            let to_free: Vec<_> = self.objects.iter().filter(|obj| obj.can_free()).collect();
            if to_free.len() > 0 {
                println!("the following objects will be freed:");
                for obj in to_free {
                    println!("{}", obj);
                }
            } else {
                println!("nothing to free");
            }
        }

        self.strings.retain(|interned| !interned.0.can_free());

        // drain_filter would be lovely here but we are using stable
        self.objects.retain(|oroot| !oroot.can_free());
        for obj in &self.objects {
            obj.unmark();
        }

        #[cfg(feature = "verbose_gc")]
        println!("--gc end, {} bytes allocated", get_allocated_bytes());
    }

    fn mark_roots(&mut self, wl: &mut Worklist) {
        for value in &self.stack {
            mark_value(value, wl);
        }
        for (k, v) in &self.globals {
            mark_root(&k.0, wl);
            mark_value(v, wl);
        }
        for f in &self.frames {
            mark_root::<Closure>(&f.closure, wl);
        }
        for uv in &self.open_upvalues {
            mark_ref::<Upvalue>(uv, wl);
        }
        // unlike clox, our GC cannot run during compilation, so we have
        // no separate mark_compiler_roots function
    }
}

fn mark_value(value: &Value, wl: &mut Worklist) {
    match value {
        Value::String(oref) => mark_ref(oref, wl),
        Value::FunctionProto(oref) => mark_ref(oref, wl),
        Value::Function(oref) => mark_ref(oref, wl),
        Value::Native(oref) => mark_ref(oref, wl),
        Value::Bool(_) | Value::Number(_) | Value::Nil => (),
    }
}

fn mark_ref<T: 'static>(oref: &ObjectRef<T>, wl: &mut Worklist)
where
    ObjectRoot<T>: Trace,
{
    mark_root(&oref.upgrade().unwrap(), wl);
}

fn mark_root<T: 'static>(oroot: &ObjectRoot<T>, wl: &mut Worklist)
where
    ObjectRoot<T>: Trace,
{
    let mut marked = oroot.marked.borrow_mut();
    if !*marked {
        *marked = true;
        wl.push(Box::new(oroot.clone()));
        #[cfg(feature = "verbose_gc")]
        println!("marking {}", oroot);
    }
}

impl Trace for ObjectRoot<String> {
    fn trace(&self, _wl: &mut Worklist) {}
}

impl Trace for ObjectRoot<Native> {
    fn trace(&self, _wl: &mut Worklist) {}
}

impl Trace for ObjectRoot<Function> {
    fn trace(&self, wl: &mut Worklist) {
        match &self.content.name {
            None => (),
            Some(s) => mark_ref(s, wl),
        }
        for c in &self.content.chunk.constants {
            mark_value(c, wl);
        }
    }
}

impl Trace for ObjectRoot<Closure> {
    fn trace(&self, wl: &mut Worklist) {
        mark_ref(&self.content.function, wl);
        for uv in &self.content.upvalues {
            mark_ref(uv, wl);
        }
    }
}

impl Trace for ObjectRoot<Upvalue> {
    fn trace(&self, wl: &mut Worklist) {
        match &*self.content.location.borrow() {
            UpvalueLocation::Stack(_) => (),
            UpvalueLocation::Heap(v) => mark_value(&v, wl),
        }
    }
}

impl<T> Mark for ObjectRoot<T> {
    fn can_free(&self) -> bool {
        *self.marked.borrow() == false
    }
    fn unmark(&self) {
        *self.marked.borrow_mut() = false;
    }
}
