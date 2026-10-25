pub fn add(a: u32, b: u32) -> u32 {
    a + b
}

pub struct Calc;

impl Calc {
    pub fn new() -> Self {
        Calc
    }

    pub fn sum(&self, a: u32) -> u32 {
        self.double(add(a, 1))
    }

    fn double(&self, x: u32) -> u32 {
        x * 2
    }
}
