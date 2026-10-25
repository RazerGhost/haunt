mod commands;

use app_core::math::Calc;

fn main() {
    let calc = Calc::new();
    commands::run(calc.sum(1));
}
