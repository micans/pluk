// The compute operators (plan/COMPUTE.design.txt section 5).
//
// One static table entry per operator. Operator families are added step by step;
// step10 has a seed set that exercises every path of the machinery. The entries
// grow with the steps: the stack effect and description now (parsing), operand
// kinds and the prepare hook with compilation, the implementation with execution.

// How many values an operator takes from the stack.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Pops {
    N(usize),       // exactly N; fewer is a stack underflow (a compile-time error)
    All,            // the whole stack, possibly empty (addall, joinall, ...)
}

// How an operator changes the static stack (compute.rs keeps track of what is on
// the stack at compile time): most push one result of a known kind; the stack
// operators rearrange.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Shape {
    Result(Kind),
    Dup,
    Xch,
    Pop,
}

// What an operator result is known to be.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Str,
    Num,
}

// Work done at compile time on a constant operand (COMPUTE.design.txt section 5).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Prepare {
    Nothing,
    ConstRegex,     // the top operand must be a constant; it is compiled as a regex
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Category {
    Stack,
    String,
    Arithmetic,
    Regex,
}

#[derive(Debug)]
pub struct OpDef {
    pub name:     &'static str,
    pub pops:     Pops,
    pub pushes:   usize,
    pub shape:    Shape,
    pub prepare:  Prepare,
    pub category: Category,
    pub operands: &'static str,     // as pick's help: the operands, top of stack last
    pub result:   &'static str,     // and what is pushed
    pub doc:      &'static str,
}

// The table; descriptions follow pick's.
pub static OPERATORS: &[OpDef] = &[
    OpDef { name: "dup",    pops: Pops::N(1), pushes: 2, shape: Shape::Dup, prepare: Prepare::Nothing, category: Category::Stack,
            operands: "x", result: "x x", doc: "Duplicate top entry x" },
    OpDef { name: "pop",    pops: Pops::N(1), pushes: 0, shape: Shape::Pop, prepare: Prepare::Nothing, category: Category::Stack,
            operands: "x", result: "-", doc: "Remove top entry x from stack" },
    OpDef { name: "xch",    pops: Pops::N(2), pushes: 2, shape: Shape::Xch, prepare: Prepare::Nothing, category: Category::Stack,
            operands: "x y", result: "y x", doc: "Exchange x and y" },
    OpDef { name: "cat",    pops: Pops::N(2), pushes: 1, shape: Shape::Result(Kind::Str), prepare: Prepare::Nothing, category: Category::String,
            operands: "x y", result: "xy", doc: "Concatenation of x and y" },
    OpDef { name: "add",    pops: Pops::N(2), pushes: 1, shape: Shape::Result(Kind::Num), prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "x y", result: "x+y", doc: "Add x and y" },
    OpDef { name: "addall", pops: Pops::All,  pushes: 1, shape: Shape::Result(Kind::Num), prepare: Prepare::Nothing, category: Category::Arithmetic,
            operands: "*", result: "sum(Stack)", doc: "Sum of all entries in stack" },
    OpDef { name: "del",    pops: Pops::N(2), pushes: 1, shape: Shape::Result(Kind::Str), prepare: Prepare::ConstRegex, category: Category::Regex,
            operands: "x p", result: "x =~ s/p//", doc: "Delete pattern p in x" },
];

pub fn lookup(name: &[u8]) -> Option<&'static OpDef> {
    OPERATORS.iter().find(|op| op.name.as_bytes() == name)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table() {
        assert_eq!(lookup(b"add").map(|op| op.pops), Some(Pops::N(2)));
        assert_eq!(lookup(b"addall").map(|op| op.pops), Some(Pops::All));
        assert!(lookup(b"nosuch").is_none());
        // Names are unique.
        for (i, a) in OPERATORS.iter().enumerate() {
            assert!(OPERATORS[i + 1..].iter().all(|b| b.name != a.name), "{}", a.name);
        }
        // The shape agrees with the stack effect.
        for op in OPERATORS {
            let ok = match op.shape {
                Shape::Result(_) => op.pushes == 1,
                Shape::Dup => op.pops == Pops::N(1) && op.pushes == 2,
                Shape::Xch => op.pops == Pops::N(2) && op.pushes == 2,
                Shape::Pop => op.pops == Pops::N(1) && op.pushes == 0,
            };
            assert!(ok, "{}", op.name);
        }
    }
}
