extern crate prusti_contracts;

struct S {
    f: i32
}

trait T {
    fn test(&mut self);
}

impl T for S {
    #[requires="self.f == 123"]
    #[ensures="self.f == 456"]
    fn test(&mut self) {
        let new_self = S {
            f: 456
        };
        *self = new_self;
    }
}

fn main() {}
